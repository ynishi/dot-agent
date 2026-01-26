//! Hub and Channel type definitions
//!
//! # Hierarchy
//! ```text
//! Hub (Channel aggregation repository)
//! ├── official (github.com/xxx/dot-agent-hub)
//! ├── company-internal (github.com/company/internal-hub)
//! └── personal (github.com/user/my-hub)
//!
//! Channel (Profile source with search capability)
//! ├── GitHubGlobal: Search all of GitHub (default enabled)
//! ├── AwesomeList: Curated markdown lists
//! ├── Hub: Channel from a Hub repository
//! └── Direct: Direct URL to a repo
//! ```

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

/// A Hub is a repository that aggregates multiple Channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hub {
    /// Unique name for this hub (e.g., "official", "company-internal")
    pub name: String,
    /// GitHub repository URL
    pub url: String,
    /// Optional description
    pub description: Option<String>,
    /// Whether this is the default hub (official)
    #[serde(default)]
    pub is_default: bool,
    /// When this hub was added
    pub added_at: String,
}

impl Hub {
    /// Create a new Hub
    pub fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            description: None,
            is_default: false,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Create the official default hub
    pub fn official() -> Self {
        Self {
            name: "official".to_string(),
            url: "https://github.com/dot-agent/dot-agent-hub".to_string(),
            description: Some("Official dot-agent hub".to_string()),
            is_default: true,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Mark as default
    pub fn as_default(mut self) -> Self {
        self.is_default = true;
        self
    }
}

/// Type of channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ChannelType {
    /// GitHub global search (searches all of GitHub)
    GitHubGlobal,
    /// Awesome List (Markdown-based curated list)
    AwesomeList,
    /// Channel from a Hub
    Hub,
    /// Direct URL (GitHub repo, etc.)
    Direct,
    /// Claude Code Plugin Marketplace
    Marketplace,
}

impl ChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::GitHubGlobal => "github-global",
            Self::AwesomeList => "awesome",
            Self::Hub => "hub",
            Self::Direct => "direct",
            Self::Marketplace => "marketplace",
        }
    }

    /// Whether this channel type supports search
    pub fn is_searchable(&self) -> bool {
        matches!(self, Self::GitHubGlobal | Self::AwesomeList | Self::Marketplace)
    }
}

impl std::fmt::Display for ChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Source of a channel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum ChannelSource {
    /// GitHub global (no specific source, searches all GitHub)
    GitHubGlobal,
    /// From a Hub
    Hub {
        /// Name of the hub
        hub_name: String,
        /// Name of the channel within the hub
        channel_name: String,
    },
    /// Direct URL
    Url {
        /// The URL
        url: String,
    },
    /// Anonymous (imported directly without registration)
    Anonymous {
        /// The original URL
        url: String,
        /// When it was imported
        imported_at: String,
    },
    /// Claude Code Plugin Marketplace
    Marketplace {
        /// GitHub repo (e.g., "anthropics/claude-plugins-official")
        repo: String,
    },
}

impl ChannelSource {
    /// Create a GitHub global source
    pub fn github_global() -> Self {
        Self::GitHubGlobal
    }

    /// Create a Hub source
    pub fn from_hub(hub_name: impl Into<String>, channel_name: impl Into<String>) -> Self {
        Self::Hub {
            hub_name: hub_name.into(),
            channel_name: channel_name.into(),
        }
    }

    /// Create a URL source
    pub fn from_url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }

    /// Create an anonymous source
    pub fn anonymous(url: impl Into<String>) -> Self {
        Self::Anonymous {
            url: url.into(),
            imported_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    /// Create a Marketplace source
    pub fn marketplace(repo: impl Into<String>) -> Self {
        Self::Marketplace { repo: repo.into() }
    }

    /// Get the URL if available
    pub fn url(&self) -> Option<&str> {
        match self {
            Self::GitHubGlobal => None,
            Self::Hub { .. } => None,
            Self::Url { url } => Some(url),
            Self::Anonymous { url, .. } => Some(url),
            Self::Marketplace { .. } => None,
        }
    }

    /// Get the GitHub repo if marketplace
    pub fn repo(&self) -> Option<&str> {
        match self {
            Self::Marketplace { repo } => Some(repo),
            _ => None,
        }
    }
}

/// A Channel is a source of profiles with search capability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Channel {
    /// Unique name for this channel
    pub name: String,
    /// Type of channel
    pub channel_type: ChannelType,
    /// Source of the channel
    pub source: ChannelSource,
    /// Optional description
    pub description: Option<String>,
    /// When this channel was added
    pub added_at: String,
    /// Whether this channel is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Whether this is a built-in channel (cannot be removed)
    #[serde(default)]
    pub builtin: bool,
}

fn default_true() -> bool {
    true
}

impl Channel {
    /// Create the default GitHub global channel
    pub fn github_global() -> Self {
        Self {
            name: "github".to_string(),
            channel_type: ChannelType::GitHubGlobal,
            source: ChannelSource::github_global(),
            description: Some("Search all of GitHub".to_string()),
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: true,
        }
    }

