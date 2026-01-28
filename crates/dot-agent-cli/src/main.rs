use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;

use dot_agent_core::channel::ChannelManager;
use dot_agent_core::config::Config;
use dot_agent_core::installer::{FileStatus, InstallOptions, Installer};
use dot_agent_core::metadata::Metadata;
use dot_agent_core::profile::{IgnoreConfig, ProfileManager};
use dot_agent_core::{DotAgentError, Result};

mod args;
use args::{
    ChannelAction, Cli, Commands, ConfigAction, HubAction, ProfileAction, ProfileSnapshotAction,
    RuleAction, Shell, SnapshotAction,
};

#[cfg(feature = "gui")]
mod gui;

fn main() -> ExitCode {
    let cli = Cli::parse();

    // Handle GUI flag
    #[cfg(feature = "gui")]
    if cli.gui {
        return match gui::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("{} {}", "[ERROR]".red().bold(), e);
                ExitCode::FAILURE
            }
        };
    }

    #[cfg(not(feature = "gui"))]
    if cli.gui {
        eprintln!(
            "{} GUI not available. Build with: cargo install --path . --features gui",
            "[ERROR]".red().bold()
        );
        return ExitCode::FAILURE;
    }

    let base_dir = resolve_base_dir(cli.base_dir);

    let result = match cli.command {
        // Top-level aliases
        Some(Commands::List) => handle_profile(ProfileAction::List, &base_dir),
        Some(Commands::Installed { path, global }) => {
            handle_status(&base_dir, path.as_deref(), global)
        }
        Some(Commands::Default { profile, clear }) => handle_default(&base_dir, profile, clear),
        Some(Commands::Outdated { path, global }) => {
            handle_outdated(&base_dir, path.as_deref(), global)
        }

        // Core commands
        Some(Commands::Profile { action }) => handle_profile(action, &base_dir),
        Some(Commands::Config { action }) => handle_config(action, &base_dir),
        Some(Commands::Search {
            query,
            limit,
            source,
            min_stars,
            keywords,
            topic,
            preset,
            sort,
            refresh,
        }) => handle_search(
            &base_dir, &query, limit, &source, min_stars, keywords, topic, preset, &sort, refresh,
        ),
        Some(Commands::Hub { action }) => handle_hub(action, &base_dir),
        Some(Commands::Channel { action }) => handle_channel(action, &base_dir),
        Some(Commands::Completions { shell, install }) => {
            handle_completions(shell, install, &base_dir)
        }
        Some(Commands::Install {
            profile,
            path,
            global,
            force,
            dry_run,
            no_prefix,
            no_merge,
            include,
            exclude,
        }) => handle_install(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            force,
            dry_run,
            no_prefix,
            no_merge,
            build_ignore_config(&base_dir, &include, &exclude),
        ),
        Some(Commands::Upgrade {
            profile,
            path,
            global,
            force,
            dry_run,
            no_prefix,
            no_merge,
            include,
            exclude,
        }) => handle_upgrade(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            force,
            dry_run,
            no_prefix,
            no_merge,
            build_ignore_config(&base_dir, &include, &exclude),
        ),
        Some(Commands::Diff {
            profile,
            path,
            global,
            include,
            exclude,
        }) => handle_diff(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            build_ignore_config(&base_dir, &include, &exclude),
        ),
        Some(Commands::Remove {
            profile,
            path,
            global,
            force,
            dry_run,
            no_merge,
            include,
            exclude,
        }) => handle_remove(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            force,
            dry_run,
            no_merge,
            build_ignore_config(&base_dir, &include, &exclude),
        ),
        Some(Commands::Status { path, global }) => {
            handle_status(&base_dir, path.as_deref(), global)
        }
        Some(Commands::Copy {
            source,
            dest,
            force,
        }) => handle_copy(&base_dir, &source, &dest, force),
        Some(Commands::Apply {
            rule,
            profile,
            path,
            force,
        }) => handle_apply(&base_dir, &rule, profile.as_deref(), path.as_deref(), force),
        Some(Commands::Rule { action }) => handle_rule(action, &base_dir),
        Some(Commands::Snapshot { action }) => handle_snapshot(action, &base_dir),
        Some(Commands::Switch {
            profile,
            path,
            global,
            no_snapshot,
            force,
        }) => handle_switch(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            no_snapshot,
            force,
        ),
        None => {
            Cli::command().print_help().ok();
            Ok(())
        }
    };

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{} {}", "[ERROR]".red().bold(), e);
            ExitCode::from(e.exit_code() as u8)
        }
    }
}

fn handle_completions(shell: Shell, install: bool, base_dir: &Path) -> Result<()> {
    let mut cmd = Cli::command();
    let clap_shell = match shell {
        Shell::Bash => clap_complete::Shell::Bash,
        Shell::Zsh => clap_complete::Shell::Zsh,
        Shell::Fish => clap_complete::Shell::Fish,
        Shell::PowerShell => clap_complete::Shell::PowerShell,
        Shell::Elvish => clap_complete::Shell::Elvish,
    };

    if install {
        let completions_dir = base_dir.join("completions");
        std::fs::create_dir_all(&completions_dir)?;

        let (filename, setup_instructions) = match shell {
            Shell::Zsh => (
                "_dot-agent",
                format!(
                    r#"# Add to ~/.zshrc (before compinit):
fpath=({} $fpath)
autoload -Uz compinit && compinit

# Then reload:
rm -f ~/.zcompdump* && exec zsh"#,
                    completions_dir.display()
                ),
            ),
            Shell::Bash => (
                "dot-agent.bash",
                format!(
                    "# Add to ~/.bashrc:\nsource {}",
                    completions_dir.join("dot-agent.bash").display()
                ),
            ),
            Shell::Fish => (
                "dot-agent.fish",
                format!(
                    "# Copy to fish completions:\ncp {} ~/.config/fish/completions/",
                    completions_dir.join("dot-agent.fish").display()
                ),
            ),
            Shell::PowerShell => (
                "_dot-agent.ps1",
                format!(
                    "# Add to $PROFILE:\n. {}",
                    completions_dir.join("_dot-agent.ps1").display()
                ),
            ),
            Shell::Elvish => (
                "dot-agent.elv",
                format!(
                    "# Add to ~/.elvish/rc.elv:\neval (slurp < {})",
                    completions_dir.join("dot-agent.elv").display()
                ),
            ),
        };

        let output_path = completions_dir.join(filename);
        let mut file = std::fs::File::create(&output_path)?;
        generate(clap_shell, &mut cmd, "dot-agent", &mut file);

        println!("{} {}", "Installed:".green(), output_path.display());
        println!();
        println!("{}", setup_instructions);
    } else {
        generate(clap_shell, &mut cmd, "dot-agent", &mut io::stdout());
    }

    Ok(())
}

fn resolve_base_dir(cli_base: Option<PathBuf>) -> PathBuf {
    if let Some(base) = cli_base {
        return base;
    }

    if let Ok(base) = std::env::var("DOT_AGENT_BASE") {
        return PathBuf::from(base);
    }

    dirs::home_dir()
        .map(|h| h.join(".dot-agent"))
        .unwrap_or_else(|| PathBuf::from(".dot-agent"))
}

#[allow(clippy::too_many_arguments)]
fn handle_search(
    base_dir: &Path,
    query: &str,
    limit: usize,
    channel_filter: &str,
    min_stars: Option<u64>,
    keywords: Vec<String>,
    topic: Option<String>,
    _preset: Option<String>,
    sort: &str,
    _refresh: bool,
) -> Result<()> {
    use dot_agent_core::channel::{ChannelManager, ChannelType, SearchOptions};

    // Create channel manager first to resolve channel type filters
    let manager = ChannelManager::new(base_dir.to_path_buf())?;

    // Build channel filter
    let channels: Vec<String> = match channel_filter.to_lowercase().as_str() {
        "github" | "gh" => vec!["github".to_string()],
        "all" | "" => vec![], // Empty means all enabled
        "marketplace" | "mp" => {
            // Filter to all marketplace channels
            manager
                .registry()
                .list_enabled()
                .iter()
                .filter(|c| c.channel_type == ChannelType::Marketplace)
                .map(|c| c.name.clone())
                .collect()
        }
        "awesome" | "al" => {
            // Filter to all awesome list channels
            manager
                .registry()
                .list_enabled()
                .iter()
                .filter(|c| c.channel_type == ChannelType::AwesomeList)
                .map(|c| c.name.clone())
                .collect()
        }
        other => vec![other.to_string()],
    };

    let options = SearchOptions {
        limit,
        channels,
        min_stars,
        keywords,
        topic,
        sort: Some(sort.to_string()),
    };

    // Perform search
    let results = manager.search(query, &options)?;

    if results.is_empty() {
        println!("No results found for: {}", query);
        let searchable = manager.registry().list_searchable();
        if searchable.len() <= 1 {
            println!();
            println!("{}", "Tip: Add Awesome Lists for more results:".dimmed());
            println!(
                "  {}",
                "dot-agent channel add https://github.com/webpro/awesome-dotfiles".dimmed()
            );
        }
        return Ok(());
    }

    println!();
    println!("{}", "Search Results:".cyan().bold());
    println!();

    for (i, profile_ref) in results.iter().enumerate() {
        let stars_str = profile_ref
            .stars
            .map(|s| format!("★{}", s).yellow().to_string())
            .unwrap_or_default();

        // Get channel type for badge
        let channel_type = manager
            .registry()
            .get(&profile_ref.channel)
            .map(|c| c.channel_type)
            .unwrap_or(ChannelType::Direct);

        let source_badge = match channel_type {
            ChannelType::GitHubGlobal => "[GH]".green(),
            ChannelType::AwesomeList => "[AL]".blue(),
            ChannelType::Marketplace => "[MP]".magenta(),
            _ => "[??]".dimmed(),
        };

        let desc: String = profile_ref.description.chars().take(57).collect();
        let desc = if profile_ref.description.chars().count() > 60 {
            format!("{}...", desc)
        } else {
            profile_ref.description.clone()
        };

        println!(
            "{}. {} {}/{} {} {}",
            (i + 1).to_string().bold(),
            source_badge,
            profile_ref.owner.yellow(),
            profile_ref.name.cyan(),
            stars_str,
            desc
        );
        println!("   {}", profile_ref.url.dimmed());
        println!();
    }

    println!("{}", "To import:".dimmed());
    println!(
        "  {}",
        "dot-agent profile import <url> --name <profile-name>".dimmed()
    );

    Ok(())
}

