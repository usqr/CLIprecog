use std::path::{
    Path,
    PathBuf,
};
use std::sync::Arc;

use fig_integrations::Integration;
use fig_integrations::shell::ShellExt;
use fig_integrations::ssh::SshIntegration;
use fig_os_shim::{
    Context,
    Env,
};
use fig_util::{
    CHAT_BINARY_NAME,
    CLI_BINARY_NAME,
    OLD_CLI_BINARY_NAMES,
    OLD_PTY_BINARY_NAMES,
    PTY_BINARY_NAME,
    Shell,
    directories,
};

use crate::Error;

bitflags::bitflags! {
    /// The different components that can be installed.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct InstallComponents: u64 {
        /// Removal of the integrations from user's dotfiles
        const SHELL_INTEGRATIONS    = 0b00000001;
        /// This handles the removal of the CLI and pty binaries as well as legacy copies
        const BINARY                = 0b00000010;
        /// Removal of the ssh integration from the ~/.ssh/config file
        const SSH                   = 0b00000100;
        const DESKTOP_APP           = 0b00001000;
        const INPUT_METHOD          = 0b00010000;
        const DESKTOP_ENTRY         = 0b00100000;
        const GNOME_SHELL_EXTENSION = 0b01000000;
    }
}

#[cfg(target_os = "linux")]
impl InstallComponents {
    pub fn all_linux_minimal() -> Self {
        Self::SHELL_INTEGRATIONS | Self::BINARY | Self::SSH
    }
}

