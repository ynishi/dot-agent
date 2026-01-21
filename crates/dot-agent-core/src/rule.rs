//! Rule module for profile customization.
//!
//! Rules are simple markdown files that describe how to customize a profile.
//! They are applied to base profiles to create new customized profiles.

use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::error::{DotAgentError, Result};
use crate::profile::{Profile, ProfileManager};

const RULES_DIR: &str = "rules";

// ============================================================================
// Rule Entity
// ============================================================================

/// A customization rule (single markdown file).
#[derive(Debug)]
pub struct Rule {
    pub name: String,
    pub path: PathBuf,
    pub content: String,
}

impl Rule {
    /// Load a rule from its file path.
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Err(DotAgentError::RuleNotFound {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().to_string())
                    .unwrap_or_default(),
            });
        }

        let content = fs::read_to_string(path)?;
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        Ok(Self {
            name,
            path: path.to_path_buf(),
            content,
        })
    }

    /// Get a short summary (first non-empty, non-heading line).
    pub fn summary(&self) -> String {
        self.content
            .lines()
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .unwrap_or("(no description)")
            .chars()
            .take(60)
            .collect()
    }
}

// ============================================================================
// RuleManager
// ============================================================================

/// Manages rule CRUD operations.
pub struct RuleManager {
    base_dir: PathBuf,
}

impl RuleManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn rules_dir(&self) -> PathBuf {
        self.base_dir.join(RULES_DIR)
    }

    fn rule_path(&self, name: &str) -> PathBuf {
        self.rules_dir().join(format!("{}.md", name))
    }

    /// List all registered rules.
    pub fn list(&self) -> Result<Vec<Rule>> {
        let dir = self.rules_dir();
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut rules = Vec::new();
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md") {
                if let Ok(rule) = Rule::load(&path) {
                    rules.push(rule);
                }
            }
        }

        rules.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(rules)
    }

    /// Get a specific rule by name.
    pub fn get(&self, name: &str) -> Result<Rule> {
        let path = self.rule_path(name);
        Rule::load(&path)
    }

    /// Create a new rule with template content.
    pub fn create(&self, name: &str) -> Result<Rule> {
        validate_name(name)?;

        let path = self.rule_path(name);
        if path.exists() {
            return Err(DotAgentError::RuleAlreadyExists {
                name: name.to_string(),
            });
        }

        fs::create_dir_all(self.rules_dir())?;

        let template = generate_rule_template(name);
        fs::write(&path, &template)?;

        Rule::load(&path)
    }

    /// Import a rule from an existing markdown file.
    pub fn import(&self, name: &str, source_file: &Path) -> Result<Rule> {
        validate_name(name)?;

        let path = self.rule_path(name);
        if path.exists() {
            return Err(DotAgentError::RuleAlreadyExists {
                name: name.to_string(),
            });
        }

        fs::create_dir_all(self.rules_dir())?;
        fs::copy(source_file, &path)?;

        Rule::load(&path)
    }

    /// Remove a rule.
    pub fn remove(&self, name: &str) -> Result<()> {
        let rule = self.get(name)?;
        fs::remove_file(&rule.path)?;
        Ok(())
    }

    /// Rename a rule.
    pub fn rename(&self, name: &str, new_name: &str) -> Result<Rule> {
        validate_name(new_name)?;

        let old_path = self.rule_path(name);
        if !old_path.exists() {
            return Err(DotAgentError::RuleNotFound {
                name: name.to_string(),
            });
        }

        let new_path = self.rule_path(new_name);
        if new_path.exists() {
            return Err(DotAgentError::RuleAlreadyExists {
                name: new_name.to_string(),
            });
        }

        fs::rename(&old_path, &new_path)?;
        Rule::load(&new_path)
    }

    /// Update rule content.
    pub fn update(&self, name: &str, content: &str) -> Result<Rule> {
        let path = self.rule_path(name);
        if !path.exists() {
            return Err(DotAgentError::RuleNotFound {
                name: name.to_string(),
            });
        }

        fs::write(&path, content)?;
        Rule::load(&path)
    }
}