fn handle_hub(action: HubAction, base_dir: &Path) -> Result<()> {
    use dot_agent_core::channel::{Hub, HubRegistry};

    let mut registry = HubRegistry::load(base_dir).unwrap_or_else(|_| HubRegistry::with_official());

    match action {
        HubAction::Add { url, name } => {
            // Derive name from URL if not provided
            let hub_name = name.unwrap_or_else(|| {
                url.trim_end_matches('/')
                    .trim_end_matches(".git")
                    .rsplit('/')
                    .next()
                    .unwrap_or("hub")
                    .to_string()
            });

            let hub = Hub::new(&hub_name, &url);
            registry.add(hub)?;
            registry.save(base_dir)?;

            println!();
            println!("{} {} ({})", "Added hub:".green(), hub_name.cyan(), url);
        }
        HubAction::List => {
            let hubs = registry.list();

            if hubs.is_empty() {
                println!("No hubs registered.");
                return Ok(());
            }

            println!();
            println!("{}", "Registered Hubs:".cyan().bold());
            println!();

            for hub in hubs {
                let default_badge = if hub.is_default {
                    " (default)".yellow().to_string()
                } else {
                    String::new()
                };
                println!("  {}{}", hub.name.cyan(), default_badge);
                println!("    URL: {}", hub.url.dimmed());
                if let Some(desc) = &hub.description {
                    println!("    {}", desc.dimmed());
                }
                println!();
            }
        }
        HubAction::Remove { name } => {
            let removed = registry.remove(&name)?;
            registry.save(base_dir)?;

            println!();
            println!("{} {}", "Removed hub:".red(), removed.name);
        }
        HubAction::Refresh { name } => {
            let hubs: Vec<_> = if let Some(n) = name {
                registry.get(&n).into_iter().cloned().collect()
            } else {
                registry.list().to_vec()
            };

            if hubs.is_empty() {
                println!("No hubs to refresh.");
                return Ok(());
            }

            println!();
            println!("Refreshing hubs...");

            for hub in &hubs {
                print!("  {} ... ", hub.name);
                // TODO: Implement actual refresh (fetch channels.toml from hub)
                println!("{}", "OK".green());
            }
        }
    }

    Ok(())
}

/// Parse a line from Awesome List markdown, returns (name, description)
fn parse_awesome_line(line: &str, _channel_name: &str) -> Option<(String, String)> {
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

    Some((name.to_string(), description))
}

fn handle_channel(action: ChannelAction, base_dir: &Path) -> Result<()> {
    use dot_agent_core::channel::{
        Channel, ChannelManager, ChannelRegistry, ChannelType, HubRegistry,
    };

    let mut registry = ChannelRegistry::load(base_dir).unwrap_or_default();

    match action {
        ChannelAction::Discover => {
            let hub_registry =
                HubRegistry::load(base_dir).unwrap_or_else(|_| HubRegistry::with_official());

            println!();
            println!("{}", "Available Channels:".cyan().bold());
            println!();

            for hub in hub_registry.list() {
                println!("From hub: {} {}", hub.name.yellow(), hub.url.dimmed());
                // TODO: Fetch and parse channels from hub
                println!("  (channel discovery not yet implemented)");
                println!();
            }

            println!("{}", "Popular Awesome Lists:".cyan().bold());
            println!();
            println!("  {} - Dotfiles resources", "awesome-dotfiles".cyan());
            println!("    dot-agent channel add https://github.com/webpro/awesome-dotfiles");
            println!();
            println!("  {} - Neovim plugins", "awesome-neovim".cyan());
            println!("    dot-agent channel add https://github.com/rockerBOO/awesome-neovim");
            println!();
        }
        ChannelAction::Add {
            source,
            marketplace,
            awesome,
            direct,
            hub,
            name,
        } => {
            // Determine channel type from flags or auto-detect
            let (channel, type_hint) = if marketplace {
                // Marketplace: source is GitHub repo like "anthropics/claude-plugins-official"
                let channel_name = name.unwrap_or_else(|| {
                    source
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("marketplace")
                        .to_string()
                });
                (
                    Channel::claude_plugin_github(&channel_name, &source),
                    "marketplace",
                )
            } else if awesome {
                // Awesome List
                let channel_name = name.unwrap_or_else(|| {
                    source
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("awesome")
                        .to_string()
                });
                (Channel::awesome_list(&channel_name, &source), "awesome")
            } else if direct {
                // Direct repository
                let channel_name = name.unwrap_or_else(|| {
                    source
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("direct")
                        .to_string()
                });
                (Channel::from_url(&channel_name, &source), "direct")
            } else if let Some(hub_name) = hub {
                // From Hub
                let channel_name = name.unwrap_or_else(|| source.clone());
                (Channel::from_hub(&channel_name, &hub_name, &source), "hub")
            } else {
                // Auto-detect (deprecated)
                eprintln!(
                    "{} Auto-detection is deprecated. Use explicit flags: -m (marketplace), -a (awesome), -d (direct), -H (hub)",
                    "[WARN]".yellow().bold()
                );

                let channel_name = name.unwrap_or_else(|| {
                    source
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .unwrap_or("channel")
                        .to_string()
                });

                if source.starts_with("http://") || source.starts_with("https://") {
                    // Guess based on URL
                    if source.contains("awesome") {
                        (
                            Channel::awesome_list(&channel_name, &source),
                            "awesome (auto)",
                        )
                    } else {
                        (Channel::from_url(&channel_name, &source), "direct (auto)")
                    }
                } else {
                    // Assume hub channel
                    (
                        Channel::from_hub(&channel_name, "official", &source),
                        "hub (auto)",
                    )
                }
            };

            let channel_name = channel.name.clone();
            registry.add(channel)?;
            registry.save(base_dir)?;

            println!();
            println!(
                "{} {} [{}]",
                "Added channel:".green(),
                channel_name.cyan(),
                type_hint
            );
        }
        ChannelAction::List { name } => {
            if let Some(channel_name) = name {
                // List importable profiles from a specific channel
                let channel =
                    registry
                        .get(&channel_name)
                        .ok_or_else(|| DotAgentError::ChannelNotFound {
                            name: channel_name.clone(),
                        })?;

                println!();
                println!(
                    "{} {} [{}]",
                    "Profiles in channel:".cyan().bold(),
                    channel_name.yellow(),
                    channel.channel_type
                );
                println!();

                match channel.channel_type {
                    ChannelType::Marketplace => {
                        let manager = ChannelManager::new(base_dir.to_path_buf())?;
                        let cache_dir = ChannelRegistry::cache_dir(base_dir, &channel_name);
                        let cache_file = cache_dir.join("marketplace.json");

                        // Auto-refresh if cache doesn't exist
                        if !cache_file.exists() {
                            println!("{}", "Fetching marketplace catalog...".dimmed());
                            manager.refresh_channel(&channel_name)?;
                        }

                        if let Ok(content) = std::fs::read_to_string(&cache_file) {
                            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&content) {
                                if let Some(plugins) =
                                    data.get("plugins").and_then(|p| p.as_array())
                                {
                                    for plugin in plugins {
                                        let name = plugin
                                            .get("name")
                                            .and_then(|n| n.as_str())
                                            .unwrap_or("(unknown)");
                                        let version = plugin
                                            .get("version")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("");
                                        let desc = plugin
                                            .get("description")
                                            .and_then(|d| d.as_str())
                                            .unwrap_or("");

                                        let version_str = if version.is_empty() {
                                            String::new()
                                        } else {
                                            format!(" ({})", version.dimmed())
                                        };
                                        println!("  {}{}", name.cyan(), version_str);
                                        if !desc.is_empty() {
                                            println!("    {}", desc.dimmed());
                                        }
                                    }
                                    println!();
                                    println!("{} {}", "Total:".dimmed(), plugins.len());
                                }
                            }
                        } else {
                            println!("  {}", "(no cached data)".dimmed());
                        }
                    }
                    ChannelType::Direct => {
                        // Direct channel = single profile (the channel itself)
                        println!("  {}", channel_name.cyan());
                        if let Some(url) = channel.source.url() {
                            println!("    {}", url.dimmed());
                        }
                        println!();
                        println!("{} 1", "Total:".dimmed());
                    }
                    ChannelType::AwesomeList => {
                        let cache_dir = ChannelRegistry::cache_dir(base_dir, &channel_name);
                        let cache_file = cache_dir.join("content.md");

                        if !cache_file.exists() {
                            println!("{}", "Fetching awesome list...".dimmed());
                            let manager = ChannelManager::new(base_dir.to_path_buf())?;
                            manager.refresh_channel(&channel_name)?;
                        }

                        if let Ok(content) = std::fs::read_to_string(&cache_file) {
                            let mut count = 0;
                            for line in content.lines() {
                                if let Some(profile_ref) = parse_awesome_line(line, &channel_name) {
                                    println!("  {}", profile_ref.0.cyan());
                                    if !profile_ref.1.is_empty() {
                                        println!("    {}", profile_ref.1.dimmed());
                                    }
                                    count += 1;
                                }
                            }
                            println!();
                            println!("{} {}", "Total:".dimmed(), count);
                        } else {
                            println!("  {}", "(no cached data)".dimmed());
                        }
                    }
                    ChannelType::GitHubGlobal | ChannelType::Hub => {
                        println!("  {}", "List not supported for this channel type.".yellow());
                        println!("  {}", "Use 'dot-agent search <query>' instead.".dimmed());
                    }
                }
            } else {
                // List all channels
                let channels = registry.list();

                if channels.is_empty() {
                    println!("No channels registered.");
                    println!();
                    println!("{}", "Add channels with:".dimmed());
                    println!(
                        "  {}",
                        "dot-agent channel add https://github.com/webpro/awesome-dotfiles".dimmed()
                    );
                    println!(
                        "  {}",
                        "dot-agent channel discover  # Show available channels".dimmed()
                    );
                    return Ok(());
                }

                println!();
                println!("{}", "Registered Channels:".cyan().bold());
                println!();

                for channel in channels {
                    let status = if channel.enabled {
                        "enabled".green()
                    } else {
                        "disabled".yellow()
                    };
                    println!(
                        "  {} [{}] ({})",
                        channel.name.cyan(),
                        channel.channel_type,
                        status
                    );
                    if let Some(url) = channel.source.url() {
                        println!("    URL: {}", url.dimmed());
                    }
                    println!();
                }
            }
        }
        ChannelAction::Remove { name } => {
            let removed = registry.remove(&name)?;
            registry.save(base_dir)?;

            println!();
            println!("{} {}", "Removed channel:".red(), removed.name);
        }
        ChannelAction::Enable { name } => {
            registry.enable(&name)?;
            registry.save(base_dir)?;

            println!();
            println!("{} {}", "Enabled channel:".green(), name.cyan());
        }
        ChannelAction::Disable { name } => {
            registry.disable(&name)?;
            registry.save(base_dir)?;

            println!();
            println!("{} {}", "Disabled channel:".yellow(), name);
        }
        ChannelAction::Refresh { name } => {
            let channel_mgr = ChannelManager::new(base_dir.to_path_buf())?;

            let channels: Vec<_> = if let Some(n) = name {
                registry.get(&n).into_iter().cloned().collect()
            } else {
                registry.list().to_vec()
            };

            if channels.is_empty() {
                println!("No channels to refresh.");
                return Ok(());
            }

            println!();
            println!("Refreshing channels...");

            for channel in &channels {
                print!("  {} ... ", channel.name);
                io::stdout().flush()?;

                match channel_mgr.refresh_channel(&channel.name) {
                    Ok(()) => println!("{}", "OK".green()),
                    Err(e) => println!("{} {}", "FAILED".red(), e),
                }
            }
        }
    }

    Ok(())
}

