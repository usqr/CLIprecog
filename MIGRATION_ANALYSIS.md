# Amazon Q to Kiro Migration Analysis

## Executive Summary

This document provides a comprehensive analysis of the Amazon Q to Kiro migration requirements and current implementation status. The codebase already contains significant migration infrastructure, but several areas need attention for a complete migration strategy.

## 1. Migration Logic Trigger Points

### Current Implementation Status: ✅ IMPLEMENTED

**Location**: `crates/kiro-cli/src/main.rs`

The kiro-cli already has migration logic in place:

```rust
fn handle_migration_compatibility() {
    // Check for dual installation
    if let Ok(true) = fig_install::detect_dual_installation() {
        // Prompt user for migration choice
        match fig_install::prompt_migration_choice() {
            Ok(true) => {
                // User chose to migrate
                if perform_migration_with_rollback().is_err() {
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
```

**Trigger Points**:
- ✅ On startup for all non-internal commands (everything except `kiro _ *`)
- ✅ Kiro CLI Desktop.app startup

**Recommendations**:
- The current implementation is comprehensive
- Consider adding telemetry to track migration success/failure rates
- Add logging for debugging migration issues

## 2. Migration Flag in Database

### Current Implementation Status: ✅ READY

**Location**: `crates/fig_settings/src/sqlite/migrations/004_state_table.sql`

The database already has a `state` table that can store the `has_migrated_to_kiro` flag:

```sql
CREATE TABLE state (
    key TEXT PRIMARY KEY,
    value TEXT
);
```

**Usage Pattern**:
```rust
// Check migration status
let has_migrated = fig_settings::state::get_bool_or("has_migrated_to_kiro", false);

// Set migration status
fig_settings::state::set_value("has_migrated_to_kiro", true)?;
```

**Database Locations**:
- Kiro: `~/.local/share/kiro/data.sqlite3` (or equivalent)
- Amazon Q: `~/.local/share/amazon-q/data.sqlite3`

**Recommendations**:
- ✅ Database infrastructure is ready
- Need to implement the flag checking logic in migration functions
- Consider adding migration timestamp for debugging

## 3. Installation Script Implementation

### Current Implementation Status: ✅ IMPLEMENTED

**Location**: `scripts/install.sh`

The installation script already uses the new naming:

```bash
BINARY_NAME="kiro-cli"
CLI_NAME="Kiro CLI"
COMMAND_NAME="kiro-cli"
```

**Symlink Creation**:
```bash
create_symlink "$macos_bin/q" "$HOME/.local/bin/$BINARY_NAME"
create_symlink "$macos_bin/qchat" "$HOME/.local/bin/${BINARY_NAME}-chat"
create_symlink "$macos_bin/qterm" "$HOME/.local/bin/${BINARY_NAME}-term"
```

**Migration Functions in `crates/fig_install/src/common.rs`**:
- ✅ `replace_symlinks()` - Replace old q/qchat/qterm with kiro variants
- ✅ `detect_dual_installation()` - Check for both Q and Kiro installations
- ✅ `prompt_migration_choice()` - Interactive migration prompt
- ✅ `backup_symlinks()` - Create backup before migration
- ✅ `rollback_migration()` - Restore on failure
- ✅ `cleanup_old_directories()` - Remove old Amazon Q directories

**Recommendations**:
- ✅ Installation infrastructure is complete
- Consider adding desktop app launch after installation
- Add verification step to ensure symlinks are working

## 4. Backwards Compatibility for Project-Level Configs

### Current Implementation Status: ⚠️ PARTIAL

**Current Project-Level Config Support**:

In `crates/chat-cli/src/cli/chat/context.rs`:
```rust
paths: vec![
    ".amazonq/rules/**/*.md".to_string(),
    "README.md".to_string(),
    AMAZONQ_FILENAME.to_string(),
],
```

**Missing .kiro Support**:
- No `.kiro` directory handling found in codebase
- No fallback logic for `.amazonq` → `.kiro` migration

**Recommendations**:

### 4.1 Project-Level Configuration Migration Strategy

```rust
// Proposed fallback logic in context.rs
fn get_project_config_paths() -> Vec<String> {
    let mut paths = vec![];
    
    // Check for .kiro first (new format)
    if Path::new(".kiro").exists() {
        paths.extend(vec![
            ".kiro/rules/**/*.md".to_string(),
            ".kiro/prompts/**/*.md".to_string(),
        ]);
    } else if Path::new(".amazonq").exists() {
        // Fallback to .amazonq (legacy format)
        paths.extend(vec![
            ".amazonq/rules/**/*.md".to_string(),
            ".amazonq/prompts/**/*.md".to_string(),
        ]);
    }
    
    // Always include common files
    paths.extend(vec![
        "README.md".to_string(),
        "AmazonQ.md".to_string(), // Keep for backwards compatibility
        "Kiro.md".to_string(),    // New format
    ]);
    
    paths
}
```

