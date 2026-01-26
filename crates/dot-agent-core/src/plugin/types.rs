//! Claude Code Plugin type definitions
//!
//! Types for working with Claude Code Plugin Marketplaces and Plugins.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Claude Code Plugin Marketplace (parsed from marketplace.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marketplace {
    /// Marketplace name (unique identifier)
    pub name: String,
    /// Description
    #[serde(default)]
    pub description: Option<String>,
    /// Owner information
    pub owner: MarketplaceOwner,
    /// Available plugins
    #[serde(default)]
    pub plugins: Vec<PluginEntry>,
    /// Optional metadata
    #[serde(default)]
    pub metadata: Option<MarketplaceMetadata>,
}

/// Marketplace owner information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplaceOwner {
    /// Owner name
    pub name: String,
    /// Contact email
    #[serde(default)]
    pub email: Option<String>,
}

/// Optional marketplace metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MarketplaceMetadata {
    /// Marketplace description
    #[serde(default)]
    pub description: Option<String>,
    /// Marketplace version
    #[serde(default)]
    pub version: Option<String>,
    /// Base directory for relative plugin paths
    #[serde(default, rename = "pluginRoot")]
    pub plugin_root: Option<String>,
}

/// Plugin entry in marketplace.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    /// Plugin name (unique identifier within marketplace)
    pub name: String,
    /// Source location
    pub source: PluginSource,
    /// Description
    #[serde(default)]
    pub description: Option<String>,
    /// Version
    #[serde(default)]
    pub version: Option<String>,
    /// Author information
    #[serde(default)]
    pub author: Option<PluginAuthor>,
    /// Category
    #[serde(default)]
    pub category: Option<String>,
    /// Tags for searchability
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Keywords
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    /// Whether plugin needs its own plugin.json (default: true)
    #[serde(default = "default_strict")]
    pub strict: bool,
}

fn default_strict() -> bool {
    true
}

/// Plugin source specification
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PluginSource {
    /// Relative path (e.g., "./plugins/my-plugin")
    Relative(String),
    /// Structured source
    Structured(StructuredSource),
}

/// Structured source specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructuredSource {
    /// Source type: "github", "url"
    pub source: String,
    /// GitHub repository (for "github" source)
    #[serde(default)]
    pub repo: Option<String>,
    /// URL (for "url" source)
    #[serde(default)]
    pub url: Option<String>,
    /// Git ref (branch/tag)
    #[serde(default)]
    pub r#ref: Option<String>,
    /// Git commit SHA
    #[serde(default)]
    pub sha: Option<String>,
}

/// Plugin author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginAuthor {
    /// Author name
    pub name: String,
    /// Contact email
    #[serde(default)]
    pub email: Option<String>,
}

/// Plugin manifest (plugin.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name
    pub name: String,
    /// Version
    #[serde(default)]
    pub version: Option<String>,
    /// Description
    #[serde(default)]
    pub description: Option<String>,
    /// Author
    #[serde(default)]
    pub author: Option<PluginAuthor>,
    /// Homepage URL
    #[serde(default)]
    pub homepage: Option<String>,
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
    /// License
    #[serde(default)]
    pub license: Option<String>,
    /// Keywords
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
}

/// Installation scope
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallScope {
    /// User scope (~/.claude/settings.json)
    #[default]
    User,
    /// Project scope (.claude/settings.json)
    Project,
    /// Local scope (.claude/settings.local.json)
    Local,
}

impl std::fmt::Display for InstallScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
            Self::Local => write!(f, "local"),
        }
    }
}

/// Installed plugin information (from installed_plugins.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    /// Installation scope
    pub scope: InstallScope,
    /// Installation path
    #[serde(rename = "installPath")]
    pub install_path: String,
    /// Version
    pub version: String,
    /// Installation timestamp
    #[serde(rename = "installedAt")]
    pub installed_at: String,
    /// Last updated timestamp
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
    /// Git commit SHA (if available)
    #[serde(default, rename = "gitCommitSha")]
    pub git_commit_sha: Option<String>,
}

/// installed_plugins.json structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginsFile {
    /// File version
    pub version: u32,
    /// Plugins map: "plugin-name@marketplace" -> [InstalledPlugin]
    pub plugins: HashMap<String, Vec<InstalledPlugin>>,
}

impl Default for InstalledPluginsFile {
    fn default() -> Self {
        Self {
            version: 2,
            plugins: HashMap::new(),
        }
    }
}

/// Known marketplace entry (from known_marketplaces.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMarketplace {
    /// Source specification
    pub source: KnownMarketplaceSource,
    /// Local installation path
    #[serde(rename = "installLocation")]
    pub install_location: String,
    /// Last updated timestamp
    #[serde(rename = "lastUpdated")]
    pub last_updated: String,
}

