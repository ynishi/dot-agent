pub mod category;
pub mod channel;
pub mod config;
pub mod error;
pub mod install;
pub mod llm;
pub mod platform;
pub mod plugin;
pub mod profile;
pub mod rule;

pub use channel::{
    Channel, ChannelManager, ChannelRef, ChannelRegistry, ChannelSource, ChannelType, Hub,
    HubRegistry, ProfileRef, SearchOptions,
};
pub use config::Config;
pub use error::{DotAgentError, Result};
pub use install::{
    is_mergeable_json, merge_json, merge_json_file, unmerge_json, unmerge_json_file, DiffResult,
    FileInfo, FileStatus, InstallOptions, InstallResult, Installer, MergeRecord, MergeResult,
    Metadata, ProfileSnapshotManager, Snapshot, SnapshotDiff, SnapshotManager, SnapshotTrigger,
    UnmergeResult,
};
pub use llm::{check_claude_cli, execute_claude, require_claude_cli, LlmConfig};
pub use platform::{InstallTarget, Platform};
pub use plugin::{
    FilterConfig, PluginManifest, PluginRegistrar, PluginRegistrationResult, DEFAULT_COMPONENT_DIRS,
};
pub use profile::{
    migrate_existing_profiles, CollectedFile, FusionConfig, FusionConflict, FusionExecutor,
    FusionPlan, FusionResult, FusionSpec, IgnoreConfig, PluginConfig, PluginScope, Profile,
    ProfileIndexEntry, ProfileInfo, ProfileManager, ProfileMetadata, ProfileSource, ProfilesIndex,
    DEFAULT_EXCLUDED_DIRS,
};
pub use rule::{extract_rule, generate_rule, ApplyResult, Rule, RuleExecutor, RuleManager};

// Category system
pub use category::{
    BuiltinCategory, CategoriesConfig, CategoryClassifier, CategoryConfigEntry, CategoryDef,
    CategoryStore, ClassificationMode, ClassifiedProfile, FileClassification, BUILTIN_CATEGORIES,
};
