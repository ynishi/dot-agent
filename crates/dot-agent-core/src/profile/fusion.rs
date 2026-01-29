//! Fusion Executor
//!
//! 複数Profileからカテゴリ単位で合成し、新Profileを作成する。

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::category::{CategoryClassifier, ClassificationMode};
use crate::error::{DotAgentError, Result};

use super::{ProfileManager, ProfileMetadata};

/// Fusion指定（Profile:Category のペア）
#[derive(Debug, Clone)]
pub struct FusionSpec {
    /// Profile名
    pub profile_name: String,
    /// 対象カテゴリ
    pub category: String,
}

impl FusionSpec {
    /// "profile:category" 形式からパース
    pub fn parse(s: &str) -> Result<Self> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(DotAgentError::ConfigParseSimple {
                message: format!("Invalid fusion spec: '{}'. Expected 'profile:category'", s),
            });
        }
        Ok(Self {
            profile_name: parts[0].to_string(),
            category: parts[1].to_string(),
        })
    }
}

/// Fusionコンフリクト
#[derive(Debug, Clone)]
pub struct FusionConflict {
    /// コンフリクトしたファイルパス
    pub path: PathBuf,
    /// 競合元のProfile:Category ペア
    pub sources: Vec<FusionSpec>,
}

/// Fusion結果
#[derive(Debug, Clone)]
pub struct FusionResult {
    /// 作成されたProfile名
    pub profile_name: String,
    /// コピーされたファイル数
    pub files_copied: usize,
    /// 検出されたコンフリクト
    pub conflicts: Vec<FusionConflict>,
    /// 各Profileからの貢献
    pub contributions: HashMap<String, usize>,
}

/// Fusion設定
#[derive(Debug, Clone)]
pub struct FusionConfig {
    /// 分類モード
    pub mode: ClassificationMode,
    /// コンフリクト時に後勝ちにするか
    pub force: bool,
    /// 最初のProfileから未分類ファイルを含めるか
    pub include_uncategorized: bool,
    /// ドライラン（ファイルコピーしない）
    pub dry_run: bool,
}

impl Default for FusionConfig {
    fn default() -> Self {
        Self {
            mode: ClassificationMode::Glob,
            force: false,
            include_uncategorized: false,
            dry_run: false,
        }
    }
}

/// 収集されたファイル情報
#[derive(Debug, Clone)]
pub struct CollectedFile {
    /// コピー元のフルパス
    pub src_path: PathBuf,
    /// 出力Profile内での相対パス
    pub dest_path: String,
    /// 元Profile名
    pub source_profile: String,
}

/// Fusion実行結果（ドライラン用）
#[derive(Debug, Clone)]
pub struct FusionPlan {
    /// コピー予定のファイル
    pub files: Vec<CollectedFile>,
    /// 検出されたコンフリクト
    pub conflicts: Vec<FusionConflict>,
    /// 各Profileからの貢献数
    pub contributions: HashMap<String, usize>,
}

/// Fusion実行器
///
/// 複数のProfile:Categoryから新Profileを作成する。
pub struct FusionExecutor {
    specs: Vec<FusionSpec>,
    config: FusionConfig,
}

impl FusionExecutor {
    /// 新規実行器を作成
    pub fn new(specs: Vec<FusionSpec>, config: FusionConfig) -> Self {
        Self { specs, config }
    }

