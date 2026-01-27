//! Plugin manifest parsing for .claude-plugin/plugin.json
//!
//! Follows Claude Code official plugin specification with include/exclude extensions.

use std::fs;
use std::path::{Path, PathBuf};

use glob::Pattern;
use serde::{Deserialize, Serialize};

use crate::error::Result;

const PLUGIN_DIR: &str = ".claude-plugin";
const PLUGIN_JSON: &str = "plugin.json";

/// Official Claude Code plugin manifest structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    /// Plugin name (required)
    pub name: String,

    /// Plugin version
    #[serde(default)]
    pub version: Option<String>,

    /// Plugin description
    #[serde(default)]
    pub description: Option<String>,

    /// Author information
    #[serde(default)]
    pub author: Option<Author>,

    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,

    /// License
    #[serde(default)]
    pub license: Option<String>,

    /// Keywords for discovery
    #[serde(default)]
    pub keywords: Vec<String>,

    // Component paths (official spec)
    /// Additional command paths
    #[serde(default)]
    pub commands: ComponentPaths,

    /// Additional agent paths
    #[serde(default)]
    pub agents: ComponentPaths,

    /// Additional skill paths
    #[serde(default)]
    pub skills: ComponentPaths,

    /// Hook configuration path or inline
    #[serde(default)]
    pub hooks: Option<serde_json::Value>,

    /// MCP server configuration
    #[serde(default)]
    pub mcp_servers: Option<serde_json::Value>,

    /// LSP server configuration
    #[serde(default)]
    pub lsp_servers: Option<serde_json::Value>,

    /// Output styles paths
    #[serde(default)]
    pub output_styles: ComponentPaths,
}

/// Author information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Author {
    pub name: Option<String>,
    pub email: Option<String>,
    pub url: Option<String>,
}

/// Component paths can be a single string or array of strings
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ComponentPaths {
    #[default]
    None,
    Single(String),
    Multiple(Vec<String>),
}

impl ComponentPaths {
    /// Convert to Vec<String>
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            ComponentPaths::None => Vec::new(),
            ComponentPaths::Single(s) => vec![s.clone()],
            ComponentPaths::Multiple(v) => v.clone(),
        }
    }

    /// Check if any paths are specified
    pub fn is_empty(&self) -> bool {
        match self {
            ComponentPaths::None => true,
            ComponentPaths::Single(_) => false,
            ComponentPaths::Multiple(v) => v.is_empty(),
        }
    }
}

/// Default component directories (official spec)
pub const DEFAULT_COMPONENT_DIRS: &[&str] = &[
    "commands",
    "agents",
    "skills",
    "hooks",
    "rules",
    "plugins",
    "mcp-configs",
    "examples",
];

impl PluginManifest {
    /// Load plugin manifest from .claude-plugin/plugin.json
    pub fn load(profile_path: &Path) -> Result<Option<Self>> {
        let manifest_path = profile_path.join(PLUGIN_DIR).join(PLUGIN_JSON);
        if !manifest_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&content)?;
        Ok(Some(manifest))
    }

    /// Check if plugin manifest exists
    pub fn exists(profile_path: &Path) -> bool {
        profile_path.join(PLUGIN_DIR).join(PLUGIN_JSON).exists()
    }

    /// Get all component paths to scan
    /// Returns paths relative to profile root
    pub fn get_component_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        // Add explicitly specified paths
        for p in self.commands.to_vec() {
            paths.push(normalize_path(&p));
        }
        for p in self.agents.to_vec() {
            paths.push(normalize_path(&p));
        }
        for p in self.skills.to_vec() {
            paths.push(normalize_path(&p));
        }
        for p in self.output_styles.to_vec() {
            paths.push(normalize_path(&p));
        }

        // If no explicit paths, use defaults
        if paths.is_empty() {
            for dir in DEFAULT_COMPONENT_DIRS {
                paths.push(PathBuf::from(dir));
            }
        }

        paths
    }

    /// Check if this manifest has explicit component paths
    pub fn has_explicit_paths(&self) -> bool {
        !self.commands.is_empty()
            || !self.agents.is_empty()
            || !self.skills.is_empty()
            || !self.output_styles.is_empty()
    }
}

/// Filter configuration for include/exclude patterns
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Patterns to include (added to base)
    #[serde(default)]
    pub include: Vec<String>,

    /// Patterns to exclude
    #[serde(default)]
    pub exclude: Vec<String>,
}

impl FilterConfig {
    /// Load filter config from .dot-agent.toml
    pub fn load(profile_path: &Path) -> Result<Option<Self>> {
        let toml_path = profile_path.join(".dot-agent.toml");
        if !toml_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&toml_path)?;
        let value: toml::Value = toml::from_str(&content)?;

        if let Some(filter) = value.get("filter") {
            let config: FilterConfig = filter.clone().try_into()?;
            return Ok(Some(config));
        }

        Ok(None)
    }

    /// Check if path matches any include pattern
    pub fn matches_include(&self, path: &Path) -> bool {
        if self.include.is_empty() {
            return false;
        }

        let path_str = path.to_string_lossy();
        for pattern in &self.include {
            if let Ok(glob) = Pattern::new(pattern) {
                if glob.matches(&path_str) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if path matches any exclude pattern
    pub fn matches_exclude(&self, path: &Path) -> bool {
        if self.exclude.is_empty() {
            return false;
        }

        let path_str = path.to_string_lossy();
        for pattern in &self.exclude {
            if let Ok(glob) = Pattern::new(pattern) {
                if glob.matches(&path_str) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if config is empty
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }
}

/// Normalize path by removing leading "./"
fn normalize_path(path: &str) -> PathBuf {
    let p = path.trim_start_matches("./");
    PathBuf::from(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_paths_single() {
        let paths = ComponentPaths::Single("./commands/".to_string());
        assert_eq!(paths.to_vec(), vec!["./commands/"]);
        assert!(!paths.is_empty());
    }

    #[test]
    fn component_paths_multiple() {
        let paths = ComponentPaths::Multiple(vec!["./a/".to_string(), "./b/".to_string()]);
        assert_eq!(paths.to_vec(), vec!["./a/", "./b/"]);
    }

    #[test]
    fn component_paths_none() {
        let paths = ComponentPaths::None;
        assert!(paths.is_empty());
        assert!(paths.to_vec().is_empty());
    }

    #[test]
    fn filter_matches_exclude() {
        let config = FilterConfig {
            include: vec![],
            exclude: vec!["**/brainstorm.md".to_string(), "tests/**".to_string()],
        };

        assert!(config.matches_exclude(Path::new("commands/brainstorm.md")));
        assert!(config.matches_exclude(Path::new("tests/unit/test.rs")));
        assert!(!config.matches_exclude(Path::new("commands/commit.md")));
    }

    #[test]
    fn filter_matches_include() {
        let config = FilterConfig {
            include: vec!["examples/*.md".to_string()],
            exclude: vec![],
        };

        assert!(config.matches_include(Path::new("examples/usage.md")));
        assert!(!config.matches_include(Path::new("commands/test.md")));
    }

    #[test]
    fn normalize_path_removes_prefix() {
        assert_eq!(normalize_path("./commands/"), PathBuf::from("commands/"));
        assert_eq!(normalize_path("agents/"), PathBuf::from("agents/"));
    }
}