### 4.2 MCP Configuration Migration

Need to add support for:
- `.kiro/mcp.json` (new)
- `.amazonq/mcp.json` (fallback)

### 4.3 Environment Variables

Continue to respect Q environment variables if KIRO variants are not present:
- `Q_*` → `KIRO_*` migration
- Fallback to `Q_*` if `KIRO_*` not set

## 5. Toolbox Integration

### Current Implementation Status: ✅ IMPLEMENTED

**Location**: `crates/chat-cli/src/telemetry/install_method.rs`

```rust
if let Ok(current_exe) = std::env::current_exe() {
    if current_exe.components().any(|c| c.as_os_str() == ".toolbox") {
        return InstallMethod::Toolbox;
    }
}
```

**Recommendations**:
- ✅ Toolbox detection is working
- Need to register `kiro` in toolbox and update `q` to point to new artifact
- Coordinate with ASBX team for toolbox registration
- Update both locations (current Q registration and new Kiro registration)

## 6. Update Process for App Bundle Name Changes

### Current Implementation Status: ✅ IMPLEMENTED

**Location**: `crates/fig_install/src/macos.rs`

The update process already supports changing app bundle names:

```rust
let same_bundle_name = app_name == Path::new(APP_BUNDLE_NAME);

let installed_app_path = if same_bundle_name {
    fig_util::app_bundle_path()
} else {
    Path::new("/Applications").join(app_name)
};

// Use RENAME_SWAP for same bundle, regular rename for different bundle
libc::renamex_np(
    src.as_ref().as_ptr(),
    dst.as_ref().as_ptr(),
    if same_bundle_name { libc::RENAME_SWAP } else { 0 },
)
```

**Recommendations**:
- ✅ App bundle name change support is implemented
- The update process can handle Amazon Q.app → Kiro.app transition
- Consider adding user notification about the name change

## 7. Dual Installation Handling

### Current Implementation Status: ✅ IMPLEMENTED

The codebase treats dual installation as "undefined behavior" but provides graceful handling:

1. **Detection**: `detect_dual_installation()` checks for both Q and Kiro artifacts
2. **User Choice**: `prompt_migration_choice()` asks user preference
3. **Migration**: Full migration with backup/rollback capability
4. **Fallback**: Silent symlink replacement if user declines

**Recommendations**:
- ✅ Current approach is sound
- Document the "undefined behavior" clearly for users
- Consider adding warning messages about dual installation

## Implementation Priority Matrix

| Area | Status | Priority | Effort | Owner |
|------|--------|----------|--------|-------|
| Migration Logic Triggers | ✅ Complete | High | Low | - |
| Database Flag | ✅ Ready | High | Low | - |
| Installation Script | ✅ Complete | High | Low | - |
| Project-Level Config Fallback | ⚠️ Partial | High | Medium | Dhanasekar Karuppasamy |
| Toolbox Registration | ⚠️ Needs Work | Medium | Medium | Kunal Kashilkar |
| App Bundle Updates | ✅ Complete | Medium | Low | - |
| Documentation | ❌ Missing | High | Medium | Ranjith Ramakrishnan |

## Next Steps

### Immediate (Week 1)
1. **Implement project-level config fallback logic**
   - Add `.kiro` directory support
   - Implement `.amazonq` → `.kiro` fallback
   - Update MCP configuration handling

2. **Add migration flag usage**
   - Implement `has_migrated_to_kiro` flag checking
   - Add flag setting in migration completion

### Short Term (Week 2-3)
3. **Toolbox Integration**
   - Register `kiro` in toolbox
   - Update `q` artifact pointer
   - Coordinate with ASBX team

4. **Testing & Validation**
   - Test migration scenarios
   - Validate backwards compatibility
   - Test dual installation handling

### Medium Term (Week 4+)
5. **Documentation & Communication**
   - User migration guide
   - Developer documentation
   - Communication plan for users

## Risk Assessment

### High Risk
- **Project-level config compatibility**: Users may lose custom configurations
- **Toolbox integration**: May break existing toolbox users

### Medium Risk
- **Dual installation edge cases**: Complex scenarios may not be handled
- **Environment variable conflicts**: Q_ vs KIRO_ variable precedence

### Low Risk
- **Database migration**: Well-tested infrastructure
- **Symlink replacement**: Simple and reversible operation

## Conclusion

The Amazon Q to Kiro migration infrastructure is largely complete, with robust migration logic, database support, and installation handling already implemented. The main gaps are in project-level configuration fallback support and toolbox integration. The migration strategy is well-designed with proper backup/rollback capabilities and user choice preservation.
