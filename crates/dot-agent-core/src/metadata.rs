use std::collections::HashMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;

const META_FILENAME: &str = ".dot-agent-meta.toml";

#[derive(Debug, Serialize, Deserialize)]
pub struct Metadata {
    pub installed: InstalledInfo,
    pub files: HashMap<String, String>,
    /// Tracks merged JSON entries per profile: profile_name -> file_path -> [json_paths]
    #[serde(default)]
    pub merged: HashMap<String, HashMap<String, Vec<String>>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstalledInfo {
    pub installed_at: DateTime<Utc>,
    pub profiles: Vec<String>,
    pub base_dir: String,
}

impl Metadata {
    pub fn new(base_dir: &Path) -> Self {
        Self {
            installed: InstalledInfo {
                installed_at: Utc::now(),
                profiles: Vec::new(),
                base_dir: base_dir.display().to_string(),
            },
            files: HashMap::new(),
            merged: HashMap::new(),
        }
    }

    pub fn load(target: &Path) -> Result<Option<Self>> {
        let meta_path = target.join(META_FILENAME);
        if !meta_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&meta_path)?;
        let meta: Metadata = toml::from_str(&content)?;
        Ok(Some(meta))
    }

    pub fn save(&self, target: &Path) -> Result<()> {
        let meta_path = target.join(META_FILENAME);
        let content = toml::to_string_pretty(self)?;
        fs::write(&meta_path, content)?;
        Ok(())
    }

    pub fn add_profile(&mut self, name: &str) {
        if !self.installed.profiles.contains(&name.to_string()) {
            self.installed.profiles.push(name.to_string());
        }
    }

    pub fn remove_profile(&mut self, name: &str) {
        self.installed.profiles.retain(|p| p != name);
    }

    pub fn add_file(&mut self, path: &str, hash: &str) {
        self.files.insert(path.to_string(), hash.to_string());
    }

    pub fn remove_file(&mut self, path: &str) {
        self.files.remove(path);
    }

    pub fn get_file_hash(&self, path: &str) -> Option<&String> {
        self.files.get(path)
    }

    /// Record merged JSON paths for a profile
    pub fn add_merged(&mut self, profile: &str, file_path: &str, json_paths: Vec<String>) {
        let profile_merged = self.merged.entry(profile.to_string()).or_default();
        profile_merged.insert(file_path.to_string(), json_paths);
    }

    /// Get merged JSON paths for a profile and file
    pub fn get_merged(&self, profile: &str, file_path: &str) -> Option<&Vec<String>> {
        self.merged.get(profile).and_then(|m| m.get(file_path))
    }

    /// Get all merged files for a profile
    pub fn get_merged_files(&self, profile: &str) -> Option<&HashMap<String, Vec<String>>> {
        self.merged.get(profile)
    }

    /// Remove all merged entries for a profile
    pub fn remove_merged(&mut self, profile: &str) {
        self.merged.remove(profile);
    }
}

pub fn compute_hash(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let result = hasher.finalize();
    format!("sha256:{}", hex::encode(result))
}

pub fn compute_file_hash(path: &Path) -> Result<String> {
    let content = fs::read(path)?;
    Ok(compute_hash(&content))
}
