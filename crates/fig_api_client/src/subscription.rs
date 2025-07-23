use std::time::SystemTime;

use amzn_codewhisperer_client::types::{
    OverageStatus,
    ResourceType,
    SubscriptionType,
};
use fig_auth::builder_id::TokenType;
use fig_auth::builder_id_token;
use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    Client,
    Error,
};

#[derive(Debug)]
pub struct SubscriptionStatusInfo {
    pub tier: SubscriptionTier,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum SubscriptionTier {
    Free,
    Pro,
    ProPlus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UsageLimitsInfo {
    pub current_usage: Option<i32>,
    pub usage_limit: Option<i32>,
    pub overage_charges: Option<f64>,
    pub overage_enabled: bool,
    pub subscription_tier: SubscriptionTier,
    pub reset_date_utc: Option<String>,
}
pub async fn generate_console_url() -> Result<String, Error> {
    let token = builder_id_token().await.ok().flatten();
    let region = token.as_ref().and_then(|t| t.region.clone());

    // IAM Identity Center (IdC) users always go to the console subscription page
    if token
        .as_ref()
        .is_some_and(|t| matches!(t.token_type(), TokenType::IamIdentityCenter))
    {
        return Ok(console_url(region.as_deref()));
    }

    // Builder ID users
    let client = Client::new().await?;
    match client.create_subscription_token().await {
        Ok(r) => Ok(r
            .encoded_verification_url()
            .map_or_else(|| console_url(region.as_deref()), str::to_owned)),
        Err(e) => {
            if e.to_string().contains("ConflictException") {
                Ok(console_url(region.as_deref()))
            } else {
                Err(e)
            }
        },
    }
}

pub async fn get_usage_limits() -> Result<UsageLimitsInfo, Error> {
    let client = Client::new().await?;
    let response = client.get_usage_limits(Some(ResourceType::AgenticRequest)).await?;

    let subscription_tier = match response.subscription_info() {
        Some(info) => match info.r#type() {
            SubscriptionType::QDeveloperStandaloneFree => SubscriptionTier::Free,
            SubscriptionType::QDeveloperStandalone => SubscriptionTier::Pro,
            SubscriptionType::QDeveloperStandaloneProPlus => SubscriptionTier::ProPlus,
            _ => SubscriptionTier::Free,
        },
        None => SubscriptionTier::Free,
    };

    // Get usage breakdown
    let (current_usage, usage_limit, overage_charges, reset_date_utc) = if let Some(ub) = response.usage_breakdown() {
        let reset_local_str = ub
            .next_date_reset()
            .and_then(|dt| SystemTime::try_from(*dt).ok())
            .map_or_else(
                || "1st of next month 12:00:00 GMT".to_string(),
                |st| {
                    let local: chrono::DateTime<chrono::Local> = st.into();
                    local.format("%m/%d/%Y at %H:%M:%S").to_string()
                },
            );
        (
            Some(ub.current_usage()),
            Some(ub.usage_limit()),
            Some(ub.overage_charges()),
            Some(reset_local_str),
        )
    } else {
        tracing::error!("get_usage_limits: missing UsageBreakdown in response");
        (None, None, None, None)
    };

    let overage_enabled = match response.overage_configuration() {
        Some(config) => matches!(config.overage_status(), OverageStatus::Enabled),
        None => false,
    };

    Ok(UsageLimitsInfo {
        current_usage,
        usage_limit,
        overage_charges,
        overage_enabled,
        subscription_tier,
        reset_date_utc,
    })
}

fn console_url(region: Option<&str>) -> String {
    match region {
        Some(r) => format!("https://{r}.console.aws.amazon.com/amazonq/developer/home#/subscriptions"),
        None => "https://docs.aws.amazon.com/console/amazonq/upgrade-builder-id".to_string(),
    }
}
