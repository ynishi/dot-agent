//! Channel search functionality
//!
//! Provides search capabilities across different channel types.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{DotAgentError, Result};

use super::channel_registry::ChannelRegistry;
use super::types::{Channel, ChannelSource, ChannelType, ProfileRef, SearchOptions};

/// A skill entry from OpenAI Codex Skills Catalog
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexSkill {
    /// Skill name (directory name)
    pub name: String,
    /// Skill category (.system, .curated, .experimental)
    pub category: String,
    /// Full path within repo (e.g., "skills/.curated/pdf")
    pub path: String,
    /// Description from SKILL.md frontmatter (if fetched)
    pub description: Option<String>,
}

/// A plugin entry from a Claude Code Plugin Marketplace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketplacePlugin {
    /// Plugin name (kebab-case identifier)
    pub name: String,
    /// Plugin description
    pub description: Option<String>,
    /// Plugin version
    pub version: Option<String>,
    /// Plugin source (relative path or git source object)
    pub source: serde_json::Value,
    /// Plugin category
    pub category: Option<String>,
    /// Plugin keywords/tags
    pub keywords: Option<Vec<String>>,
    /// Plugin author
    pub author: Option<serde_json::Value>,
    /// LSP servers configuration (inline definition)
    pub lsp_servers: Option<serde_json::Value>,
    /// MCP servers configuration (inline definition)
    pub mcp_servers: Option<serde_json::Value>,
    /// Hooks configuration (inline definition)
    pub hooks: Option<serde_json::Value>,
    /// Commands configuration (inline definition)
    pub commands: Option<serde_json::Value>,
    /// Agents configuration (inline definition)
    pub agents: Option<serde_json::Value>,
}

impl MarketplacePlugin {
    /// Create from JSON value
    pub fn from_json(value: &serde_json::Value) -> Self {
        Self {
            name: value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            description: value
                .get("description")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            version: value
                .get("version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            source: value
                .get("source")
                .cloned()
                .unwrap_or(serde_json::Value::Null),
            category: value
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            keywords: value.get("keywords").and_then(|v| {
                v.as_array().map(|arr| {
                    arr.iter()
                        .filter_map(|k| k.as_str().map(|s| s.to_string()))
                        .collect()
                })
            }),
            author: value.get("author").cloned(),
            lsp_servers: value.get("lspServers").cloned(),
            mcp_servers: value.get("mcpServers").cloned(),
            hooks: value.get("hooks").cloned(),
            commands: value.get("commands").cloned(),
            agents: value.get("agents").cloned(),
        }
    }

    /// Get the source as a relative path (if it's a string)
    pub fn source_path(&self) -> Option<&str> {
        self.source.as_str()
    }

    /// Get the source as a git repo (if it's an object with "source": "github")
    pub fn source_github_repo(&self) -> Option<&str> {
        self.source.get("repo").and_then(|v| v.as_str())
    }

    /// Get the source as a URL (if it's an object with "source": "url")
    pub fn source_url(&self) -> Option<&str> {
        if self.source.get("source").and_then(|v| v.as_str()) == Some("url") {
            self.source.get("url").and_then(|v| v.as_str())
        } else {
            None
        }
    }

    /// Check if plugin has inline configurations (strict: false pattern)
    pub fn has_inline_config(&self) -> bool {
        self.lsp_servers.is_some()
            || self.mcp_servers.is_some()
            || self.hooks.is_some()
            || self.commands.is_some()
            || self.agents.is_some()
    }

    /// Write inline configurations to the target directory as separate files
    pub fn write_config_files(&self, target_dir: &std::path::Path) -> Result<Vec<String>> {
        use std::fs;

        let mut written_files = Vec::new();

        // Write .lsp.json if lspServers is defined
        if let Some(lsp) = &self.lsp_servers {
            let lsp_path = target_dir.join(".lsp.json");
            let content =
                serde_json::to_string_pretty(lsp).map_err(|e| DotAgentError::GitHubApiError {
                    message: format!("Failed to serialize lspServers: {}", e),
                })?;
            fs::write(&lsp_path, content)?;
            written_files.push(".lsp.json".to_string());
        }

        // Write .mcp.json if mcpServers is defined
        if let Some(mcp) = &self.mcp_servers {
            let mcp_path = target_dir.join(".mcp.json");
            let content =
                serde_json::to_string_pretty(mcp).map_err(|e| DotAgentError::GitHubApiError {
                    message: format!("Failed to serialize mcpServers: {}", e),
                })?;
            fs::write(&mcp_path, content)?;
            written_files.push(".mcp.json".to_string());
        }

        // Write hooks.json if hooks is defined
        if let Some(hooks) = &self.hooks {
            let hooks_path = target_dir.join("hooks.json");
            let content =
                serde_json::to_string_pretty(hooks).map_err(|e| DotAgentError::GitHubApiError {
                    message: format!("Failed to serialize hooks: {}", e),
                })?;
            fs::write(&hooks_path, content)?;
            written_files.push("hooks.json".to_string());
        }

        // Write commands.json if commands is defined
        if let Some(commands) = &self.commands {
            let commands_path = target_dir.join("commands.json");
            let content = serde_json::to_string_pretty(commands).map_err(|e| {
                DotAgentError::GitHubApiError {
                    message: format!("Failed to serialize commands: {}", e),
                }
            })?;
            fs::write(&commands_path, content)?;
            written_files.push("commands.json".to_string());
        }

        // Write agents.json if agents is defined
        if let Some(agents) = &self.agents {
            let agents_path = target_dir.join("agents.json");
            let content = serde_json::to_string_pretty(agents).map_err(|e| {
                DotAgentError::GitHubApiError {
                    message: format!("Failed to serialize agents: {}", e),
                }
            })?;
            fs::write(&agents_path, content)?;
            written_files.push("agents.json".to_string());
        }

        Ok(written_files)
    }
}

/// Channel manager for search operations
pub struct ChannelManager {
    base_dir: PathBuf,
    registry: ChannelRegistry,
}

impl ChannelManager {
    /// Create a new channel manager
    pub fn new(base_dir: PathBuf) -> Result<Self> {
        let registry = ChannelRegistry::load(&base_dir)?;
        Ok(Self { base_dir, registry })
    }

