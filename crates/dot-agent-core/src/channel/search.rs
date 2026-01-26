//! Channel search functionality
//!
//! Provides search capabilities across different channel types.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use crate::error::{DotAgentError, Result};

use super::channel_registry::ChannelRegistry;
use super::types::{Channel, ChannelType, ProfileRef, SearchOptions};

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
            ChannelType::ClaudePlugin => {
                // Claude Plugin search is handled separately via plugin module
                // TODO: Implement search_claude_plugin when Phase 2 is complete
                Ok(Vec::new())
            }
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

        // Fetch from URL
        let raw_url = Self::to_raw_url(url);
        let content = Self::fetch_url(&raw_url)?;

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

    /// Convert GitHub URL to raw content URL
    fn to_raw_url(url: &str) -> String {
        if url.contains("github.com") && !url.contains("raw.githubusercontent.com") {
            // https://github.com/user/repo -> raw README
            let url = url
                .replace("github.com", "raw.githubusercontent.com")
                .trim_end_matches('/')
                .to_string();
            format!("{}/main/README.md", url)
        } else {
            url.to_string()
        }
    }

    /// Fetch URL content
    fn fetch_url(url: &str) -> Result<String> {
        let output = Command::new("curl")
            .args(["-sL", url])
            .output()
            .map_err(DotAgentError::Io)?;

        if !output.status.success() {
            return Err(DotAgentError::GitHubApiError {
                message: format!("Failed to fetch: {}", url),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
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

        if let Some(url) = channel.source.url() {
            let raw_url = Self::to_raw_url(url);
            let content = Self::fetch_url(&raw_url)?;

            let cache_dir = ChannelRegistry::cache_dir(&self.base_dir, channel_name);
            std::fs::create_dir_all(&cache_dir)?;
            std::fs::write(cache_dir.join("content.md"), content)?;
        }

        Ok(())
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
        let raw = ChannelManager::to_raw_url(url);
        assert!(raw.contains("raw.githubusercontent.com"));
        assert!(raw.contains("README.md"));
    }
}
