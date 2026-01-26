//! Plugin Registration for Claude Code
//!
//! Automatically registers profiles with plugin features (hooks, MCP, LSP)
//! to Claude Code's settings.json.
//!
//! # Claude Code Settings Structure
//!
//! ```json
//! {
//!   "plugins": {
//!     "installed": ["path/to/plugin1", "path/to/plugin2"]
//!   }
//! }
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::error::{DotAgentError, Result};
use crate::profile_metadata::{PluginScope, ProfileMetadata};

/// Plugin-related files/directories that trigger registration
const PLUGIN_HOOKS_DIR: &str = "hooks";
const PLUGIN_MCP_FILE: &str = ".mcp.json";
const PLUGIN_LSP_FILE: &str = ".lsp.json";

/// Claude Code settings file names
const SETTINGS_FILE: &str = "settings.json";
const SETTINGS_LOCAL_FILE: &str = "settings.local.json";

/// Plugin registration result
#[derive(Debug, Default)]
pub struct PluginRegistrationResult {
    /// Whether plugin was registered
    pub registered: bool,
    /// Path where plugin was registered
    pub settings_path: Option<PathBuf>,
    /// Plugin features found
    pub features: Vec<String>,
    /// Error message if failed
    pub error: Option<String>,
}

/// Plugin registrar for Claude Code
pub struct PluginRegistrar {
    /// Home directory for user scope
    home_dir: PathBuf,
}

impl PluginRegistrar {
    /// Create new registrar
    pub fn new() -> Result<Self> {
        let home_dir = dirs::home_dir().ok_or(DotAgentError::HomeNotFound)?;
        Ok(Self { home_dir })
    }

    /// Check if a profile has plugin features
    pub fn has_plugin_features(profile_path: &Path) -> bool {
        ProfileMetadata::has_plugin_features(profile_path)
    }

    /// Get list of plugin features in a profile
    pub fn get_plugin_features(profile_path: &Path) -> Vec<String> {
        let mut features = Vec::new();

        let hooks_dir = profile_path.join(PLUGIN_HOOKS_DIR);
        if hooks_dir.exists() && hooks_dir.is_dir() {
            // Check for hooks.json or hook files
            if hooks_dir.join("hooks.json").exists() {
                features.push("hooks".to_string());
            } else if fs::read_dir(&hooks_dir)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
            {
                features.push("hooks".to_string());
            }
        }

        if profile_path.join(PLUGIN_MCP_FILE).exists() {
            features.push("mcp".to_string());
        }

        if profile_path.join(PLUGIN_LSP_FILE).exists() {
            features.push("lsp".to_string());
        }

        features
    }

    /// Register a profile as a plugin
    pub fn register_plugin(
        &self,
        profile_path: &Path,
        _profile_name: &str,
        scope: PluginScope,
        target_dir: Option<&Path>,
    ) -> Result<PluginRegistrationResult> {
        let mut result = PluginRegistrationResult::default();

        // Check for plugin features
        let features = Self::get_plugin_features(profile_path);
        if features.is_empty() {
            return Ok(result);
        }
        result.features = features;

        // Determine settings file path based on scope
        let settings_path = self.get_settings_path(scope, target_dir)?;
        result.settings_path = Some(settings_path.clone());

        // Load or create settings
        let mut settings = self.load_settings(&settings_path)?;

        // Add plugin to installed list
        let plugin_path = profile_path.to_string_lossy().to_string();
        self.add_plugin_to_settings(&mut settings, &plugin_path)?;

        // Save settings
        self.save_settings(&settings_path, &settings)?;

        result.registered = true;
        Ok(result)
    }

    /// Unregister a profile plugin
    pub fn unregister_plugin(
        &self,
        profile_path: &Path,
        scope: PluginScope,
        target_dir: Option<&Path>,
    ) -> Result<bool> {
        let settings_path = self.get_settings_path(scope, target_dir)?;

        if !settings_path.exists() {
            return Ok(false);
        }

        let mut settings = self.load_settings(&settings_path)?;
        let plugin_path = profile_path.to_string_lossy().to_string();

        let removed = self.remove_plugin_from_settings(&mut settings, &plugin_path)?;

        if removed {
            self.save_settings(&settings_path, &settings)?;
        }

        Ok(removed)
    }

    /// Get settings file path based on scope
    fn get_settings_path(&self, scope: PluginScope, target_dir: Option<&Path>) -> Result<PathBuf> {
        match scope {
            PluginScope::User => {
                let claude_dir = self.home_dir.join(".claude");
                Ok(claude_dir.join(SETTINGS_FILE))
            }
            PluginScope::Project => {
                let base = target_dir
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let claude_dir = base.join(".claude");
                Ok(claude_dir.join(SETTINGS_FILE))
            }
            PluginScope::Local => {
                let base = target_dir
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
                let claude_dir = base.join(".claude");
                Ok(claude_dir.join(SETTINGS_LOCAL_FILE))
            }
        }
    }