/// Source specification for known marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownMarketplaceSource {
    /// Source type: "github", "url", "local"
    pub source: String,
    /// GitHub repository (for "github" source)
    #[serde(default)]
    pub repo: Option<String>,
    /// URL (for "url" source)
    #[serde(default)]
    pub url: Option<String>,
    /// Local path (for "local" source)
    #[serde(default)]
    pub path: Option<String>,
}

/// known_marketplaces.json structure
pub type KnownMarketplacesFile = HashMap<String, KnownMarketplace>;

/// Plugin with full resolved information
#[derive(Debug, Clone)]
pub struct ResolvedPlugin {
    /// Plugin name
    pub name: String,
    /// Marketplace name
    pub marketplace: String,
    /// Full identifier (name@marketplace)
    pub full_id: String,
    /// Version
    pub version: Option<String>,
    /// Description
    pub description: Option<String>,
    /// Author
    pub author: Option<PluginAuthor>,
    /// Category
    pub category: Option<String>,
    /// Tags
    pub tags: Vec<String>,
    /// Local cache path (if installed)
    pub cache_path: Option<PathBuf>,
    /// Installation info (if installed)
    pub installed: Option<InstalledPlugin>,
}

impl ResolvedPlugin {
    /// Create from plugin entry and marketplace name
    pub fn from_entry(entry: &PluginEntry, marketplace: &str) -> Self {
        Self {
            name: entry.name.clone(),
            marketplace: marketplace.to_string(),
            full_id: format!("{}@{}", entry.name, marketplace),
            version: entry.version.clone(),
            description: entry.description.clone(),
            author: entry.author.clone(),
            category: entry.category.clone(),
            tags: entry.tags.clone().unwrap_or_default(),
            cache_path: None,
            installed: None,
        }
    }

    /// Check if this plugin is installed
    pub fn is_installed(&self) -> bool {
        self.installed.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_marketplace_json() {
        let json = r#"{
            "name": "test-marketplace",
            "owner": { "name": "Test Owner" },
            "plugins": [
                {
                    "name": "test-plugin",
                    "source": "./plugins/test-plugin",
                    "description": "A test plugin"
                }
            ]
        }"#;

        let marketplace: Marketplace = serde_json::from_str(json).unwrap();
        assert_eq!(marketplace.name, "test-marketplace");
        assert_eq!(marketplace.owner.name, "Test Owner");
        assert_eq!(marketplace.plugins.len(), 1);
        assert_eq!(marketplace.plugins[0].name, "test-plugin");
    }

    #[test]
    fn test_parse_plugin_source_relative() {
        let json = r#""./plugins/my-plugin""#;
        let source: PluginSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, PluginSource::Relative(_)));
    }

    #[test]
    fn test_parse_plugin_source_structured() {
        let json = r#"{"source": "github", "repo": "user/repo"}"#;
        let source: PluginSource = serde_json::from_str(json).unwrap();
        assert!(matches!(source, PluginSource::Structured(_)));
    }

    #[test]
    fn test_installed_plugins_file() {
        let json = r#"{
            "version": 2,
            "plugins": {
                "test@marketplace": [{
                    "scope": "user",
                    "installPath": "/path/to/plugin",
                    "version": "1.0.0",
                    "installedAt": "2025-01-01T00:00:00Z",
                    "lastUpdated": "2025-01-01T00:00:00Z"
                }]
            }
        }"#;

        let file: InstalledPluginsFile = serde_json::from_str(json).unwrap();
        assert_eq!(file.version, 2);
        assert!(file.plugins.contains_key("test@marketplace"));
    }

    #[test]
    fn test_known_marketplace() {
        let json = r#"{
            "source": {
                "source": "github",
                "repo": "anthropics/claude-plugins-official"
            },
            "installLocation": "/path/to/marketplace",
            "lastUpdated": "2025-01-01T00:00:00Z"
        }"#;

        let km: KnownMarketplace = serde_json::from_str(json).unwrap();
        assert_eq!(km.source.source, "github");
        assert_eq!(
            km.source.repo,
            Some("anthropics/claude-plugins-official".to_string())
        );
    }

    #[test]
    fn test_resolved_plugin() {
        let entry = PluginEntry {
            name: "test-plugin".to_string(),
            source: PluginSource::Relative("./plugins/test".to_string()),
            description: Some("Test".to_string()),
            version: Some("1.0.0".to_string()),
            author: None,
            category: Some("dev".to_string()),
            tags: Some(vec!["rust".to_string()]),
            keywords: None,
            strict: true,
        };

        let resolved = ResolvedPlugin::from_entry(&entry, "my-marketplace");
        assert_eq!(resolved.full_id, "test-plugin@my-marketplace");
        assert!(!resolved.is_installed());
    }
}
