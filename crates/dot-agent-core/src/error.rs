use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum DotAgentError {
    #[error("Profile not found: {name}")]
    ProfileNotFound { name: String },

    #[error("Target directory does not exist: {path}")]
    TargetNotFound { path: PathBuf },

    #[error("Profile already exists: {name}")]
    ProfileAlreadyExists { name: String },

    #[error("Invalid profile name: '{name}' - must contain only alphanumeric, hyphen, underscore")]
    InvalidProfileName { name: String },

    #[error("Conflict detected - file exists with different content: {path}")]
    Conflict { path: PathBuf },

    #[error("Local modifications detected: {paths:?}")]
    LocalModifications { paths: Vec<PathBuf> },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("TOML serialization error: {0}")]
    TomlSer(#[from] toml::ser::Error),

    #[error("TOML deserialization error: {0}")]
    TomlDe(#[from] toml::de::Error),

    #[error("Home directory not found")]
    HomeNotFound,

    #[error("GUI error: {0}")]
    Gui(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Rule not found: {name}")]
    RuleNotFound { name: String },

    #[error("Rule already exists: {name}")]
    RuleAlreadyExists { name: String },

    #[error("Invalid rule name: '{name}' - must be alphanumeric, hyphen, underscore, 1-64 chars")]
    InvalidRuleName { name: String },

    #[error("Claude CLI not found. Install with: brew install claude")]
    ClaudeNotFound,

    #[error("Claude CLI execution failed: {message}")]
    ClaudeExecutionFailed { message: String },

    #[error("Glob pattern error: {0}")]
    GlobPattern(#[from] glob::PatternError),

    #[error("Glob error: {0}")]
    Glob(#[from] glob::GlobError),

    #[error("Snapshot not found: {id}")]
    SnapshotNotFound { id: String },

    #[error("Config parse error in {path}: {message}")]
    ConfigParse { path: PathBuf, message: String },

    #[error("Config key not found: {key}")]
    ConfigKeyNotFound { key: String },

    #[error("Config parse error: {message}")]
    ConfigParseSimple { message: String },

    #[error("GitHub CLI (gh) not found. Install with: brew install gh")]
    GitHubCliNotFound,

    #[error("GitHub API error: {message}")]
    GitHubApiError { message: String },

    #[error("Hub already exists: {name}")]
    HubAlreadyExists { name: String },

    #[error("Hub not found: {name}")]
    HubNotFound { name: String },

    #[error("Cannot remove default hub")]
    CannotRemoveDefaultHub,

    #[error("Channel already exists: {name}")]
    ChannelAlreadyExists { name: String },

    #[error("Channel not found: {name}")]
    ChannelNotFound { name: String },

    #[error("Cannot remove built-in channel: {name}")]
    CannotRemoveBuiltinChannel { name: String },

    #[error("JSON parse error: {message}")]
    JsonParseError { message: String },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, DotAgentError>;

impl DotAgentError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ProfileNotFound { .. } => 2,
            Self::TargetNotFound { .. } => 3,
            Self::LocalModifications { .. } => 4,
            Self::InvalidProfileName { .. } => 5,
            Self::Conflict { .. } => 6,
            Self::RuleNotFound { .. } => 7,
            Self::RuleAlreadyExists { .. } => 8,
            Self::InvalidRuleName { .. } => 9,
            Self::ClaudeNotFound => 10,
            Self::ClaudeExecutionFailed { .. } => 11,
            Self::SnapshotNotFound { .. } => 12,
            Self::ConfigParse { .. } => 13,
            Self::ConfigKeyNotFound { .. } => 14,
            Self::ConfigParseSimple { .. } => 15,
            Self::GitHubCliNotFound => 16,
            Self::GitHubApiError { .. } => 17,
            Self::HubAlreadyExists { .. } => 18,
            Self::HubNotFound { .. } => 19,
            Self::CannotRemoveDefaultHub => 20,
            Self::ChannelAlreadyExists { .. } => 21,
            Self::ChannelNotFound { .. } => 22,
            Self::CannotRemoveBuiltinChannel { .. } => 23,
            Self::JsonParseError { .. } => 24,
            _ => 1,
        }
    }
}