    /// Load settings file or create default
    fn load_settings(&self, path: &Path) -> Result<Value> {
        if path.exists() {
            let content = fs::read_to_string(path)?;
            let settings: Value =
                serde_json::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
                    path: path.to_path_buf(),
                    message: e.to_string(),
                })?;
            Ok(settings)
        } else {
            Ok(serde_json::json!({}))
        }
    }

    /// Save settings file
    fn save_settings(&self, path: &Path, settings: &Value) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(settings).map_err(|e| {
            DotAgentError::ConfigParse {
                path: path.to_path_buf(),
                message: e.to_string(),
            }
        })?;

        fs::write(path, content)?;
        Ok(())
    }

    /// Add plugin to settings
    fn add_plugin_to_settings(&self, settings: &mut Value, plugin_path: &str) -> Result<()> {
        // Ensure plugins.installed exists
        if settings.get("plugins").is_none() {
            settings["plugins"] = serde_json::json!({});
        }

        let plugins = settings["plugins"].as_object_mut().ok_or_else(|| {
            DotAgentError::ConfigParse {
                path: PathBuf::from("<settings>"),
                message: "plugins must be an object".to_string(),
            }
        })?;

        // Get or create installed array
        let installed = plugins
            .entry("installed")
            .or_insert_with(|| serde_json::json!([]));

        let installed_arr = installed.as_array_mut().ok_or_else(|| {
            DotAgentError::ConfigParse {
                path: PathBuf::from("<settings>"),
                message: "plugins.installed must be an array".to_string(),
            }
        })?;

        // Check if already installed
        let path_value = serde_json::json!(plugin_path);
        if !installed_arr.contains(&path_value) {
            installed_arr.push(path_value);
        }

        Ok(())
    }

    /// Remove plugin from settings
    fn remove_plugin_from_settings(&self, settings: &mut Value, plugin_path: &str) -> Result<bool> {
        let plugins = match settings.get_mut("plugins") {
            Some(p) => p,
            None => return Ok(false),
        };

        let plugins_obj = match plugins.as_object_mut() {
            Some(o) => o,
            None => return Ok(false),
        };

        let installed = match plugins_obj.get_mut("installed") {
            Some(i) => i,
            None => return Ok(false),
        };

        let installed_arr = match installed.as_array_mut() {
            Some(a) => a,
            None => return Ok(false),
        };

        let original_len = installed_arr.len();
        installed_arr.retain(|v| v.as_str() != Some(plugin_path));

        Ok(installed_arr.len() < original_len)
    }

    /// List all registered plugins for a scope
    pub fn list_plugins(&self, scope: PluginScope, target_dir: Option<&Path>) -> Result<Vec<String>> {
        let settings_path = self.get_settings_path(scope, target_dir)?;

        if !settings_path.exists() {
            return Ok(Vec::new());
        }

        let settings = self.load_settings(&settings_path)?;

        let plugins = settings
            .get("plugins")
            .and_then(|p| p.get("installed"))
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        Ok(plugins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn get_plugin_features_empty() {
        let tmp = TempDir::new().unwrap();
        let features = PluginRegistrar::get_plugin_features(tmp.path());
        assert!(features.is_empty());
    }

    #[test]
    fn get_plugin_features_hooks() {
        let tmp = TempDir::new().unwrap();
        let hooks_dir = tmp.path().join("hooks");
        fs::create_dir(&hooks_dir).unwrap();
        fs::write(hooks_dir.join("hooks.json"), "{}").unwrap();

        let features = PluginRegistrar::get_plugin_features(tmp.path());
        assert!(features.contains(&"hooks".to_string()));
    }

    #[test]
    fn get_plugin_features_mcp() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join(".mcp.json"), "{}").unwrap();

        let features = PluginRegistrar::get_plugin_features(tmp.path());
        assert!(features.contains(&"mcp".to_string()));
    }

    #[test]
    fn add_plugin_to_empty_settings() {
        let registrar = PluginRegistrar {
            home_dir: PathBuf::from("/tmp"),
        };

        let mut settings = serde_json::json!({});
        registrar
            .add_plugin_to_settings(&mut settings, "/path/to/plugin")
            .unwrap();

        let installed = settings["plugins"]["installed"].as_array().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].as_str().unwrap(), "/path/to/plugin");
    }

    #[test]
    fn add_plugin_idempotent() {
        let registrar = PluginRegistrar {
            home_dir: PathBuf::from("/tmp"),
        };

        let mut settings = serde_json::json!({});
        registrar
            .add_plugin_to_settings(&mut settings, "/path/to/plugin")
            .unwrap();
        registrar
            .add_plugin_to_settings(&mut settings, "/path/to/plugin")
            .unwrap();

        let installed = settings["plugins"]["installed"].as_array().unwrap();
        assert_eq!(installed.len(), 1);
    }

    #[test]
    fn remove_plugin_from_settings() {
        let registrar = PluginRegistrar {
            home_dir: PathBuf::from("/tmp"),
        };

        let mut settings = serde_json::json!({
            "plugins": {
                "installed": ["/path/to/plugin1", "/path/to/plugin2"]
            }
        });

        let removed = registrar
            .remove_plugin_from_settings(&mut settings, "/path/to/plugin1")
            .unwrap();
        assert!(removed);

        let installed = settings["plugins"]["installed"].as_array().unwrap();
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].as_str().unwrap(), "/path/to/plugin2");
    }
}
