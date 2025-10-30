//! Hierarchical path management for the application

#![allow(dead_code)] // Allow unused code during migration phase

use std::env::VarError;
use std::path::{
    PathBuf,
    StripPrefixError,
};

use globset::{
    Glob,
    GlobSetBuilder,
};
use thiserror::Error;

use crate::os::Os;

#[derive(Debug, Error)]
pub enum DirectoryError {
    #[error("home directory not found")]
    NoHomeDirectory,
    #[cfg(unix)]
    #[error("runtime directory not found: neither XDG_RUNTIME_DIR nor TMPDIR were found")]
    NoRuntimeDirectory,
    #[error("IO Error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    TimeFormat(#[from] time::error::Format),
    #[error(transparent)]
    Utf8FromPath(#[from] camino::FromPathError),
    #[error(transparent)]
    Utf8FromPathBuf(#[from] camino::FromPathBufError),
    #[error(transparent)]
    FromVecWithNul(#[from] std::ffi::FromVecWithNulError),
    #[error(transparent)]
    IntoString(#[from] std::ffi::IntoStringError),
    #[error(transparent)]
    StripPrefix(#[from] StripPrefixError),
    #[error(transparent)]
    PathExpand(#[from] shellexpand::LookupError<VarError>),
    #[error(transparent)]
    GlobCreation(#[from] globset::Error),
}

pub mod workspace {
    //! Project-level paths (relative to current working directory)
    pub const AGENTS_DIR: &str = ".amazonq/cli-agents";
    pub const PROMPTS_DIR: &str = ".amazonq/prompts";
    pub const MCP_CONFIG: &str = ".amazonq/mcp.json";
    pub const TODO_LISTS_DIR: &str = ".amazonq/cli-todo-lists";
    pub const SUBAGENTS_DIR: &str = ".amazonq/.subagents";
    pub const RULES_PATTERN: &str = ".amazonq/rules/**/*.md";

    // Default documentation files for agent resources
    pub const DEFAULT_AGENT_RESOURCES: &[&str] = &["file://AmazonQ.md", "file://AGENTS.md", "file://README.md"];
}

pub mod global {
    //! User-level paths (relative to home directory)
    pub const AGENTS_DIR: &str = ".aws/amazonq/cli-agents";
    pub const PROMPTS_DIR: &str = ".aws/amazonq/prompts";
    pub const MCP_CONFIG: &str = ".aws/amazonq/mcp.json";
    pub const SHADOW_REPO_DIR: &str = ".aws/amazonq/cli-checkouts";
    pub const CLI_BASH_HISTORY: &str = ".aws/amazonq/.cli_bash_history";
    pub const GLOBAL_CONTEXT: &str = ".aws/amazonq/global_context.json";
    pub const PROFILES_DIR: &str = ".aws/amazonq/profiles";
    pub const KNOWLEDGE_BASES_DIR: &str = ".aws/amazonq/knowledge_bases";
}

pub mod application {
    //! Application data paths (system-specific)
    pub const DATA_DIR_NAME: &str = "amazon-q";
    pub const SETTINGS_FILE: &str = "settings.json";
    pub const DATABASE_FILE: &str = "data.sqlite3";
    pub const UPDATE_LOCK_FILE: &str = "update.lock";
    pub const BACKUP_DIR_NAME: &str = ".amazon-q.dotfiles.bak";
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

/// Get the macos tempdir from the `confstr` function
#[cfg(target_os = "macos")]
fn macos_tempdir() -> Result<PathBuf> {
    let len = unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, std::ptr::null::<i8>().cast_mut(), 0) };
    let mut buf: Vec<u8> = vec![0; len];
    unsafe { libc::confstr(libc::_CS_DARWIN_USER_TEMP_DIR, buf.as_mut_ptr().cast(), buf.len()) };
    let c_string = std::ffi::CString::from_vec_with_nul(buf)?;
    let str = c_string.into_string()?;
    Ok(PathBuf::from(str))
}

/// Runtime dir for logs and sockets
#[cfg(unix)]
pub fn runtime_dir() -> Result<PathBuf> {
    let mut dir = dirs::runtime_dir();
    dir = dir.or_else(|| std::env::var_os("TMPDIR").map(PathBuf::from));

    #[cfg(target_os = "macos")]
    {
        let macos_tempdir = macos_tempdir()?;
        dir = dir.or(Some(macos_tempdir));
    }
    #[cfg(not(target_os = "macos"))]
    {
        dir = dir.or_else(|| Some(std::env::temp_dir()));
    }

    dir.ok_or(DirectoryError::NoRuntimeDirectory)
}

/// The application data directory
/// - Linux: `$XDG_DATA_HOME/amazon-q` or `$HOME/.local/share/amazon-q`
/// - MacOS: `$HOME/Library/Application Support/amazon-q`
/// - Windows: `%LOCALAPPDATA%\AmazonQ`
pub fn app_data_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join(application::DATA_DIR_NAME))
    }
    #[cfg(windows)]
    {
        Ok(dirs::data_local_dir()
            .ok_or(DirectoryError::NoHomeDirectory)?
            .join("AmazonQ"))
    }
}

/// The directory to all the logs
pub fn logs_dir() -> Result<PathBuf> {
    #[cfg(unix)]
    {
        Ok(runtime_dir()?.join("qlog"))
    }
    #[cfg(windows)]
    {
        Ok(std::env::temp_dir().join(application::DATA_DIR_NAME).join("logs"))
    }
}

/// Canonicalizes path given by expanding the path given
pub fn canonicalizes_path(os: &Os, path_as_str: &str) -> Result<String> {
    let context = |input: &str| Ok(os.env.get(input).ok());
    let home_dir_fn = || os.env.home().map(|p| p.to_string_lossy().to_string());

    let expanded = shellexpand::full_with_context(path_as_str, home_dir_fn, context)?;
    let path_buf = if !expanded.starts_with("/") {
        let current_dir = os.env.current_dir()?;
        current_dir.join(expanded.as_ref() as &str)
    } else {
        PathBuf::from(expanded.as_ref() as &str)
    };

    match path_buf.canonicalize() {
        Ok(normalized) => Ok(normalized.as_path().to_string_lossy().to_string()),
        Err(_) => {
            let normalized = normalize_path(&path_buf);
            Ok(normalized.to_string_lossy().to_string())
        },
    }
}

/// Manually normalize a path by resolving . and .. components
fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {},
            std::path::Component::ParentDir => {
                components.pop();
            },
            _ => {
                components.push(component);
            },
        }
    }
    components.iter().collect()
}

