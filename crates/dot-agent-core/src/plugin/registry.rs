//! Claude Code Plugin Registry
//!
//! Manages known_marketplaces.json and installed_plugins.json

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{DotAgentError, Result};
use crate::plugin::types::{
    InstallScope, InstalledPlugin, InstalledPluginsFile, KnownMarketplace, KnownMarketplaceSource,
    KnownMarketplacesFile,
};

const KNOWN_MARKETPLACES_FILE: &str = "known_marketplaces.json";
const INSTALLED_PLUGINS_FILE: &str = "installed_plugins.json";

/// Plugin Registry - manages Claude Code plugin state
pub struct PluginRegistry {
    /// Base directory (~/.claude/plugins)
    plugins_dir: PathBuf,
}

impl PluginRegistry {
    /// Create a new PluginRegistry
    pub fn new() -> Result<Self> {
        let home = dirs::home_dir().ok_or(DotAgentError::HomeNotFound)?;
        let plugins_dir = home.join(".claude").join("plugins");

        Ok(Self { plugins_dir })
    }

    /// Create with custom plugins directory (for testing)
    pub fn with_dir(plugins_dir: PathBuf) -> Self {
        Self { plugins_dir }
    }

    /// Get the plugins directory path
    pub fn plugins_dir(&self) -> &Path {
        &self.plugins_dir
    }

    /// Get the marketplaces directory path
    pub fn marketplaces_dir(&self) -> PathBuf {
        self.plugins_dir.join("marketplaces")
    }

    /// Get the cache directory path
    pub fn cache_dir(&self) -> PathBuf {
        self.plugins_dir.join("cache")
    }

    // ========== Known Marketplaces ==========

