use std::path::PathBuf;

use eyre::Result;
use rustix::fs::{
    FlockOperation,
    flock,
};
use tokio::fs;
use tracing::debug;

const KIRO_MIGRATION_KEY: &str = "migration.kiro.completed";
const MIGRATION_TIMEOUT_SECS: u64 = 10;

pub async fn migrate_if_needed() -> Result<bool> {
    let status = detect_migration().await?;

    match status {
        MigrationStatus::Completed => {
            debug!("Migration already completed");
            return Ok(false);
        },
        MigrationStatus::NotNeeded => {
            debug!("No migration needed");
            mark_migration_completed()?;
            return Ok(false);
        },
        MigrationStatus::Needed => {
            debug!("Migrating database and settings");
        },
    }

    let _lock = match acquire_migration_lock()? {
        Some(lock) => lock,
        None => {
            debug!("Migration already in progress");
            return Ok(false);
        },
    };

    let old_dir = fig_util::directories::old_fig_data_dir()?;
    let new_dir = fig_util::directories::fig_data_dir()?;

    debug!("Old directory: {}", old_dir.display());
    debug!("New directory: {}", new_dir.display());

    // Copy essential files from old directory to new directory
    if !new_dir.exists() {
        fs::create_dir_all(&new_dir).await?;
    }
    debug!("Copying essential files from old to new directory");
    copy_essential_files(&old_dir, &new_dir).await?;

    // Mark migration as completed in database
    debug!("Marking migration as completed");
    mark_migration_completed()?;

    debug!("Migration completed successfully");
    Ok(true)
}

#[derive(Debug)]
enum MigrationStatus {
    NotNeeded,
    Needed,
    Completed,
}

async fn detect_migration() -> Result<MigrationStatus> {
    let old_dir = fig_util::directories::old_fig_data_dir()?;
    let new_dir = fig_util::directories::fig_data_dir()?;

    // If new directory doesn't exist yet, check if old directory exists
    if !new_dir.exists() {
        if old_dir.exists() && old_dir.is_dir() {
            return Ok(MigrationStatus::Needed);
        } else {
            return Ok(MigrationStatus::NotNeeded);
        }
    }

    // New directory exists, check database flag (safe now since new_dir exists)
    let migration_completed = is_migration_completed()?;

    if migration_completed {
        Ok(MigrationStatus::Completed)
    } else if old_dir.exists() && old_dir.is_dir() {
        Ok(MigrationStatus::Needed)
    } else {
        Ok(MigrationStatus::NotNeeded)
    }
}

async fn copy_essential_files(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
    // Copy data-local-dir files (database and history)
    let data_files = ["data.sqlite3", "history"];
    for file_name in data_files {
        let src_path = src.join(file_name);
        let dst_path = dst.join(file_name);

        if src_path.exists() {
            debug!("Copying {} to {}", src_path.display(), dst_path.display());
            fs::copy(&src_path, &dst_path).await?;
        }
    }

    // Copy settings file to its specific location
    let old_settings = src.join("settings.json");
    if old_settings.exists() {
        let new_settings = fig_util::directories::settings_path()?;
        if !new_settings.exists() {
            if let Some(parent) = new_settings.parent() {
                fs::create_dir_all(parent).await?;
            }
            debug!("Copying settings to {}", new_settings.display());
            fs::copy(&old_settings, &new_settings).await?;
        }
    }

    Ok(())
}

fn mark_migration_completed() -> Result<()> {
    let db = fig_settings::sqlite::database()?;
    db.set_state_value(KIRO_MIGRATION_KEY, true)?;
    Ok(())
}

struct MigrationLock {
    _file: std::fs::File,
    path: PathBuf,
}

impl Drop for MigrationLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn acquire_migration_lock() -> Result<Option<MigrationLock>> {
    let lock_path = migration_lock_path()?;

    if let Some(parent) = lock_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Check if lock file exists and is stale
    if lock_path.exists() {
        match std::fs::metadata(&lock_path)
            .and_then(|m| m.modified())
            .and_then(|t| t.elapsed().map_err(std::io::Error::other))
        {
            Ok(elapsed) if elapsed.as_secs() > MIGRATION_TIMEOUT_SECS => {
                let _ = std::fs::remove_file(&lock_path);
            },
            _ => {
                // Continue with lock attempt even if metadata operations fail
                debug!(
                    "Failed to get lock file metadata, continuing with lock attempt: {}",
                    lock_path.display()
                );
            },
        }
    }

    // Try to acquire the lock
    let file = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)?;

    match flock(&file, FlockOperation::NonBlockingLockExclusive) {
        Ok(()) => Ok(Some(MigrationLock {
            _file: file,
            path: lock_path,
        })),
        Err(_) => Ok(None),
    }
}

fn migration_lock_path() -> Result<PathBuf> {
    Ok(fig_util::directories::fig_data_dir()?.join("migration.lock"))
}

fn is_migration_completed() -> Result<bool> {
    Ok(fig_settings::state::get_bool_or(KIRO_MIGRATION_KEY, false))
}
