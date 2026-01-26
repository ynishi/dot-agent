//! Marketplace Parser
//!
//! Parses Claude Code Plugin marketplace.json files

use std::fs;
use std::path::Path;

use crate::error::{DotAgentError, Result};
use crate::plugin::types::{Marketplace, PluginEntry, ResolvedPlugin};

const MARKETPLACE_FILE: &str = ".claude-plugin/marketplace.json";

/// Parse marketplace.json from a directory
pub fn parse_marketplace(marketplace_dir: &Path) -> Result<Marketplace> {
    let path = marketplace_dir.join(MARKETPLACE_FILE);

    if !path.exists() {
        return Err(DotAgentError::FileNotFound { path });
    }

    let content = fs::read_to_string(&path)?;
    let marketplace: Marketplace =
        serde_json::from_str(&content).map_err(|e| DotAgentError::ConfigParse {
            path: path.clone(),
            message: e.to_string(),
        })?;

    Ok(marketplace)
}

/// Parse marketplace.json from a string
pub fn parse_marketplace_str(content: &str) -> Result<Marketplace> {
    let marketplace: Marketplace =
        serde_json::from_str(content).map_err(|e| DotAgentError::ConfigParse {
            path: std::path::PathBuf::from("<string>"),
            message: e.to_string(),
        })?;

    Ok(marketplace)
}

/// Search plugins in a marketplace
pub fn search_plugins(marketplace: &Marketplace, query: &str) -> Vec<ResolvedPlugin> {
    let query_lower = query.to_lowercase();

    marketplace
        .plugins
        .iter()
        .filter(|p| matches_query(p, &query_lower))
        .map(|p| ResolvedPlugin::from_entry(p, &marketplace.name))
        .collect()
}

/// Check if a plugin matches a search query
fn matches_query(plugin: &PluginEntry, query: &str) -> bool {
    // Match name
    if plugin.name.to_lowercase().contains(query) {
        return true;
    }

    // Match description
    if let Some(desc) = &plugin.description {
        if desc.to_lowercase().contains(query) {
            return true;
        }
    }

    // Match category
    if let Some(cat) = &plugin.category {
        if cat.to_lowercase().contains(query) {
            return true;
        }
    }

    // Match tags
    if let Some(tags) = &plugin.tags {
        for tag in tags {
            if tag.to_lowercase().contains(query) {
                return true;
            }
        }
    }

    // Match keywords
    if let Some(keywords) = &plugin.keywords {
        for keyword in keywords {
            if keyword.to_lowercase().contains(query) {
                return true;
            }
        }
    }

    false
}

/// List all plugins in a marketplace
pub fn list_plugins(marketplace: &Marketplace) -> Vec<ResolvedPlugin> {
    marketplace
        .plugins
        .iter()
        .map(|p| ResolvedPlugin::from_entry(p, &marketplace.name))
        .collect()
}

/// Get a plugin by name from a marketplace
pub fn get_plugin(marketplace: &Marketplace, name: &str) -> Option<ResolvedPlugin> {
    marketplace
        .plugins
        .iter()
        .find(|p| p.name == name)
        .map(|p| ResolvedPlugin::from_entry(p, &marketplace.name))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_marketplace_json() -> &'static str {
        r#"{
            "name": "test-marketplace",
            "owner": { "name": "Test Owner", "email": "test@example.com" },
            "plugins": [
                {
                    "name": "typescript-lsp",
                    "source": "./plugins/typescript-lsp",
                    "description": "TypeScript language server",
                    "version": "1.0.0",
                    "category": "development",
                    "tags": ["typescript", "lsp", "javascript"]
                },
                {
                    "name": "rust-analyzer-lsp",
                    "source": {
                        "source": "github",
                        "repo": "rust-lang/rust-analyzer"
                    },
                    "description": "Rust language server",
                    "version": "1.0.0",
                    "category": "development",
                    "tags": ["rust", "lsp"]
                },
                {
                    "name": "commit-commands",
                    "source": "./plugins/commit-commands",
                    "description": "Git commit workflow commands",
                    "category": "workflow"
                }
            ]
        }"#
    }

    #[test]
    fn test_parse_marketplace_str() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();

        assert_eq!(marketplace.name, "test-marketplace");
        assert_eq!(marketplace.owner.name, "Test Owner");
        assert_eq!(marketplace.plugins.len(), 3);
    }

    #[test]
    fn test_search_by_name() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let results = search_plugins(&marketplace, "typescript");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "typescript-lsp");
    }

    #[test]
    fn test_search_by_tag() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let results = search_plugins(&marketplace, "lsp");

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_description() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let results = search_plugins(&marketplace, "language server");

        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_search_by_category() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let results = search_plugins(&marketplace, "workflow");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "commit-commands");
    }

    #[test]
    fn test_list_plugins() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let plugins = list_plugins(&marketplace);

        assert_eq!(plugins.len(), 3);
    }

    #[test]
    fn test_get_plugin() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();

        let plugin = get_plugin(&marketplace, "rust-analyzer-lsp");
        assert!(plugin.is_some());
        assert_eq!(plugin.unwrap().name, "rust-analyzer-lsp");

        let not_found = get_plugin(&marketplace, "nonexistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_resolved_plugin_full_id() {
        let marketplace = parse_marketplace_str(sample_marketplace_json()).unwrap();
        let plugins = list_plugins(&marketplace);

        assert_eq!(plugins[0].full_id, "typescript-lsp@test-marketplace");
    }
}