/// Build IgnoreConfig from global config + CLI options
/// Priority: CLI options > config file > defaults
fn build_ignore_config(base_dir: &Path, include: &[String], exclude: &[String]) -> IgnoreConfig {
    // Start with config file settings (or defaults if no config)
    let mut config = Config::load(base_dir)
        .map(|c| c.to_ignore_config())
        .unwrap_or_else(|_| IgnoreConfig::with_defaults());

    // CLI options override/extend config
    for dir in include {
        if !config.included_dirs.contains(dir) {
            config.included_dirs.push(dir.clone());
        }
    }

    for dir in exclude {
        if !config.excluded_dirs.contains(dir) {
            config.excluded_dirs.push(dir.clone());
        }
    }

    config
}

fn handle_config(action: ConfigAction, base_dir: &Path) -> Result<()> {
    match action {
        ConfigAction::Get { key } => {
            let config = Config::load(base_dir)?;
            match config.get(&key) {
                Some(value) => {
                    println!("{}", value);
                }
                None => {
                    return Err(DotAgentError::ConfigKeyNotFound { key });
                }
            }
        }
        ConfigAction::Set { key, value } => {
            let mut config = Config::load(base_dir)?;
            config.set(&key, &value)?;
            config.save(base_dir)?;
            println!("{} {} = {}", "Set:".green(), key, value);
        }
        ConfigAction::List => {
            let config = Config::load(base_dir)?;
            println!();
            for (key, value) in config.list() {
                println!("{} = {}", key.cyan(), value);
            }
            println!();
        }
        ConfigAction::Path => {
            let path = Config::path(base_dir);
            println!("{}", path.display());
        }
        ConfigAction::Init => {
            let path = Config::init(base_dir)?;
            println!("{} {}", "Initialized:".green(), path.display());
        }
    }

    Ok(())
}

