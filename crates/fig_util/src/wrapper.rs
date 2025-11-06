use std::path::Path;

use crate::Error;

/// Create legacy q wrapper script for backward compatibility
pub async fn create_q_wrapper(install_dir: &Path) -> Result<(), Error> {
    let wrapper_path = install_dir.join("q");

    // Don't create wrapper if it never existed
    if !wrapper_path.exists() {
        return Ok(());
    }

    tokio::fs::remove_file(&wrapper_path).await?;

    // Create wrapper script content
    let wrapper_content = format!(
        "#!/bin/sh\n\"{}/kiro-cli\" --show-legacy-warning \"$@\"\n",
        install_dir.display()
    );

    // Write wrapper script
    tokio::fs::write(&wrapper_path, wrapper_content).await?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&wrapper_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&wrapper_path, perms).await?;
    }

    Ok(())
}

/// Create legacy qchat wrapper script for backward compatibility
pub async fn create_qchat_wrapper(install_dir: &Path) -> Result<(), Error> {
    let wrapper_path = install_dir.join("qchat");

    // Don't create wrapper if it never existed
    if !wrapper_path.exists() {
        return Ok(());
    }

    // Remove existing qchat command if it exists
    tokio::fs::remove_file(&wrapper_path).await?;

    // Create wrapper script content that calls q chat
    let wrapper_content = format!("#!/bin/sh\n\"{}/q\" chat \"$@\"\n", install_dir.display());

    // Write wrapper script
    tokio::fs::write(&wrapper_path, wrapper_content).await?;

    // Make executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&wrapper_path).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&wrapper_path, perms).await?;
    }

    Ok(())
}
