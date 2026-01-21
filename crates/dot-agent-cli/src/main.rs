use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;

use dot_agent_core::config::Config;
use dot_agent_core::installer::{FileStatus, Installer};
use dot_agent_core::metadata::Metadata;
use dot_agent_core::profile::{IgnoreConfig, ProfileManager};
use dot_agent_core::{DotAgentError, Result};

mod args;
use args::{
    Cli, Commands, ConfigAction, ProfileAction, ProfileSnapshotAction, RuleAction, Shell,
    SnapshotAction,
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
        Some(Commands::Profile { action }) => handle_profile(action, &base_dir),
        Some(Commands::Config { action }) => handle_config(action, &base_dir),
        Some(Commands::Search { query, limit }) => handle_search(&query, limit),
        Some(Commands::Completions { shell }) => {
            handle_completions(shell);
            Ok(())
        }
        Some(Commands::Install {
            profile,
            path,
            global,
            force,
            dry_run,
            no_prefix,
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
            build_ignore_config(&base_dir, &include, &exclude),
        ),
        Some(Commands::Upgrade {
            profile,
            path,
            global,
            force,
            dry_run,
            no_prefix,
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
            include,
            exclude,
        }) => handle_remove(
            &base_dir,
            &profile,
            path.as_deref(),
            global,
            force,
            dry_run,
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

fn handle_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let shell = match shell {
        Shell::Bash => clap_complete::Shell::Bash,
        Shell::Zsh => clap_complete::Shell::Zsh,
        Shell::Fish => clap_complete::Shell::Fish,
        Shell::PowerShell => clap_complete::Shell::PowerShell,
        Shell::Elvish => clap_complete::Shell::Elvish,
    };
    generate(shell, &mut cmd, "dot-agent", &mut io::stdout());
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

fn handle_search(query: &str, limit: usize) -> Result<()> {
    use std::process::Command;

    // Expand query with related keywords (pick best match)
    let query_lower = query.to_lowercase();
    let has_claude = query_lower.contains("claude");
    let has_skills = query_lower.contains("skills") || query_lower.contains("skill");

    let search_query = match (has_claude, has_skills) {
        (true, true) => query.to_string(),
        (true, false) => format!("{} skills", query),
        (false, true) => format!("{} claude", query),
        (false, false) => format!("{} claude skills", query),
    };

    let output = Command::new("gh")
        .args([
            "search",
            "repos",
            &search_query,
            "--limit",
            &limit.to_string(),
            "--json",
            "name,owner,description,url,stargazersCount",
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let json: serde_json::Value =
                serde_json::from_slice(&output.stdout).unwrap_or_default();

            if let Some(repos) = json.as_array() {
                if repos.is_empty() {
                    println!("No results found for: {}", query);
                    return Ok(());
                }

                println!();
                println!("{}", "Search Results:".cyan().bold());
                println!();

                for (i, repo) in repos.iter().enumerate() {
                    let name = repo["name"].as_str().unwrap_or("?");
                    let owner = repo["owner"]["login"].as_str().unwrap_or("?");
                    let desc = repo["description"]
                        .as_str()
                        .unwrap_or("No description")
                        .chars()
                        .take(60)
                        .collect::<String>();
                    let stars = repo["stargazersCount"].as_u64().unwrap_or(0);
                    let url = repo["url"].as_str().unwrap_or("");

                    println!(
                        "{}. {}/{} {} {}",
                        (i + 1).to_string().bold(),
                        owner.yellow(),
                        name.cyan(),
                        format!("★{}", stars).yellow(),
                        if desc.len() >= 60 {
                            format!("{}...", desc)
                        } else {
                            desc
                        }
                    );
                    println!("   {}", url.dimmed());
                    println!();
                }

                println!("{}", "To import:".dimmed());
                println!(
                    "  {}",
                    "dot-agent profile import <url> --name <profile-name>".dimmed()
                );
            }
        }
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("gh: command not found") || stderr.contains("not found") {
                eprintln!(
                    "{} GitHub CLI (gh) not found. Install: {}",
                    "[ERROR]".red().bold(),
                    "brew install gh".cyan()
                );
            } else {
                eprintln!("{} {}", "[ERROR]".red().bold(), stderr);
            }
        }
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                eprintln!(
                    "{} GitHub CLI (gh) not found. Install: {}",
                    "[ERROR]".red().bold(),
                    "brew install gh".cyan()
                );
            } else {
                return Err(e.into());
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
        ProfileAction::Show { name } => {
            let profile = manager.get_profile(&name)?;

            println!();
            println!("Profile: {}", profile.name.cyan().bold());
            println!("Path: {}", profile.path.display());
            println!();

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
    if no_prefix {
        println!("{}", "(no prefix)".yellow());
    }
    println!();
    println!("Installing...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "OK" => format!("[{}]", status).green(),
            "SKIP" => format!("[{}]", status).yellow(),
            "WARN" => format!("[{}]", status).yellow().bold(),
            "CONFLICT" => format!("[{}]", status).red().bold(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let result = installer.install(
        &profile,
        &target_dir,
        force,
        dry_run,
        no_prefix,
        &ignore_config,
        Some(&on_file),
    )?;

    println!();
    println!("Summary:");
    println!("  Installed: {}", result.installed);
    println!("  Skipped: {}", result.skipped);
    println!("  Conflicts: {}", result.conflicts);

    if result.conflicts > 0 {
        println!();
        return Err(DotAgentError::Conflict { path: target_dir });
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
    println!();
    println!("Checking for updates...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "OK" => format!("[{}]", status).green(),
            "NEW" => format!("[{}]", status).green(),
            "UPDATE" => format!("[{}]", status).cyan(),
            "SKIP" => format!("[{}]", status).yellow(),
            "WARN" => format!("[{}]", status).yellow().bold(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let (updated, new, skipped, unchanged) = installer.upgrade(
        &profile,
        &target_dir,
        force,
        dry_run,
        no_prefix,
        &ignore_config,
        Some(&on_file),
    )?;

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

fn handle_remove(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
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
    println!();
    println!("Checking for local modifications...");

    let on_file = |status: &str, path: &str| {
        let status_str = match status {
            "KEEP" => format!("[{}]", status).blue(),
            "DEL" => format!("[{}]", status).red(),
            _ => format!("[{}]", status).normal(),
        };
        println!("  {} {}", status_str, path);
    };

    let (removed, kept) = installer.remove(
        &profile,
        &target_dir,
        force,
        dry_run,
        &ignore_config,
        Some(&on_file),
    )?;

    println!();
    println!("Summary:");
    println!("  Removed: {}", removed);
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
                let file_count = meta
                    .files
                    .keys()
                    .filter(|f| {
                        f.starts_with(&format!("{}:", profile)) || meta.files.contains_key(*f)
                    })
                    .count();
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

    // Import
    let profile = manager.import_profile(&import_path, &profile_name, force)?;

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
        let result = installer.install(
            &new_profile,
            &target_dir,
            force,
            false,
            false,
            &ignore_config,
            None,
        )?;
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
    for current_name in &current_profiles {
        if let Ok(current_profile) = profile_manager.get_profile(current_name) {
            match installer.remove(
                &current_profile,
                &target_dir,
                force,
                false,
                &ignore_config,
                None,
            ) {
                Ok((removed, _)) => {
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
    let result = installer.install(
        &new_profile,
        &target_dir,
        force,
        false,
        false,
        &ignore_config,
        None,
    )?;

    println!();
    println!("{}", "Switch complete!".green());
    println!("  Installed: {} files", result.installed);
    if result.skipped > 0 {
        println!("  Skipped: {} files", result.skipped);
    }

    Ok(())
}
