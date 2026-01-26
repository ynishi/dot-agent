//! Claude Code Plugin Integration
//!
//! This module provides integration with Claude Code's plugin system,
//! allowing dot-agent to manage plugins through the native plugin infrastructure.
//!
//! # Architecture
//!
//! ```text
//! ~/.claude/plugins/
//! ├── known_marketplaces.json    # Managed by PluginRegistry
//! ├── installed_plugins.json     # Managed by PluginRegistry
//! ├── marketplaces/              # Marketplace data cache
//! │   └── <marketplace-name>/
//! │       └── .claude-plugin/marketplace.json
//! └── cache/                     # Plugin cache
//!     └── <marketplace-name>/
//!         └── <plugin-name>/
//!             └── <version>/
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use dot_agent_core::plugin::{PluginInstaller, InstallScope};
//!
//! // Create installer
//! let installer = PluginInstaller::new()?;
//!
//! // Add a marketplace
//! installer.add_marketplace_github("my-plugins", "user/my-marketplace")?;
//!
//! // Search plugins
//! let plugins = installer.search_plugins("lsp")?;
//!
//! // Install a plugin
//! installer.install_plugin("rust-analyzer-lsp", "my-plugins", InstallScope::User)?;
//! ```

pub mod fetcher;
pub mod installer;
pub mod marketplace;
pub mod publisher;
pub mod registry;
pub mod types;

pub use fetcher::PluginFetcher;
pub use installer::PluginInstaller;
pub use marketplace::{
    get_plugin, list_plugins, parse_marketplace, parse_marketplace_str, search_plugins,
};
pub use publisher::{ProfilePluginConfig, ProfilePublisher, PublishResult, VersionBump};
pub use registry::PluginRegistry;
pub use types::{
    InstallScope, InstalledPlugin, InstalledPluginsFile, KnownMarketplace, KnownMarketplaceSource,
    KnownMarketplacesFile, Marketplace, MarketplaceMetadata, MarketplaceOwner, PluginAuthor,
    PluginEntry, PluginManifest, PluginSource, ResolvedPlugin, StructuredSource,
};