    /// Create a new Channel from a Hub
    pub fn from_hub(
        name: impl Into<String>,
        hub_name: impl Into<String>,
        channel_name: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            channel_type: ChannelType::Hub,
            source: ChannelSource::from_hub(hub_name, channel_name),
            description: None,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: false,
        }
    }

    /// Create a new Channel from a direct URL
    pub fn from_url(name: impl Into<String>, url: impl Into<String>) -> Self {
        let url_str = url.into();
        let channel_type = if url_str.contains("awesome") {
            ChannelType::AwesomeList
        } else {
            ChannelType::Direct
        };

        Self {
            name: name.into(),
            channel_type,
            source: ChannelSource::from_url(url_str),
            description: None,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: false,
        }
    }

    /// Create an Awesome List channel
    pub fn awesome_list(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channel_type: ChannelType::AwesomeList,
            source: ChannelSource::from_url(url),
            description: None,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: false,
        }
    }

    /// Create an anonymous channel
    pub fn anonymous(url: impl Into<String>) -> Self {
        let url_str = url.into();
        let id = format!("anon-{}", &sha256_short(&url_str));

        Self {
            name: id,
            channel_type: ChannelType::Direct,
            source: ChannelSource::anonymous(url_str),
            description: None,
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: false,
        }
    }

    /// Create a Claude Code Plugin Marketplace channel
    pub fn claude_plugin_github(name: impl Into<String>, repo: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channel_type: ChannelType::Marketplace,
            source: ChannelSource::marketplace(repo),
            description: Some("Claude Code Plugin Marketplace".to_string()),
            added_at: chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
            enabled: true,
            builtin: false,
        }
    }

    /// Set description
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Disable this channel
    pub fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    /// Whether this channel supports search
    pub fn is_searchable(&self) -> bool {
        self.channel_type.is_searchable()
    }
}

/// Generate a short hash for anonymous channel IDs
fn sha256_short(input: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    input.hash(&mut hasher);
    format!("{:08x}", hasher.finish() as u32)
}

/// A reference to a profile found via search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileRef {
    /// Unique identifier (e.g., "github:user/repo" or "awesome:channel#item")
    pub id: String,
    /// Display name
    pub name: String,
    /// Owner/author
    pub owner: String,
    /// Description
    pub description: String,
    /// Source URL (git clone URL or web URL)
    pub url: String,
    /// Star count (if available)
    pub stars: Option<u64>,
    /// Channel name this came from
    pub channel: String,
    /// Additional metadata
    pub metadata: HashMap<String, String>,
}

/// Search options
#[derive(Debug, Clone, Default)]
pub struct SearchOptions {
    /// Maximum results per channel
    pub limit: usize,
    /// Filter by channel names (empty = all enabled)
    pub channels: Vec<String>,
    /// Minimum stars (for GitHub)
    pub min_stars: Option<u64>,
    /// Additional keywords to add to search
    pub keywords: Vec<String>,
    /// GitHub topic filter
    pub topic: Option<String>,
    /// Sort order (stars, updated, etc.)
    pub sort: Option<String>,
}

/// A reference to a channel within a Hub's channel list
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelRef {
    /// Name of the channel
    pub name: String,
    /// URL to the channel content
    pub url: String,
    /// Description
    pub description: Option<String>,
    /// Type hint (awesome, github, etc.)
    #[serde(default)]
    pub channel_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hub_creation() {
        let hub = Hub::new("test", "https://github.com/test/hub");
        assert_eq!(hub.name, "test");
        assert!(!hub.is_default);
    }

    #[test]
    fn hub_official() {
        let hub = Hub::official();
        assert_eq!(hub.name, "official");
        assert!(hub.is_default);
    }

    #[test]
    fn channel_github_global() {
        let channel = Channel::github_global();
        assert_eq!(channel.name, "github");
        assert_eq!(channel.channel_type, ChannelType::GitHubGlobal);
        assert!(channel.builtin);
        assert!(channel.is_searchable());
    }

    #[test]
    fn channel_from_hub() {
        let channel = Channel::from_hub("awesome-dotfiles", "official", "awesome-dotfiles");
        assert_eq!(channel.channel_type, ChannelType::Hub);
        assert!(channel.enabled);
        assert!(!channel.builtin);
    }

    #[test]
    fn channel_from_url() {
        let channel = Channel::from_url("my-awesome", "https://github.com/user/awesome-list");
        assert_eq!(channel.channel_type, ChannelType::AwesomeList);
        assert!(channel.is_searchable());
    }

    #[test]
    fn channel_anonymous() {
        let channel = Channel::anonymous("https://github.com/user/dotfiles");
        assert!(channel.name.starts_with("anon-"));
        assert_eq!(channel.channel_type, ChannelType::Direct);
        assert!(!channel.is_searchable());
    }

    #[test]
    fn channel_type_searchable() {
        assert!(ChannelType::GitHubGlobal.is_searchable());
        assert!(ChannelType::AwesomeList.is_searchable());
        assert!(!ChannelType::Hub.is_searchable());
        assert!(!ChannelType::Direct.is_searchable());
    }
}