    /// Create with a specific registry
    pub fn with_registry(base_dir: PathBuf, registry: ChannelRegistry) -> Self {
        Self { base_dir, registry }
    }

    /// Get the registry
    pub fn registry(&self) -> &ChannelRegistry {
        &self.registry
    }

    /// Get mutable registry
    pub fn registry_mut(&mut self) -> &mut ChannelRegistry {
        &mut self.registry
    }

    /// Save registry
    pub fn save(&self) -> Result<()> {
        self.registry.save(&self.base_dir)
    }

    /// Search across all enabled searchable channels
    pub fn search(&self, query: &str, options: &SearchOptions) -> Result<Vec<ProfileRef>> {
        let mut results = Vec::new();

        // Get channels to search
        let channels: Vec<&Channel> = if options.channels.is_empty() {
            self.registry.list_searchable()
        } else {
            self.registry
                .list_enabled()
                .into_iter()
                .filter(|c| options.channels.contains(&c.name))
                .collect()
        };

        for channel in channels {
            match self.search_channel(channel, query, options) {
                Ok(refs) => results.extend(refs),
                Err(e) => {
                    eprintln!("Warning: {} search failed: {}", channel.name, e);
                }
            }
        }

        // Sort by stars (descending)
        results.sort_by(|a, b| b.stars.cmp(&a.stars));

        // Apply limit
        if options.limit > 0 {
            results.truncate(options.limit);
        }

        Ok(results)
    }

