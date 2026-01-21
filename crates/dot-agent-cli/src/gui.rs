use std::path::PathBuf;

use eframe::egui;

use dot_agent_core::installer::Installer;
use dot_agent_core::profile::ProfileManager;
use dot_agent_core::rule::RuleManager;
use dot_agent_core::{DotAgentError, Result};

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 650.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "dot-agent",
        options,
        Box::new(|cc| Ok(Box::new(DotAgentApp::new(cc)))),
    )
    .map_err(|e| DotAgentError::Gui(e.to_string()))?;

    Ok(())
}

#[derive(PartialEq, Clone, Copy)]
enum SidebarTab {
    Profiles,
    Rules,
}

struct DotAgentApp {
    base_dir: PathBuf,
    profile_manager: ProfileManager,
    rule_manager: RuleManager,

    // UI State
    sidebar_tab: SidebarTab,
    selected_profile: Option<String>,
    selected_rule: Option<String>,

    // Install options
    target_path: String,
    install_global: bool,
    no_prefix: bool,
    force: bool,

    // Rule apply options
    apply_profile: Option<String>,
    new_profile_name: String,

    // Status
    status_message: Option<(String, MessageType)>,
}

enum MessageType {
    Success,
    Error,
    Info,
}

impl DotAgentApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let base_dir = dirs::home_dir()
            .map(|h| h.join(".dot-agent"))
            .unwrap_or_else(|| PathBuf::from(".dot-agent"));

        let profile_manager = ProfileManager::new(base_dir.clone());
        let rule_manager = RuleManager::new(base_dir.clone());

        Self {
            base_dir,
            profile_manager,
            rule_manager,
            sidebar_tab: SidebarTab::Profiles,
            selected_profile: None,
            selected_rule: None,
            target_path: String::new(),
            install_global: false,
            no_prefix: false,
            force: false,
            apply_profile: None,
            new_profile_name: String::new(),
            status_message: None,
        }
    }

    fn refresh(&mut self) {
        self.profile_manager = ProfileManager::new(self.base_dir.clone());
        self.rule_manager = RuleManager::new(self.base_dir.clone());
    }
}

impl eframe::App for DotAgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Top panel - header
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("dot-agent");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟳ Refresh").clicked() {
                        self.refresh();
                        self.status_message =
                            Some(("Refreshed".to_string(), MessageType::Info));
                    }
                });
            });
            ui.add_space(8.0);
        });

        // Left sidebar with tabs
        egui::SidePanel::left("sidebar")
            .resizable(true)
            .default_width(220.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);

                // Tab buttons
                ui.horizontal(|ui| {
                    if ui
                        .selectable_label(self.sidebar_tab == SidebarTab::Profiles, "📦 Profiles")
                        .clicked()
                    {
                        self.sidebar_tab = SidebarTab::Profiles;
                    }
                    if ui
                        .selectable_label(self.sidebar_tab == SidebarTab::Rules, "📋 Rules")
                        .clicked()
                    {
                        self.sidebar_tab = SidebarTab::Rules;
                    }
                });
                ui.separator();

                match self.sidebar_tab {
                    SidebarTab::Profiles => {
                        self.render_profiles_list(ui);
                    }
                    SidebarTab::Rules => {
                        self.render_rules_list(ui);
                    }
                }
            });

        // Central panel
        egui::CentralPanel::default().show(ctx, |ui| {
            // Status message
            self.render_status_message(ui);

            match self.sidebar_tab {
                SidebarTab::Profiles => {
                    self.render_profile_detail(ui);
                }
                SidebarTab::Rules => {
                    self.render_rule_detail(ui);
                }
            }
        });
    }
}

impl DotAgentApp {
    fn render_profiles_list(&mut self, ui: &mut egui::Ui) {
        match self.profile_manager.list_profiles() {
            Ok(profiles) => {
                if profiles.is_empty() {
                    ui.label("No profiles found");
                    ui.add_space(8.0);
                    ui.label("Create with CLI:");
                    ui.code("dot-agent profile add <name>");
                } else {
                    for profile in profiles {
                        let selected = self.selected_profile.as_ref() == Some(&profile.name);
                        if ui.selectable_label(selected, &profile.name).clicked() {
                            self.selected_profile = Some(profile.name.clone());
                        }
                    }
                }
            }
            Err(e) => {
                ui.colored_label(egui::Color32::RED, format!("Error: {e}"));
            }
        }
    }

