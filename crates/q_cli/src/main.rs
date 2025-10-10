pub mod cli;
pub mod util;

use std::process::ExitCode;

use anstream::eprintln;
use clap::Parser;
use clap::error::{
    ContextKind,
    ErrorKind,
};
use crossterm::style::Stylize;
use eyre::Result;
use fig_log::get_log_level_max;
use fig_util::{
    CLI_BINARY_NAME,
    PRODUCT_NAME,
};
use tracing::metadata::LevelFilter;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() -> Result<ExitCode> {
    color_eyre::install()?;
    fig_telemetry::set_dispatch_mode(fig_telemetry::DispatchMode::On);
    fig_telemetry::init_global_telemetry_emitter();

    // Handle migration with compatibility mode
    handle_migration_compatibility();

    let mut args = std::env::args();
    let subcommand = args.nth(1);
    let multithread = matches!(
        subcommand.as_deref(),
        Some("init" | "_" | "internal" | "completion" | "hook" | "chat")
    );

    let parsed = match cli::Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let _ = err.print();

            let unknown_arg = matches!(err.kind(), ErrorKind::UnknownArgument | ErrorKind::InvalidSubcommand)
                && !err.context().any(|(context_kind, _)| {
                    matches!(
                        context_kind,
                        ContextKind::SuggestedSubcommand | ContextKind::SuggestedArg
                    )
                });

            if unknown_arg {
                eprintln!(
                    "\nThis command may be valid in newer versions of the {PRODUCT_NAME} CLI. Try running {} {}.",
                    CLI_BINARY_NAME.magenta(),
                    "update".magenta()
                );
            }

            return Ok(ExitCode::from(err.exit_code().try_into().unwrap_or(2)));
        },
    };

    let verbose = parsed.verbose > 0;

    let runtime = if multithread {
        tokio::runtime::Builder::new_multi_thread()
    } else {
        tokio::runtime::Builder::new_current_thread()
    }
    .enable_all()
    .build()?;

    let result = runtime.block_on(async {
        let result = parsed.execute().await;
        fig_telemetry::finish_telemetry().await;
        result
    });

    match result {
        Ok(exit_code) => Ok(exit_code),
        Err(err) => {
            if verbose || get_log_level_max() > LevelFilter::INFO {
                eprintln!("{} {err:?}", "error:".bold().red());
            } else {
                eprintln!("{} {err}", "error:".bold().red());
            }
            Ok(ExitCode::FAILURE)
        },
    }
}

/// Handle migration with backward compatibility support
fn handle_migration_compatibility() {
    // Check for dual installation
    if let Ok(true) = fig_install::detect_dual_installation() {
        // Prompt user for migration choice
        match fig_install::prompt_migration_choice() {
            Ok(true) => {
                // User chose to migrate
                if let Err(_) = perform_migration_with_rollback() {
                    eprintln!("Migration failed. Your original Amazon Q installation has been preserved.");
                } else {
                    println!("Migration completed successfully! You can now use 'kiro' commands.");
                    // Clean up old directories after successful migration
                    let _ = fig_install::cleanup_old_directories();
                    let _ = fig_integrations::remove_old_shell_integrations();
                }
            },
            Ok(false) => {
                // User chose not to migrate - just do silent symlink replacement
                let _ = fig_install::replace_symlinks();
            },
            Err(_) => {
                // Error in prompting - fall back to silent replacement
                let _ = fig_install::replace_symlinks();
            },
        }
    } else {
        // No dual installation detected - just do silent symlink replacement
        let _ = fig_install::replace_symlinks();
    }
}

/// Perform migration with automatic rollback on failure
fn perform_migration_with_rollback() -> Result<(), Box<dyn std::error::Error>> {
    // Create backup before migration
    fig_install::backup_symlinks()?;

    // Attempt migration
    match fig_install::replace_symlinks() {
        Ok(_) => Ok(()),
        Err(e) => {
            // Migration failed - rollback
            let _ = fig_install::rollback_migration();
            Err(e)
        },
    }
}
