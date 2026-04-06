use std::path::PathBuf;

use rmcp::{
    handler::server::{tool::ToolRouter, wrapper::Parameters},
    model::{
        CallToolRequestParams, CallToolResult, Content, Implementation, ListToolsResult,
        PaginatedRequestParams, ProtocolVersion, ServerCapabilities, ServerInfo,
    },
    service::{RequestContext, RoleServer},
    tool, tool_router,
    transport::stdio,
    ErrorData as McpError, ServerHandler, ServiceExt,
};
use schemars::JsonSchema;
use serde::Deserialize;

use dot_agent_core::{
    FileStatus, IgnoreConfig, InstallOptions, Installer, ProfileManager, SnapshotManager,
    SnapshotTrigger,
};

// =============================================================================
// Public entry point
// =============================================================================

pub async fn run() -> anyhow::Result<()> {
    let base_dir = resolve_base_dir()?;
    let server = DotAgentMcp::new(base_dir);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}

fn resolve_base_dir() -> anyhow::Result<PathBuf> {
    if let Ok(base) = std::env::var("DOT_AGENT_BASE") {
        return Ok(PathBuf::from(base));
    }
    dirs::home_dir()
        .map(|h| h.join(".dot-agent"))
        .ok_or_else(|| anyhow::anyhow!("Failed to determine home directory"))
}

// =============================================================================
// Helpers
// =============================================================================

fn ok_text(text: String) -> Result<CallToolResult, McpError> {
    Ok(CallToolResult::success(vec![Content::text(text)]))
}

/// Always-keep-local conflict resolver for switch operations.
struct KeepLocalResolver;

impl dot_agent_core::ConflictResolver for KeepLocalResolver {
    fn resolve(
        &self,
        _: &std::path::Path,
        _: &[u8],
        _: &[u8],
    ) -> dot_agent_core::Result<dot_agent_core::Resolution> {
        Ok(dot_agent_core::Resolution::KeepLocal)
    }
}

// =============================================================================
// MCP Server
// =============================================================================

#[derive(Clone)]
pub struct DotAgentMcp {
    tool_router: ToolRouter<Self>,
    base_dir: PathBuf,
}

impl DotAgentMcp {
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            tool_router: Self::tool_router(),
            base_dir,
        }
    }

    fn profile_manager(&self) -> ProfileManager {
        ProfileManager::new(self.base_dir.clone())
    }

    fn installer(&self) -> Installer {
        Installer::new(self.base_dir.clone())
    }

    fn snapshot_manager(&self) -> SnapshotManager {
        SnapshotManager::new(self.base_dir.clone())
    }

    /// Save a snapshot before a destructive operation. Returns warning text on failure.
    fn save_pre_snapshot(
        &self,
        target: &std::path::Path,
        trigger: SnapshotTrigger,
        profiles: &[String],
    ) -> Option<String> {
        match self
            .snapshot_manager()
            .save_target(target, trigger, None, profiles)
        {
            Ok(snap) => Some(format!(
                "Snapshot '{}' saved ({} files)",
                snap.id, snap.file_count
            )),
            Err(e) => Some(format!("[WARNING] Failed to save snapshot: {e}")),
        }
    }

    fn resolve_target(&self, path: Option<String>, global: bool) -> Result<PathBuf, McpError> {
        let installer = self.installer();
        let target_path = path.map(PathBuf::from);
        installer
            .resolve_target(target_path.as_deref(), global)
            .map_err(Self::to_mcp_error)
    }

    fn to_mcp_error(e: dot_agent_core::DotAgentError) -> McpError {
        McpError::internal_error(format!("{e}"), None)
    }
}

// =============================================================================
// ServerHandler impl
// =============================================================================