    /// Load known_marketplaces.json
    pub fn load_known_marketplaces(&self) -> Result<KnownMarketplacesFile> {
        let path = self.plugins_dir.join(KNOWN_MARKETPLACES_FILE);

        if !path.exists() {
            return Ok(KnownMarketplacesFile::new());
        }

        let content = fs::read_to_string(&path)?;
        let file: KnownMarketplacesFile =
            serde_json::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
                path: path.clone(),
                message: e.to_string(),
            })?;

        Ok(file)
    }

    /// Save known_marketplaces.json
    pub fn save_known_marketplaces(&self, file: &KnownMarketplacesFile) -> Result<()> {
        fs::create_dir_all(&self.plugins_dir)?;

        let path = self.plugins_dir.join(KNOWN_MARKETPLACES_FILE);
        let content =
            serde_json::to_string_pretty(file).map_err(|e| DotAgentError::ConfigParse {
                path: path.clone(),
                message: e.to_string(),
            })?;

        fs::write(&path, content)?;
        Ok(())
    }

    /// Add a marketplace from GitHub
    pub fn add_marketplace_github(&self, name: &str, repo: &str) -> Result<KnownMarketplace> {
        let mut marketplaces = self.load_known_marketplaces()?;

        if marketplaces.contains_key(name) {
            return Err(DotAgentError::MarketplaceAlreadyExists {
                name: name.to_string(),
            });
        }

        let install_location = self.marketplaces_dir().join(name);
        let marketplace = KnownMarketplace {
            source: KnownMarketplaceSource {
                source: "github".to_string(),
                repo: Some(repo.to_string()),
                url: None,
                path: None,
            },
            install_location: install_location.to_string_lossy().to_string(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        };

        marketplaces.insert(name.to_string(), marketplace.clone());
        self.save_known_marketplaces(&marketplaces)?;

        Ok(marketplace)
    }

    /// Add a marketplace from URL
    pub fn add_marketplace_url(&self, name: &str, url: &str) -> Result<KnownMarketplace> {
        let mut marketplaces = self.load_known_marketplaces()?;

        if marketplaces.contains_key(name) {
            return Err(DotAgentError::MarketplaceAlreadyExists {
                name: name.to_string(),
            });
        }

        let install_location = self.marketplaces_dir().join(name);
        let marketplace = KnownMarketplace {
            source: KnownMarketplaceSource {
                source: "url".to_string(),
                repo: None,
                url: Some(url.to_string()),
                path: None,
            },
            install_location: install_location.to_string_lossy().to_string(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        };

        marketplaces.insert(name.to_string(), marketplace.clone());
        self.save_known_marketplaces(&marketplaces)?;

        Ok(marketplace)
    }

    /// Add a marketplace from local path
    pub fn add_marketplace_local(&self, name: &str, path: &str) -> Result<KnownMarketplace> {
        let mut marketplaces = self.load_known_marketplaces()?;

        if marketplaces.contains_key(name) {
            return Err(DotAgentError::MarketplaceAlreadyExists {
                name: name.to_string(),
            });
        }

        let install_location = self.marketplaces_dir().join(name);
        let marketplace = KnownMarketplace {
            source: KnownMarketplaceSource {
                source: "local".to_string(),
                repo: None,
                url: None,
                path: Some(path.to_string()),
            },
            install_location: install_location.to_string_lossy().to_string(),
            last_updated: chrono::Utc::now().to_rfc3339(),
        };

        marketplaces.insert(name.to_string(), marketplace.clone());
        self.save_known_marketplaces(&marketplaces)?;

        Ok(marketplace)
    }

    /// Remove a marketplace
    pub fn remove_marketplace(&self, name: &str) -> Result<()> {
        let mut marketplaces = self.load_known_marketplaces()?;

        if !marketplaces.contains_key(name) {
            return Err(DotAgentError::ProfileNotFound {
                name: name.to_string(),
            });
        }

        // Remove from JSON
        marketplaces.remove(name);
        self.save_known_marketplaces(&marketplaces)?;

        // Remove cached marketplace data
        let marketplace_path = self.marketplaces_dir().join(name);
        if marketplace_path.exists() {
            fs::remove_dir_all(&marketplace_path)?;
        }

        Ok(())
    }

    /// Get a marketplace by name
    pub fn get_marketplace(&self, name: &str) -> Result<Option<KnownMarketplace>> {
        let marketplaces = self.load_known_marketplaces()?;
        Ok(marketplaces.get(name).cloned())
    }

    /// List all known marketplaces
    pub fn list_marketplaces(&self) -> Result<Vec<(String, KnownMarketplace)>> {
        let marketplaces = self.load_known_marketplaces()?;
        Ok(marketplaces.into_iter().collect())
    }

    // ========== Installed Plugins ==========

    /// Load installed_plugins.json
    pub fn load_installed_plugins(&self) -> Result<InstalledPluginsFile> {
        let path = self.plugins_dir.join(INSTALLED_PLUGINS_FILE);

        if !path.exists() {
            return Ok(InstalledPluginsFile::default());
        }

        let content = fs::read_to_string(&path)?;
        let file: InstalledPluginsFile =
            serde_json::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
                path: path.clone(),
                message: e.to_string(),
            })?;

        Ok(file)
    }

    /// Save installed_plugins.json
    pub fn save_installed_plugins(&self, file: &InstalledPluginsFile) -> Result<()> {
        fs::create_dir_all(&self.plugins_dir)?;

        let path = self.plugins_dir.join(INSTALLED_PLUGINS_FILE);
        let content =
            serde_json::to_string_pretty(file).map_err(|e| DotAgentError::ConfigParse {
                path: path.clone(),
                message: e.to_string(),
            })?;

        fs::write(&path, content)?;
        Ok(())
    }

    /// Add an installed plugin
    pub fn add_installed_plugin(
        &self,
        plugin_name: &str,
        marketplace: &str,
        install_path: &Path,
        version: &str,
        scope: InstallScope,
    ) -> Result<InstalledPlugin> {
        let mut installed = self.load_installed_plugins()?;
        let key = format!("{}@{}", plugin_name, marketplace);

        let plugin = InstalledPlugin {
            scope,
            install_path: install_path.to_string_lossy().to_string(),
            version: version.to_string(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            last_updated: chrono::Utc::now().to_rfc3339(),
            git_commit_sha: None,
        };

        // Add or update plugin entry
        installed
            .plugins
            .entry(key)
            .or_insert_with(Vec::new)
            .push(plugin.clone());

        self.save_installed_plugins(&installed)?;

        Ok(plugin)
    }

    /// Remove an installed plugin
    pub fn remove_installed_plugin(&self, plugin_name: &str, marketplace: &str) -> Result<()> {
        let mut installed = self.load_installed_plugins()?;
        let key = format!("{}@{}", plugin_name, marketplace);

        if !installed.plugins.contains_key(&key) {
            return Err(DotAgentError::ProfileNotFound { name: key });
        }

        installed.plugins.remove(&key);
        self.save_installed_plugins(&installed)?;

        // Remove cached plugin files
        let cache_path = self.cache_dir().join(marketplace).join(plugin_name);
        if cache_path.exists() {
            fs::remove_dir_all(&cache_path)?;
        }

        Ok(())
    }

    /// Get installed plugin by name and marketplace
    pub fn get_installed_plugin(
        &self,
        plugin_name: &str,
        marketplace: &str,
    ) -> Result<Option<InstalledPlugin>> {
        let installed = self.load_installed_plugins()?;
        let key = format!("{}@{}", plugin_name, marketplace);

        Ok(installed.plugins.get(&key).and_then(|v| v.first()).cloned())
    }

    /// List all installed plugins
    pub fn list_installed_plugins(&self) -> Result<Vec<(String, InstalledPlugin)>> {
        let installed = self.load_installed_plugins()?;
        let mut result = Vec::new();

        for (key, plugins) in installed.plugins {
            for plugin in plugins {
                result.push((key.clone(), plugin));
            }
        }

        Ok(result)
    }

    /// Check if a plugin is installed
    pub fn is_plugin_installed(&self, plugin_name: &str, marketplace: &str) -> Result<bool> {
        let installed = self.load_installed_plugins()?;
        let key = format!("{}@{}", plugin_name, marketplace);
        Ok(installed.plugins.contains_key(&key))
    }
}