    fn render_rules_list(&mut self, ui: &mut egui::Ui) {
        match self.rule_manager.list() {
            Ok(rules) => {
                if rules.is_empty() {
                    ui.label("No rules found");
                    ui.add_space(8.0);
                    ui.label("Create with CLI:");
                    ui.code("dot-agent rule add <name>");
                } else {
                    for rule in rules {
                        let selected = self.selected_rule.as_ref() == Some(&rule.name);
                        if ui.selectable_label(selected, &rule.name).clicked() {
                            self.selected_rule = Some(rule.name.clone());
                            // Auto-generate new profile name
                            if let Some(ref profile) = self.apply_profile {
                                self.new_profile_name = format!("{}-{}", profile, rule.name);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                ui.colored_label(egui::Color32::RED, format!("Error: {e}"));
            }
        }
    }

    fn render_status_message(&mut self, ui: &mut egui::Ui) {
        let mut clear_status = false;
        if let Some((msg, msg_type)) = &self.status_message {
            let color = match msg_type {
                MessageType::Success => egui::Color32::GREEN,
                MessageType::Error => egui::Color32::RED,
                MessageType::Info => egui::Color32::LIGHT_BLUE,
            };
            ui.horizontal(|ui| {
                ui.colored_label(color, msg.clone());
                if ui.small_button("✕").clicked() {
                    clear_status = true;
                }
            });
            ui.separator();
        }
        if clear_status {
            self.status_message = None;
        }
    }

    fn render_profile_detail(&mut self, ui: &mut egui::Ui) {
        if let Some(profile_name) = &self.selected_profile.clone() {
            ui.heading(format!("📦 Profile: {profile_name}"));
            ui.add_space(8.0);

            match self.profile_manager.get_profile(profile_name) {
                Ok(profile) => {
                    ui.label(format!("Path: {}", profile.path.display()));
                    ui.label(format!("Contents: {}", profile.contents_summary()));
                    ui.add_space(16.0);

                    ui.separator();
                    ui.heading("Install");
                    ui.add_space(8.0);

                    ui.horizontal(|ui| {
                        ui.label("Target:");
                        ui.text_edit_singleline(&mut self.target_path);
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.target_path = path.display().to_string();
                            }
                        }
                    });

                    ui.checkbox(&mut self.install_global, "Install to ~/.claude (global)");
                    ui.checkbox(&mut self.no_prefix, "No prefix (use original filenames)");
                    ui.checkbox(&mut self.force, "Force overwrite conflicts");

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("⬇ Install").clicked() {
                            self.do_install(&profile);
                        }

                        if ui.button("📊 Diff").clicked() {
                            self.do_diff(&profile);
                        }
                    });
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Error loading profile: {e}"));
                }
            }
        } else {
            self.render_empty_state(ui, "Select a profile from the sidebar");
        }
    }

    fn render_rule_detail(&mut self, ui: &mut egui::Ui) {
        if let Some(rule_name) = &self.selected_rule.clone() {
            match self.rule_manager.get(rule_name) {
                Ok(rule) => {
                    ui.heading(format!("📋 Rule: {}", rule_name));
                    ui.add_space(8.0);

                    ui.label(format!("Path: {}", rule.path.display()));
                    ui.add_space(8.0);

                    // Rule content preview
                    ui.separator();
                    ui.label("Content:");
                    egui::ScrollArea::vertical()
                        .max_height(200.0)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut rule.content.as_str())
                                    .font(egui::TextStyle::Monospace)
                                    .desired_width(f32::INFINITY)
                                    .interactive(false),
                            );
                        });

                    ui.add_space(16.0);
                    ui.separator();
                    ui.heading("Apply Rule");
                    ui.add_space(8.0);

                    // Profile selection dropdown
                    ui.horizontal(|ui| {
                        ui.label("Source Profile:");
                        egui::ComboBox::from_id_salt("apply_profile")
                            .selected_text(
                                self.apply_profile
                                    .as_deref()
                                    .unwrap_or("Select profile..."),
                            )
                            .show_ui(ui, |ui| {
                                if let Ok(profiles) = self.profile_manager.list_profiles() {
                                    for profile in profiles {
                                        let selected =
                                            self.apply_profile.as_ref() == Some(&profile.name);
                                        if ui.selectable_label(selected, &profile.name).clicked() {
                                            self.apply_profile = Some(profile.name.clone());
                                            self.new_profile_name =
                                                format!("{}-{}", profile.name, rule_name);
                                        }
                                    }
                                }
                            });
                    });

