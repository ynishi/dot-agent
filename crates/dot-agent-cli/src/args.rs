use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "dot-agent")]
#[command(about = "Profile-based configuration manager for AI agents")]
#[command(version)]
pub struct Cli {
    /// Verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Quiet output (errors only)
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Base directory (default: ~/.dot-agent)
    #[arg(long, global = true)]
    pub base_dir: Option<PathBuf>,

    /// Launch GUI (requires dot-agent-gui)
    #[arg(long)]
    pub gui: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
#[allow(clippy::enum_variant_names)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    PowerShell,
    Elvish,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Manage profiles
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },

    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },

    /// Search for skills/profiles across registered sources
    Search {
        /// Search query (e.g., "rust tdd", "nextjs")
        query: String,

        /// Maximum results to show
        #[arg(short, long, default_value = "10")]
        limit: usize,

        /// Filter by source type (github, awesome, all)
        #[arg(short, long, default_value = "all")]
        source: String,

        /// Minimum stars (GitHub only)
        #[arg(long)]
        min_stars: Option<u64>,

        /// Additional keywords to include in search
        #[arg(short, long)]
        keywords: Vec<String>,

        /// Search by GitHub topic (e.g., dotfiles, neovim, claude)
        #[arg(short, long)]
        topic: Option<String>,

        /// Use preset search (claude-config, dotfiles, neovim, devenv)
        #[arg(long)]
        preset: Option<String>,

        /// Sort order (stars, updated, forks)
        #[arg(long, default_value = "stars")]
        sort: String,

        /// Refresh cache for Awesome Lists
        #[arg(long)]
        refresh: bool,
    },

    /// Manage hubs (repositories that aggregate channels)
    Hub {
        #[command(subcommand)]
        action: HubAction,
    },

    /// Manage channels (profile sources)
    Channel {
        #[command(subcommand)]
        action: ChannelAction,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Install a profile to current directory (or --path target)
    Install {
        /// Profile name to install
        profile: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Install to ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Force overwrite on conflicts
        #[arg(short, long)]
        force: bool,

        /// Dry run (don't actually copy files)
        #[arg(short, long)]
        dry_run: bool,

        /// Don't add profile prefix to files (stop on conflict)
        #[arg(long)]
        no_prefix: bool,

        /// Include directories that are excluded by default (e.g., --include=.git)
        #[arg(long, value_name = "DIR")]
        include: Vec<String>,

        /// Exclude additional directories (e.g., --exclude=node_modules)
        #[arg(long, value_name = "DIR")]
        exclude: Vec<String>,
    },

    /// Upgrade installed profile to latest
    Upgrade {
        /// Profile name to upgrade
        profile: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Upgrade ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Force overwrite local modifications
        #[arg(short, long)]
        force: bool,

        /// Dry run
        #[arg(short, long)]
        dry_run: bool,

        /// Don't add profile prefix to files
        #[arg(long)]
        no_prefix: bool,

        /// Include directories that are excluded by default (e.g., --include=.git)
        #[arg(long, value_name = "DIR")]
        include: Vec<String>,

        /// Exclude additional directories (e.g., --exclude=node_modules)
        #[arg(long, value_name = "DIR")]
        exclude: Vec<String>,
    },

    /// Show diff between profile and installed files
    Diff {
        /// Profile name to diff
        profile: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Diff ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Include directories that are excluded by default (e.g., --include=.git)
        #[arg(long, value_name = "DIR")]
        include: Vec<String>,

        /// Exclude additional directories (e.g., --exclude=node_modules)
        #[arg(long, value_name = "DIR")]
        exclude: Vec<String>,
    },

    /// Remove installed profile
    Remove {
        /// Profile name to remove
        profile: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Remove from ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Force remove even with local modifications
        #[arg(short, long)]
        force: bool,

        /// Dry run
        #[arg(short, long)]
        dry_run: bool,

        /// Include directories that are excluded by default (e.g., --include=.git)
        #[arg(long, value_name = "DIR")]
        include: Vec<String>,

        /// Exclude additional directories (e.g., --exclude=node_modules)
        #[arg(long, value_name = "DIR")]
        exclude: Vec<String>,
    },

    /// Show installation status
    Status {
        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Check ~/.claude (global)
        #[arg(short, long)]
        global: bool,
    },

    /// Copy an existing profile to a new name
    Copy {
        /// Source profile name
        source: String,

        /// Destination profile name
        dest: String,

        /// Force overwrite if destination exists
        #[arg(short, long)]
        force: bool,
    },

    /// Apply a rule to installed files
    Apply {
        /// Rule name to apply
        rule: String,

        /// Apply only to files from this installed profile
        #[arg(short, long)]
        profile: Option<String>,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Apply without confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Manage customization rules
    Rule {
        #[command(subcommand)]
        action: RuleAction,
    },

    /// Manage snapshots of installed files
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },

    /// Switch to a different profile (remove current, install new)
    Switch {
        /// Profile name to switch to
        profile: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Switch ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Skip snapshot before switching
        #[arg(long)]
        no_snapshot: bool,

        /// Force remove even with local modifications
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum SnapshotAction {
    /// Save a snapshot of current installed files
    Save {
        /// Optional message describing the snapshot
        #[arg(short, long)]
        message: Option<String>,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Snapshot ~/.claude (global)
        #[arg(short, long)]
        global: bool,
    },

    /// List all snapshots
    List {
        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// List for ~/.claude (global)
        #[arg(short, long)]
        global: bool,
    },

    /// Restore a snapshot
    Restore {
        /// Snapshot ID to restore
        id: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Restore to ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Show diff between snapshot and current state
    Diff {
        /// Snapshot ID to compare
        id: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Diff for ~/.claude (global)
        #[arg(short, long)]
        global: bool,
    },

    /// Delete a snapshot
    Delete {
        /// Snapshot ID to delete
        id: String,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Delete for ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Prune old snapshots, keeping only the most recent N
    Prune {
        /// Number of snapshots to keep (default: 10)
        #[arg(short, long, default_value = "10")]
        keep: usize,

        /// Target directory (default: current directory's .claude)
        #[arg(long)]
        path: Option<PathBuf>,

        /// Prune for ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// Create a new profile
    Add {
        /// Profile name
        name: String,
    },

    /// List all profiles
    List,

    /// Show profile details (local or GitHub)
    Show {
        /// Profile name or GitHub URL/repo (e.g., "my-profile" or "user/repo")
        name: String,

        /// Number of README lines to show for GitHub repos
        #[arg(long, default_value = "20")]
        lines: usize,
    },

    /// Remove a profile
    Remove {
        /// Profile name
        name: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Import a directory or git repository as a profile
    Import {
        /// Source directory or git URL (https://... or git@...)
        source: String,

        /// Profile name (default: directory/repo name)
        #[arg(short, long)]
        name: Option<String>,

        /// Subdirectory within the repo to import
        #[arg(long)]
        path: Option<PathBuf>,

        /// Git branch/tag/commit to checkout
        #[arg(short, long)]
        branch: Option<String>,

        /// Force overwrite if profile exists
        #[arg(short, long)]
        force: bool,
    },

    /// Manage profile snapshots
    Snapshot {
        #[command(subcommand)]
        action: ProfileSnapshotAction,
    },
}

#[derive(Subcommand)]
pub enum ProfileSnapshotAction {
    /// Save a snapshot of a profile
    Save {
        /// Profile name
        profile: String,

        /// Optional message describing the snapshot
        #[arg(short, long)]
        message: Option<String>,
    },

    /// List all snapshots for a profile
    List {
        /// Profile name
        profile: String,
    },

    /// Restore a snapshot to the profile
    Restore {
        /// Profile name
        profile: String,

        /// Snapshot ID to restore
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Show diff between snapshot and current profile state
    Diff {
        /// Profile name
        profile: String,

        /// Snapshot ID to compare
        id: String,
    },

    /// Delete a snapshot
    Delete {
        /// Profile name
        profile: String,

        /// Snapshot ID to delete
        id: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Prune old snapshots
    Prune {
        /// Profile name
        profile: String,

        /// Number of snapshots to keep (default: 10)
        #[arg(short, long, default_value = "10")]
        keep: usize,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum RuleAction {
    /// Create a new rule (generates template .md file)
    Add {
        /// Rule name
        name: String,

        /// Import from existing markdown file
        #[arg(short, long)]
        file: Option<PathBuf>,
    },

    /// List all rules
    List,

    /// Show rule contents
    Show {
        /// Rule name
        name: String,
    },

    /// Open rule in editor
    Edit {
        /// Rule name
        name: String,
    },

    /// Remove a rule
    Remove {
        /// Rule name
        name: String,

        /// Skip confirmation
        #[arg(short, long)]
        force: bool,
    },

    /// Extract rule from existing profile (AI-powered)
    Extract {
        /// Source profile to extract from
        #[arg(short = 'p', long)]
        profile: String,

        /// Name for the new rule
        #[arg(short, long)]
        name: String,
    },

    /// Generate rule from natural language instruction (AI-powered)
    Generate {
        /// Natural language instruction (e.g., "Rust用にして")
        instruction: String,

        /// Name for the new rule (auto-generated if not specified)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// Rename a rule
    Rename {
        /// Current rule name
        name: String,

        /// New rule name
        new_name: String,
    },

    /// Apply rule to profile, creating new customized profile
    Apply {
        /// Source profile name
        #[arg(short = 'p', long)]
        profile: String,

        /// Rule name to apply
        #[arg(short = 'r', long)]
        rule: String,

        /// Name for the new profile (default: {profile}-{rule})
        #[arg(short, long)]
        name: Option<String>,

        /// Dry run (don't create new profile)
        #[arg(short, long)]
        dry_run: bool,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Get a config value
    Get {
        /// Config key (e.g., profile.exclude)
        key: String,
    },

    /// Set a config value
    Set {
        /// Config key (e.g., profile.exclude)
        key: String,

        /// Value to set (e.g., ".git,node_modules" or "[.git, node_modules]")
        value: String,
    },

    /// List all config values
    List,

    /// Show config file path
    Path,

    /// Initialize config file with defaults
    Init,
}

#[derive(Subcommand)]
pub enum HubAction {
    /// Add a hub repository
    Add {
        /// Hub URL (GitHub repository)
        url: String,

        /// Hub name (default: derived from URL)
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List registered hubs
    List,

    /// Remove a hub
    Remove {
        /// Hub name
        name: String,
    },

    /// Refresh hub cache (fetch latest channel list)
    Refresh {
        /// Hub name (refresh all if not specified)
        name: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ChannelAction {
    /// Discover available channels from all hubs
    Discover,

    /// Add/enable a channel
    ///
    /// Use type flags (-m, -a, -d, -H) for explicit channel type:
    ///   -m/--marketplace: Claude Code Plugin Marketplace
    ///   -a/--awesome: Awesome List (curated markdown)
    ///   -d/--direct: Direct GitHub repository
    ///   -H/--hub: Channel from a registered Hub
    Add {
        /// URL or name (GitHub repo, marketplace, awesome list, or hub channel)
        source: String,

        /// Add as Marketplace (Claude Code Plugin)
        #[arg(short = 'm', long, group = "channel_type")]
        marketplace: bool,

        /// Add as Awesome List
        #[arg(short = 'a', long, group = "channel_type")]
        awesome: bool,

        /// Add as Direct repository
        #[arg(short = 'd', long, group = "channel_type")]
        direct: bool,

        /// Add from Hub (specify hub name)
        #[arg(short = 'H', long, group = "channel_type")]
        hub: Option<String>,

        /// Custom name for the channel
        #[arg(short, long)]
        name: Option<String>,
    },

    /// List enabled channels
    List,

    /// Remove/disable a channel
    Remove {
        /// Channel name
        name: String,
    },

    /// Enable a disabled channel
    Enable {
        /// Channel name
        name: String,
    },

    /// Disable a channel (keep config but don't use)
    Disable {
        /// Channel name
        name: String,
    },

    /// Refresh channel cache
    Refresh {
        /// Channel name (refresh all if not specified)
        name: Option<String>,
    },
}