    /// Fusionを計画（ドライラン用）
    ///
    /// ファイルのコピーは行わず、計画のみを返す。
    pub fn plan(&self, manager: &ProfileManager) -> Result<FusionPlan> {
        if self.specs.is_empty() {
            return Err(DotAgentError::ConfigParseSimple {
                message: "No fusion specs provided".to_string(),
            });
        }

        // dest_path -> CollectedFile
        let mut all_files: HashMap<String, CollectedFile> = HashMap::new();
        let mut conflicts: Vec<FusionConflict> = Vec::new();
        let mut contributions: HashMap<String, usize> = HashMap::new();

        for (idx, spec) in self.specs.iter().enumerate() {
            let profile = manager.get_profile(&spec.profile_name)?;
            let classifier = CategoryClassifier::from_profile(&profile, self.config.mode)?;
            let result = classifier.classify(&profile)?;

            // Validate category exists
            if classifier.get_category(&spec.category).is_none() {
                return Err(DotAgentError::CategoryNotFound {
                    name: spec.category.clone(),
                });
            }

            let files = result.files_in_category(&spec.category);
            let file_count = files.len();

            for file in files {
                let dest_key = file.to_string_lossy().to_string();
                let src_path = profile.path.join(file);

                // Check for conflict before inserting
                if let Some(existing) = all_files.get(&dest_key) {
                    let conflict_entry = conflicts
                        .iter_mut()
                        .find(|c| c.path.to_string_lossy() == dest_key);

                    if let Some(conflict) = conflict_entry {
                        conflict.sources.push(spec.clone());
                    } else {
                        conflicts.push(FusionConflict {
                            path: PathBuf::from(&dest_key),
                            sources: vec![
                                FusionSpec {
                                    profile_name: existing.source_profile.clone(),
                                    category: spec.category.clone(),
                                },
                                spec.clone(),
                            ],
                        });
                    }
                }

                // Insert or overwrite (later spec wins)
                all_files.insert(
                    dest_key.clone(),
                    CollectedFile {
                        src_path,
                        dest_path: dest_key,
                        source_profile: spec.profile_name.clone(),
                    },
                );
            }

            *contributions.entry(spec.profile_name.clone()).or_insert(0) += file_count;

            // Include uncategorized from first profile if requested
            if idx == 0 && self.config.include_uncategorized {
                let uncategorized = result.uncategorized();
                for file in uncategorized {
                    let dest_key = file.to_string_lossy().to_string();
                    let src_path = profile.path.join(file);
                    all_files
                        .entry(dest_key.clone())
                        .or_insert_with(|| CollectedFile {
                            src_path,
                            dest_path: dest_key,
                            source_profile: spec.profile_name.clone(),
                        });
                }
            }
        }

        // Sort files by dest_path for deterministic output
        let mut files: Vec<_> = all_files.into_values().collect();
        files.sort_by(|a, b| a.dest_path.cmp(&b.dest_path));

        Ok(FusionPlan {
            files,
            conflicts,
            contributions,
        })
    }

    /// Fusionを実行（plan + execute_plan のショートカット）
    pub fn execute(&self, manager: &ProfileManager, output_name: &str) -> Result<FusionResult> {
        let plan = self.plan(manager)?;
        self.execute_plan(&plan, manager, output_name)
    }

    /// 計画済みFusionを実行
    ///
    /// plan()で取得したFusionPlanを実行する。
    /// CLI側で計画を表示した後に実行する場合はこちらを使用。
    pub fn execute_plan(
        &self,
        plan: &FusionPlan,
        manager: &ProfileManager,
        output_name: &str,
    ) -> Result<FusionResult> {
        // Check output profile
        let output_exists = manager.get_profile(output_name).is_ok();
        if output_exists && !self.config.force {
            return Err(DotAgentError::ProfileAlreadyExists {
                name: output_name.to_string(),
            });
        }

        if self.config.dry_run {
            return Ok(FusionResult {
                profile_name: output_name.to_string(),
                files_copied: 0,
                conflicts: plan.conflicts.clone(),
                contributions: plan.contributions.clone(),
            });
        }

        // Create output profile
        let output_path = manager.profiles_dir().join(output_name);
        if output_path.exists() && self.config.force {
            fs::remove_dir_all(&output_path)?;
        }
        fs::create_dir_all(&output_path)?;

        // Copy files
        let mut copied = 0;
        for file in &plan.files {
            let dest_path = output_path.join(&file.dest_path);

            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    std::io::Error::new(
                        e.kind(),
                        format!("Failed to create directory {:?}: {}", parent, e),
                    )
                })?;
            }

            fs::copy(&file.src_path, &dest_path).map_err(|e| {
                std::io::Error::new(
                    e.kind(),
                    format!(
                        "Failed to copy {:?} -> {:?}: {}",
                        file.src_path, dest_path, e
                    ),
                )
            })?;
            copied += 1;
        }

        // Create .dot-agent.toml
        let metadata = ProfileMetadata::new_local(output_name);
        metadata.save(&output_path)?;

        Ok(FusionResult {
            profile_name: output_name.to_string(),
            files_copied: copied,
            conflicts: plan.conflicts.clone(),
            contributions: plan.contributions.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fusion_spec_parse() {
        let spec = FusionSpec::parse("my-profile:plan").unwrap();
        assert_eq!(spec.profile_name, "my-profile");
        assert_eq!(spec.category, "plan");

        assert!(FusionSpec::parse("invalid").is_err());
    }

    #[test]
    fn test_fusion_spec_parse_with_colon_in_name() {
        let spec = FusionSpec::parse("my:profile:plan").unwrap();
        assert_eq!(spec.profile_name, "my");
        assert_eq!(spec.category, "profile:plan");
    }
}
