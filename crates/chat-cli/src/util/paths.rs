//! Hierarchical path management for the application

use std::path::PathBuf;

use crate::os::Os;

#[derive(Debug, thiserror::Error)]
pub enum DirectoryError {
    #[error("home directory not found")]
    NoHomeDirectory,
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
}

pub mod workspace {
    //! Project-level paths (relative to current working directory)
    pub const MCP_CONFIG: &str = ".amazonq/mcp.json";
    pub const RULES_PATTERN: &str = ".amazonq/rules/**/*.md";
}

pub mod global {
    //! User-level paths (relative to home directory)
    pub const MCP_CONFIG: &str = ".aws/amazonq/mcp.json";
    pub const GLOBAL_CONTEXT: &str = ".aws/amazonq/global_context.json";
    pub const PROFILES_DIR: &str = ".aws/amazonq/profiles";
}

pub mod application {
    //! Application data paths (system-specific)
    #[cfg(unix)]
    pub const DATA_DIR_NAME: &str = "amazon-q";
    #[cfg(windows)]
    pub const DATA_DIR_NAME: &str = "AmazonQ";
    pub const SETTINGS_FILE: &str = "settings.json";
    pub const DATABASE_FILE: &str = "data.sqlite3";
}

type Result<T, E = DirectoryError> = std::result::Result<T, E>;

/// The directory of the users home
/// - Linux: /home/Alice
/// - MacOS: /Users/Alice
/// - Windows: C:\Users\Alice
pub fn home_dir(#[cfg_attr(windows, allow(unused_variables))] os: &Os) -> Result<PathBuf> {
    #[cfg(unix)]
    match cfg!(test) {
        true => os
            .env
            .get("HOME")
            .map_err(|_err| DirectoryError::NoHomeDirectory)
            .and_then(|h| {
                if h.is_empty() {
                    Err(DirectoryError::NoHomeDirectory)
                } else {
                    Ok(h)
                }
            })
            .map(PathBuf::from)
            .map(|p| os.fs.chroot_path(p)),
        false => dirs::home_dir().ok_or(DirectoryError::NoHomeDirectory),
    }

    #[cfg(windows)]
    match cfg!(test) {
        true => os
            .env
            .get("USERPROFILE")
            .map_err(|_err| DirectoryError::NoHomeDirectory)
            .and_then(|h| {
                if h.is_empty() {
                    Err(DirectoryError::NoHomeDirectory)
                } else {
                    Ok(h)
                }
            })
            .map(PathBuf::from)
            .map(|p| os.fs.chroot_path(p)),
        false => dirs::home_dir().ok_or(DirectoryError::NoHomeDirectory),
    }
}

/// The application data directory
/// - Linux: `$XDG_DATA_HOME/{data_dir}` or `$HOME/.local/share/{data_dir}`
/// - MacOS: `$HOME/Library/Application Support/{data_dir}`
/// - Windows: `%LOCALAPPDATA%\{data_dir}`
pub fn app_data_dir() -> Result<PathBuf> {
    Ok(dirs::data_local_dir()
        .ok_or(DirectoryError::NoHomeDirectory)?
        .join(application::DATA_DIR_NAME))
}

/// Path resolver with hierarchy-aware methods
pub struct PathResolver<'a> {
    os: &'a Os,
}

impl<'a> PathResolver<'a> {
    pub fn new(os: &'a Os) -> Self {
        Self { os }
    }

    /// Get workspace-scoped path resolver
    pub fn workspace(&self) -> WorkspacePaths<'_> {
        WorkspacePaths { os: self.os }
    }

    /// Get global-scoped path resolver  
    pub fn global(&self) -> GlobalPaths<'_> {
        GlobalPaths { os: self.os }
    }
}

/// Workspace-scoped path methods
pub struct WorkspacePaths<'a> {
    os: &'a Os,
}

impl<'a> WorkspacePaths<'a> {
    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::MCP_CONFIG))
    }
}

/// Global-scoped path methods
pub struct GlobalPaths<'a> {
    os: &'a Os,
}

impl<'a> GlobalPaths<'a> {
    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::MCP_CONFIG))
    }

    pub fn global_context(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::GLOBAL_CONTEXT))
    }

    pub fn profiles_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::PROFILES_DIR))
    }
}

/// Application path static methods
pub struct ApplicationPaths;

impl ApplicationPaths {
    /// Static method for settings path (to avoid circular dependency)
    pub fn settings_path_static() -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::SETTINGS_FILE))
    }

    /// Static method for database path (to avoid circular dependency)
    pub fn database_path_static() -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::DATABASE_FILE))
    }
}