impl ServerHandler for DotAgentMcp {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_03_26,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation {
                name: "dot-agent-mcp".to_string(),
                title: Some("dot-agent MCP — Profile Management".to_string()),
                description: Some(
                    "Manage Claude Code profiles: list, install, remove, switch, diff, sync-back."
                        .to_string(),
                ),
                version: env!("CARGO_PKG_VERSION").to_string(),
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "dot-agent profile management via MCP.\n\
                 \n\
                 Tools:\n\
                 - `list`: List available profiles\n\
                 - `installed`: Show installed profiles at a target\n\
                 - `install`: Install a profile\n\
                 - `remove`: Remove an installed profile\n\
                 - `switch`: Switch to a different profile\n\
                 - `diff`: Show differences between profile and installed files\n\
                 - `sync_back`: Write modified installed files back to the source profile\n\
                 - `status`: Show detailed installation status\n\
                 - `snapshot_list`: List all snapshots for a target\n\
                 - `snapshot_restore`: Restore a snapshot (saves pre-restore snapshot first)\n\
                 \n\
                 Use `--global` or `global: true` to target ~/.claude directly.\n\
                 Use `--path` or `path: \"...\"` to target a specific directory."
                    .to_string(),
            ),
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _cx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        Ok(ListToolsResult {
            tools: self.tool_router.list_all(),
            next_cursor: None,
            meta: None,
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        cx: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let tool_ctx = rmcp::handler::server::tool::ToolCallContext::new(self, request, cx);
        self.tool_router.call(tool_ctx).await
    }
}

// =============================================================================
// Tool parameter types
// =============================================================================

#[derive(Debug, Deserialize, JsonSchema)]
struct ListParams {}

#[derive(Debug, Deserialize, JsonSchema)]
struct InstalledParams {
    /// Target directory path (default: current dir)
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct InstallParams {
    /// Profile name to install
    profile: String,
    /// Target directory path
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
    /// Force overwrite conflicts
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct RemoveParams {
    /// Profile name to remove
    profile: String,
    /// Target directory path
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
    /// Force remove even with local modifications
    #[serde(default)]
    force: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SwitchParams {
    /// Profile name to switch to
    profile: String,
    /// Target directory path
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
    /// Force overwrite conflicts
    #[serde(default)]
    force: bool,
    /// Keep local files on conflict
    #[serde(default)]
    keep_local: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct DiffParams {
    /// Profile name to diff
    profile: String,
    /// Target directory path
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SyncBackParams {
    /// Profile name to sync back to
    profile: String,
    /// Target directory path
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
    /// Dry run (don't write, just show changes)
    #[serde(default)]
    dry_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SnapshotListParams {
    /// Target directory path (default: current dir)
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SnapshotRestoreParams {
    /// Snapshot ID to restore (e.g. "20260406_123456")
    id: String,
    /// Target directory path (default: current dir)
    path: Option<String>,
    /// Use ~/.claude directly
    #[serde(default)]
    global: bool,
}

// =============================================================================
// Tool implementations
// =============================================================================

#[tool_router]
impl DotAgentMcp {
    #[tool(
        name = "list",
        description = "List all available profiles",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_list(
        &self,
        Parameters(_params): Parameters<ListParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let profiles = manager.list_profiles().map_err(Self::to_mcp_error)?;

        if profiles.is_empty() {
            return ok_text("No profiles found.".to_string());
        }

        let mut lines = vec![format!("Available profiles ({}):", profiles.len())];
        for p in &profiles {
            let summary = p.contents_summary();
            lines.push(format!("  {} — {}", p.name, summary));
        }
        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "installed",
        description = "Show installed profiles at a target directory",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_installed(
        &self,
        Parameters(params): Parameters<InstalledParams>,
    ) -> Result<CallToolResult, McpError> {
        let target_dir = self.resolve_target(params.path, params.global)?;

        if !target_dir.exists() {
            return ok_text(format!("Target does not exist: {}", target_dir.display()));
        }

        let metadata = dot_agent_core::Metadata::load(&target_dir).map_err(Self::to_mcp_error)?;

        match metadata {
            Some(meta) => {
                let mut lines = vec![format!("Target: {}", target_dir.display())];
                if meta.installed.profiles.is_empty() {
                    lines.push("No profiles installed.".to_string());
                } else {
                    lines.push(format!(
                        "Installed profiles: {}",
                        meta.installed.profiles.join(", ")
                    ));
                    lines.push(format!("Tracked files: {}", meta.files.len()));
                }
                ok_text(lines.join("\n"))
            }
            None => ok_text(format!(
                "Target: {}\nNo dot-agent metadata found.",
                target_dir.display()
            )),
        }
    }

    #[tool(
        name = "install",
        description = "Install a profile to a target directory",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false
        )
    )]
    async fn tool_install(
        &self,
        Parameters(params): Parameters<InstallParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let installer = self.installer();
        let profile = manager
            .get_profile(&params.profile)
            .map_err(Self::to_mcp_error)?;
        let target_dir = self.resolve_target(params.path, params.global)?;
        let ignore_config = IgnoreConfig::with_defaults();

        // Pre-install snapshot
        let mut lines = Vec::new();
        if target_dir.exists() {
            if let Some(msg) = self.save_pre_snapshot(
                &target_dir,
                SnapshotTrigger::PreInstall,
                std::slice::from_ref(&params.profile),
            ) {
                lines.push(msg);
            }
        }

        let opts = InstallOptions::new()
            .force(params.force)
            .ignore_config(ignore_config);

        let result = installer
            .install(&profile, &target_dir, &opts)
            .map_err(Self::to_mcp_error)?;

        lines.push(format!(
            "Installed profile '{}' to {}",
            params.profile,
            target_dir.display()
        ));
        lines.push(format!("  Installed: {}", result.installed));
        if result.merged > 0 {
            lines.push(format!("  Merged: {}", result.merged));
        }
        lines.push(format!("  Skipped: {}", result.skipped));
        if result.conflicts > 0 {
            lines.push(format!("  Conflicts: {}", result.conflicts));
        }
        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "remove",
        description = "Remove an installed profile from a target directory",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn tool_remove(
        &self,
        Parameters(params): Parameters<RemoveParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let installer = self.installer();
        let profile = manager
            .get_profile(&params.profile)
            .map_err(Self::to_mcp_error)?;
        let target_dir = self.resolve_target(params.path, params.global)?;
        let ignore_config = IgnoreConfig::with_defaults();

        // Pre-uninstall snapshot
        let mut lines = Vec::new();
        if let Some(msg) = self.save_pre_snapshot(
            &target_dir,
            SnapshotTrigger::PreUninstall,
            std::slice::from_ref(&params.profile),
        ) {
            lines.push(msg);
        }

        let opts = InstallOptions::new()
            .force(params.force)
            .ignore_config(ignore_config);

        let (removed, kept, unmerged) = installer
            .remove(&profile, &target_dir, &opts)
            .map_err(Self::to_mcp_error)?;

        lines.push(format!(
            "Removed profile '{}' from {}",
            params.profile,
            target_dir.display()
        ));
        lines.push(format!("  Removed: {}", removed));
        if kept > 0 {
            lines.push(format!("  Kept: {}", kept));
        }
        if unmerged > 0 {
            lines.push(format!("  Unmerged: {}", unmerged));
        }
        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "switch",
        description = "Switch to a different profile (remove current, install new)",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn tool_switch(
        &self,
        Parameters(params): Parameters<SwitchParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let installer = self.installer();
        let ignore_config = IgnoreConfig::with_defaults();

        let new_profile = manager
            .get_profile(&params.profile)
            .map_err(Self::to_mcp_error)?;
        let target_dir = self.resolve_target(params.path, params.global)?;

        // Find currently installed profiles
        let metadata = dot_agent_core::Metadata::load(&target_dir).map_err(Self::to_mcp_error)?;
        let current_profiles: Vec<String> = metadata
            .as_ref()
            .map(|m| m.installed.profiles.clone())
            .unwrap_or_default();

        let mut output = Vec::new();

        // Pre-switch snapshot
        if let Some(msg) =
            self.save_pre_snapshot(&target_dir, SnapshotTrigger::PreUpdate, &current_profiles)
        {
            output.push(msg);
        }

        // Remove current profiles (fail-fast: any error aborts the switch)
        let remove_opts = InstallOptions::new()
            .force(params.force)
            .ignore_config(ignore_config.clone());

        for name in &current_profiles {
            let old_profile = manager.get_profile(name).map_err(Self::to_mcp_error)?;
            let (removed, _, _) = installer
                .remove(&old_profile, &target_dir, &remove_opts)
                .map_err(Self::to_mcp_error)?;
            output.push(format!("Removed '{}' ({} files)", name, removed));
        }

        // Build install options
        let resolver = KeepLocalResolver;
        let install_opts = if params.force {
            InstallOptions::new()
                .force(true)
                .ignore_config(ignore_config)
        } else if params.keep_local {
            InstallOptions::new()
                .ignore_config(ignore_config)
                .conflict_resolver(&resolver)
        } else {
            InstallOptions::new().ignore_config(ignore_config)
        };

        let result = installer
            .install(&new_profile, &target_dir, &install_opts)
            .map_err(Self::to_mcp_error)?;

        output.push(format!(
            "Installed '{}': {} files (skipped: {}, conflicts: {})",
            params.profile, result.installed, result.skipped, result.conflicts
        ));

        ok_text(output.join("\n"))
    }

    #[tool(
        name = "diff",
        description = "Show differences between a profile and installed files",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_diff(
        &self,
        Parameters(params): Parameters<DiffParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let installer = self.installer();
        let profile = manager
            .get_profile(&params.profile)
            .map_err(Self::to_mcp_error)?;
        let target_dir = self.resolve_target(params.path, params.global)?;
        let ignore_config = IgnoreConfig::with_defaults();

        let diff = installer
            .diff(&profile, &target_dir, &ignore_config)
            .map_err(Self::to_mcp_error)?;

        let mut lines = vec![format!(
            "Diff: profile '{}' vs {}",
            params.profile,
            target_dir.display()
        )];

        for f in &diff.files {
            let status = match f.status {
                FileStatus::Unchanged => "=",
                FileStatus::Modified => "M",
                FileStatus::Added => "A",
                FileStatus::Missing => "!",
            };
            lines.push(format!("  [{}] {}", status, f.relative_path.display()));
        }

        lines.push(String::new());
        lines.push(format!(
            "Summary: {} unchanged, {} modified, {} added, {} missing",
            diff.unchanged, diff.modified, diff.added, diff.missing
        ));

        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "sync_back",
        description = "Write modified installed files back to the source profile. Creates a profile snapshot before writing.",
        annotations(
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_sync_back(
        &self,
        Parameters(params): Parameters<SyncBackParams>,
    ) -> Result<CallToolResult, McpError> {
        let manager = self.profile_manager();
        let installer = self.installer();
        let profile = manager
            .get_profile(&params.profile)
            .map_err(Self::to_mcp_error)?;
        let target_dir = self.resolve_target(params.path, params.global)?;
        let ignore_config = IgnoreConfig::with_defaults();

        // Create profile snapshot before sync (unless dry run)
        if !params.dry_run {
            let snapshot_manager =
                dot_agent_core::ProfileSnapshotManager::new(self.base_dir.clone());
            snapshot_manager
                .save_profile(&params.profile, &profile.path, Some("pre-sync-back"))
                .map_err(|e| {
                    McpError::internal_error(
                        format!("Failed to create snapshot before sync-back: {e}"),
                        None,
                    )
                })?;
        }

        let result = installer
            .sync_back(&profile, &target_dir, &ignore_config, params.dry_run, None)
            .map_err(Self::to_mcp_error)?;

        let mut lines = Vec::new();
        if params.dry_run {
            lines.push(format!(
                "Sync back (dry run): {} -> profile '{}'",
                target_dir.display(),
                params.profile
            ));
        } else {
            lines.push(format!(
                "Sync back: {} -> profile '{}'",
                target_dir.display(),
                params.profile
            ));
        }

        for f in &result.files {
            lines.push(format!("  SYNC  {}", f.display()));
        }

        lines.push(String::new());
        if params.dry_run {
            lines.push(format!("Would sync: {} files", result.synced));
        } else {
            lines.push(format!("Synced: {} files", result.synced));
        }
        if result.unchanged > 0 {
            lines.push(format!("Unchanged: {} files", result.unchanged));
        }

        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "status",
        description = "Show detailed installation status for a target",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_status(
        &self,
        Parameters(params): Parameters<InstalledParams>,
    ) -> Result<CallToolResult, McpError> {
        let target_dir = self.resolve_target(params.path, params.global)?;
        let manager = self.profile_manager();
        let installer = self.installer();
        let ignore_config = IgnoreConfig::with_defaults();

        if !target_dir.exists() {
            return ok_text(format!("Target does not exist: {}", target_dir.display()));
        }

        let metadata = dot_agent_core::Metadata::load(&target_dir).map_err(Self::to_mcp_error)?;

        let mut lines = vec![format!("Target: {}", target_dir.display())];

        match metadata {
            Some(meta) => {
                if meta.installed.profiles.is_empty() {
                    lines.push("No profiles installed.".to_string());
                } else {
                    lines.push(format!(
                        "Installed profiles: {}",
                        meta.installed.profiles.join(", ")
                    ));
                    lines.push(format!("Tracked files: {}", meta.files.len()));

                    // Show diff for each profile
                    for name in &meta.installed.profiles {
                        match manager.get_profile(name) {
                            Ok(profile) => {
                                match installer.diff(&profile, &target_dir, &ignore_config) {
                                    Ok(diff) => {
                                        lines.push(format!(
                                            "\n  {}: {} unchanged, {} modified, {} missing",
                                            name, diff.unchanged, diff.modified, diff.missing
                                        ));
                                    }
                                    Err(e) => {
                                        lines.push(format!("\n  {}: [error] {}", name, e));
                                    }
                                }
                            }
                            Err(e) => {
                                lines.push(format!("\n  {}: [error] {}", name, e));
                            }
                        }
                    }
                }
            }
            None => {
                lines.push("No dot-agent metadata found.".to_string());
            }
        }

        // Check for CLAUDE.md
        let claude_md = target_dir.join("CLAUDE.md");
        lines.push(format!(
            "\nCLAUDE.md: {}",
            if claude_md.exists() {
                "present"
            } else {
                "absent"
            }
        ));

        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "snapshot_list",
        description = "List all snapshots for a target directory",
        annotations(
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true
        )
    )]
    async fn tool_snapshot_list(
        &self,
        Parameters(params): Parameters<SnapshotListParams>,
    ) -> Result<CallToolResult, McpError> {
        let target_dir = self.resolve_target(params.path, params.global)?;
        let snap_mgr = self.snapshot_manager();

        let snapshots = snap_mgr
            .list_target(&target_dir)
            .map_err(Self::to_mcp_error)?;

        if snapshots.is_empty() {
            return ok_text(format!("No snapshots for {}", target_dir.display()));
        }

        let mut lines = vec![format!(
            "Snapshots for {} ({}):",
            target_dir.display(),
            snapshots.len()
        )];
        for snap in &snapshots {
            let msg = snap
                .message
                .as_deref()
                .map(|m| format!(" — {m}"))
                .unwrap_or_default();
            lines.push(format!(
                "  {} [{}] {} files, trigger: {}{}",
                snap.id,
                snap.display_time(),
                snap.file_count,
                snap.trigger,
                msg,
            ));
        }
        ok_text(lines.join("\n"))
    }

    #[tool(
        name = "snapshot_restore",
        description = "Restore a snapshot to a target directory. Saves a pre-restore snapshot first.",
        annotations(
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false
        )
    )]
    async fn tool_snapshot_restore(
        &self,
        Parameters(params): Parameters<SnapshotRestoreParams>,
    ) -> Result<CallToolResult, McpError> {
        let target_dir = self.resolve_target(params.path, params.global)?;
        let snap_mgr = self.snapshot_manager();

        // Save a pre-restore snapshot for safety
        let mut output = Vec::new();
        if let Some(msg) = self.save_pre_snapshot(&target_dir, SnapshotTrigger::PreUpdate, &[]) {
            output.push(msg);
        }

        let (removed, restored) = snap_mgr
            .restore_target(&target_dir, &params.id)
            .map_err(Self::to_mcp_error)?;

        output.push(format!(
            "Restored snapshot '{}': removed {} files, restored {} files",
            params.id, removed, restored
        ));

        ok_text(output.join("\n"))
    }
}