    /// Search a specific channel
    fn search_channel(
        &self,
        channel: &Channel,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<ProfileRef>> {
        match channel.channel_type {
            ChannelType::GitHubGlobal => self.search_github(channel, query, options),
            ChannelType::AwesomeList => self.search_awesome_list(channel, query, options),
            ChannelType::Marketplace => self.search_marketplace(channel, query, options),
            ChannelType::CodexCatalog => self.search_codex_catalog(channel, query, options),
            ChannelType::Hub | ChannelType::Direct => {
                // Not searchable
                Ok(Vec::new())
            }
        }
    }

    /// Search GitHub globally
    fn search_github(
        &self,
        channel: &Channel,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<ProfileRef>> {
        // Check if gh CLI is available
        if !Self::check_gh_available() {
            return Err(DotAgentError::GitHubCliNotFound);
        }

        let mut args = vec!["search".to_string(), "repos".to_string()];

        // Build query
        let search_query = if options.keywords.is_empty() {
            query.to_string()
        } else {
            format!("{} {}", query, options.keywords.join(" "))
        };

        if !search_query.is_empty() {
            args.push(search_query);
        }

        // Topic filter
        if let Some(topic) = &options.topic {
            args.push("--topic".to_string());
            args.push(topic.clone());
        }

        // Sort order
        args.push("--sort".to_string());
        args.push(options.sort.clone().unwrap_or_else(|| "stars".to_string()));

        // Limit (per channel)
        let limit = if options.limit > 0 { options.limit } else { 10 };
        args.push("--limit".to_string());
        args.push(limit.to_string());

        // Stars filter
        if let Some(min) = options.min_stars {
            args.push("--stars".to_string());
            args.push(format!(">={}", min));
        }

        // JSON output
        args.push("--json".to_string());
        args.push("name,owner,description,url,stargazersCount".to_string());

        let output = Command::new("gh").args(&args).output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                DotAgentError::GitHubCliNotFound
            } else {
                DotAgentError::Io(e)
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DotAgentError::GitHubApiError {
                message: stderr.to_string(),
            });
        }

        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Array(vec![]));

        let repos = json
            .as_array()
            .ok_or_else(|| DotAgentError::GitHubApiError {
                message: "Invalid JSON response".to_string(),
            })?;

        let mut results = Vec::new();

        for repo in repos {
            let name = repo["name"].as_str().unwrap_or("").to_string();
            let owner = repo["owner"]["login"].as_str().unwrap_or("").to_string();
            let description = repo["description"]
                .as_str()
                .unwrap_or("No description")
                .to_string();
            let url = repo["url"].as_str().unwrap_or("").to_string();
            let stars = repo["stargazersCount"].as_u64();

            let id = format!("github:{}/{}", owner, name);

            results.push(ProfileRef {
                id,
                name,
                owner,
                description,
                url,
                stars,
                channel: channel.name.clone(),
                metadata: HashMap::new(),
            });
        }

        Ok(results)
    }

    /// Search an Awesome List
    fn search_awesome_list(
        &self,
        channel: &Channel,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<ProfileRef>> {
        let url = match channel.source.url() {
            Some(u) => u,
            None => return Ok(Vec::new()),
        };

        // Get cached content or fetch
        let content = self.fetch_awesome_list(url, &channel.name)?;

        // Parse and search
        let query_lower = query.to_lowercase();
        let keywords: Vec<String> = options.keywords.iter().map(|k| k.to_lowercase()).collect();

        let mut results = Vec::new();

        for line in content.lines() {
            if let Some(profile_ref) = Self::parse_awesome_line(line, &channel.name) {
                // Check if matches query
                let text = format!(
                    "{} {} {}",
                    profile_ref.name, profile_ref.owner, profile_ref.description
                )
                .to_lowercase();

                let matches_query = query.is_empty() || text.contains(&query_lower);
                let matches_keywords =
                    keywords.is_empty() || keywords.iter().all(|k| text.contains(k));

                if matches_query && matches_keywords {
                    results.push(profile_ref);
                }
            }
        }

        // Apply limit
        if options.limit > 0 {
            results.truncate(options.limit);
        }

        Ok(results)
    }

    /// Fetch Awesome List content (with caching)
    fn fetch_awesome_list(&self, url: &str, channel_name: &str) -> Result<String> {
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
        let cache_file = cache_dir.join("content.md");

        // Check cache
        if cache_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&cache_file) {
                return Ok(content);
            }
        }

        // Fetch from URL, try main first, then master
        let content = if let Some(c) = Self::fetch_url(&Self::to_raw_url(url, "main"))? {
            c
        } else if let Some(c) = Self::fetch_url(&Self::to_raw_url(url, "master"))? {
            c
        } else {
            return Err(DotAgentError::GitHubApiError {
                message: format!(
                    "Failed to fetch README from: {} (tried main and master)",
                    url
                ),
            });
        };

        // Cache it
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::write(&cache_file, &content)?;