// Note: Default impl removed - use PluginRegistry::new() which returns Result

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_registry() -> (PluginRegistry, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let registry = PluginRegistry::with_dir(temp_dir.path().to_path_buf());
        (registry, temp_dir)
    }

    #[test]
    fn test_add_marketplace_github() {
        let (registry, _temp) = create_test_registry();

        let result = registry.add_marketplace_github("test-market", "user/repo");
        assert!(result.is_ok());

        let marketplace = result.unwrap();
        assert_eq!(marketplace.source.source, "github");
        assert_eq!(marketplace.source.repo, Some("user/repo".to_string()));

        // Verify it's saved
        let marketplaces = registry.load_known_marketplaces().unwrap();
        assert!(marketplaces.contains_key("test-market"));
    }

    #[test]
    fn test_add_duplicate_marketplace() {
        let (registry, _temp) = create_test_registry();

        registry
            .add_marketplace_github("test-market", "user/repo")
            .unwrap();
        let result = registry.add_marketplace_github("test-market", "user/other");

        assert!(result.is_err());
    }

    #[test]
    fn test_remove_marketplace() {
        let (registry, _temp) = create_test_registry();

        registry
            .add_marketplace_github("test-market", "user/repo")
            .unwrap();
        registry.remove_marketplace("test-market").unwrap();

        let marketplaces = registry.load_known_marketplaces().unwrap();
        assert!(!marketplaces.contains_key("test-market"));
    }

    #[test]
    fn test_list_marketplaces() {
        let (registry, _temp) = create_test_registry();

        registry
            .add_marketplace_github("market1", "user/repo1")
            .unwrap();
        registry
            .add_marketplace_github("market2", "user/repo2")
            .unwrap();

        let list = registry.list_marketplaces().unwrap();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_add_installed_plugin() {
        let (registry, temp) = create_test_registry();

        let install_path = temp.path().join("cache/market/plugin/1.0.0");
        let result = registry.add_installed_plugin(
            "test-plugin",
            "test-market",
            &install_path,
            "1.0.0",
            InstallScope::User,
        );

        assert!(result.is_ok());

        let installed = registry
            .get_installed_plugin("test-plugin", "test-market")
            .unwrap();
        assert!(installed.is_some());
        assert_eq!(installed.unwrap().version, "1.0.0");
    }

    #[test]
    fn test_is_plugin_installed() {
        let (registry, temp) = create_test_registry();

        assert!(!registry.is_plugin_installed("plugin", "market").unwrap());

        let install_path = temp.path().join("cache/market/plugin/1.0.0");
        registry
            .add_installed_plugin(
                "plugin",
                "market",
                &install_path,
                "1.0.0",
                InstallScope::User,
            )
            .unwrap();

        assert!(registry.is_plugin_installed("plugin", "market").unwrap());
    }
}