pub async fn uninstall(components: InstallComponents, ctx: Arc<Context>) -> Result<(), Error> {
    let ssh_result = if components.contains(InstallComponents::SSH) {
        SshIntegration::new()?.uninstall().await
    } else {
        Ok(())
    };

    let shell_integration_result = {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            for integration in shell.get_shell_integrations(ctx.env())? {
                integration.uninstall().await?;
            }
        }
        Ok(())
    };

    if components.contains(InstallComponents::BINARY) {
        let remove_binary = |path: PathBuf| async move {
            match tokio::fs::remove_file(&path).await {
                Ok(_) => tracing::info!("Removed binary: {path:?}"),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {},
                Err(err) => tracing::warn!(%err, "Failed to remove binary: {path:?}"),
            }
        };

        // let folders = [directories::home_local_bin()?, Path::new("/usr/local/bin").into()];
        let folders = [directories::home_local_bin()?];

        let mut all_binary_names = vec![CLI_BINARY_NAME, CHAT_BINARY_NAME, PTY_BINARY_NAME];
        all_binary_names.extend(OLD_CLI_BINARY_NAMES);
        all_binary_names.extend(OLD_PTY_BINARY_NAMES);

        let mut pty_names = vec![PTY_BINARY_NAME];
        pty_names.extend(OLD_PTY_BINARY_NAMES);

        for folder in folders {
            for binary_name in &all_binary_names {
                let binary_path = folder.join(binary_name);
                remove_binary(binary_path).await;
            }

            for shell in Shell::all() {
                for pty_name in &pty_names {
                    let pty_path = folder.join(format!("{shell} ({pty_name})"));
                    remove_binary(pty_path).await;
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    if components.contains(InstallComponents::GNOME_SHELL_EXTENSION) {
        let shell_extensions = dbus::gnome_shell::ShellExtensions::new(Arc::downgrade(&ctx));
        super::os::uninstall_gnome_extension(&ctx, &shell_extensions).await?;
    }

    #[cfg(target_os = "linux")]
    if components.contains(InstallComponents::DESKTOP_ENTRY) {
        super::os::uninstall_desktop_entries(&ctx).await?;
    }

    let daemon_result = Ok(());

    #[cfg(target_os = "macos")]
    if components.contains(InstallComponents::INPUT_METHOD) {
        use fig_integrations::Error;
        use fig_integrations::input_method::{
            InputMethod,
            InputMethodError,
        };

        match InputMethod::default().uninstall().await {
            Ok(_) | Err(Error::InputMethod(InputMethodError::CouldNotListInputSources)) => {},
            Err(err) => return Err(err.into()),
        }
    }

    if components.contains(InstallComponents::DESKTOP_APP) {
        super::os::uninstall_desktop(&ctx).await?;
        // Must be last -- this will kill the running desktop process if this is
        // called from the desktop app.
        let quit_res = tokio::process::Command::new("killall")
            .args([fig_util::consts::APP_PROCESS_NAME])
            .output()
            .await;
        if let Err(err) = quit_res {
            tracing::warn!("Failed to quit running Fig app: {err}");
        }
    }

    daemon_result
        .and(shell_integration_result)
        .and(ssh_result.map_err(|e| e.into()))
}

pub async fn install(components: InstallComponents, env: &Env) -> Result<(), Error> {
    if components.contains(InstallComponents::SHELL_INTEGRATIONS) {
        let mut errs: Vec<Error> = vec![];
        for shell in Shell::all() {
            match shell.get_shell_integrations(env) {
                Ok(integrations) => {
                    for integration in integrations {
                        if let Err(e) = integration.install().await {
                            errs.push(e.into());
                        }
                    }
                },
                Err(e) => {
                    errs.push(e.into());
                },
            }
        }

        if let Some(err) = errs.pop() {
            return Err(err);
        }
    }

    if components.contains(InstallComponents::SSH) {
        SshIntegration::new()?.install().await?;
    }

    #[cfg(target_os = "macos")]
    if components.contains(InstallComponents::INPUT_METHOD) {
        use fig_integrations::input_method::InputMethod;
        InputMethod::default().install().await?;
    }

    Ok(())
}

/// Replace old q/qchat/qterm symlinks with kiro/kiro-cli-chat/kiro-cli-term symlinks
pub fn replace_symlinks() -> Result<(), Box<dyn std::error::Error>> {
    let bin_dir = directories::home_local_bin()?;

    // Remove old symlinks
    remove_old_symlinks(&bin_dir)?;

    // Create new symlinks
    create_new_symlinks(&bin_dir)?;

    Ok(())
}

fn remove_old_symlinks(bin_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let old_links = ["q", "qchat", "qterm"];

    for link in &old_links {
        let link_path = bin_dir.join(link);
        if link_path.is_symlink() {
            let _ = std::fs::remove_file(&link_path); // Ignore errors
        }
    }

    Ok(())
}

fn create_new_symlinks(bin_dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let kiro_bin = std::env::current_exe()?;
    let new_links = ["kiro", "kiro-cli-chat", "kiro-cli-term"];

    std::fs::create_dir_all(bin_dir)?;

    for link_name in &new_links {
        let link_path = bin_dir.join(link_name);
        if !link_path.exists() {
            #[cfg(unix)]
            let _ = std::os::unix::fs::symlink(&kiro_bin, &link_path); // Ignore errors
        }
    }

    Ok(())
}

/// Clean up old Amazon Q directories
pub fn cleanup_old_directories() -> Result<(), Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    let old_dirs = [
        Path::new(&home).join(".fig"),
        Path::new(&home).join(".local/share/amazon-q"),
    ];

    for dir in &old_dirs {
        if dir.exists() {
            let _ = std::fs::remove_dir_all(dir); // Ignore errors
        }
    }

    Ok(())
}

/// Detect if both Amazon Q and Kiro installations exist
pub fn detect_dual_installation() -> Result<bool, Box<dyn std::error::Error>> {
    let home = std::env::var("HOME")?;
    let bin_dir = directories::home_local_bin()?;

    // Check for old Amazon Q artifacts
    let old_q_exists = bin_dir.join("q").exists()
        || Path::new(&home).join(".fig").exists()
        || Path::new(&home).join(".local/share/amazon-q").exists();

    // Check for new Kiro artifacts
    let kiro_exists = bin_dir.join("kiro").exists();

    Ok(old_q_exists && kiro_exists)
}

/// Prompt user for migration choice
pub fn prompt_migration_choice() -> Result<bool, Box<dyn std::error::Error>> {
    use std::io::{
        self,
        Write,
    };

    println!("Amazon Q CLI installation detected alongside Kiro.");
    println!("Would you like to migrate your Amazon Q settings and clean up old files? (y/n)");
    print!("> ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;

    Ok(input.trim().to_lowercase() == "y")
}

/// Create backup of current symlinks before migration
pub fn backup_symlinks() -> Result<(), Box<dyn std::error::Error>> {
    let bin_dir = directories::home_local_bin()?;
    let backup_dir = bin_dir.join(".amazon-q-backup");

    std::fs::create_dir_all(&backup_dir)?;

    let old_links = ["q", "qchat", "qterm"];
    for link in &old_links {
        let link_path = bin_dir.join(link);
        if link_path.is_symlink() {
            if let Ok(target) = std::fs::read_link(&link_path) {
                let backup_file = backup_dir.join(format!("{}.target", link));
                std::fs::write(backup_file, target.to_string_lossy().as_bytes())?;
            }
        }
    }

    Ok(())
}

/// Rollback migration by restoring old symlinks
pub fn rollback_migration() -> Result<(), Box<dyn std::error::Error>> {
    let bin_dir = directories::home_local_bin()?;
    let backup_dir = bin_dir.join(".amazon-q-backup");

    if !backup_dir.exists() {
        return Ok(()); // No backup to restore
    }

    // Remove new kiro symlinks
    let new_links = ["kiro", "kiro-cli-chat", "kiro-cli-term"];
    for link in &new_links {
        let link_path = bin_dir.join(link);
        if link_path.is_symlink() {
            let _ = std::fs::remove_file(&link_path);
        }
    }

    // Restore old symlinks
    let old_links = ["q", "qchat", "qterm"];
    for link in &old_links {
        let backup_file = backup_dir.join(format!("{}.target", link));
        if backup_file.exists() {
            if let Ok(target) = std::fs::read_to_string(&backup_file) {
                let link_path = bin_dir.join(link);
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&target, &link_path);
            }
        }
    }

    // Clean up backup directory
    let _ = std::fs::remove_dir_all(&backup_dir);

    Ok(())
}
