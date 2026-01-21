use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::error::{DotAgentError, Result};

const PROFILES_DIR: &str = "profiles";
const IGNORED_FILES: &[&str] = &[".git", ".DS_Store", ".gitignore", ".gitkeep"];
const IGNORED_EXTENSIONS: &[&str] = &[];

pub struct Profile {
    pub name: String,
    pub path: PathBuf,
}

impl Profile {
    pub fn new(name: String, path: PathBuf) -> Self {
        Self { name, path }
    }

    /// List all files in the profile directory (relative paths)
    pub fn list_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        for entry in WalkDir::new(&self.path).into_iter().filter_map(|e| e.ok()) {
            let path = entry.path();
            if path.is_file() && !should_ignore(path) {
                if let Ok(relative) = path.strip_prefix(&self.path) {
                    files.push(relative.to_path_buf());
                }
            }
        }

        files.sort();
        Ok(files)
    }

    /// Get contents summary (e.g., "skills (5), commands (3)")
    pub fn contents_summary(&self) -> String {
        let mut summary = Vec::new();

        if let Ok(entries) = fs::read_dir(&self.path) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    let count = WalkDir::new(&path)
                        .into_iter()
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_file() && !should_ignore(e.path()))
                        .count();
                    if count > 0 {
                        summary.push(format!("{} ({})", name, count));
                    }
                } else if path.is_file() {
                    let name = path.file_name().unwrap().to_string_lossy().to_string();
                    if !should_ignore(&path) {
                        summary.push(name);
                    }
                }
            }
        }

        if summary.is_empty() {
            "(empty)".to_string()
        } else {
            summary.join(", ")
        }
    }
}

pub struct ProfileManager {
    base_dir: PathBuf,
}

impl ProfileManager {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    pub fn profiles_dir(&self) -> PathBuf {
        self.base_dir.join(PROFILES_DIR)
    }

    /// Discover all profiles
    pub fn list_profiles(&self) -> Result<Vec<Profile>> {
        let profiles_dir = self.profiles_dir();
        if !profiles_dir.exists() {
            return Ok(Vec::new());
        }

        let mut profiles = Vec::new();
        for entry in fs::read_dir(&profiles_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name().unwrap().to_string_lossy().to_string();
                profiles.push(Profile::new(name, path));
            }
        }

        profiles.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(profiles)
    }

    /// Get a specific profile
    pub fn get_profile(&self, name: &str) -> Result<Profile> {
        let path = self.profiles_dir().join(name);
        if !path.exists() {
            return Err(DotAgentError::ProfileNotFound {
                name: name.to_string(),
            });
        }
        Ok(Profile::new(name.to_string(), path))
    }

    /// Create a new profile
    pub fn create_profile(&self, name: &str) -> Result<Profile> {
        validate_profile_name(name)?;

        let path = self.profiles_dir().join(name);
        if path.exists() {
            return Err(DotAgentError::ProfileAlreadyExists {
                name: name.to_string(),
            });
        }

        fs::create_dir_all(&path)?;
        Ok(Profile::new(name.to_string(), path))
    }

    /// Remove a profile
    pub fn remove_profile(&self, name: &str) -> Result<()> {
        let profile = self.get_profile(name)?;
        fs::remove_dir_all(&profile.path)?;
        Ok(())
    }

    /// Copy an existing profile to a new name
    pub fn copy_profile(&self, source_name: &str, dest_name: &str, force: bool) -> Result<Profile> {
        let source = self.get_profile(source_name)?;
        validate_profile_name(dest_name)?;

        let dest_path = self.profiles_dir().join(dest_name);

        if dest_path.exists() {
            if !force {
                return Err(DotAgentError::ProfileAlreadyExists {
                    name: dest_name.to_string(),
                });
            }
            fs::remove_dir_all(&dest_path)?;
        }

        copy_dir_recursive(&source.path, &dest_path)?;

        Ok(Profile::new(dest_name.to_string(), dest_path))
    }

    /// Import a directory as a profile
    pub fn import_profile(&self, source: &Path, name: &str, force: bool) -> Result<Profile> {
        validate_profile_name(name)?;

        if !source.exists() {
            return Err(DotAgentError::TargetNotFound {
                path: source.to_path_buf(),
            });
        }

        let dest = self.profiles_dir().join(name);

        if dest.exists() {
            if !force {
                return Err(DotAgentError::ProfileAlreadyExists {
                    name: name.to_string(),
                });
            }
            fs::remove_dir_all(&dest)?;
        }

        // Ensure profiles directory exists
        fs::create_dir_all(self.profiles_dir())?;

        // Copy directory recursively
        copy_dir_recursive(source, &dest)?;

        Ok(Profile::new(name.to_string(), dest))
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;

    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();
        let relative = src_path.strip_prefix(src).unwrap();
        let dst_path = dst.join(relative);

        if src_path.is_dir() {
            fs::create_dir_all(&dst_path)?;
        } else if src_path.is_file() && !should_ignore(src_path) {
            if let Some(parent) = dst_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(src_path, &dst_path)?;
        }
    }

    Ok(())
}

fn validate_profile_name(name: &str) -> Result<()> {
    if name.is_empty() || name.len() > 64 {
        return Err(DotAgentError::InvalidProfileName {
            name: name.to_string(),
        });
    }

    let first_char = name.chars().next().unwrap();
    if !first_char.is_ascii_alphabetic() {
        return Err(DotAgentError::InvalidProfileName {
            name: name.to_string(),
        });
    }

    for c in name.chars() {
        if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
            return Err(DotAgentError::InvalidProfileName {
                name: name.to_string(),
            });
        }
    }

    Ok(())
}

fn should_ignore(path: &Path) -> bool {
    if let Some(name) = path.file_name() {
        let name = name.to_string_lossy();
        if IGNORED_FILES.contains(&name.as_ref()) {
            return true;
        }
    }

    if let Some(ext) = path.extension() {
        let ext = ext.to_string_lossy();
        if IGNORED_EXTENSIONS.contains(&ext.as_ref()) {
            return true;
        }
    }

    false
}
