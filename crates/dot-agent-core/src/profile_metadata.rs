//! Profile Metadata Management
//!
//! # Files
//!
//! - `~/.dot-agent/profiles.toml` - Profile index (all profiles)
//! - `~/.dot-agent/profiles/<name>/.dot-agent.toml` - Per-profile metadata

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};

const PROFILES_INDEX_FILE: &str = "profiles.toml";
const PROFILE_METADATA_FILE: &str = ".dot-agent.toml";

/// Profile source specification
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ProfileSource {
    /// Locally created profile
    Local,

    /// Imported from Git repository
    Git {
        url: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        commit: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
    },

    /// Imported from Claude Code Plugin Marketplace
    Marketplace {
        channel: String,
        plugin: String,
        version: String,
    },
}

impl Default for ProfileSource {
    fn default() -> Self {
        Self::Local
    }
}

/// Profile entry in profiles.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileIndexEntry {
    /// Relative path from base_dir (e.g., "profiles/my-profile")
    pub path: String,

    /// Source information
    pub source: ProfileSource,

    /// Creation timestamp
    pub created_at: String,

    /// Last update timestamp
    pub updated_at: String,
}

impl ProfileIndexEntry {
    /// Create a new local profile entry
    pub fn new_local(name: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            path: format!("profiles/{}", name),
            source: ProfileSource::Local,
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Create a new git profile entry
    pub fn new_git(
        name: &str,
        url: &str,
        branch: Option<&str>,
        commit: Option<&str>,
        path: Option<&str>,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            path: format!("profiles/{}", name),
            source: ProfileSource::Git {
                url: url.to_string(),
                branch: branch.map(|s| s.to_string()),
                commit: commit.map(|s| s.to_string()),
                path: path.map(|s| s.to_string()),
            },
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Create a new marketplace profile entry
    pub fn new_marketplace(name: &str, channel: &str, plugin: &str, version: &str) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            path: format!("profiles/{}", name),
            source: ProfileSource::Marketplace {
                channel: channel.to_string(),
                plugin: plugin.to_string(),
                version: version.to_string(),
            },
            created_at: now.clone(),
            updated_at: now,
        }
    }

    /// Update timestamp
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

/// profiles.toml structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfilesIndex {
    /// File format version
    #[serde(default = "default_version")]
    pub version: u32,

    /// Profiles map: name -> entry
    #[serde(default)]
    pub profiles: HashMap<String, ProfileIndexEntry>,
}

fn default_version() -> u32 {
    1
}

impl Default for ProfilesIndex {
    fn default() -> Self {
        Self {
            version: 1,
            profiles: HashMap::new(),
        }
    }
}

impl ProfilesIndex {
    /// Load from file, creating default if not exists
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(PROFILES_INDEX_FILE);

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)?;
        let index: Self = toml::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        Ok(index)
    }

    /// Save to file
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(PROFILES_INDEX_FILE);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        fs::write(&path, content)?;
        Ok(())
    }

    /// Add or update a profile entry
    pub fn upsert(&mut self, name: &str, entry: ProfileIndexEntry) {
        self.profiles.insert(name.to_string(), entry);
    }

    /// Remove a profile entry
    pub fn remove(&mut self, name: &str) -> Option<ProfileIndexEntry> {
        self.profiles.remove(name)
    }

    /// Get a profile entry
    pub fn get(&self, name: &str) -> Option<&ProfileIndexEntry> {
        self.profiles.get(name)
    }

    /// Check if profile exists
    pub fn contains(&self, name: &str) -> bool {
        self.profiles.contains_key(name)
    }

    /// List all profile names
    pub fn names(&self) -> Vec<&str> {
        let mut names: Vec<_> = self.profiles.keys().map(|s| s.as_str()).collect();
        names.sort();
        names
    }
}

/// Plugin configuration in profile metadata
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginConfig {
    /// Whether plugin features are enabled (hooks, MCP, LSP)
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Installation scope (user, project, local)
    #[serde(default)]
    pub scope: PluginScope,
}