// ============================================================================
// RuleExecutor - Applies rule to profile
// ============================================================================

/// Result of rule application.
#[derive(Debug)]
pub struct ApplyResult {
    pub new_profile_name: String,
    pub new_profile_path: PathBuf,
    pub files_modified: usize,
}

/// Executes a rule against a profile to create a new customized profile.
pub struct RuleExecutor<'a> {
    rule: &'a Rule,
    profile_manager: &'a ProfileManager,
}

impl<'a> RuleExecutor<'a> {
    pub fn new(rule: &'a Rule, profile_manager: &'a ProfileManager) -> Self {
        Self {
            rule,
            profile_manager,
        }
    }

    /// Generate the full prompt for AI.
    pub fn generate_prompt(&self, profile: &Profile) -> Result<String> {
        // Collect profile files for context
        let mut files_content = String::new();
        for entry in walkdir::WalkDir::new(&profile.path)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext == "md" || ext == "toml")
            })
        {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                let relative = entry
                    .path()
                    .strip_prefix(&profile.path)
                    .unwrap_or(entry.path());
                files_content.push_str(&format!(
                    "### {}\n```\n{}\n```\n\n",
                    relative.display(),
                    content
                ));
            }
        }

        Ok(format!(
            r#"You are customizing a Claude Code configuration profile.

## Source Profile: {}

### Current Files
{}

## Customization Rule

{}

## Your Task

Apply the customization rule to the profile. Output the changes in this format:

```
ACTION: CREATE|MODIFY|DELETE
FILE: <relative path>
CONTENT:
<file content>
```

Only output file changes. No explanations needed.
"#,
            profile.name, files_content, self.rule.content
        ))
    }

    /// Apply the rule to create a new profile.
    pub fn apply(
        &self,
        profile: &Profile,
        new_name: Option<&str>,
        dry_run: bool,
    ) -> Result<ApplyResult> {
        if !check_claude_cli() {
            return Err(DotAgentError::ClaudeNotFound);
        }

        // Determine new profile name
        let new_profile_name = new_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("{}-{}", profile.name, self.rule.name));

        let new_profile_path = self.profile_manager.profiles_dir().join(&new_profile_name);

        if dry_run {
            return Ok(ApplyResult {
                new_profile_name,
                new_profile_path,
                files_modified: 0,
            });
        }

        // Copy base profile to new location
        let new_profile =
            self.profile_manager
                .import_profile(&profile.path, &new_profile_name, false)?;

        // Generate prompt and execute AI
        let prompt = self.generate_prompt(profile)?;
        let output = execute_claude(&new_profile.path, &prompt)?;

        // Apply changes
        let files_modified = apply_ai_output(&new_profile.path, &output)?;

        Ok(ApplyResult {
            new_profile_name,
            new_profile_path: new_profile.path,
            files_modified,
        })
    }
}

// ============================================================================
// AI Operations
// ============================================================================

/// Extract a rule from an existing profile using AI.
pub fn extract_rule(profile: &Profile, rule_name: &str, manager: &RuleManager) -> Result<Rule> {
    if !check_claude_cli() {
        return Err(DotAgentError::ClaudeNotFound);
    }

    // Collect profile files
    let mut files_content = String::new();
    for entry in walkdir::WalkDir::new(&profile.path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "md"))
    {
        if let Ok(content) = fs::read_to_string(entry.path()) {
            let relative = entry
                .path()
                .strip_prefix(&profile.path)
                .unwrap_or(entry.path());
            files_content.push_str(&format!(
                "### {}\n```\n{}\n```\n\n",
                relative.display(),
                content
            ));
        }
    }

    let extract_prompt = format!(
        r#"Analyze this profile and extract the key customization patterns as a reusable rule.

## Profile: {}

{}

## Task

Create a markdown rule that captures:
1. Language/framework specific patterns
2. Coding style preferences
3. Tool configurations
4. Recommended libraries/crates

Output ONLY the rule content in markdown format. Start with a heading.
"#,
        profile.name, files_content
    );

    let rules_dir = manager.rules_dir();
    fs::create_dir_all(&rules_dir)?;

    let generated_content = execute_claude(&rules_dir, &extract_prompt)?;

    // Create the rule file
    let rule_path = manager.rules_dir().join(format!("{}.md", rule_name));
    fs::write(&rule_path, &generated_content)?;

    Rule::load(&rule_path)
}

