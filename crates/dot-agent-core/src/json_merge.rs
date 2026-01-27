//! JSON merge functionality for dot-agent install/remove operations.
//!
//! Handles merging of JSON configuration files (hooks.json, mcp.json, settings.json)
//! during profile installation, and removal of merged entries during uninstallation.

use std::fs;
use std::path::Path;

use serde_json::{Map, Value};

use crate::error::{DotAgentError, Result};

/// Files that support JSON merging
const MERGEABLE_FILES: &[&str] = &[
    "hooks.json",
    "mcp.json",
    "settings.json",
    "settings.local.json",
];

/// Marker key to identify which profile added an entry
const PROFILE_MARKER: &str = "_dot_agent_profile";

/// Check if a file path is a mergeable JSON file
pub fn is_mergeable_json(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| MERGEABLE_FILES.contains(&name))
}

/// Record of what was merged into a JSON file
#[derive(Debug, Clone, Default)]
pub struct MergeRecord {
    /// JSON paths that were added (e.g., "hooks.SessionStart[0]", "mcpServers.my-server")
    pub added_paths: Vec<String>,
}

/// Result of a merge operation
#[derive(Debug)]
pub struct MergeResult {
    /// The merged JSON content
    pub content: String,
    /// Record of what was merged (for metadata tracking)
    pub record: MergeRecord,
    /// Whether any actual changes were made
    pub changed: bool,
}

/// Result of an unmerge operation
#[derive(Debug)]
pub struct UnmergeResult {
    /// The JSON content after removing merged entries
    pub content: String,
    /// Paths that were removed
    pub removed_paths: Vec<String>,
    /// Whether any actual changes were made
    pub changed: bool,
}

/// Merge source JSON into target JSON, marking entries with profile name
pub fn merge_json(
    target_content: Option<&str>,
    source_content: &str,
    profile_name: &str,
) -> Result<MergeResult> {
    let source: Value =
        serde_json::from_str(source_content).map_err(|e| DotAgentError::JsonParseError {
            message: e.to_string(),
        })?;

    let mut target: Value = match target_content {
        Some(content) if !content.trim().is_empty() => {
            serde_json::from_str(content).map_err(|e| DotAgentError::JsonParseError {
                message: e.to_string(),
            })?
        }
        _ => Value::Object(Map::new()),
    };

    let mut record = MergeRecord::default();
    let changed = merge_value(
        &mut target,
        &source,
        profile_name,
        "",
        &mut record.added_paths,
    );

    let content =
        serde_json::to_string_pretty(&target).map_err(|e| DotAgentError::JsonParseError {
            message: e.to_string(),
        })?;

    Ok(MergeResult {
        content,
        record,
        changed,
    })
}

/// Remove entries that were added by a specific profile
pub fn unmerge_json(target_content: &str, profile_name: &str) -> Result<UnmergeResult> {
    let mut target: Value =
        serde_json::from_str(target_content).map_err(|e| DotAgentError::JsonParseError {
            message: e.to_string(),
        })?;

    let mut removed_paths = Vec::new();
    let changed = unmerge_value(&mut target, profile_name, "", &mut removed_paths);

    // Clean up empty objects and arrays
    cleanup_empty(&mut target);

    let content =
        serde_json::to_string_pretty(&target).map_err(|e| DotAgentError::JsonParseError {
            message: e.to_string(),
        })?;

    Ok(UnmergeResult {
        content,
        removed_paths,
        changed,
    })
}

/// Recursively merge source into target, marking arrays entries with profile marker
fn merge_value(
    target: &mut Value,
    source: &Value,
    profile_name: &str,
    path: &str,
    added_paths: &mut Vec<String>,
) -> bool {
    let mut changed = false;

    match (target, source) {
        (Value::Object(target_map), Value::Object(source_map)) => {
            for (key, source_val) in source_map {
                // Skip schema fields
                if key == "$schema" {
                    continue;
                }

                let new_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };

                if let Some(target_val) = target_map.get_mut(key) {
                    // Key exists, recurse
                    if merge_value(target_val, source_val, profile_name, &new_path, added_paths) {
                        changed = true;
                    }
                } else {
                    // Key doesn't exist, add it (with marker if it's an object)
                    let marked_val = mark_value(source_val.clone(), profile_name);
                    target_map.insert(key.clone(), marked_val);
                    added_paths.push(new_path);
                    changed = true;
                }
            }
        }
        (Value::Array(target_arr), Value::Array(source_arr)) => {
            // For arrays, append new items with profile marker
            for (i, source_item) in source_arr.iter().enumerate() {
                // Check if this exact item (minus marker) already exists
                if !array_contains_equivalent(target_arr, source_item) {
                    let mut marked_item = source_item.clone();
                    if let Value::Object(ref mut obj) = marked_item {
                        obj.insert(
                            PROFILE_MARKER.to_string(),
                            Value::String(profile_name.to_string()),
                        );
                    }
                    target_arr.push(marked_item);
                    added_paths.push(format!("{}[{}]", path, i));
                    changed = true;
                }
            }
        }
        _ => {
            // For other types, don't overwrite existing values
        }
    }

    changed
}

