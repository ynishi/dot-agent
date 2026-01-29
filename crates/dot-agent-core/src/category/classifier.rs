//! Category Classifier
//!
//! ProfileのファイルをCategoryStoreの定義に基づいて分類する。

use std::collections::HashMap;
use std::path::PathBuf;

use glob::Pattern;

use crate::error::Result;
use crate::Profile;

use super::builtin::CategoryDef;
use super::store::CategoryStore;

/// 分類モード
#[derive(Debug, Clone, Copy, Default)]
pub enum ClassificationMode {
    /// Globパターンマッチング（高速、デフォルト）
    #[default]
    Glob,
    /// LLMによるセマンティック分類（高精度）
    Llm,
}

/// ファイルの分類結果
#[derive(Debug, Clone)]
pub struct FileClassification {
    /// ファイルパス（Profile相対）
    pub path: PathBuf,
    /// 所属カテゴリ（複数可、priority順）
    pub categories: Vec<String>,
    /// 分類の確信度（LLMモード時のみ有効、0.0-1.0）
    pub confidence: Option<f32>,
}

/// Profile全体の分類結果
#[derive(Debug, Clone)]
pub struct ClassifiedProfile {
    /// 元Profile名
    pub profile_name: String,
    /// ファイル分類結果
    pub files: Vec<FileClassification>,
    /// 使用した分類モード
    pub mode: ClassificationMode,
    /// 分類中の警告メッセージ
    pub warnings: Vec<String>,
}

impl ClassifiedProfile {
    /// 特定カテゴリのファイルを取得
    pub fn files_in_category(&self, category: &str) -> Vec<&PathBuf> {
        self.files
            .iter()
            .filter(|f| f.categories.iter().any(|c| c == category))
            .map(|f| &f.path)
            .collect()
    }

