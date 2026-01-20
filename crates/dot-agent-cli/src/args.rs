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

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Install a profile to target
    Install {
        /// Profile name
        #[arg(short, long)]
        profile: String,

        /// Target directory (default: current directory)
        target: Option<PathBuf>,

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
    },

    /// Upgrade installed profile to latest
    Upgrade {
        /// Profile name
        #[arg(short, long)]
        profile: String,

        /// Target directory
        target: Option<PathBuf>,

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
    },

    /// Show diff between profile and installed files
    Diff {
        /// Profile name
        #[arg(short, long)]
        profile: String,

        /// Target directory
        target: Option<PathBuf>,

        /// Diff ~/.claude (global)
        #[arg(short, long)]
        global: bool,
    },

    /// Remove installed profile
    Remove {
        /// Profile name
        #[arg(short, long)]
        profile: String,

        /// Target directory
        target: Option<PathBuf>,

        /// Remove from ~/.claude (global)
        #[arg(short, long)]
        global: bool,

        /// Force remove even with local modifications
        #[arg(short, long)]
        force: bool,

        /// Dry run
        #[arg(short, long)]
        dry_run: bool,
    },

    /// Show installation status
    Status {
        /// Target directory
        target: Option<PathBuf>,

        /// Check ~/.claude (global)
        #[arg(short, long)]
        global: bool,
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
}