                    ui.horizontal(|ui| {
                        ui.label("New Profile Name:");
                        ui.text_edit_singleline(&mut self.new_profile_name);
                    });

                    ui.add_space(8.0);

                    let can_apply =
                        self.apply_profile.is_some() && !self.new_profile_name.is_empty();

                    ui.add_enabled_ui(can_apply, |ui| {
                        if ui.button("🔨 Apply Rule").clicked() {
                            self.do_apply_rule(rule_name);
                        }
                    });

                    if !can_apply {
                        ui.label("Select a source profile and enter a name for the new profile");
                    }
                }
                Err(e) => {
                    ui.colored_label(egui::Color32::RED, format!("Error loading rule: {e}"));
                }
            }
        } else {
            self.render_empty_state(ui, "Select a rule from the sidebar");
        }
    }

    fn render_empty_state(&self, ui: &mut egui::Ui, message: &str) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.heading(message);
        });
    }

    fn do_install(&mut self, profile: &dot_agent_core::Profile) {
        let installer = Installer::new(self.base_dir.clone());
        let target = if self.install_global || self.target_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.target_path))
        };

        match installer.resolve_target(target.as_deref(), self.install_global) {
            Ok(target_dir) => {
                match installer.install(profile, &target_dir, self.force, false, self.no_prefix, None)
                {
                    Ok(result) => {
                        self.status_message = Some((
                            format!(
                                "✓ Installed {} files to {}",
                                result.installed,
                                target_dir.display()
                            ),
                            MessageType::Success,
                        ));
                    }
                    Err(e) => {
                        self.status_message =
                            Some((format!("Install failed: {e}"), MessageType::Error));
                    }
                }
            }
            Err(e) => {
                self.status_message = Some((format!("Invalid target: {e}"), MessageType::Error));
            }
        }
    }

    fn do_diff(&mut self, profile: &dot_agent_core::Profile) {
        let installer = Installer::new(self.base_dir.clone());
        let target = if self.install_global || self.target_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.target_path))
        };

        match installer.resolve_target(target.as_deref(), self.install_global) {
            Ok(target_dir) => match installer.diff(profile, &target_dir) {
                Ok(result) => {
                    self.status_message = Some((
                        format!(
                            "Modified: {}, Missing: {}, Unchanged: {}",
                            result.modified, result.missing, result.unchanged
                        ),
                        MessageType::Info,
                    ));
                }
                Err(e) => {
                    self.status_message = Some((format!("Diff failed: {e}"), MessageType::Error));
                }
            },
            Err(e) => {
                self.status_message = Some((format!("Invalid target: {e}"), MessageType::Error));
            }
        }
    }

    fn do_apply_rule(&mut self, rule_name: &str) {
        use dot_agent_core::rule::RuleExecutor;

        let Some(ref profile_name) = self.apply_profile else {
            self.status_message = Some(("Select a profile first".to_string(), MessageType::Error));
            return;
        };

        let profile = match self.profile_manager.get_profile(profile_name) {
            Ok(p) => p,
            Err(e) => {
                self.status_message =
                    Some((format!("Profile error: {e}"), MessageType::Error));
                return;
            }
        };

        let rule = match self.rule_manager.get(rule_name) {
            Ok(r) => r,
            Err(e) => {
                self.status_message = Some((format!("Rule error: {e}"), MessageType::Error));
                return;
            }
        };

        let executor = RuleExecutor::new(&rule, &self.profile_manager);
        let new_name = if self.new_profile_name.is_empty() {
            None
        } else {
            Some(self.new_profile_name.as_str())
        };

        match executor.apply(&profile, new_name, false) {
            Ok(result) => {
                self.status_message = Some((
                    format!(
                        "✓ Created profile '{}' ({} files modified)",
                        result.new_profile_name, result.files_modified
                    ),
                    MessageType::Success,
                ));
                // Refresh to show new profile
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some((format!("Apply failed: {e}"), MessageType::Error));
            }
        }
    }
}
