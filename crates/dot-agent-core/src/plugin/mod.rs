//! Plugin Module
//!
//! Claude Code プラグイン関連の機能を提供する。
//!
//! - `manifest`: `.claude-plugin/plugin.json` のパース
//! - `registrar`: Claude Code settings.json へのプラグイン登録

pub mod manifest;
pub mod registrar;

// Re-exports
pub use manifest::{FilterConfig, PluginManifest, DEFAULT_COMPONENT_DIRS};
pub use registrar::{PluginRegistrar, PluginRegistrationResult};
