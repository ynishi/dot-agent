//! Category Store
//!
//! カテゴリ定義のランタイムストア。
//! ビルトインとProfile設定をマージして保持。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::builtin::{CategoryDef, BUILTIN_CATEGORIES, DEFAULT_CATEGORY_PRIORITY};

/// カテゴリ定義のランタイムストア
#[derive(Debug, Clone)]
pub struct CategoryStore {
    categories: HashMap<String, CategoryDef>,
}

impl CategoryStore {
    /// ビルトインカテゴリのみで初期化
    pub fn builtin() -> Self {
        let categories = BUILTIN_CATEGORIES
            .iter()
            .map(|b| (b.name.to_string(), CategoryDef::from(b)))
            .collect();
        Self { categories }
    }

    /// Profile設定でオーバーライド
    ///
    /// - 同名カテゴリは上書き
    /// - 新規カテゴリは追加
    pub fn with_config(mut self, config: &CategoriesConfig) -> Self {
        for (name, entry) in &config.categories {
            self.categories.insert(
                name.clone(),
                CategoryDef {
                    name: name.clone(),
                    description: entry.description.clone(),
                    patterns: entry.patterns.clone(),
                    priority: entry.priority.unwrap_or(DEFAULT_CATEGORY_PRIORITY),
                },
            );
        }
        self
    }

    /// カテゴリ定義を取得
    pub fn get(&self, name: &str) -> Option<&CategoryDef> {
        self.categories.get(name)
    }

    /// 全カテゴリを取得（priority順）
    pub fn all(&self) -> Vec<&CategoryDef> {
        let mut categories: Vec<_> = self.categories.values().collect();
        categories.sort_by(|a, b| b.priority.cmp(&a.priority));
        categories
    }

    /// カテゴリ名一覧
    pub fn names(&self) -> Vec<&str> {
        self.categories.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for CategoryStore {
    fn default() -> Self {
        Self::builtin()
    }
}

/// `.dot-agent.toml`のcategoriesセクション
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CategoriesConfig {
    #[serde(flatten)]
    pub categories: HashMap<String, CategoryConfigEntry>,
}

/// 個別カテゴリの設定エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryConfigEntry {
    /// カテゴリの説明
    pub description: String,
    /// Globパターン
    pub patterns: Vec<String>,
    /// 優先度（オプション）
    #[serde(default)]
    pub priority: Option<i32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_category_store_builtin() {
        let store = CategoryStore::builtin();
        assert!(store.get("plan").is_some());
        assert!(store.get("execute").is_some());
        assert!(store.get("nonexistent").is_none());
    }

    #[test]
    fn test_category_store_override() {
        let store = CategoryStore::builtin();
        let config = CategoriesConfig {
            categories: [(
                "plan".to_string(),
                CategoryConfigEntry {
                    description: "Custom plan".to_string(),
                    patterns: vec!["custom/*".to_string()],
                    priority: Some(200),
                },
            )]
            .into_iter()
            .collect(),
        };

        let store = store.with_config(&config);
        let plan = store.get("plan").unwrap();
        assert_eq!(plan.description, "Custom plan");
        assert_eq!(plan.priority, 200);
    }
}
