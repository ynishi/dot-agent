//! Channel registry management
//!
//! Manages the list of registered Channels in ~/.dot-agent/channels.toml

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};

use super::types::{Channel, ChannelType};

/// Channel registry configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRegistry {
    /// Registered channels
    pub channels: Vec<Channel>,
}

impl Default for ChannelRegistry {
    fn default() -> Self {
        Self::with_defaults()
    }
}

impl ChannelRegistry {
    const FILENAME: &'static str = "channels.toml";

    /// Create registry with default channels (github-global)
    pub fn with_defaults() -> Self {
        Self {
            channels: vec![Channel::github_global()],
        }
    }

    /// Load channel registry from base directory
    pub fn load(base_dir: &Path) -> Result<Self> {
        let path = base_dir.join(Self::FILENAME);
        if !path.exists() {
            return Ok(Self::with_defaults());
        }

        let content = fs::read_to_string(&path)?;
        let mut registry: Self =
            toml::from_str(&content).map_err(|e| DotAgentError::ConfigParseSimple {
                message: e.to_string(),
            })?;

        // Ensure github-global is always present
        if !registry
            .channels
            .iter()
            .any(|c| c.builtin && c.name == "github")
        {
            registry.channels.insert(0, Channel::github_global());
        }

        Ok(registry)
    }

    /// Save channel registry to base directory
    pub fn save(&self, base_dir: &Path) -> Result<()> {
        let path = base_dir.join(Self::FILENAME);
        fs::create_dir_all(base_dir)?;
        let content =
            toml::to_string_pretty(self).map_err(|e| DotAgentError::ConfigParseSimple {
                message: e.to_string(),
            })?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Add a channel
    pub fn add(&mut self, channel: Channel) -> Result<()> {
        if self.channels.iter().any(|c| c.name == channel.name) {
            return Err(DotAgentError::ChannelAlreadyExists { name: channel.name });
        }
        self.channels.push(channel);
        Ok(())
    }

    /// Remove a channel by name (cannot remove builtin channels)
    pub fn remove(&mut self, name: &str) -> Result<Channel> {
        let idx = self
            .channels
            .iter()
            .position(|c| c.name == name)
            .ok_or_else(|| DotAgentError::ChannelNotFound {
                name: name.to_string(),
            })?;

        if self.channels[idx].builtin {
            return Err(DotAgentError::CannotRemoveBuiltinChannel {
                name: name.to_string(),
            });
        }

        Ok(self.channels.remove(idx))
    }

    /// Get a channel by name
    pub fn get(&self, name: &str) -> Option<&Channel> {
        self.channels.iter().find(|c| c.name == name)
    }

    /// Get a mutable reference to a channel by name
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Channel> {
        self.channels.iter_mut().find(|c| c.name == name)
    }

    /// List all channels
    pub fn list(&self) -> &[Channel] {
        &self.channels
    }

    /// List enabled channels
    pub fn list_enabled(&self) -> Vec<&Channel> {
        self.channels.iter().filter(|c| c.enabled).collect()
    }

    /// List searchable channels (enabled and supports search)
    pub fn list_searchable(&self) -> Vec<&Channel> {
        self.channels
            .iter()
            .filter(|c| c.enabled && c.is_searchable())
            .collect()
    }

    /// List channels by type
    pub fn list_by_type(&self, channel_type: ChannelType) -> Vec<&Channel> {
        self.channels
            .iter()
            .filter(|c| c.channel_type == channel_type)
            .collect()
    }

    /// Enable a channel
    pub fn enable(&mut self, name: &str) -> Result<()> {
        let channel = self
            .get_mut(name)
            .ok_or_else(|| DotAgentError::ChannelNotFound {
                name: name.to_string(),
            })?;
        channel.enabled = true;
        Ok(())
    }

    /// Disable a channel
    pub fn disable(&mut self, name: &str) -> Result<()> {
        let channel = self
            .get_mut(name)
            .ok_or_else(|| DotAgentError::ChannelNotFound {
                name: name.to_string(),
            })?;
        channel.enabled = false;
        Ok(())
    }

    /// Get cache directory for a channel
    pub fn cache_dir(base_dir: &Path, channel_name: &str) -> PathBuf {
        base_dir.join("cache").join("channels").join(channel_name)
    }

    /// Check if a channel exists
    pub fn contains(&self, name: &str) -> bool {
        self.channels.iter().any(|c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn registry_with_defaults() {
        let registry = ChannelRegistry::with_defaults();
        assert_eq!(registry.channels.len(), 1);
        assert_eq!(registry.channels[0].name, "github");
        assert!(registry.channels[0].builtin);
    }

    #[test]
    fn registry_add_remove() {
        let mut registry = ChannelRegistry::with_defaults();

        let channel = Channel::from_url("test", "https://github.com/test/awesome");
        registry.add(channel).unwrap();
        assert_eq!(registry.channels.len(), 2);

        // Duplicate should fail
        let channel2 = Channel::from_url("test", "https://github.com/other/awesome");
        assert!(registry.add(channel2).is_err());

        // Remove
        let removed = registry.remove("test").unwrap();
        assert_eq!(removed.name, "test");
        assert_eq!(registry.channels.len(), 1);

        // Cannot remove builtin
        assert!(registry.remove("github").is_err());
    }

    #[test]
    fn registry_enable_disable() {
        let mut registry = ChannelRegistry::with_defaults();
        registry
            .add(Channel::from_url("test", "https://example.com"))
            .unwrap();

        assert!(registry.get("test").unwrap().enabled);

        registry.disable("test").unwrap();
        assert!(!registry.get("test").unwrap().enabled);

        registry.enable("test").unwrap();
        assert!(registry.get("test").unwrap().enabled);
    }

    #[test]
    fn registry_save_load() {
        let temp = TempDir::new().unwrap();
        let base = temp.path();

        let mut registry = ChannelRegistry::with_defaults();
        registry
            .add(Channel::from_url("test", "https://github.com/test/awesome"))
            .unwrap();
        registry.save(base).unwrap();

        let loaded = ChannelRegistry::load(base).unwrap();
        assert_eq!(loaded.channels.len(), 2);
        assert!(loaded.get("github").is_some());
        assert!(loaded.get("test").is_some());
    }

    #[test]
    fn list_searchable() {
        let mut registry = ChannelRegistry::with_defaults();
        registry
            .add(Channel::from_url(
                "awesome1",
                "https://github.com/test/awesome-list",
            ))
            .unwrap();
        registry
            .add(Channel::from_url(
                "direct1",
                "https://github.com/test/dotfiles",
            ))
            .unwrap();

        let searchable = registry.list_searchable();
        // github-global and awesome1 are searchable
        assert_eq!(searchable.len(), 2);
    }

    #[test]
    fn list_by_type() {
        let mut registry = ChannelRegistry::with_defaults();
        registry
            .add(Channel::from_url(
                "awesome1",
                "https://github.com/test/awesome-list",
            ))
            .unwrap();
        registry
            .add(Channel::from_hub("hub1", "official", "test"))
            .unwrap();

        let github = registry.list_by_type(ChannelType::GitHubGlobal);
        assert_eq!(github.len(), 1);

        let awesome = registry.list_by_type(ChannelType::AwesomeList);
        assert_eq!(awesome.len(), 1);

        let hub = registry.list_by_type(ChannelType::Hub);
        assert_eq!(hub.len(), 1);
    }
}