/// Check if array contains an equivalent item (ignoring profile marker)
fn array_contains_equivalent(arr: &[Value], item: &Value) -> bool {
    arr.iter().any(|existing| values_equivalent(existing, item))
}

/// Check if two values are equivalent (ignoring profile marker)
fn values_equivalent(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Object(a_map), Value::Object(b_map)) => {
            // Compare all keys except profile marker
            let a_keys: Vec<_> = a_map.keys().filter(|k| *k != PROFILE_MARKER).collect();
            let b_keys: Vec<_> = b_map.keys().filter(|k| *k != PROFILE_MARKER).collect();

            if a_keys.len() != b_keys.len() {
                return false;
            }

            for key in &a_keys {
                match (a_map.get(*key), b_map.get(*key)) {
                    (Some(av), Some(bv)) => {
                        if !values_equivalent(av, bv) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            true
        }
        (Value::Array(a_arr), Value::Array(b_arr)) => {
            if a_arr.len() != b_arr.len() {
                return false;
            }
            a_arr
                .iter()
                .zip(b_arr.iter())
                .all(|(a, b)| values_equivalent(a, b))
        }
        _ => a == b,
    }
}

/// Add profile marker to a value
fn mark_value(mut value: Value, profile_name: &str) -> Value {
    match &mut value {
        Value::Object(obj) => {
            obj.insert(
                PROFILE_MARKER.to_string(),
                Value::String(profile_name.to_string()),
            );
        }
        Value::Array(arr) => {
            // Mark each object in the array
            for item in arr.iter_mut() {
                if let Value::Object(obj) = item {
                    obj.insert(
                        PROFILE_MARKER.to_string(),
                        Value::String(profile_name.to_string()),
                    );
                }
            }
        }
        _ => {}
    }
    value
}

/// Recursively remove entries marked with the given profile
fn unmerge_value(
    target: &mut Value,
    profile_name: &str,
    path: &str,
    removed_paths: &mut Vec<String>,
) -> bool {
    let mut changed = false;

    match target {
        Value::Object(map) => {
            // Check if this object itself is marked by the profile
            if let Some(Value::String(marker)) = map.get(PROFILE_MARKER) {
                if marker == profile_name {
                    // This entire object was added by the profile
                    // The parent should handle removal
                    return true;
                }
            }

            // Collect keys to remove
            let keys_to_remove: Vec<_> = map
                .iter()
                .filter_map(|(key, val)| {
                    if key == PROFILE_MARKER {
                        return None;
                    }
                    if let Value::Object(obj) = val {
                        if let Some(Value::String(marker)) = obj.get(PROFILE_MARKER) {
                            if marker == profile_name {
                                return Some(key.clone());
                            }
                        }
                    }
                    None
                })
                .collect();

            for key in keys_to_remove {
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };
                map.remove(&key);
                removed_paths.push(key_path);
                changed = true;
            }

            // Recurse into remaining values
            for (key, val) in map.iter_mut() {
                if key == PROFILE_MARKER {
                    continue;
                }
                let key_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{}.{}", path, key)
                };
                if unmerge_value(val, profile_name, &key_path, removed_paths) {
                    changed = true;
                }
            }
        }
        Value::Array(arr) => {
            // Remove array items marked with the profile
            let original_len = arr.len();
            arr.retain(|item| {
                if let Value::Object(obj) = item {
                    if let Some(Value::String(marker)) = obj.get(PROFILE_MARKER) {
                        if marker == profile_name {
                            return false;
                        }
                    }
                }
                true
            });

            if arr.len() != original_len {
                removed_paths.push(format!("{}[*]", path));
                changed = true;
            }

            // Recurse into remaining items
            for (i, item) in arr.iter_mut().enumerate() {
                let item_path = format!("{}[{}]", path, i);
                if unmerge_value(item, profile_name, &item_path, removed_paths) {
                    changed = true;
                }
            }
        }
        _ => {}
    }

    changed
}