    /// カテゴリ別のファイル数を取得
    pub fn category_counts(&self) -> HashMap<String, usize> {
        let mut counts = HashMap::new();
        for file in &self.files {
            for cat in &file.categories {
                *counts.entry(cat.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    /// 未分類ファイルを取得
    pub fn uncategorized(&self) -> Vec<&PathBuf> {
        self.files
            .iter()
            .filter(|f| f.categories.is_empty())
            .map(|f| &f.path)
            .collect()
    }
}

/// カテゴリ分類器
pub struct CategoryClassifier {
    mode: ClassificationMode,
    store: CategoryStore,
    compiled_patterns: HashMap<String, Vec<Pattern>>,
}

impl CategoryClassifier {
    /// 新規分類器を作成
    pub fn new(mode: ClassificationMode, store: CategoryStore) -> Result<Self> {
        let mut compiled_patterns = HashMap::new();

        for cat in store.all() {
            let patterns = cat
                .patterns
                .iter()
                .map(|p| Pattern::new(p))
                .collect::<std::result::Result<Vec<_>, _>>()?;
            compiled_patterns.insert(cat.name.clone(), patterns);
        }

        Ok(Self {
            mode,
            store,
            compiled_patterns,
        })
    }

    /// ProfileからCategoryStoreを取得して分類器を構築
    pub fn from_profile(profile: &Profile, mode: ClassificationMode) -> Result<Self> {
        let store = profile.category_store()?;
        Self::new(mode, store)
    }

    /// ビルトインカテゴリのみで分類器を構築
    pub fn builtin(mode: ClassificationMode) -> Result<Self> {
        Self::new(mode, CategoryStore::builtin())
    }

    /// 利用可能なカテゴリ名を取得
    pub fn category_names(&self) -> Vec<&str> {
        self.store.names()
    }

    /// カテゴリ定義を取得
    pub fn get_category(&self, name: &str) -> Option<&CategoryDef> {
        self.store.get(name)
    }

    /// Profileを分類
    pub fn classify(&self, profile: &Profile) -> Result<ClassifiedProfile> {
        let files = profile.list_files()?;

        let (classifications, warnings) = match self.mode {
            ClassificationMode::Glob => (self.classify_by_glob(&files)?, Vec::new()),
            ClassificationMode::Llm => self.classify_by_llm_with_warnings(profile, &files)?,
        };

        Ok(ClassifiedProfile {
            profile_name: profile.name.clone(),
            files: classifications,
            mode: self.mode,
            warnings,
        })
    }

    /// Globパターンで分類
    pub fn classify_by_glob(&self, files: &[PathBuf]) -> Result<Vec<FileClassification>> {
        let mut results = Vec::with_capacity(files.len());

        for file in files {
            let file_str = file.to_string_lossy();
            let mut matched_categories = Vec::new();

            for cat in self.store.all() {
                if let Some(patterns) = self.compiled_patterns.get(&cat.name) {
                    if patterns.iter().any(|p| p.matches(&file_str)) {
                        matched_categories.push(cat.name.clone());
                    }
                }
            }

            results.push(FileClassification {
                path: file.clone(),
                categories: matched_categories,
                confidence: None,
            });
        }

        Ok(results)
    }

    /// LLMで分類（警告情報付き）
    fn classify_by_llm_with_warnings(
        &self,
        profile: &Profile,
        files: &[PathBuf],
    ) -> Result<(Vec<FileClassification>, Vec<String>)> {
        use crate::llm::{check_claude_cli, execute_claude};

        let mut warnings = Vec::new();

        if !check_claude_cli() {
            warnings.push("Claude CLI not found, falling back to glob classification".to_string());
            return Ok((self.classify_by_glob(files)?, warnings));
        }

        let mut category_descriptions = String::new();
        for cat in self.store.all() {
            category_descriptions.push_str(&format!("- **{}**: {}\n", cat.name, cat.description));
        }

        let file_list: String = files
            .iter()
            .map(|f| format!("- {}", f.display()))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = format!(
            r#"Classify the following files into categories based on their purpose.

## Available Categories

{category_descriptions}

## Files to Classify

{file_list}

## Output Format

Output a JSON array where each element has:
- "path": the file path
- "categories": array of category names that apply
- "confidence": confidence score 0.0-1.0

Example:
```json
[
  {{"path": "skills/planning/SKILL.md", "categories": ["plan"], "confidence": 0.9}},
  {{"path": "agents/code-reviewer.md", "categories": ["review"], "confidence": 0.95}}
]
```

Output ONLY the JSON array, no other text.
"#
        );

        let output = match execute_claude(&profile.path, &prompt) {
            Ok(output) => output,
            Err(e) => {
                warnings.push(format!(
                    "LLM classification failed, falling back to glob: {}",
                    e
                ));
                return Ok((self.classify_by_glob(files)?, warnings));
            }
        };

        self.parse_llm_output_with_warnings(&output, files, warnings)
    }

    /// LLM出力をパース（警告情報付き）
    fn parse_llm_output_with_warnings(
        &self,
        output: &str,
        files: &[PathBuf],
        mut warnings: Vec<String>,
    ) -> Result<(Vec<FileClassification>, Vec<String>)> {
        let json_str = extract_json_from_output(output);

        #[derive(serde::Deserialize)]
        struct LlmClassification {
            path: String,
            categories: Vec<String>,
            confidence: Option<f32>,
        }

        let classifications: Vec<LlmClassification> = match serde_json::from_str(json_str) {
            Ok(c) => c,
            Err(e) => {
                warnings.push(format!(
                    "Failed to parse LLM output as JSON, falling back to glob: {}",
                    e
                ));
                return Ok((self.classify_by_glob(files)?, warnings));
            }
        };

        let mut results = Vec::with_capacity(files.len());
        for file in files {
            let file_str = file.to_string_lossy();
            let classification = classifications.iter().find(|c| c.path == file_str.as_ref());

            let (categories, confidence) = match classification {
                Some(c) => {
                    let valid_cats: Vec<String> = c
                        .categories
                        .iter()
                        .filter(|cat| self.store.get(cat).is_some())
                        .cloned()
                        .collect();
                    (valid_cats, c.confidence)
                }
                None => (Vec::new(), None),
            };

            results.push(FileClassification {
                path: file.clone(),
                categories,
                confidence,
            });
        }

        Ok((results, warnings))
    }
}

/// LLM出力からJSON部分を抽出
fn extract_json_from_output(output: &str) -> &str {
    if let Some(start) = output.find("```json") {
        let start = start + 7;
        if let Some(end) = output[start..].find("```") {
            return output[start..start + end].trim();
        }
    }
    if let Some(start) = output.find("```") {
        let start = start + 3;
        if let Some(end) = output[start..].find("```") {
            return output[start..start + end].trim();
        }
    }
    if let Some(start) = output.find('[') {
        if let Some(end) = output.rfind(']') {
            return &output[start..=end];
        }
    }
    output.trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classify_paths(paths: &[&str]) -> Vec<FileClassification> {
        let store = CategoryStore::builtin();
        let classifier = CategoryClassifier::new(ClassificationMode::Glob, store).unwrap();
        let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
        classifier.classify_by_glob(&path_bufs).unwrap()
    }

    fn get_categories(results: &[FileClassification], path: &str) -> Vec<String> {
        results
            .iter()
            .find(|f| f.path == PathBuf::from(path))
            .map(|f| f.categories.clone())
            .unwrap_or_default()
    }

    #[test]
    fn test_glob_matches_directory_name() {
        let results = classify_paths(&["skills/planning/SKILL.md"]);
        let cats = get_categories(&results, "skills/planning/SKILL.md");
        assert!(
            cats.contains(&"plan".to_string()),
            "Expected 'plan' in {:?}",
            cats
        );
    }

    #[test]
    fn test_glob_matches_filename() {
        let results = classify_paths(&["agents/code-reviewer.md"]);
        let cats = get_categories(&results, "agents/code-reviewer.md");
        assert!(
            cats.contains(&"review".to_string()),
            "Expected 'review' in {:?}",
            cats
        );
    }

    #[test]
    fn test_glob_matches_multiple_categories() {
        let results = classify_paths(&["skills/tdd-review/SKILL.md"]);
        let cats = get_categories(&results, "skills/tdd-review/SKILL.md");
        assert!(
            cats.contains(&"execute".to_string()),
            "Expected 'execute' in {:?}",
            cats
        );
        assert!(
            cats.contains(&"review".to_string()),
            "Expected 'review' in {:?}",
            cats
        );
    }

    #[test]
    fn test_glob_no_match() {
        let results = classify_paths(&["CLAUDE.md"]);
        let cats = get_categories(&results, "CLAUDE.md");
        assert!(
            cats.is_empty(),
            "Expected no categories for CLAUDE.md, got {:?}",
            cats
        );
    }

    #[test]
    fn test_glob_debug_category() {
        let results = classify_paths(&["agents/build-error-resolver.md"]);
        let cats = get_categories(&results, "agents/build-error-resolver.md");
        assert!(
            cats.contains(&"debug".to_string()),
            "Expected 'debug' in {:?}",
            cats
        );
    }

    #[test]
    fn test_glob_various_patterns() {
        let results = classify_paths(&[
            "skills/brainstorm/SKILL.md",
            "agents/architect.md",
            "rules/coding-style.md",
            "skills/test-runner/SKILL.md",
            "agents/troubleshoot.md",
        ]);

        assert!(
            get_categories(&results, "skills/brainstorm/SKILL.md").contains(&"plan".to_string())
        );
        assert!(get_categories(&results, "agents/architect.md").contains(&"plan".to_string()));
        assert!(get_categories(&results, "rules/coding-style.md").contains(&"execute".to_string()));
        assert!(
            get_categories(&results, "skills/test-runner/SKILL.md").contains(&"review".to_string())
        );
        assert!(get_categories(&results, "agents/troubleshoot.md").contains(&"debug".to_string()));
    }
}