fn handle_profile(action: ProfileAction, base_dir: &Path) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());

    match action {
        ProfileAction::Add { name } => {
            let profile = manager.create_profile(&name)?;
            println!();
            println!("{} {}", "Created:".green(), profile.path.display());
            println!();
            println!("Structure:");
            println!("  {}/", profile.path.display());
            println!("  ├── CLAUDE.md      # Project instructions");
            println!("  ├── agents/        # Custom agents");
            println!("  ├── commands/      # Slash commands");
            println!("  ├── hooks/         # Event hooks");
            println!("  ├── plugins/       # MCP plugins");
            println!("  ├── rules/         # Coding rules");
            println!("  └── skills/        # Reusable skills");
            println!();
            println!("Next steps:");
            println!("  1. Edit CLAUDE.md with your project instructions");
            println!("  2. Add agents/rules/skills as needed");
            println!("  3. Install: dot-agent install {}", name);
        }
        ProfileAction::List => {
            let profiles = manager.list_profiles()?;
            if profiles.is_empty() {
                println!("No profiles found.");
                println!();
                println!("Create one with: dot-agent profile add <name>");
                return Ok(());
            }

            println!();
            println!("Available profiles:");
            println!();
            for profile in profiles {
                println!("  {}", profile.name.cyan().bold());
                println!("    Contents: {}", profile.contents_summary());
                println!();
            }
        }
        ProfileAction::Show { name, lines } => {
            let profile = manager.get_profile(&name)?;

            println!();
            println!("Profile: {}", profile.name.cyan().bold());
            println!("Path: {}", profile.path.display());
            println!();

            // Show README if exists
            let readme_path = profile.path.join("README.md");
            if readme_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&readme_path) {
                    println!("{}", "README.md".green().bold());
                    println!("{}", "─".repeat(60).dimmed());
                    for (i, line) in content.lines().take(lines).enumerate() {
                        println!("{}", line);
                        if i + 1 == lines && content.lines().count() > lines {
                            println!(
                                "{}",
                                format!("... ({} more lines)", content.lines().count() - lines)
                                    .dimmed()
                            );
                        }
                    }
                    println!("{}", "─".repeat(60).dimmed());
                    println!();
                }
            }

            // Group files by directory
            let files = profile.list_files()?;
            let mut grouped: std::collections::BTreeMap<String, Vec<(PathBuf, usize, usize)>> =
                std::collections::BTreeMap::new();

            let mut total_lines = 0usize;
            let mut total_chars = 0usize;

            for file in &files {
                let full_path = profile.path.join(file);
                let (lines, chars) = if let Ok(content) = std::fs::read_to_string(&full_path) {
                    (content.lines().count(), content.len())
                } else {
                    (0, 0)
                };
                total_lines += lines;
                total_chars += chars;

                let category = file
                    .components()
                    .next()
                    .map(|c| c.as_os_str().to_string_lossy().to_string())
                    .unwrap_or_else(|| "(root)".to_string());

                grouped
                    .entry(category)
                    .or_default()
                    .push((file.clone(), lines, chars));
            }

            // Display grouped files
            for (category, files) in &grouped {
                let cat_lines: usize = files.iter().map(|(_, l, _)| l).sum();
                let cat_chars: usize = files.iter().map(|(_, _, c)| c).sum();
                println!(
                    "{} ({} files, {} lines, {} chars)",
                    category.cyan().bold(),
                    files.len(),
                    cat_lines,
                    cat_chars
                );
                for (file, lines, chars) in files {
                    let file_name = file
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_default();
                    println!("  {} ({} lines, {} chars)", file_name, lines, chars);
                }
                println!();
            }

            println!(
                "Total: {} files, {} lines, {} chars",
                files.len(),
                total_lines,
                total_chars
            );
        }
        ProfileAction::Remove { name, force } => {
            let profile = manager.get_profile(&name)?;

            if !force {
                println!();
                println!("Remove profile '{}'? This will delete:", name.yellow());
                println!("  {}", profile.path.display());
                println!();
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            manager.remove_profile(&name)?;
            println!();
            println!("{} {}", "Removed:".red(), profile.path.display());
        }
        ProfileAction::Import {
            source,
            name,
            path,
            branch,
            force,
        } => {
            handle_import(&manager, &source, name, path, branch, force)?;
        }
        ProfileAction::ApplyRule {
            profile,
            rule,
            name,
            dry_run,
        } => {
            // Delegate to rule apply (creates new profile)
            handle_rule(
                RuleAction::Apply {
                    profile,
                    rule,
                    name,
                    dry_run,
                },
                base_dir,
            )?;
        }
        ProfileAction::Snapshot { action } => {
            handle_profile_snapshot(action, base_dir, &manager)?;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_install(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
    no_prefix: bool,
    no_merge: bool,
    ignore_config: IgnoreConfig,
) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    // Check if this is a marketplace plugin reference (plugin@marketplace format)
    let (actual_profile_name, profile) = if let Some((plugin, marketplace)) =
        parse_marketplace_ref(profile_name)
    {
        // Fetch and import plugin from marketplace
        let profile = import_marketplace_plugin(base_dir, &plugin, &marketplace, &manager, force)?;
        (profile.name.clone(), profile)
    } else {
        // Normal local profile
        let profile = manager.get_profile(profile_name)?;
        (profile_name.to_string(), profile)
    };

    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Profile: {}", actual_profile_name.cyan());
    println!("Target: {}", target_dir.display());
    if dry_run {
        println!("{}", "(dry run)".yellow());
    }
    if no_prefix {
        println!("{}", "(no prefix)".yellow());
    }
    if no_merge {
        println!("{}", "(no merge)".yellow());
    }
    println!();
    println!("Installing...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "OK" => format!("[{}]", status).green(),
            "SKIP" => format!("[{}]", status).yellow(),
            "WARN" => format!("[{}]", status).yellow().bold(),
            "CONFLICT" => format!("[{}]", status).red().bold(),
            "MERGE" => format!("[{}]", status).cyan(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let opts = InstallOptions::new()
        .force(force)
        .dry_run(dry_run)
        .no_prefix(no_prefix)
        .no_merge(no_merge)
        .ignore_config(ignore_config.clone())
        .on_file(Some(&on_file));
    let result = installer.install(&profile, &target_dir, &opts)?;

    println!();
    println!("Summary:");
    println!("  Installed: {}", result.installed);
    if result.merged > 0 {
        println!("  Merged: {}", result.merged);
    }
    println!("  Skipped: {}", result.skipped);
    println!("  Conflicts: {}", result.conflicts);

    if result.conflicts > 0 {
        println!();
        return Err(DotAgentError::Conflict { path: target_dir });
    }

    // Auto-register plugin if profile has plugin features
    if !dry_run {
        use dot_agent_core::{PluginRegistrar, PluginScope};

        let features = PluginRegistrar::get_plugin_features(&profile.path);
        if !features.is_empty() {
            println!();
            println!(
                "{} {}",
                "Plugin features detected:".cyan(),
                features.join(", ")
            );

            // Determine scope based on global flag
            let scope = if global {
                PluginScope::User
            } else {
                PluginScope::Project
            };

            match PluginRegistrar::new() {
                Ok(registrar) => {
                    match registrar.register_plugin(
                        &profile.path,
                        &actual_profile_name,
                        scope,
                        target.map(|p| p.parent().unwrap_or(p)),
                    ) {
                        Ok(reg_result) => {
                            if reg_result.registered {
                                println!(
                                    "{} Plugin registered to {}",
                                    "[OK]".green(),
                                    reg_result
                                        .settings_path
                                        .as_ref()
                                        .map(|p| p.display().to_string())
                                        .unwrap_or_default()
                                );
                            }
                        }
                        Err(e) => {
                            eprintln!(
                                "{} Failed to register plugin: {}",
                                "[WARN]".yellow().bold(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    eprintln!(
                        "{} Failed to initialize plugin registrar: {}",
                        "[WARN]".yellow().bold(),
                        e
                    );
                }
            }
        }
    }

    println!();
    println!(
        "{} {}",
        "Installation complete:".green(),
        target_dir.display()
    );

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_upgrade(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
    no_prefix: bool,
    no_merge: bool,
    ignore_config: IgnoreConfig,
) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    let profile = manager.get_profile(profile_name)?;
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Profile: {}", profile_name.cyan());
    println!("Target: {}", target_dir.display());
    if dry_run {
        println!("{}", "(dry run)".yellow());
    }
    if no_merge {
        println!("{}", "(no merge)".yellow());
    }
    println!();
    println!("Checking for updates...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "OK" => format!("[{}]", status).green(),
            "NEW" => format!("[{}]", status).green(),
            "UPDATE" => format!("[{}]", status).cyan(),
            "SKIP" => format!("[{}]", status).yellow(),
            "WARN" => format!("[{}]", status).yellow().bold(),
            "MERGE" => format!("[{}]", status).cyan(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let opts = InstallOptions::new()
        .force(force)
        .dry_run(dry_run)
        .no_prefix(no_prefix)
        .no_merge(no_merge)
        .ignore_config(ignore_config.clone())
        .on_file(Some(&on_file));
    let (updated, new, skipped, unchanged) = installer.upgrade(&profile, &target_dir, &opts)?;

    println!();
    println!("Summary:");
    println!("  Updated: {}", updated);
    println!("  New: {}", new);
    println!("  Skipped: {} (local modifications)", skipped);
    println!("  Unchanged: {}", unchanged);

    if skipped > 0 {
        println!();
        println!(
            "{} {} file(s) skipped due to local modifications.",
            "WARNING:".yellow().bold(),
            skipped
        );
        println!("         Use --force to overwrite, or review with 'dot-agent diff'");
    }

    Ok(())
}

fn handle_diff(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    ignore_config: IgnoreConfig,
) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    let profile = manager.get_profile(profile_name)?;
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Profile: {}", profile_name.cyan());
    println!("Target: {}", target_dir.display());
    println!();

    let result = installer.diff(&profile, &target_dir, &ignore_config)?;

    for file in &result.files {
        let status_str = match file.status {
            FileStatus::Unchanged => "[UNCHANGED]".green(),
            FileStatus::Modified => "[MODIFIED]".yellow(),
            FileStatus::Added => "[ADDED]".blue(),
            FileStatus::Missing => "[MISSING]".red(),
        };
        println!("{} {}", status_str, file.relative_path.display());
    }

    println!();
    println!("Summary:");
    println!("  Unchanged: {}", result.unchanged);
    println!("  Modified: {}", result.modified);
    println!("  Added: {} (user files)", result.added);
    println!("  Missing: {} (not installed)", result.missing);

    if result.modified > 0 {
        println!();
        println!("Status: {}", "HAS LOCAL MODIFICATIONS".yellow().bold());
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_remove(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
    no_merge: bool,
    ignore_config: IgnoreConfig,
) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    let profile = manager.get_profile(profile_name)?;
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Profile: {}", profile_name.cyan());
    println!("Target: {}", target_dir.display());
    if dry_run {
        println!("{}", "(dry run)".yellow());
    }
    if no_merge {
        println!("{}", "(no merge)".yellow());
    }
    println!();
    println!("Checking for local modifications...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "KEEP" => format!("[{}]", status).blue(),
            "DEL" => format!("[{}]", status).red(),
            "UNMERGE" => format!("[{}]", status).cyan(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let opts = InstallOptions::new()
        .force(force)
        .dry_run(dry_run)
        .no_merge(no_merge)
        .ignore_config(ignore_config.clone())
        .on_file(Some(&on_file));
    let (removed, kept, unmerged) = installer.remove(&profile, &target_dir, &opts)?;

    // Unregister plugin if profile had plugin features
    if !dry_run {
        use dot_agent_core::{PluginRegistrar, PluginScope};

        let features = PluginRegistrar::get_plugin_features(&profile.path);
        if !features.is_empty() {
            let scope = if global {
                PluginScope::User
            } else {
                PluginScope::Project
            };

            if let Ok(registrar) = PluginRegistrar::new() {
                match registrar.unregister_plugin(
                    &profile.path,
                    scope,
                    target.map(|p| p.parent().unwrap_or(p)),
                ) {
                    Ok(true) => {
                        println!("{} Plugin unregistered", "[OK]".green());
                    }
                    Ok(false) => {
                        // Plugin wasn't registered
                    }
                    Err(e) => {
                        eprintln!(
                            "{} Failed to unregister plugin: {}",
                            "[WARN]".yellow().bold(),
                            e
                        );
                    }
                }
            }
        }
    }

    println!();
    println!("Summary:");
    println!("  Removed: {}", removed);
    if unmerged > 0 {
        println!("  Unmerged: {}", unmerged);
    }
    println!("  Kept: {} (user files)", kept);
    println!();
    println!("{}", "Removal complete.".green());

    Ok(())
}

fn handle_apply(
    base_dir: &Path,
    rule_name: &str,
    profile_filter: Option<&str>,
    target: Option<&Path>,
    force: bool,
) -> Result<()> {
    use dot_agent_core::rule::RuleManager;

    let rule_manager = RuleManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    let rule = rule_manager.get(rule_name)?;
    let target_dir = installer.resolve_target(target, false)?;

    // Load metadata to find installed files
    let metadata = Metadata::load(&target_dir)?;
    let meta = metadata.ok_or_else(|| DotAgentError::TargetNotFound {
        path: target_dir.clone(),
    })?;

    // Collect target files
    let target_files: Vec<PathBuf> = if let Some(profile) = profile_filter {
        // Filter to specific profile's files
        if !meta.installed.profiles.contains(&profile.to_string()) {
            return Err(DotAgentError::ProfileNotFound {
                name: profile.to_string(),
            });
        }
        meta.files
            .keys()
            .filter(|f| f.starts_with(&format!("{}:", profile)))
            .map(|f| {
                // Remove profile prefix to get actual filename
                let parts: Vec<&str> = f.splitn(2, ':').collect();
                target_dir.join(parts.get(1).unwrap_or(&f.as_str()))
            })
            .filter(|p| p.exists())
            .collect()
    } else {
        // All installed files
        meta.files
            .keys()
            .map(|f| {
                let parts: Vec<&str> = f.splitn(2, ':').collect();
                target_dir.join(parts.get(1).unwrap_or(&f.as_str()))
            })
            .filter(|p| p.exists())
            .collect()
    };

    if target_files.is_empty() {
        println!("No files to apply rule to.");
        return Ok(());
    }

    println!();
    println!("Rule: {}", rule_name.cyan());
    println!("Target: {}", target_dir.display());
    if let Some(p) = profile_filter {
        println!("Profile filter: {}", p.cyan());
    }
    println!();
    println!("Files to modify ({}):", target_files.len());
    for f in &target_files {
        if let Ok(rel) = f.strip_prefix(&target_dir) {
            println!("  {}", rel.display());
        }
    }
    println!();

    if !force {
        print!("Apply rule to these files? [y/N]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;

        if !input.trim().eq_ignore_ascii_case("y") {
            println!("Aborted.");
            return Ok(());
        }
    }

    // Apply rule to each file
    println!("Applying rule...");
    let mut modified = 0;

    for file_path in &target_files {
        if let Ok(rel) = file_path.strip_prefix(&target_dir) {
            print!("  {} ... ", rel.display());
            io::stdout().flush()?;

            match apply_rule_to_file(&rule.content, file_path) {
                Ok(changed) => {
                    if changed {
                        println!("{}", "modified".green());
                        modified += 1;
                    } else {
                        println!("{}", "unchanged".yellow());
                    }
                }
                Err(e) => {
                    println!("{}: {}", "error".red(), e);
                }
            }
        }
    }

    println!();
    println!("{} {} file(s) modified.", "Done:".green(), modified);

    Ok(())
}

fn apply_rule_to_file(rule_content: &str, file_path: &Path) -> Result<bool> {
    use std::process::Command;

    let original = std::fs::read_to_string(file_path)?;

    // Use claude CLI to apply the rule
    let prompt = format!(
        "Apply the following customization rule to the file content. \
         Return ONLY the modified file content, no explanations.\n\n\
         === RULE ===\n{}\n\n=== FILE CONTENT ===\n{}",
        rule_content, original
    );

    let output = Command::new("claude")
        .arg("-p")
        .arg(&prompt)
        .output()
        .map_err(DotAgentError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DotAgentError::ClaudeExecutionFailed {
            message: stderr.to_string(),
        });
    }

    let modified = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if modified == original {
        return Ok(false);
    }

    std::fs::write(file_path, &modified)?;
    Ok(true)
}

fn handle_copy(base_dir: &Path, source: &str, dest: &str, force: bool) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());

    let profile = manager.copy_profile(source, dest, force)?;

    println!();
    println!("{} {} -> {}", "Copied:".green(), source.cyan(), dest.cyan());
    println!("  Path: {}", profile.path.display());
    println!();
    println!("Contents: {}", profile.contents_summary());

    Ok(())
}

fn handle_status(base_dir: &Path, target: Option<&Path>, global: bool) -> Result<()> {
    let installer = Installer::new(base_dir.to_path_buf());
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Target: {}", target_dir.display());
    println!();

    if !target_dir.exists() {
        println!("No installation found.");
        return Ok(());
    }

    let metadata = Metadata::load(&target_dir)?;

    match metadata {
        Some(meta) => {
            println!("Installed profiles:");
            for profile in &meta.installed.profiles {
                let prefix = format!("{}:", profile);
                let file_count = meta.files.keys().filter(|f| f.starts_with(&prefix)).count();
                println!("  {} ({} files)", profile.cyan(), file_count);
            }
            println!();
            println!("Total tracked files: {}", meta.files.len());
        }
        None => {
            println!("No metadata found (may be manually installed).");
        }
    }

    // Check for CLAUDE.md
    let claude_md = target_dir.join("CLAUDE.md");
    if claude_md.exists() {
        println!();
        println!("CLAUDE.md: {} (user-managed)", "present".green());
    }

    Ok(())
}

fn handle_default(base_dir: &Path, profile: Option<String>, clear: bool) -> Result<()> {
    let mut config = Config::load(base_dir)?;
    let manager = ProfileManager::new(base_dir.to_path_buf());

    if clear {
        config.clear_default();
        config.save(base_dir)?;
        println!();
        println!("{}", "Default profile cleared.".yellow());
        return Ok(());
    }

    match profile {
        Some(name) => {
            // Verify profile exists
            manager.get_profile(&name)?;

            config.set("profile.default", &name)?;
            config.save(base_dir)?;

            println!();
            println!("{} {}", "Default profile set:".green(), name.cyan());
        }
        None => {
            // Show current default
            println!();
            match &config.profile.default {
                Some(default) => {
                    println!("Default profile: {}", default.cyan());
                }
                None => {
                    println!("No default profile set.");
                    println!();
                    println!("Set with: dot-agent default <profile>");
                }
            }
        }
    }

    Ok(())
}

fn handle_outdated(base_dir: &Path, target: Option<&Path>, global: bool) -> Result<()> {
    let installer = Installer::new(base_dir.to_path_buf());
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Target: {}", target_dir.display());
    println!();

    if !target_dir.exists() {
        println!("No installation found.");
        return Ok(());
    }

    let metadata = Metadata::load(&target_dir)?;
    let meta = match metadata {
        Some(m) => m,
        None => {
            println!("No metadata found.");
            return Ok(());
        }
    };

    let mut outdated_count = 0;

    println!("Installed profiles:");
    println!();

    for profile_name in &meta.installed.profiles {
        // Get current profile
        let profile = match manager.get_profile(profile_name) {
            Ok(p) => p,
            Err(_) => {
                // Profile no longer exists locally
                println!(
                    "  {} {}",
                    profile_name.yellow(),
                    "(profile not found locally)".red()
                );
                continue;
            }
        };

        // Get profile version from metadata
        let current_version = profile
            .metadata()
            .ok()
            .flatten()
            .and_then(|m| m.profile.version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        // Check if any installed files differ from source
        let prefix = format!("{}:", profile_name);
        let installed_files: Vec<_> = meta
            .files
            .keys()
            .filter(|f| f.starts_with(&prefix))
            .collect();

        let mut has_changes = false;
        for tracked_file in &installed_files {
            // Extract actual filename (after profile: prefix)
            let actual_filename = tracked_file.strip_prefix(&prefix).unwrap_or(tracked_file);
            let installed_path = target_dir.join(actual_filename);
            let source_path = profile.path.join(actual_filename);

            if source_path.exists() && installed_path.exists() {
                // Compare file hashes
                if let (Ok(installed_content), Ok(source_content)) = (
                    std::fs::read_to_string(&installed_path),
                    std::fs::read_to_string(&source_path),
                ) {
                    if installed_content != source_content {
                        has_changes = true;
                        break;
                    }
                }
            } else if source_path.exists() != installed_path.exists() {
                has_changes = true;
                break;
            }
        }

        if has_changes {
            println!(
                "  {} v{} {}",
                profile_name.cyan(),
                current_version,
                "[updates available]".yellow()
            );
            outdated_count += 1;
        } else {
            println!(
                "  {} v{} {}",
                profile_name.cyan(),
                current_version,
                "[up to date]".green()
            );
        }
    }

    println!();

    if outdated_count == 0 {
        println!("{}", "All profiles are up to date.".green());
    } else {
        println!(
            "{} profile(s) can be upgraded.",
            outdated_count.to_string().yellow()
        );
        println!();
        println!("Upgrade with: dot-agent upgrade <profile>");
    }

    Ok(())
}

fn handle_import(
    manager: &ProfileManager,
    source: &str,
    name: Option<String>,
    subpath: Option<PathBuf>,
    branch: Option<String>,
    force: bool,
) -> Result<()> {
    let is_git_url = source.starts_with("https://")
        || source.starts_with("git@")
        || source.starts_with("http://")
        || source.starts_with("ssh://");

    if is_git_url {
        import_from_git(manager, source, name, subpath, branch, force)
    } else {
        import_from_path(manager, source, name, subpath, force)
    }
}

fn import_from_path(
    manager: &ProfileManager,
    source: &str,
    name: Option<String>,
    subpath: Option<PathBuf>,
    force: bool,
) -> Result<()> {
    let source_path = PathBuf::from(source);
    let import_path = if let Some(sub) = subpath {
        source_path.join(sub)
    } else {
        source_path.clone()
    };

    let profile_name = name.unwrap_or_else(|| {
        import_path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("profile"))
            .to_string_lossy()
            .to_string()
    });

    let profile = manager.import_profile(&import_path, &profile_name, force)?;
    println!();
    println!(
        "{} {} -> {}",
        "Imported:".green(),
        import_path.display(),
        profile.path.display()
    );
    println!();
    println!("Contents: {}", profile.contents_summary());

    Ok(())
}

fn import_from_git(
    manager: &ProfileManager,
    url: &str,
    name: Option<String>,
    subpath: Option<PathBuf>,
    branch: Option<String>,
    force: bool,
) -> Result<()> {
    // Extract repo name from URL for default profile name
    let repo_name = extract_repo_name(url);

    // Create temp directory
    let temp_dir = std::env::temp_dir().join(format!("dot-agent-{}", std::process::id()));

    println!();
    println!("Cloning {} ...", url.cyan());

    // Build git clone command
    let mut cmd = Command::new("git");
    cmd.arg("clone");
    cmd.arg("--depth").arg("1");

    if let Some(ref b) = branch {
        cmd.arg("--branch").arg(b);
    }

    cmd.arg(url);
    cmd.arg(&temp_dir);

    let output = cmd.output().map_err(DotAgentError::Io)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DotAgentError::Git(format!("git clone failed: {}", stderr)));
    }

    // Determine import path
    let import_path = if let Some(ref sub) = subpath {
        temp_dir.join(sub)
    } else {
        temp_dir.clone()
    };

    if !import_path.exists() {
        // Cleanup
        let _ = std::fs::remove_dir_all(&temp_dir);
        return Err(DotAgentError::TargetNotFound { path: import_path });
    }

    // Determine profile name: {repo}_{subpath}_{branch} or subset
    let profile_name = name.unwrap_or_else(|| {
        let mut parts = vec![repo_name.clone()];

        // Add subpath component if specified
        if let Some(ref sub) = subpath {
            if let Some(name) = sub.file_name() {
                parts.push(name.to_string_lossy().to_string());
            }
        }

        // Add branch if specified
        if let Some(ref b) = branch {
            parts.push(b.clone());
        }

        parts.join("_")
    });

    // Import with git source info
    let subpath_str = subpath.as_ref().map(|p| p.to_string_lossy().to_string());
    let profile = manager.import_profile_from_git(
        &import_path,
        &profile_name,
        force,
        url,
        branch.as_deref(),
        None, // commit (could get from git rev-parse HEAD)
        subpath_str.as_deref(),
    )?;

    // Cleanup temp directory
    let _ = std::fs::remove_dir_all(&temp_dir);

    println!();
    println!(
        "{} {} -> {}",
        "Imported:".green(),
        url,
        profile.path.display()
    );
    if branch.is_some() {
        println!("Branch: {}", branch.unwrap().cyan());
    }
    println!();
    println!("Contents: {}", profile.contents_summary());

    Ok(())
}

fn extract_repo_name(url: &str) -> String {
    // Handle various URL formats:
    // https://github.com/user/repo.git
    // git@github.com:user/repo.git
    // https://github.com/user/repo
    url.trim_end_matches('/')
        .trim_end_matches(".git")
        .rsplit('/')
        .next()
        .unwrap_or("profile")
        .to_string()
}

fn handle_profile_snapshot(
    action: ProfileSnapshotAction,
    base_dir: &Path,
    profile_manager: &ProfileManager,
) -> Result<()> {
    use dot_agent_core::snapshot::ProfileSnapshotManager;

    let snapshot_manager = ProfileSnapshotManager::new(base_dir.to_path_buf());

    match action {
        ProfileSnapshotAction::Save { profile, message } => {
            let p = profile_manager.get_profile(&profile)?;

            println!();
            println!("Creating snapshot of profile '{}'...", profile.cyan());

            let snapshot = snapshot_manager.save_profile(&profile, &p.path, message.as_deref())?;

            println!();
            println!("{} {}", "Snapshot saved:".green(), snapshot.id.cyan());
            println!("  Time: {}", snapshot.display_time());
            println!("  Files: {}", snapshot.file_count);
            if let Some(msg) = &snapshot.message {
                println!("  Message: {}", msg);
            }
        }
        ProfileSnapshotAction::List { profile } => {
            let _ = profile_manager.get_profile(&profile)?;

            let snapshots = snapshot_manager.list_profile(&profile)?;

            if snapshots.is_empty() {
                println!("No snapshots found for profile '{}'.", profile);
                println!();
                println!(
                    "Create one with: dot-agent profile snapshot save {}",
                    profile
                );
                return Ok(());
            }

            println!();
            println!("Snapshots for profile '{}':", profile.cyan());
            println!();

            for snap in snapshots {
                println!(
                    "  {} {} ({} files)",
                    snap.id.cyan(),
                    snap.display_time(),
                    snap.file_count
                );
                if let Some(msg) = &snap.message {
                    println!("    {}", msg.dimmed());
                }
            }
        }
        ProfileSnapshotAction::Restore { profile, id, force } => {
            let p = profile_manager.get_profile(&profile)?;
            let snapshot = snapshot_manager.get_profile(&profile, &id)?;

            println!();
            println!("Restore snapshot {} for profile '{}'?", id.cyan(), profile);
            println!("  Time: {}", snapshot.display_time());
            println!("  Files: {}", snapshot.file_count);
            println!();
            println!(
                "{}",
                "WARNING: This will replace all files in the profile directory.".yellow()
            );

            if !force {
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let (removed, restored) = snapshot_manager.restore_profile(&profile, &p.path, &id)?;

            println!();
            println!("{}", "Snapshot restored!".green());
            println!("  Removed: {} files", removed);
            println!("  Restored: {} files", restored);
        }
        ProfileSnapshotAction::Diff { profile, id } => {
            let p = profile_manager.get_profile(&profile)?;

            let diff = snapshot_manager.diff_profile(&profile, &p.path, &id)?;

            println!();
            println!(
                "Diff: snapshot {} vs current profile '{}'",
                id.cyan(),
                profile
            );
            println!();

            if !diff.modified.is_empty() {
                println!("Modified ({}):", diff.modified.len());
                for f in &diff.modified {
                    println!("  {} {}", "[M]".yellow(), f);
                }
            }

            if !diff.added.is_empty() {
                println!("Added ({}):", diff.added.len());
                for f in &diff.added {
                    println!("  {} {}", "[A]".green(), f);
                }
            }

            if !diff.deleted.is_empty() {
                println!("Deleted ({}):", diff.deleted.len());
                for f in &diff.deleted {
                    println!("  {} {}", "[D]".red(), f);
                }
            }

            if !diff.has_changes() {
                println!("{}", "No changes since snapshot.".green());
            }

            println!();
            println!(
                "Summary: {} unchanged, {} modified, {} added, {} deleted",
                diff.unchanged.len(),
                diff.modified.len(),
                diff.added.len(),
                diff.deleted.len()
            );
        }
        ProfileSnapshotAction::Delete { profile, id, force } => {
            let _ = profile_manager.get_profile(&profile)?;
            let snapshot = snapshot_manager.get_profile(&profile, &id)?;

            if !force {
                println!();
                println!("Delete snapshot {} for profile '{}'?", id.yellow(), profile);
                println!("  Time: {}", snapshot.display_time());
                println!("  Files: {}", snapshot.file_count);
                println!();
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            snapshot_manager.delete_profile(&profile, &id)?;
            println!();
            println!("{} snapshot {}", "Deleted:".red(), id);
        }
        ProfileSnapshotAction::Prune {
            profile,
            keep,
            force,
        } => {
            let _ = profile_manager.get_profile(&profile)?;

            let snapshots = snapshot_manager.list_profile(&profile)?;
            let to_delete = snapshots.len().saturating_sub(keep);

            if to_delete == 0 {
                println!();
                println!(
                    "No snapshots to prune ({} snapshots, keeping {})",
                    snapshots.len(),
                    keep
                );
                return Ok(());
            }

            println!();
            println!(
                "Prune {} snapshot(s) for profile '{}', keeping {} most recent?",
                to_delete, profile, keep
            );

            if !force {
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let deleted = snapshot_manager.prune_profile(&profile, keep)?;

            println!();
            println!("{} {} snapshot(s)", "Pruned:".green(), deleted.len());
            for id in &deleted {
                println!("  {}", id.dimmed());
            }
        }
    }

    Ok(())
}

fn handle_rule(action: RuleAction, base_dir: &Path) -> Result<()> {
    use dot_agent_core::rule::{extract_rule, generate_rule, RuleExecutor, RuleManager};

    let manager = RuleManager::new(base_dir.to_path_buf());
    let profile_manager = ProfileManager::new(base_dir.to_path_buf());

    match action {
        RuleAction::Add { name, file } => {
            let rule = if let Some(source_file) = file {
                manager.import(&name, &source_file)?
            } else {
                manager.create(&name)?
            };

            println!();
            println!("{} {}", "Created:".green(), rule.path.display());
            println!();
            println!("Next steps:");
            println!("  1. Edit rule: {}", rule.path.display());
            println!("  2. Apply: dot-agent rule apply -p <profile> -r {}", name);
        }
        RuleAction::List => {
            let rules = manager.list()?;
            if rules.is_empty() {
                println!("No rules found.");
                println!();
                println!("Create one with: dot-agent rule add <name>");
                return Ok(());
            }

            println!();
            println!("Available rules:");
            println!();
            for r in rules {
                println!("  {}", r.name.cyan().bold());
                println!("    {}", r.summary());
                println!();
            }
        }
        RuleAction::Show { name } => {
            let rule = manager.get(&name)?;
            println!();
            println!("Rule: {}", rule.name.cyan().bold());
            println!("Path: {}", rule.path.display());
            println!();
            println!("--- Content ---");
            println!("{}", rule.content);
        }
        RuleAction::Edit { name } => {
            let rule = manager.get(&name)?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

            println!("Opening {} in {}...", rule.path.display(), editor);

            Command::new(&editor)
                .arg(&rule.path)
                .status()
                .map_err(DotAgentError::Io)?;
        }
        RuleAction::Remove { name, force } => {
            let rule = manager.get(&name)?;

            if !force {
                println!();
                println!("Remove rule '{}'? This will delete:", name.yellow());
                println!("  {}", rule.path.display());
                println!();
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            manager.remove(&name)?;
            println!();
            println!("{} {}", "Removed:".red(), rule.path.display());
        }
        RuleAction::Extract { profile, name } => {
            let source_profile = profile_manager.get_profile(&profile)?;

            println!();
            println!("Extracting rule from profile '{}'...", profile.cyan());
            println!();

            let rule = extract_rule(&source_profile, &name, &manager)?;

            println!("{} {}", "Created:".green(), rule.path.display());
            println!();
            println!("The AI has extracted customization patterns from the profile.");
            println!("Review and edit: {}", rule.path.display());
        }
        RuleAction::Generate { instruction, name } => {
            println!();
            println!("Generating rule from instruction...");
            println!("  \"{}\"", instruction.cyan());
            if name.is_none() {
                println!("  (name will be auto-generated)");
            }
            println!();

            let rule = generate_rule(&instruction, name.as_deref(), &manager)?;

            println!("{} {}", "Created:".green(), rule.path.display());
            println!("  Name: {}", rule.name.cyan());
            println!();
            println!("The AI has generated a customization rule.");
            println!("Review and edit: {}", rule.path.display());
        }
        RuleAction::Rename { name, new_name } => {
            let rule = manager.rename(&name, &new_name)?;

            println!();
            println!(
                "{} '{}' -> '{}'",
                "Renamed:".green(),
                name.yellow(),
                rule.name.cyan()
            );
            println!("  Path: {}", rule.path.display());
        }
        RuleAction::Apply {
            profile,
            rule,
            name,
            dry_run,
        } => {
            let source_profile = profile_manager.get_profile(&profile)?;
            let r = manager.get(&rule)?;

            println!();
            println!("Profile: {}", profile.cyan());
            println!("Rule: {}", rule.cyan());
            if dry_run {
                println!("{}", "(dry run)".yellow());
            }
            println!();

            let executor = RuleExecutor::new(&r, &profile_manager);
            let result = executor.apply(&source_profile, name.as_deref(), dry_run)?;

            if dry_run {
                println!(
                    "Would create new profile: {}",
                    result.new_profile_name.green()
                );
                println!("  Path: {}", result.new_profile_path.display());
            } else {
                println!("{}", "Customization complete!".green());
                println!();
                println!("New profile created: {}", result.new_profile_name.cyan());
                println!("  Path: {}", result.new_profile_path.display());
                println!("  Files modified: {}", result.files_modified);
                println!();
                println!(
                    "Install with: dot-agent install -p {}",
                    result.new_profile_name
                );
            }
        }
        RuleAction::ApplyInstalled {
            rule,
            profile,
            path,
            force,
        } => {
            // Delegate to top-level apply (applies to installed files)
            handle_apply(base_dir, &rule, profile.as_deref(), path.as_deref(), force)?;
        }
    }

    Ok(())
}

fn handle_snapshot(action: SnapshotAction, base_dir: &Path) -> Result<()> {
    use dot_agent_core::snapshot::{SnapshotManager, SnapshotTrigger};

    let snapshot_manager = SnapshotManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    match action {
        SnapshotAction::Save {
            message,
            path,
            global,
        } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            if !target_dir.exists() {
                return Err(DotAgentError::TargetNotFound { path: target_dir });
            }

            println!();
            println!("Creating snapshot of {}...", target_dir.display());

            let snapshot = snapshot_manager.save_target(
                &target_dir,
                SnapshotTrigger::Manual,
                message.as_deref(),
                &[],
            )?;

            println!();
            println!("{} {}", "Snapshot saved:".green(), snapshot.id.cyan());
            println!("  Time: {}", snapshot.display_time());
            println!("  Files: {}", snapshot.file_count);
            if let Some(msg) = &snapshot.message {
                println!("  Message: {}", msg);
            }
        }
        SnapshotAction::List { path, global } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            let snapshots = snapshot_manager.list_target(&target_dir)?;

            if snapshots.is_empty() {
                println!("No snapshots found.");
                println!();
                println!("Create one with: dot-agent snapshot save");
                return Ok(());
            }

            println!();
            println!("Snapshots for {}:", target_dir.display());
            println!();

            for snap in snapshots {
                println!(
                    "  {} {} [{}] ({} files)",
                    snap.id.cyan(),
                    snap.display_time(),
                    snap.trigger.as_str().yellow(),
                    snap.file_count
                );
                if !snap.profiles_affected.is_empty() {
                    println!(
                        "    Profiles: {}",
                        snap.profiles_affected.join(", ").dimmed()
                    );
                }
                if let Some(msg) = &snap.message {
                    println!("    {}", msg.dimmed());
                }
            }
        }
        SnapshotAction::Restore {
            id,
            path,
            global,
            force,
        } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            let snapshot = snapshot_manager.get_target(&target_dir, &id)?;

            println!();
            println!("Restore snapshot {}?", id.cyan());
            println!("  Time: {}", snapshot.display_time());
            println!("  Files: {}", snapshot.file_count);
            println!();
            println!(
                "{}",
                "WARNING: This will replace all files in the target directory.".yellow()
            );

            if !force {
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let (removed, restored) = snapshot_manager.restore_target(&target_dir, &id)?;

            println!();
            println!("{}", "Snapshot restored!".green());
            println!("  Removed: {} files", removed);
            println!("  Restored: {} files", restored);
        }
        SnapshotAction::Diff { id, path, global } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            let diff = snapshot_manager.diff_target(&target_dir, &id)?;

            println!();
            println!("Diff: snapshot {} vs current", id.cyan());
            println!();

            if !diff.modified.is_empty() {
                println!("Modified ({}):", diff.modified.len());
                for f in &diff.modified {
                    println!("  {} {}", "[M]".yellow(), f);
                }
            }

            if !diff.added.is_empty() {
                println!("Added ({}):", diff.added.len());
                for f in &diff.added {
                    println!("  {} {}", "[A]".green(), f);
                }
            }

            if !diff.deleted.is_empty() {
                println!("Deleted ({}):", diff.deleted.len());
                for f in &diff.deleted {
                    println!("  {} {}", "[D]".red(), f);
                }
            }

            if !diff.has_changes() {
                println!("{}", "No changes since snapshot.".green());
            }

            println!();
            println!(
                "Summary: {} unchanged, {} modified, {} added, {} deleted",
                diff.unchanged.len(),
                diff.modified.len(),
                diff.added.len(),
                diff.deleted.len()
            );
        }
        SnapshotAction::Delete {
            id,
            path,
            global,
            force,
        } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            let snapshot = snapshot_manager.get_target(&target_dir, &id)?;

            if !force {
                println!();
                println!("Delete snapshot {}?", id.yellow());
                println!("  Time: {}", snapshot.display_time());
                println!("  Files: {}", snapshot.file_count);
                println!();
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            snapshot_manager.delete_target(&target_dir, &id)?;
            println!();
            println!("{} snapshot {}", "Deleted:".red(), id);
        }
        SnapshotAction::Prune {
            keep,
            path,
            global,
            force,
        } => {
            let target_dir = installer.resolve_target(path.as_deref(), global)?;

            let snapshots = snapshot_manager.list_target(&target_dir)?;
            let to_delete = snapshots.len().saturating_sub(keep);

            if to_delete == 0 {
                println!();
                println!(
                    "No snapshots to prune ({} snapshots, keeping {})",
                    snapshots.len(),
                    keep
                );
                return Ok(());
            }

            println!();
            println!(
                "Prune {} snapshot(s), keeping {} most recent?",
                to_delete, keep
            );

            if !force {
                print!("Type 'yes' to confirm: ");
                io::stdout().flush()?;

                let mut input = String::new();
                io::stdin().read_line(&mut input)?;

                if input.trim() != "yes" {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let deleted = snapshot_manager.prune_target(&target_dir, keep)?;

            println!();
            println!("{} {} snapshot(s)", "Pruned:".green(), deleted.len());
            for id in &deleted {
                println!("  {}", id.dimmed());
            }
        }
    }

    Ok(())
}

fn handle_switch(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    no_snapshot: bool,
    force: bool,
) -> Result<()> {
    use dot_agent_core::snapshot::{SnapshotManager, SnapshotTrigger};

    let profile_manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());
    let snapshot_manager = SnapshotManager::new(base_dir.to_path_buf());

    // Verify new profile exists
    let new_profile = profile_manager.get_profile(profile_name)?;
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Switching to profile: {}", profile_name.cyan());
    println!("Target: {}", target_dir.display());
    println!();

    // Get current installed profiles
    let metadata = Metadata::load(&target_dir)?;
    let current_profiles: Vec<String> = metadata
        .as_ref()
        .map(|m| m.installed.profiles.clone())
        .unwrap_or_default();

    // Use default ignore config for switch operation
    let ignore_config = IgnoreConfig::with_defaults();

    if current_profiles.is_empty() {
        println!(
            "No profiles currently installed. Installing {}...",
            profile_name
        );
        let opts = InstallOptions::new()
            .force(force)
            .ignore_config(ignore_config.clone());
        let result = installer.install(&new_profile, &target_dir, &opts)?;
        println!();
        println!("{} Installed {} files.", "Done:".green(), result.installed);
        return Ok(());
    }

    println!("Current profiles: {}", current_profiles.join(", ").yellow());

    // Create snapshot before switching (unless --no-snapshot)
    if !no_snapshot && target_dir.exists() {
        println!();
        println!("Creating snapshot before switch...");
        let snapshot = snapshot_manager.save_target(
            &target_dir,
            SnapshotTrigger::PreInstall,
            Some(&format!("before switch to {}", profile_name)),
            &[profile_name.to_string()],
        )?;
        println!(
            "  Saved: {} ({} files)",
            snapshot.id.cyan(),
            snapshot.file_count
        );
    }

    // Remove current profiles
    println!();
    println!("Removing current profiles...");
    let remove_opts = InstallOptions::new()
        .force(force)
        .ignore_config(ignore_config.clone());
    for current_name in &current_profiles {
        if let Ok(current_profile) = profile_manager.get_profile(current_name) {
            match installer.remove(&current_profile, &target_dir, &remove_opts) {
                Ok((removed, _, _)) => {
                    println!(
                        "  {} {} ({} files)",
                        "Removed:".red(),
                        current_name,
                        removed
                    );
                }
                Err(e) => {
                    println!("  {} removing {}: {}", "Warning:".yellow(), current_name, e);
                }
            }
        }
    }

    // Install new profile
    println!();
    println!("Installing {}...", profile_name);
    let install_opts = InstallOptions::new()
        .force(force)
        .ignore_config(ignore_config.clone());
    let result = installer.install(&new_profile, &target_dir, &install_opts)?;

    println!();
    println!("{}", "Switch complete!".green());
    println!("  Installed: {} files", result.installed);
    if result.skipped > 0 {
        println!("  Skipped: {} files", result.skipped);
    }

    Ok(())
}

/// Parse a marketplace plugin reference in the format "plugin@marketplace"
/// Returns (plugin_name, marketplace_name) if the format matches
fn parse_marketplace_ref(input: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = input.splitn(2, '@').collect();
    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Import a plugin from a marketplace channel as a local profile
fn import_marketplace_plugin(
    base_dir: &Path,
    plugin_name: &str,
    marketplace_name: &str,
    manager: &ProfileManager,
    force: bool,
) -> Result<dot_agent_core::profile::Profile> {
    use dot_agent_core::channel::{ChannelSource, ChannelType};
    use std::fs;

    println!();
    println!(
        "Fetching plugin {} from marketplace {}...",
        plugin_name.cyan(),
        marketplace_name.yellow()
    );

    // Get channel manager and find the marketplace
    let channel_mgr = ChannelManager::new(base_dir.to_path_buf())?;

    let channel = channel_mgr
        .registry()
        .get(marketplace_name)
        .ok_or_else(|| DotAgentError::ChannelNotFound {
            name: marketplace_name.to_string(),
        })?;

    // Verify it's a marketplace channel
    if channel.channel_type != ChannelType::Marketplace {
        return Err(DotAgentError::GitHubApiError {
            message: format!("'{}' is not a marketplace channel", marketplace_name),
        });
    }

    // Get repo from channel source
    let repo = match &channel.source {
        ChannelSource::Marketplace { repo } => repo.clone(),
        _ => {
            return Err(DotAgentError::GitHubApiError {
                message: "Invalid marketplace channel source".to_string(),
            })
        }
    };

    // Get plugin info from marketplace catalog
    let plugin_info = channel_mgr
        .get_marketplace_plugin(marketplace_name, plugin_name)?
        .ok_or_else(|| DotAgentError::GitHubApiError {
            message: format!(
                "Plugin '{}' not found in marketplace '{}'. Run 'channel refresh {}' first.",
                plugin_name, marketplace_name, marketplace_name
            ),
        })?;

    println!("  Plugin: {}", plugin_info.name);
    if let Some(desc) = &plugin_info.description {
        println!("  Description: {}", desc);
    }
    let version = plugin_info
        .version
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    println!("  Version: {}", version);

    // Determine the source URL for the plugin
    let (plugin_url, is_external) = if let Some(github_repo) = plugin_info.source_github_repo() {
        // External GitHub repo (source: "github", repo: "owner/repo")
        (format!("https://github.com/{}", github_repo), true)
    } else if let Some(url) = plugin_info.source_url() {
        // External URL (source: "url", url: "https://...")
        (url.to_string(), true)
    } else if plugin_info.source_path().is_some() {
        // Relative path within the marketplace repo
        (format!("https://github.com/{}", repo), false)
    } else {
        return Err(DotAgentError::GitHubApiError {
            message: format!("Plugin '{}' has unsupported source type", plugin_name),
        });
    };

    // Profile name for the imported plugin (use plugin name only, @ not allowed in profile names)
    let profile_name = plugin_name.to_string();

    // Import the plugin as a profile
    println!("  Importing from: {}", plugin_url);

    // Determine the subdirectory path for the plugin
    let subdir = if is_external {
        None
    } else {
        plugin_info.source_path().map(|p| {
            // Remove leading "./" if present
            p.trim_start_matches("./").to_string()
        })
    };

    // Create temp directory for git clone
    let temp_base = std::env::temp_dir().join("dot-agent-marketplace");
    fs::create_dir_all(&temp_base)?;
    let clone_path = temp_base.join(format!("{}-{}", plugin_name, std::process::id()));

    // Clean up if exists
    if clone_path.exists() {
        fs::remove_dir_all(&clone_path)?;
    }

    let clone_result = Command::new("git")
        .args([
            "clone",
            "--depth",
            "1",
            &plugin_url,
            clone_path.to_str().unwrap(),
        ])
        .output()
        .map_err(DotAgentError::Io)?;

    if !clone_result.status.success() {
        return Err(DotAgentError::Git(format!(
            "Failed to clone {}: {}",
            plugin_url,
            String::from_utf8_lossy(&clone_result.stderr)
        )));
    }

    // Determine source path (plugin subdirectory if relative path)
    let source_path = if let Some(sub) = &subdir {
        clone_path.join(sub)
    } else {
        clone_path.clone()
    };

    if !source_path.exists() {
        // Cleanup
        let _ = fs::remove_dir_all(&clone_path);
        return Err(DotAgentError::TargetNotFound { path: source_path });
    }

    // Import the plugin directory as a profile using marketplace source
    let result = manager.import_profile_from_marketplace(
        &source_path,
        &profile_name,
        force,
        marketplace_name,
        plugin_name,
        &version,
    );

    // Cleanup temp directory
    let _ = fs::remove_dir_all(&clone_path);

    result?;

    println!("  {} Imported as profile: {}", "[OK]".green(), profile_name);

    // Write inline configuration files if plugin has them (strict: false pattern)
    if plugin_info.has_inline_config() {
        let profile = manager.get_profile(&profile_name)?;
        let written = plugin_info.write_config_files(&profile.path)?;
        if !written.is_empty() {
            println!(
                "  {} Generated config files: {}",
                "[OK]".green(),
                written.join(", ")
            );
        }
    }

    // Return the newly imported profile
    manager.get_profile(&profile_name)
}