/// Remove empty objects and arrays recursively
fn cleanup_empty(value: &mut Value) {
    match value {
        Value::Object(map) => {
            // First recurse
            for val in map.values_mut() {
                cleanup_empty(val);
            }
            // Then remove empty children
            map.retain(|_, v| !is_empty_container(v));
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                cleanup_empty(item);
            }
            arr.retain(|v| !is_empty_container(v));
        }
        _ => {}
    }
}

fn is_empty_container(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.is_empty(),
        Value::Array(arr) => arr.is_empty(),
        _ => false,
    }
}

/// Merge a JSON file from source profile into target
pub fn merge_json_file(
    target_path: &Path,
    source_path: &Path,
    profile_name: &str,
) -> Result<MergeResult> {
    let source_content = fs::read_to_string(source_path)?;

    let target_content = if target_path.exists() {
        Some(fs::read_to_string(target_path)?)
    } else {
        None
    };

    merge_json(target_content.as_deref(), &source_content, profile_name)
}

/// Remove profile's merged entries from a JSON file
pub fn unmerge_json_file(target_path: &Path, profile_name: &str) -> Result<Option<UnmergeResult>> {
    if !target_path.exists() {
        return Ok(None);
    }

    let content = fs::read_to_string(target_path)?;
    let result = unmerge_json(&content, profile_name)?;

    Ok(Some(result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_hooks_json() {
        let target = r#"{
            "hooks": {
                "PreToolUse": [
                    {"matcher": "existing", "hooks": []}
                ]
            }
        }"#;

        let source = r#"{
            "hooks": {
                "SessionStart": [
                    {"matcher": "startup", "hooks": [{"type": "command", "command": "echo hi"}]}
                ]
            }
        }"#;

        let result = merge_json(Some(target), source, "superpowers").unwrap();

        let merged: Value = serde_json::from_str(&result.content).unwrap();

        // Check that existing PreToolUse is preserved
        assert!(merged["hooks"]["PreToolUse"].is_array());

        // Check that SessionStart was added with marker
        let session_start = &merged["hooks"]["SessionStart"][0];
        assert_eq!(session_start["_dot_agent_profile"], "superpowers");
        assert_eq!(session_start["matcher"], "startup");
    }

    #[test]
    fn test_unmerge_hooks_json() {
        let content = r#"{
            "hooks": {
                "PreToolUse": [
                    {"matcher": "existing", "hooks": []}
                ],
                "SessionStart": [
                    {"matcher": "startup", "_dot_agent_profile": "superpowers", "hooks": []}
                ]
            }
        }"#;

        let result = unmerge_json(content, "superpowers").unwrap();

        let unmerged: Value = serde_json::from_str(&result.content).unwrap();

        // Check that PreToolUse is preserved
        assert!(unmerged["hooks"]["PreToolUse"].is_array());

        // Check that SessionStart array is now empty (or removed)
        assert!(
            unmerged["hooks"]["SessionStart"].is_null()
                || unmerged["hooks"]["SessionStart"]
                    .as_array()
                    .unwrap()
                    .is_empty()
        );
    }

    #[test]
    fn test_merge_mcp_json() {
        let target = r#"{
            "mcpServers": {
                "existing-server": {"command": "existing"}
            }
        }"#;

        let source = r#"{
            "mcpServers": {
                "new-server": {"command": "new"}
            }
        }"#;

        let result = merge_json(Some(target), source, "my-profile").unwrap();

        let merged: Value = serde_json::from_str(&result.content).unwrap();

        // Check both servers exist
        assert!(merged["mcpServers"]["existing-server"].is_object());
        assert!(merged["mcpServers"]["new-server"].is_object());
        assert_eq!(
            merged["mcpServers"]["new-server"]["_dot_agent_profile"],
            "my-profile"
        );
    }

    #[test]
    fn test_no_duplicate_merge() {
        let target = r#"{
            "hooks": {
                "SessionStart": [
                    {"matcher": "startup", "_dot_agent_profile": "superpowers", "hooks": []}
                ]
            }
        }"#;

        let source = r#"{
            "hooks": {
                "SessionStart": [
                    {"matcher": "startup", "hooks": []}
                ]
            }
        }"#;

        let result = merge_json(Some(target), source, "superpowers").unwrap();

        let merged: Value = serde_json::from_str(&result.content).unwrap();

        // Should not duplicate
        let session_start = merged["hooks"]["SessionStart"].as_array().unwrap();
        assert_eq!(session_start.len(), 1);
    }
}
