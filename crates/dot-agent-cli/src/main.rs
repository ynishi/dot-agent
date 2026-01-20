use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use colored::Colorize;

use dot_agent_core::installer::{FileStatus, Installer};
use dot_agent_core::metadata::Metadata;
use dot_agent_core::profile::ProfileManager;
use dot_agent_core::{DotAgentError, Result};

mod args;
use args::{Cli, Commands, ProfileAction, Shell};

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
        Some(Commands::Completions { shell }) => {
            handle_completions(shell);
            Ok(())
        }
        Some(Commands::Install {
            profile,
            target,
            global,
            force,
            dry_run,
            no_prefix,
        }) => handle_install(
            &base_dir,
            &profile,
            target.as_deref(),
            global,
            force,
            dry_run,
            no_prefix,
        ),
        Some(Commands::Upgrade {
            profile,
            target,
            global,
            force,
            dry_run,
            no_prefix,
        }) => handle_upgrade(
            &base_dir,
            &profile,
            target.as_deref(),
            global,
            force,
            dry_run,
            no_prefix,
        ),
        Some(Commands::Diff {
            profile,
            target,
            global,
        }) => handle_diff(&base_dir, &profile, target.as_deref(), global),
        Some(Commands::Remove {
            profile,
            target,
            global,
            force,
            dry_run,
        }) => handle_remove(
            &base_dir,
            &profile,
            target.as_deref(),
            global,
            force,
            dry_run,
        ),
        Some(Commands::Status { target, global }) => {
            handle_status(&base_dir, target.as_deref(), global)
        }
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

fn handle_profile(action: ProfileAction, base_dir: &Path) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());

    match action {
        ProfileAction::Add { name } => {
            let profile = manager.create_profile(&name)?;
            println!();
            println!("{} {}", "Created:".green(), profile.path.display());
            println!();
            println!("Next steps:");
            println!(
                "  1. Add skills:       mkdir {}/skills/",
                profile.path.display()
            );
            println!(
                "  2. Add commands:     mkdir {}/commands/",
                profile.path.display()
            );
            println!(
                "  3. Add CLAUDE.md:    touch {}/CLAUDE.md",
                profile.path.display()
            );
            println!(
                "  4. Install to project: dot-agent install -p {} ~/your-project",
                name
            );
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
    }

    Ok(())
}

fn handle_install(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
    no_prefix: bool,
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

fn handle_upgrade(
    base_dir: &Path,
    profile_name: &str,
    target: Option<&Path>,
    global: bool,
    force: bool,
    dry_run: bool,
    no_prefix: bool,
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
) -> Result<()> {
    let manager = ProfileManager::new(base_dir.to_path_buf());
    let installer = Installer::new(base_dir.to_path_buf());

    let profile = manager.get_profile(profile_name)?;
    let target_dir = installer.resolve_target(target, global)?;

    println!();
    println!("Profile: {}", profile_name.cyan());
    println!("Target: {}", target_dir.display());
    println!();

    let result = installer.diff(&profile, &target_dir)?;

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

    let (removed, kept) =
        installer.remove(&profile, &target_dir, force, dry_run, Some(&on_file))?;

    println!();
    println!("Summary:");
    println!("  Removed: {}", removed);
    println!("  Kept: {} (user files)", kept);
    println!();
    println!("{}", "Removal complete.".green());

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
