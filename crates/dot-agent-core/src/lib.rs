pub mod channel;
pub mod config;
pub mod error;
pub mod installer;
pub mod metadata;
pub mod profile;
pub mod rule;
pub mod snapshot;

pub use channel::{
    Channel, ChannelManager, ChannelRef, ChannelRegistry, ChannelSource, ChannelType, Hub,
    HubRegistry, ProfileRef, SearchOptions,
};
pub use config::Config;
pub use error::{DotAgentError, Result};
pub use installer::{DiffResult, FileInfo, FileStatus, InstallResult, Installer};
pub use metadata::Metadata;
pub use profile::{IgnoreConfig, Profile, ProfileManager, DEFAULT_EXCLUDED_DIRS};
pub use rule::{extract_rule, generate_rule, ApplyResult, Rule, RuleExecutor, RuleManager};
pub use snapshot::{
    ProfileSnapshotManager, Snapshot, SnapshotDiff, SnapshotManager, SnapshotTrigger,
};