/// Generate a rule from natural language instruction.
/// If `rule_name` is None, the AI will generate a suitable name.
pub fn generate_rule(
    instruction: &str,
    rule_name: Option<&str>,
    manager: &RuleManager,
) -> Result<Rule> {
    if !check_claude_cli() {
        return Err(DotAgentError::ClaudeNotFound);
    }

    let rules_dir = manager.rules_dir();
    fs::create_dir_all(&rules_dir)?;

    let (final_name, generated_content) = match rule_name {
        Some(name) => {
            let prompt = format!(
                r##"Create a customization rule based on this instruction:

"{}"

The rule will be used to customize Claude Code configuration profiles.

Output a markdown document that includes:
1. Clear section headings
2. Specific patterns or conventions to follow
3. Any recommended libraries, tools, or configurations

Start with a heading like "# {} Customization Rule"
"##,
                instruction, name
            );
            let content = execute_claude(&rules_dir, &prompt)?;
            (name.to_string(), content)
        }
        None => {
            let prompt = format!(
                r##"Create a customization rule based on this instruction:

"{}"

The rule will be used to customize Claude Code configuration profiles.

IMPORTANT: On the FIRST line, output a suggested rule name in this exact format:
NAME: <kebab-case-name>

The name should be:
- Lowercase kebab-case (e.g., "rust-optimization", "python-style")
- Short and descriptive (2-4 words)
- Based on the instruction content

Then output a markdown document that includes:
1. Clear section headings
2. Specific patterns or conventions to follow
3. Any recommended libraries, tools, or configurations
"##,
                instruction
            );
            let content = execute_claude(&rules_dir, &prompt)?;
            let (name, content) = parse_name_from_output(&content)?;
            (name, content)
        }
    };

    let rule_path = rules_dir.join(format!("{}.md", final_name));
    fs::write(&rule_path, &generated_content)?;

    Rule::load(&rule_path)
}

/// Parse NAME: line from AI output and return (name, remaining_content)
fn parse_name_from_output(output: &str) -> Result<(String, String)> {
    let mut lines = output.lines();

    // Find NAME: line
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("NAME:") {
            let name = name.trim().to_lowercase().replace(' ', "-");
            // Validate name
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                return Err(DotAgentError::RuleNotFound {
                    name: "AI generated invalid rule name".to_string(),
                });
            }
            // Collect remaining content
            let remaining: String = lines.collect::<Vec<_>>().join("\n");
            let content = remaining.trim_start().to_string();
            return Ok((name, content));
        }
    }

    Err(DotAgentError::RuleNotFound {
        name: "AI did not generate NAME: line".to_string(),
    })
}

// ============================================================================
// Helpers
// ============================================================================