fn default_true() -> bool {
    true
}

/// Plugin installation scope
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PluginScope {
    #[default]
    User,
    Project,
    Local,
}

impl std::fmt::Display for PluginScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::User => write!(f, "user"),
            Self::Project => write!(f, "project"),
            Self::Local => write!(f, "local"),
        }
    }
}

/// .dot-agent.toml structure (per-profile metadata)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileMetadata {
    /// Profile section
    pub profile: ProfileInfo,

    /// Source information
    #[serde(default)]
    pub source: ProfileSource,

    /// Plugin configuration
    #[serde(default)]
    pub plugin: PluginConfig,
}

/// Profile info section in .dot-agent.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileInfo {
    /// Profile name
    pub name: String,

    /// Version
    #[serde(default)]
    pub version: Option<String>,

    /// Description
    #[serde(default)]
    pub description: Option<String>,

    /// Author
    #[serde(default)]
    pub author: Option<String>,
}

impl ProfileMetadata {
    /// Create new local profile metadata
    pub fn new_local(name: &str) -> Self {
        Self {
            profile: ProfileInfo {
                name: name.to_string(),
                version: Some("0.1.0".to_string()),
                description: None,
                author: None,
            },
            source: ProfileSource::Local,
            plugin: PluginConfig::default(),
        }
    }

    /// Create new git profile metadata
    pub fn new_git(
        name: &str,
        url: &str,
        branch: Option<&str>,
        commit: Option<&str>,
        path: Option<&str>,
    ) -> Self {
        Self {
            profile: ProfileInfo {
                name: name.to_string(),
                version: None,
                description: None,
                author: None,
            },
            source: ProfileSource::Git {
                url: url.to_string(),
                branch: branch.map(|s| s.to_string()),
                commit: commit.map(|s| s.to_string()),
                path: path.map(|s| s.to_string()),
            },
            plugin: PluginConfig::default(),
        }
    }

    /// Create new marketplace profile metadata
    pub fn new_marketplace(name: &str, channel: &str, plugin: &str, version: &str) -> Self {
        Self {
            profile: ProfileInfo {
                name: name.to_string(),
                version: Some(version.to_string()),
                description: None,
                author: None,
            },
            source: ProfileSource::Marketplace {
                channel: channel.to_string(),
                plugin: plugin.to_string(),
                version: version.to_string(),
            },
            plugin: PluginConfig::default(),
        }
    }

    /// Load from profile directory
    pub fn load(profile_dir: &Path) -> Result<Option<Self>> {
        let path = profile_dir.join(PROFILE_METADATA_FILE);

        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)?;
        let metadata: Self = toml::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        Ok(Some(metadata))
    }

    /// Save to profile directory
    pub fn save(&self, profile_dir: &Path) -> Result<()> {
        let path = profile_dir.join(PROFILE_METADATA_FILE);

        let content = toml::to_string_pretty(self).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

        fs::write(&path, content)?;
        Ok(())
    }

    /// Check if profile has plugin features (hooks, MCP, LSP)
    pub fn has_plugin_features(profile_dir: &Path) -> bool {
        let hooks_dir = profile_dir.join("hooks");
        let mcp_file = profile_dir.join(".mcp.json");
        let lsp_file = profile_dir.join(".lsp.json");

        (hooks_dir.exists() && hooks_dir.is_dir())
            || mcp_file.exists()
            || lsp_file.exists()
    }
}