        Ok(content)
    }

    /// Parse a line from Awesome List markdown
    fn parse_awesome_line(line: &str, channel_name: &str) -> Option<ProfileRef> {
        // Format: - [name](url) - description
        // or: * [name](url) - description
        let line = line.trim();

        if !line.starts_with('-') && !line.starts_with('*') {
            return None;
        }

        // Find [name](url)
        let start_bracket = line.find('[')?;
        let end_bracket = line.find(']')?;
        let start_paren = line.find('(')?;
        let end_paren = line.find(')')?;

        if end_bracket >= start_paren || start_paren >= end_paren {
            return None;
        }

        let name = &line[start_bracket + 1..end_bracket];
        let url = &line[start_paren + 1..end_paren];

        // Skip non-http links
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return None;
        }

        // Extract description (after " - " or just after ")")
        let desc_start = end_paren + 1;
        let description = if desc_start < line.len() {
            let desc = &line[desc_start..];
            let desc = desc.trim_start_matches([' ', '-']);
            desc.trim().to_string()
        } else {
            String::new()
        };

        // Extract owner from URL
        let owner = Self::extract_owner_from_url(url);

        let id = format!("awesome:{}#{}", channel_name, name);

        Some(ProfileRef {
            id,
            name: name.to_string(),
            owner,
            description,
            url: url.to_string(),
            stars: None,
            channel: channel_name.to_string(),
            metadata: HashMap::new(),
        })
    }

    /// Extract owner from GitHub URL
    fn extract_owner_from_url(url: &str) -> String {
        // https://github.com/owner/repo -> owner
        if url.contains("github.com") {
            let parts: Vec<&str> = url.split('/').collect();
            if parts.len() >= 4 {
                return parts[3].to_string();
            }
        }
        "unknown".to_string()
    }

    /// Convert GitHub URL to raw content URL with specified branch
    fn to_raw_url(url: &str, branch: &str) -> String {
        if url.contains("github.com") && !url.contains("raw.githubusercontent.com") {
            // https://github.com/user/repo -> raw README
            let url = url
                .replace("github.com", "raw.githubusercontent.com")
                .trim_end_matches('/')
                .to_string();
            format!("{}/{}/README.md", url, branch)
        } else {
            url.to_string()
        }
    }

    /// Normalize repo string to owner/repo format
    /// Accepts: "owner/repo", "https://github.com/owner/repo", "github.com/owner/repo"
    fn normalize_repo(repo: &str) -> String {
        let repo = repo.trim_end_matches('/');

        // Already in owner/repo format
        if !repo.contains("://") && !repo.contains("github.com") {
            return repo.to_string();
        }

        // Extract owner/repo from URL
        // https://github.com/owner/repo -> owner/repo
        if let Some(pos) = repo.find("github.com/") {
            let after = &repo[pos + 11..]; // skip "github.com/"
            return after.to_string();
        }

        repo.to_string()
    }

    /// Fetch URL content, returns None for 404
    fn fetch_url(url: &str) -> Result<Option<String>> {
        let output = Command::new("curl")
            .args(["-sL", "-w", "%{http_code}", "-o", "-", url])
            .output()
            .map_err(DotAgentError::Io)?;

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Extract HTTP status code (last 3 chars)
        if stdout.len() >= 3 {
            let (content, status) = stdout.split_at(stdout.len() - 3);
            if status == "404" {
                return Ok(None);
            }
            if status.starts_with('2') {
                return Ok(Some(content.to_string()));
            }
        }

        // Fallback: check if output looks like an error
        if !output.status.success() || stdout.contains("404") {
            return Ok(None);
        }

        Ok(Some(stdout.to_string()))
    }

    /// Check if gh CLI is available
    fn check_gh_available() -> bool {
        Command::new("gh")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Refresh a channel's cache
    pub fn refresh_channel(&self, channel_name: &str) -> Result<()> {
        let channel =
            self.registry
                .get(channel_name)
                .ok_or_else(|| DotAgentError::ChannelNotFound {
                    name: channel_name.to_string(),
                })?;

        match &channel.source {
            ChannelSource::Marketplace { repo } => {
                self.fetch_marketplace_catalog(repo, channel_name)?;
            }
            ChannelSource::CodexCatalog { repo, base_path } => {
                self.fetch_codex_catalog(repo, base_path, channel_name)?;
            }
            _ => {
                if let Some(url) = channel.source.url() {
                    // Try main first, then master
                    let content = if let Some(c) = Self::fetch_url(&Self::to_raw_url(url, "main"))?
                    {
                        c
                    } else if let Some(c) = Self::fetch_url(&Self::to_raw_url(url, "master"))? {
                        c
                    } else {
                        return Err(DotAgentError::GitHubApiError {
                            message: format!(
                                "Failed to fetch README from: {} (tried main and master)",
                                url
                            ),
                        });
                    };

                    let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
                    std::fs::create_dir_all(&cache_dir)?;
                    std::fs::write(cache_dir.join("content.md"), content)?;
                }
            }
        }

        Ok(())
    }

    /// Fetch marketplace catalog from GitHub repository
    ///
    /// Downloads `.claude-plugin/marketplace.json` from the repository
    /// and caches it locally.
    fn fetch_marketplace_catalog(&self, repo: &str, channel_name: &str) -> Result<()> {
        // Normalize repo: extract owner/repo from full URL if needed
        let repo = Self::normalize_repo(repo);

        // Try main first, then master
        let content = if let Some(c) = Self::fetch_url(&format!(
            "https://raw.githubusercontent.com/{}/main/.claude-plugin/marketplace.json",
            repo
        ))? {
            c
        } else if let Some(c) = Self::fetch_url(&format!(
            "https://raw.githubusercontent.com/{}/master/.claude-plugin/marketplace.json",
            repo
        ))? {
            c
        } else {
            return Err(DotAgentError::GitHubApiError {
                message: format!(
                    "Failed to fetch marketplace.json from: {} (tried main and master)",
                    repo
                ),
            });
        };

        // Validate JSON structure
        let data: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| DotAgentError::GitHubApiError {
                message: format!("Invalid marketplace.json: {}", e),
            })?;

        // Check required fields
        if data.get("name").is_none() || data.get("plugins").is_none() {
            return Err(DotAgentError::GitHubApiError {
                message: "marketplace.json missing required fields (name, plugins)".to_string(),
            });
        }

        // Cache the catalog
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
        std::fs::create_dir_all(&cache_dir)?;
        std::fs::write(cache_dir.join("marketplace.json"), content)?;

        Ok(())
    }

    /// Get a plugin entry from marketplace catalog
    pub fn get_marketplace_plugin(
        &self,
        channel_name: &str,
        plugin_name: &str,
    ) -> Result<Option<MarketplacePlugin>> {
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
        let cache_file = cache_dir.join("marketplace.json");

        if !cache_file.exists() {
            return Ok(None);
        }

        let content = std::fs::read_to_string(&cache_file)?;
        let data: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| DotAgentError::GitHubApiError {
                message: format!("Invalid marketplace.json: {}", e),
            })?;

        let plugins = data
            .get("plugins")
            .and_then(|p| p.as_array())
            .ok_or_else(|| DotAgentError::GitHubApiError {
                message: "marketplace.json has no plugins array".to_string(),
            })?;

        for plugin in plugins {
            let name = plugin.get("name").and_then(|n| n.as_str()).unwrap_or("");
            if name == plugin_name {
                return Ok(Some(MarketplacePlugin::from_json(plugin)));
            }
        }

        Ok(None)
    }

    /// Search a Marketplace channel
    fn search_marketplace(
        &self,
        channel: &Channel,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<ProfileRef>> {
        let repo = match &channel.source {
            ChannelSource::Marketplace { repo } => repo,
            _ => return Ok(Vec::new()),
        };

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Check if marketplace content is cached
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, &channel.name);
        let cache_file = cache_dir.join("marketplace.json");

        // Auto-fetch if cache doesn't exist
        if !cache_file.exists() {
            self.fetch_marketplace_catalog(repo, &channel.name)?;
        }

        // Parse cached marketplace data
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(plugins) = data.get("plugins").and_then(|p| p.as_array()) {
                    for plugin in plugins {
                        let name = plugin
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or_default();
                        let description = plugin
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or_default();
                        let version = plugin
                            .get("version")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        let category = plugin
                            .get("category")
                            .and_then(|c| c.as_str())
                            .unwrap_or("");

                        // Filter by query (search in name, description, category)
                        let text = format!("{} {} {}", name, description, category).to_lowercase();
                        if query.is_empty() || text.contains(&query_lower) {
                            let mut metadata = HashMap::new();
                            metadata.insert("version".to_string(), version.to_string());
                            if !category.is_empty() {
                                metadata.insert("category".to_string(), category.to_string());
                            }

                            results.push(ProfileRef {
                                id: format!("marketplace:{}@{}", name, channel.name),
                                name: name.to_string(),
                                owner: repo.split('/').next().unwrap_or("unknown").to_string(),
                                description: description.to_string(),
                                url: format!("https://github.com/{}", repo),
                                stars: None,
                                channel: channel.name.clone(),
                                metadata,
                            });
                        }
                    }
                }
            }
        }

        // Apply limit
        if options.limit > 0 {
            results.truncate(options.limit);
        }

        Ok(results)
    }

    /// Fetch Codex skills catalog from GitHub repository using GitHub API
    ///
    /// Scans directory structure like Codex CLI's $skill-installer does:
    /// - skills/.system (preinstalled system skills)
    /// - skills/.curated (recommended skills)
    /// - skills/.experimental (experimental skills)
    fn fetch_codex_catalog(&self, repo: &str, base_path: &str, channel_name: &str) -> Result<()> {
        let repo = Self::normalize_repo(repo);

        // Categories to scan (matching Codex CLI's structure)
        let categories = [".system", ".curated", ".experimental"];
        let mut all_skills: Vec<CodexSkill> = Vec::new();

        for category in &categories {
            let path = format!("{}/{}", base_path, category);
            match self.fetch_github_directory_contents(&repo, &path) {
                Ok(skills) => {
                    for skill_name in skills {
                        all_skills.push(CodexSkill {
                            name: skill_name.clone(),
                            category: category.to_string(),
                            path: format!("{}/{}", path, skill_name),
                            description: None,
                        });
                    }
                }
                Err(e) => {
                    // Log but continue - some categories may not exist
                    eprintln!("Warning: Failed to fetch {}/{}: {}", repo, path, e);
                }
            }
        }

        if all_skills.is_empty() {
            return Err(DotAgentError::GitHubApiError {
                message: format!(
                    "No skills found in {}/{} (tried .system, .curated, .experimental)",
                    repo, base_path
                ),
            });
        }

        // Cache the catalog as JSON
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
        std::fs::create_dir_all(&cache_dir)?;

        let catalog = serde_json::json!({
            "repo": repo,
            "base_path": base_path,
            "skills": all_skills,
            "fetched_at": chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        });

        std::fs::write(
            cache_dir.join("codex_catalog.json"),
            serde_json::to_string_pretty(&catalog).map_err(|e| DotAgentError::GitHubApiError {
                message: format!("Failed to serialize catalog: {}", e),
            })?,
        )?;

        Ok(())
    }

    /// Fetch directory contents from GitHub API
    /// Returns list of directory names (skills)
    fn fetch_github_directory_contents(&self, repo: &str, path: &str) -> Result<Vec<String>> {
        // Use gh CLI for GitHub API access
        if !Self::check_gh_available() {
            return Err(DotAgentError::GitHubCliNotFound);
        }

        let api_path = format!("repos/{}/contents/{}", repo, path);
        let output = Command::new("gh")
            .args([
                "api",
                &api_path,
                "--jq",
                "[.[] | select(.type == \"dir\") | .name]",
            ])
            .output()
            .map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    DotAgentError::GitHubCliNotFound
                } else {
                    DotAgentError::Io(e)
                }
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(DotAgentError::GitHubApiError {
                message: format!("GitHub API error for {}/{}: {}", repo, path, stderr),
            });
        }

        let json: Vec<String> =
            serde_json::from_slice(&output.stdout).map_err(|e| DotAgentError::GitHubApiError {
                message: format!("Invalid JSON response: {}", e),
            })?;

        Ok(json)
    }

    /// Search a Codex Catalog channel
    fn search_codex_catalog(
        &self,
        channel: &Channel,
        query: &str,
        options: &SearchOptions,
    ) -> Result<Vec<ProfileRef>> {
        let (repo, base_path) = match &channel.source {
            ChannelSource::CodexCatalog { repo, base_path } => (repo.clone(), base_path.clone()),
            _ => return Ok(Vec::new()),
        };

        let query_lower = query.to_lowercase();
        let mut results = Vec::new();

        // Check if catalog is cached
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, &channel.name);
        let cache_file = cache_dir.join("codex_catalog.json");

        // Auto-fetch if cache doesn't exist
        if !cache_file.exists() {
            self.fetch_codex_catalog(&repo, &base_path, &channel.name)?;
        }

        // Parse cached catalog
        if let Ok(content) = std::fs::read_to_string(&cache_file) {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(skills) = data.get("skills").and_then(|s| s.as_array()) {
                    for skill in skills {
                        let name = skill
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or_default();
                        let category = skill
                            .get("category")
                            .and_then(|c| c.as_str())
                            .unwrap_or_default();
                        let path = skill
                            .get("path")
                            .and_then(|p| p.as_str())
                            .unwrap_or_default();
                        let description = skill
                            .get("description")
                            .and_then(|d| d.as_str())
                            .unwrap_or("");

                        // Filter by query
                        let text = format!("{} {} {}", name, category, description).to_lowercase();
                        if query.is_empty() || text.contains(&query_lower) {
                            let mut metadata = HashMap::new();
                            metadata.insert("category".to_string(), category.to_string());
                            metadata.insert("path".to_string(), path.to_string());

                            results.push(ProfileRef {
                                id: format!("codex:{}@{}", name, channel.name),
                                name: name.to_string(),
                                owner: repo.split('/').next().unwrap_or("openai").to_string(),
                                description: if description.is_empty() {
                                    format!("Codex skill ({})", category.trim_start_matches('.'))
                                } else {
                                    description.to_string()
                                },
                                url: format!("https://github.com/{}/tree/main/{}", repo, path),
                                stars: None,
                                channel: channel.name.clone(),
                                metadata,
                            });
                        }
                    }
                }
            }
        }

        // Apply limit
        if options.limit > 0 {
            results.truncate(options.limit);
        }

        Ok(results)
    }

    /// Get Codex skills from cached catalog
    pub fn get_codex_skills(&self, channel_name: &str) -> Result<Vec<CodexSkill>> {
        let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
        let cache_file = cache_dir.join("codex_catalog.json");

        if !cache_file.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&cache_file)?;
        let data: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| DotAgentError::GitHubApiError {
                message: format!("Invalid codex_catalog.json: {}", e),
            })?;

        let skills = data
            .get("skills")
            .and_then(|s| s.as_array())
            .ok_or_else(|| DotAgentError::GitHubApiError {
                message: "codex_catalog.json has no skills array".to_string(),
            })?;

        let result: Vec<CodexSkill> = skills
            .iter()
            .filter_map(|s| serde_json::from_value(s.clone()).ok())
            .collect();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_awesome_line_basic() {
        let line =
            "- [dotbot](https://github.com/anishathalye/dotbot) - A tool for managing dotfiles";
        let result = ChannelManager::parse_awesome_line(line, "test");

        assert!(result.is_some());
        let profile = result.unwrap();
        assert_eq!(profile.name, "dotbot");
        assert_eq!(profile.owner, "anishathalye");
        assert!(profile.description.contains("managing dotfiles"));
    }

    #[test]
    fn parse_awesome_line_star() {
        let line = "* [stow](https://github.com/aspiers/stow) - Symlink farm manager";
        let result = ChannelManager::parse_awesome_line(line, "test");

        assert!(result.is_some());
        let profile = result.unwrap();
        assert_eq!(profile.name, "stow");
    }

    #[test]
    fn parse_awesome_line_no_description() {
        let line = "- [tool](https://github.com/user/tool)";
        let result = ChannelManager::parse_awesome_line(line, "test");

        assert!(result.is_some());
        let profile = result.unwrap();
        assert_eq!(profile.name, "tool");
        assert!(profile.description.is_empty());
    }

    #[test]
    fn parse_awesome_line_skip_non_http() {
        let line = "- [Section](#section)";
        let result = ChannelManager::parse_awesome_line(line, "test");
        assert!(result.is_none());
    }

    #[test]
    fn to_raw_url_github() {
        let url = "https://github.com/webpro/awesome-dotfiles";
        let raw = ChannelManager::to_raw_url(url, "main");
        assert!(raw.contains("raw.githubusercontent.com"));
        assert!(raw.contains("README.md"));
        assert!(raw.contains("/main/"));

        let raw_master = ChannelManager::to_raw_url(url, "master");
        assert!(raw_master.contains("/master/"));
    }
}