fn check_claude_cli() -> bool {
    Command::new("claude")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn execute_claude(working_dir: &Path, prompt: &str) -> Result<String> {
    let mut cmd = Command::new("claude");
    cmd.arg("--print");
    cmd.arg("--dangerously-skip-permissions");
    cmd.current_dir(working_dir);
    cmd.stdin(Stdio::piped());
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| DotAgentError::ClaudeExecutionFailed {
            message: format!("Failed to spawn claude: {}", e),
        })?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(prompt.as_bytes())
            .map_err(|e| DotAgentError::ClaudeExecutionFailed {
                message: format!("Failed to write prompt: {}", e),
            })?;
    }

    let output = child
        .wait_with_output()
        .map_err(|e| DotAgentError::ClaudeExecutionFailed {
            message: format!("Execution failed: {}", e),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(DotAgentError::ClaudeExecutionFailed {
            message: format!("Claude exited with error: {}", stderr),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn apply_ai_output(profile_path: &Path, output: &str) -> Result<usize> {
    let mut files_modified = 0;

    let mut lines = output.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with("ACTION:") {
            let action = line.trim_start_matches("ACTION:").trim();

            let file_line = lines.next().unwrap_or("");
            if !file_line.starts_with("FILE:") {
                continue;
            }
            let file_path = file_line.trim_start_matches("FILE:").trim();

            let content_line = lines.next().unwrap_or("");
            if !content_line.starts_with("CONTENT:") {
                continue;
            }

            let mut content = String::new();
            while let Some(line) = lines.peek() {
                if line.starts_with("ACTION:") {
                    break;
                }
                content.push_str(lines.next().unwrap_or(""));
                content.push('\n');
            }

            let target_path = profile_path.join(file_path);

            match action {
                "CREATE" | "MODIFY" => {
                    if let Some(parent) = target_path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::write(&target_path, content.trim())?;
                    files_modified += 1;
                }
                "DELETE" => {
                    if target_path.exists() {
                        fs::remove_file(&target_path)?;
                        files_modified += 1;
                    }
                }
                _ => {}
            }
        }
    }

    Ok(files_modified)
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(DotAgentError::InvalidRuleName {
            name: name.to_string(),
        });
    }

    let first = name.chars().next().unwrap();
    if !first.is_ascii_alphabetic() {
        return Err(DotAgentError::InvalidRuleName {
            name: name.to_string(),
        });
    }

    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(DotAgentError::InvalidRuleName {
                name: name.to_string(),
            });
        }
    }

    Ok(())
}

fn generate_rule_template(name: &str) -> String {
    format!(
        r#"# {} Customization Rule

## Language
(e.g., Rust, Kotlin, TypeScript)

## Recommended Libraries
- library1
- library2

## Coding Style
- Style guideline 1
- Style guideline 2

## Replace Sections
(Describe what sections to replace and with what content)

## Additional Rules
(Any other customization instructions)
"#,
        name
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name("rust").is_ok());
        assert!(validate_name("my-rule").is_ok());
        assert!(validate_name("python_3").is_ok());
    }

    #[test]
    fn test_validate_name_invalid() {
        assert!(validate_name("").is_err());
        assert!(validate_name("123start").is_err());
        assert!(validate_name("has space").is_err());
        assert!(validate_name("has.dot").is_err());
    }

    #[test]
    fn test_create_rule() {
        let temp = TempDir::new().unwrap();
        let manager = RuleManager::new(temp.path().to_path_buf());

        let rule = manager.create("test").unwrap();
        assert_eq!(rule.name, "test");
        assert!(rule.path.exists());
        assert!(rule.content.contains("# test Customization Rule"));
    }

    #[test]
    fn test_list_rules() {
        let temp = TempDir::new().unwrap();
        let manager = RuleManager::new(temp.path().to_path_buf());

        manager.create("alpha").unwrap();
        manager.create("beta").unwrap();

        let list = manager.list().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "alpha");
        assert_eq!(list[1].name, "beta");
    }

    #[test]
    fn test_remove_rule() {
        let temp = TempDir::new().unwrap();
        let manager = RuleManager::new(temp.path().to_path_buf());

        manager.create("test").unwrap();
        assert!(manager.get("test").is_ok());

        manager.remove("test").unwrap();
        assert!(manager.get("test").is_err());
    }

    #[test]
    fn test_rule_already_exists() {
        let temp = TempDir::new().unwrap();
        let manager = RuleManager::new(temp.path().to_path_buf());

        manager.create("test").unwrap();
        let result = manager.create("test");
        assert!(matches!(
            result,
            Err(DotAgentError::RuleAlreadyExists { .. })
        ));
    }
}
