use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};
use crate::profile::{IgnoreConfig, DEFAULT_EXCLUDED_DIRS};

const CONFIG_FILE: &str = "config.toml";

/// Default config template with rich comments
const DEFAULT_CONFIG_TEMPLATE: &str = r#"# dot-agent configuration file
# Location: ~/.dot-agent/config.toml

[profile]
# Directories to exclude when installing profiles
# Default: [".git"]
# Example: exclude = [".git", "node_modules", ".venv"]
exclude = [".git"]

# Directories to always include (overrides exclude)
# Default: []
# Example: include = [".git"]  # to include .git in installations
include = []
"#;

/// Global configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub profile: ProfileConfig,
}

/// Profile-related configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Directories to exclude
    #[serde(default = "default_exclude")]
    pub exclude: Vec<String>,

    /// Directories to include (overrides exclude)
    #[serde(default)]
    pub include: Vec<String>,
}

fn default_exclude() -> Vec<String> {
    DEFAULT_EXCLUDED_DIRS
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            exclude: default_exclude(),
            include: Vec::new(),
        }
    }
}

impl Config {
    /// Load config from base directory
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(CONFIG_FILE);
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let config: Config = toml::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        Ok(config)
    }

    /// Save config to base directory
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(CONFIG_FILE);
        fs::create_dir_all(base_dir)?;

        let content = toml::to_string_pretty(self).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        fs::write(&path, content)?;
        Ok(())
    }

    /// Get config file path
    pub fn path(base_dir: &Path) -> PathBuf {
        base_dir.join(CONFIG_FILE)
    }

    /// Initialize config with default template (rich comments)
    pub fn init(base_dir: &Path) -> Result<PathBuf> {
        let path = base_dir.join(CONFIG_FILE);
        fs::create_dir_all(base_dir)?;

        if !path.exists() {
            fs::write(&path, DEFAULT_CONFIG_TEMPLATE)?;
        }

        Ok(path)
    }

    /// Get a config value by dot-notation key
    pub fn get(&self, key: &str) -> Option<String> {
        match key {
            "profile.exclude" => Some(format!("{:?}", self.profile.exclude)),
            "profile.include" => Some(format!("{:?}", self.profile.include)),
            _ => None,
        }
    }

    /// Set a config value by dot-notation key
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "profile.exclude" => {
                self.profile.exclude = parse_string_list(value)?;
                Ok(())
            }
            "profile.include" => {
                self.profile.include = parse_string_list(value)?;
                Ok(())
            }
            _ => Err(DotAgentError::ConfigKeyNotFound {
                key: key.to_string(),
            }),
        }
    }

    /// List all config keys with their current values
    pub fn list(&self) -> Vec<(String, String)> {
        vec![
            (
                "profile.exclude".to_string(),
                format!("{:?}", self.profile.exclude),
            ),
            (
                "profile.include".to_string(),
                format!("{:?}", self.profile.include),
            ),
        ]
    }

    /// Convert to IgnoreConfig for use in install/upgrade
    pub fn to_ignore_config(&self) -> IgnoreConfig {
        IgnoreConfig {
            excluded_dirs: self.profile.exclude.clone(),
            included_dirs: self.profile.include.clone(),
        }
    }
}

/// Parse a comma-separated or JSON-like list string
fn parse_string_list(value: &str) -> Result<Vec<String>> {
    let trimmed = value.trim();

    // Try JSON array format first: ["a", "b"]
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        if inner.trim().is_empty() {
            return Ok(Vec::new());
        }

        let items: Vec<String> = inner
            .split(',')
            .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        return Ok(items);
    }

    // Comma-separated format: a,b,c or "a","b"
    let items: Vec<String> = trimmed
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect();

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string_list_comma() {
        let result = parse_string_list(".git,node_modules").unwrap();
        assert_eq!(result, vec![".git", "node_modules"]);
    }

    #[test]
    fn test_parse_string_list_json() {
        let result = parse_string_list(r#"[".git", "node_modules"]"#).unwrap();
        assert_eq!(result, vec![".git", "node_modules"]);
    }

    #[test]
    fn test_parse_string_list_empty() {
        let result = parse_string_list("[]").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_config_get_set() {
        let mut config = Config::default();

        config.set("profile.exclude", ".git,node_modules").unwrap();
        assert_eq!(config.profile.exclude, vec![".git", "node_modules"]);

        let value = config.get("profile.exclude").unwrap();
        assert!(value.contains(".git"));
    }

    #[test]
    fn test_to_ignore_config() {
        let mut config = Config::default();
        config.profile.exclude = vec![".git".to_string(), "node_modules".to_string()];
        config.profile.include = vec![".gitkeep".to_string()];

        let ignore = config.to_ignore_config();
        assert_eq!(ignore.excluded_dirs, vec![".git", "node_modules"]);
        assert_eq!(ignore.included_dirs, vec![".gitkeep"]);
    }
}
