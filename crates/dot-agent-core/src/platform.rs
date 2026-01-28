//! Platform abstraction for multi-platform support
//!
//! Supports installing profiles/skills to different AI coding assistant platforms:
//! - Claude Code (~/.claude/)
//! - Codex CLI (~/.codex/skills/)

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Target platform for installation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Platform {
    /// Claude Code (~/.claude/)
    #[default]
    Claude,
    /// OpenAI Codex CLI (~/.codex/skills/)
    Codex,
}

/// Directories supported by Claude Code
pub const CLAUDE_SUPPORTED_DIRS: &[&str] = &[
    "skills", "agents", "hooks", "rules", "commands",
    "settings",
    // Also supports root files like CLAUDE.md, settings.json, mcp.json, etc.
];

/// Directories supported by Codex CLI (skills only, using SKILL.md format)
pub const CODEX_SUPPORTED_DIRS: &[&str] = &["skills"];

/// Files that are platform-specific and should be filtered
pub const CLAUDE_SPECIFIC_FILES: &[&str] = &[
    "CLAUDE.md",
    "settings.json",
    "settings.local.json",
    "mcp.json",
    ".mcp.json",
    "hooks.json",
];

impl Platform {
    /// Get the base directory for this platform
    pub fn base_dir(&self) -> PathBuf {
        let home = dirs::home_dir().expect("Could not determine home directory");
        match self {
            Self::Claude => home.join(".claude"),
            Self::Codex => home.join(".codex").join("skills"),
        }
    }

    /// Get platform name for display
    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex CLI",
        }
    }

    /// Get short identifier
    pub fn id(&self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    /// Get all supported platforms
    pub fn all() -> &'static [Platform] {
        &[Platform::Claude, Platform::Codex]
    }

    /// Get supported directories for this platform
    pub fn supported_dirs(&self) -> &'static [&'static str] {
        match self {
            Self::Claude => CLAUDE_SUPPORTED_DIRS,
            Self::Codex => CODEX_SUPPORTED_DIRS,
        }
    }

    /// Check if a directory/file path is supported by this platform
    ///
    /// Returns true if:
    /// - Claude: Almost everything is supported
    /// - Codex: Only skills/ directory is supported
    pub fn supports_path(&self, path: &std::path::Path) -> bool {
        match self {
            Self::Claude => true, // Claude supports everything
            Self::Codex => {
                // Codex only supports skills/
                if let Some(first_component) = path.components().next() {
                    let dir_name = first_component.as_os_str().to_string_lossy();
                    // Check if it's a skills directory or a skill file at root
                    dir_name == "skills" || path.extension().is_some_and(|ext| ext == "md")
                } else {
                    false
                }
            }
        }
    }

    /// Check if a file is platform-specific (should not be copied to other platforms)
    pub fn is_platform_specific_file(&self, filename: &str) -> bool {
        match self {
            Self::Claude => false, // Claude owns these files
            Self::Codex => CLAUDE_SPECIFIC_FILES.contains(&filename),
        }
    }
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for Platform {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude" | "claude-code" => Ok(Self::Claude),
            "codex" | "codex-cli" => Ok(Self::Codex),
            _ => Err(format!("Unknown platform: {}", s)),
        }
    }
}

/// Installation target specifying which platform(s) to install to
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallTarget {
    /// Install to a single platform
    Single(Platform),
    /// Install to all supported platforms
    All,
    /// Install to custom path (ignores platform)
    Custom(PathBuf),
}

impl Default for InstallTarget {
    fn default() -> Self {
        Self::Single(Platform::default())
    }
}

impl InstallTarget {
    /// Create target for Claude Code
    pub fn claude() -> Self {
        Self::Single(Platform::Claude)
    }

    /// Create target for Codex CLI
    pub fn codex() -> Self {
        Self::Single(Platform::Codex)
    }

    /// Create target for all platforms
    pub fn all() -> Self {
        Self::All
    }

    /// Create target for custom path
    pub fn custom(path: PathBuf) -> Self {
        Self::Custom(path)
    }

    /// Get the platforms this target represents
    pub fn platforms(&self) -> Vec<Platform> {
        match self {
            Self::Single(p) => vec![*p],
            Self::All => Platform::all().to_vec(),
            Self::Custom(_) => vec![], // Custom path doesn't map to a platform
        }
    }

    /// Get installation directories for this target
    pub fn install_dirs(&self) -> Vec<PathBuf> {
        match self {
            Self::Single(p) => vec![p.base_dir()],
            Self::All => Platform::all().iter().map(|p| p.base_dir()).collect(),
            Self::Custom(path) => vec![path.clone()],
        }
    }

    /// Check if this target includes a specific platform
    pub fn includes(&self, platform: Platform) -> bool {
        match self {
            Self::Single(p) => *p == platform,
            Self::All => true,
            Self::Custom(_) => false,
        }
    }

    /// Check if this is a multi-platform target
    pub fn is_multi(&self) -> bool {
        matches!(self, Self::All)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_base_dir() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(Platform::Claude.base_dir(), home.join(".claude"));
        assert_eq!(Platform::Codex.base_dir(), home.join(".codex/skills"));
    }

    #[test]
    fn platform_from_str() {
        assert_eq!("claude".parse::<Platform>().unwrap(), Platform::Claude);
        assert_eq!("codex".parse::<Platform>().unwrap(), Platform::Codex);
        assert!("unknown".parse::<Platform>().is_err());
    }

    #[test]
    fn install_target_platforms() {
        assert_eq!(InstallTarget::claude().platforms(), vec![Platform::Claude]);
        assert_eq!(InstallTarget::codex().platforms(), vec![Platform::Codex]);
        assert_eq!(
            InstallTarget::all().platforms(),
            vec![Platform::Claude, Platform::Codex]
        );
    }

    #[test]
    fn install_target_includes() {
        assert!(InstallTarget::claude().includes(Platform::Claude));
        assert!(!InstallTarget::claude().includes(Platform::Codex));
        assert!(InstallTarget::all().includes(Platform::Claude));
        assert!(InstallTarget::all().includes(Platform::Codex));
    }
}