/// Migration helper: scan existing profiles and create index
pub fn migrate_existing_profiles(base_dir: &Path) -> Result<ProfilesIndex> {
    let profiles_dir = base_dir.join("profiles");
    let mut index = ProfilesIndex::default();

    if !profiles_dir.exists() {
        return Ok(index);
    }

    for entry in fs::read_dir(&profiles_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| DotAgentError::InvalidProfileName {
                name: path.display().to_string(),
            })?;

        // Check if .dot-agent.toml exists
        if let Some(metadata) = ProfileMetadata::load(&path)? {
            // Use existing metadata
            let entry = match &metadata.source {
                ProfileSource::Local => ProfileIndexEntry::new_local(name),
                ProfileSource::Git {
                    url,
                    branch,
                    commit,
                    path,
                } => ProfileIndexEntry::new_git(
                    name,
                    url,
                    branch.as_deref(),
                    commit.as_deref(),
                    path.as_deref(),
                ),
                ProfileSource::Marketplace {
                    channel,
                    plugin,
                    version,
                } => ProfileIndexEntry::new_marketplace(name, channel, plugin, version),
            };
            index.upsert(name, entry);
        } else {
            // Create default local entry and metadata
            let entry = ProfileIndexEntry::new_local(name);
            let metadata = ProfileMetadata::new_local(name);
            metadata.save(&path)?;
            index.upsert(name, entry);
        }
    }

    index.save(base_dir)?;
    Ok(index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn profile_source_local_serialization() {
        let source = ProfileSource::Local;
        let toml = toml::to_string(&source).unwrap();
        assert!(toml.contains("type = \"local\""));
    }

    #[test]
    fn profile_source_git_serialization() {
        let source = ProfileSource::Git {
            url: "https://github.com/user/repo".to_string(),
            branch: Some("main".to_string()),
            commit: Some("abc123".to_string()),
            path: None,
        };
        let toml = toml::to_string(&source).unwrap();
        assert!(toml.contains("type = \"git\""));
        assert!(toml.contains("url = "));
        assert!(toml.contains("branch = "));
    }

    #[test]
    fn profile_source_marketplace_serialization() {
        let source = ProfileSource::Marketplace {
            channel: "claude-official".to_string(),
            plugin: "rust-lsp".to_string(),
            version: "1.0.0".to_string(),
        };
        let toml = toml::to_string(&source).unwrap();
        assert!(toml.contains("type = \"marketplace\""));
        assert!(toml.contains("channel = "));
    }

    #[test]
    fn profiles_index_save_load() {
        let tmp = TempDir::new().unwrap();
        let base_dir = tmp.path();

        let mut index = ProfilesIndex::default();
        index.upsert("test-profile", ProfileIndexEntry::new_local("test-profile"));

        index.save(base_dir).unwrap();

        let loaded = ProfilesIndex::load(base_dir).unwrap();
        assert!(loaded.contains("test-profile"));
    }

    #[test]
    fn profile_metadata_save_load() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path();

        let metadata = ProfileMetadata::new_local("test");
        metadata.save(profile_dir).unwrap();

        let loaded = ProfileMetadata::load(profile_dir).unwrap();
        assert!(loaded.is_some());
        assert_eq!(loaded.unwrap().profile.name, "test");
    }

    #[test]
    fn profile_metadata_git() {
        let metadata = ProfileMetadata::new_git(
            "dotfiles",
            "https://github.com/user/dotfiles",
            Some("main"),
            Some("abc123"),
            None,
        );

        assert!(matches!(metadata.source, ProfileSource::Git { .. }));
    }

    #[test]
    fn profile_metadata_marketplace() {
        let metadata = ProfileMetadata::new_marketplace(
            "rust-lsp",
            "claude-official",
            "rust-lsp",
            "1.0.0",
        );

        assert!(matches!(metadata.source, ProfileSource::Marketplace { .. }));
    }

    #[test]
    fn has_plugin_features_detects_hooks() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path();

        // No hooks initially
        assert!(!ProfileMetadata::has_plugin_features(profile_dir));

        // Create hooks directory
        fs::create_dir(profile_dir.join("hooks")).unwrap();
        assert!(ProfileMetadata::has_plugin_features(profile_dir));
    }

    #[test]
    fn has_plugin_features_detects_mcp() {
        let tmp = TempDir::new().unwrap();
        let profile_dir = tmp.path();

        // Create .mcp.json
        fs::write(profile_dir.join(".mcp.json"), "{}").unwrap();
        assert!(ProfileMetadata::has_plugin_features(profile_dir));
    }
}
