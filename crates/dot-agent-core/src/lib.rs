pub mod error;
pub mod installer;
pub mod metadata;
pub mod profile;
pub mod rule;
pub mod snapshot;

pub use error::{DotAgentError, Result};
pub use installer::{DiffResult, FileInfo, FileStatus, InstallResult, Installer};
pub use metadata::Metadata;
pub use profile::{Profile, ProfileManager};
pub use rule::{extract_rule, generate_rule, ApplyResult, Rule, RuleExecutor, RuleManager};
pub use snapshot::{ProfileSnapshotManager, Snapshot, SnapshotDiff, SnapshotManager, SnapshotTrigger};