/// Given a globset builder and a path, build globs for both the file and directory patterns
/// This is needed because by default glob does not match children of a dir so we need both
/// patterns to exist in a globset.
pub fn add_gitignore_globs(builder: &mut GlobSetBuilder, path: &str) -> Result<()> {
    let glob_for_file = Glob::new(path)?;
    let dir_pattern: String = format!("{}/**", path.trim_end_matches('/'));
    let glob_for_dir = Glob::new(&dir_pattern)?;
    builder.add(glob_for_file);
    builder.add(glob_for_dir);
    Ok(())
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

    /// Get application-scoped path resolver
    pub fn application(&self) -> ApplicationPaths<'_> {
        ApplicationPaths { os: self.os }
    }
}

/// Workspace-scoped path methods
pub struct WorkspacePaths<'a> {
    os: &'a Os,
}

impl<'a> WorkspacePaths<'a> {
    pub fn agents_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::AGENTS_DIR))
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::PROMPTS_DIR))
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::MCP_CONFIG))
    }

    pub fn todo_lists_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::TODO_LISTS_DIR))
    }

    pub fn subagents_dir(&self) -> Result<PathBuf> {
        Ok(self.os.env.current_dir()?.join(workspace::SUBAGENTS_DIR))
    }

    pub async fn ensure_subagents_dir(&self) -> Result<PathBuf> {
        let dir = self.subagents_dir()?;
        if !dir.exists() {
            self.os.fs.create_dir_all(&dir).await?;
        }
        Ok(dir)
    }
}

/// Global-scoped path methods
pub struct GlobalPaths<'a> {
    os: &'a Os,
}

impl<'a> GlobalPaths<'a> {
    pub fn agents_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::AGENTS_DIR))
    }

    pub fn prompts_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::PROMPTS_DIR))
    }

    pub fn mcp_config(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::MCP_CONFIG))
    }

    pub fn shadow_repo_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::SHADOW_REPO_DIR))
    }

    pub fn cli_bash_history(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::CLI_BASH_HISTORY))
    }

    pub fn global_context(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::GLOBAL_CONTEXT))
    }

    pub fn profiles_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::PROFILES_DIR))
    }

    pub fn knowledge_bases_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(global::KNOWLEDGE_BASES_DIR))
    }

    pub fn mcp_auth_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(".aws").join("sso").join("cache"))
    }

    pub async fn ensure_agents_dir(&self) -> Result<PathBuf> {
        let dir = self.agents_dir()?;
        if !dir.exists() {
            self.os.fs.create_dir_all(&dir).await?;
        }
        Ok(dir)
    }
}

/// Application-scoped path methods
pub struct ApplicationPaths<'a> {
    os: &'a Os,
}

#[allow(clippy::unused_self)] // Allow unused self during migration
impl<'a> ApplicationPaths<'a> {
    /// The application data directory
    pub fn data_dir(&self) -> Result<PathBuf> {
        app_data_dir()
    }

    /// The application settings file
    pub fn settings_path(&self) -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::SETTINGS_FILE))
    }

    /// The application database file
    pub fn database_path(&self) -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::DATABASE_FILE))
    }

    /// The application update lock file
    pub fn update_lock_path(&self) -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::UPDATE_LOCK_FILE))
    }

    /// The backup directory
    pub fn backups_dir(&self) -> Result<PathBuf> {
        Ok(home_dir(self.os)?.join(application::BACKUP_DIR_NAME))
    }

    /// The logs directory
    pub fn logs_dir(&self) -> Result<PathBuf> {
        logs_dir()
    }

    /// Static method for settings path (to avoid circular dependency)
    pub fn settings_path_static() -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::SETTINGS_FILE))
    }

    /// Static method for database path (to avoid circular dependency)
    pub fn database_path_static() -> Result<PathBuf> {
        Ok(app_data_dir()?.join(application::DATABASE_FILE))
    }
}
