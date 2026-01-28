use std::path::PathBuf;
use std::process::Command;

use eframe::egui;

use dot_agent_core::installer::{InstallOptions, Installer};
use dot_agent_core::profile::{IgnoreConfig, ProfileManager};
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

    // Dialog state - Profile
    show_create_profile_dialog: bool,
    create_profile_name: String,
    show_delete_profile_confirm: bool,

    // Dialog state - Rule
    show_create_rule_dialog: bool,
    create_rule_name: String,
    show_delete_rule_confirm: bool,

    // Dialog state - Import
    show_import_dialog: bool,
    import_source_is_git: bool,
    import_url: String,
    import_local_path: String,
    import_name: String,
    import_branch: String,
    import_subpath: String,
    import_force: bool,

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
            show_create_profile_dialog: false,
            create_profile_name: String::new(),
            show_delete_profile_confirm: false,
            show_create_rule_dialog: false,
            create_rule_name: String::new(),
            show_delete_rule_confirm: false,
            show_import_dialog: false,
            import_source_is_git: true,
            import_url: String::new(),
            import_local_path: String::new(),
            import_name: String::new(),
            import_branch: String::new(),
            import_subpath: String::new(),
            import_force: false,
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
        // Render dialogs first (modal)
        self.render_create_profile_dialog(ctx);
        self.render_delete_profile_confirm(ctx);
        self.render_import_dialog(ctx);
        self.render_create_rule_dialog(ctx);
        self.render_delete_rule_confirm(ctx);

        // Top panel - header
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("dot-agent");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟳ Refresh").clicked() {
                        self.refresh();
                        self.status_message = Some(("Refreshed".to_string(), MessageType::Info));
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
        // Add buttons at top
        ui.horizontal(|ui| {
            if ui.button("+ New").clicked() {
                self.show_create_profile_dialog = true;
                self.create_profile_name.clear();
            }
            if ui.button("⬇ Import").clicked() {
                self.show_import_dialog = true;
                self.import_source_is_git = true;
                self.import_url.clear();
                self.import_local_path.clear();
                self.import_name.clear();
                self.import_branch.clear();
                self.import_subpath.clear();
                self.import_force = false;
            }
        });
        ui.add_space(8.0);

        match self.profile_manager.list_profiles() {
            Ok(profiles) => {
                if profiles.is_empty() {
                    ui.label("No profiles found");
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
        // Add button at top
        if ui.button("+ Add Rule").clicked() {
            self.show_create_rule_dialog = true;
            self.create_rule_name.clear();
        }
        ui.add_space(8.0);

        match self.rule_manager.list() {
            Ok(rules) => {
                if rules.is_empty() {
                    ui.label("No rules found");
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

                        if ui.button("⬆ Upgrade").clicked() {
                            self.do_upgrade(&profile);
                        }

                        if ui.button("📊 Diff").clicked() {
                            self.do_diff(&profile);
                        }
                    });

                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("🗑 Remove Installed").clicked() {
                            self.do_remove_installed(&profile);
                        }
                    });

                    // Danger zone - delete profile
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Danger Zone:");
                        if ui
                            .button("🗑 Delete Profile")
                            .on_hover_text("Permanently delete this profile")
                            .clicked()
                        {
                            self.show_delete_profile_confirm = true;
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
                                self.apply_profile.as_deref().unwrap_or("Select profile..."),
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

                    // Danger zone - delete rule
                    ui.add_space(24.0);
                    ui.separator();
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label("Danger Zone:");
                        if ui
                            .button("🗑 Delete Rule")
                            .on_hover_text("Permanently delete this rule")
                            .clicked()
                        {
                            self.show_delete_rule_confirm = true;
                        }
                    });
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

    // =========================================================================
    // Dialogs
    // =========================================================================

    fn render_create_profile_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_create_profile_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Create Profile")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.create_profile_name);
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let can_create = !self.create_profile_name.is_empty();
                    if ui
                        .add_enabled(can_create, egui::Button::new("Create"))
                        .clicked()
                    {
                        self.do_create_profile();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_create_profile_dialog = false;
                    }
                });
            });

        if !open {
            self.show_create_profile_dialog = false;
        }
    }

    fn render_delete_profile_confirm(&mut self, ctx: &egui::Context) {
        if !self.show_delete_profile_confirm {
            return;
        }

        let profile_name = match &self.selected_profile {
            Some(name) => name.clone(),
            None => {
                self.show_delete_profile_confirm = false;
                return;
            }
        };

        let mut open = true;
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Are you sure you want to delete profile '{}'?",
                    profile_name
                ));
                ui.colored_label(egui::Color32::RED, "This action cannot be undone.");

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.do_delete_profile(&profile_name);
                        self.show_delete_profile_confirm = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_delete_profile_confirm = false;
                    }
                });
            });

        if !open {
            self.show_delete_profile_confirm = false;
        }
    }

    fn render_import_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_import_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Import Profile")
            .collapsible(false)
            .resizable(true)
            .default_width(450.0)
            .open(&mut open)
            .show(ctx, |ui| {
                // Source type selector
                ui.horizontal(|ui| {
                    ui.label("Source:");
                    ui.selectable_value(&mut self.import_source_is_git, true, "Git URL");
                    ui.selectable_value(&mut self.import_source_is_git, false, "Local Directory");
                });

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);

                if self.import_source_is_git {
                    // Git URL input
                    ui.horizontal(|ui| {
                        ui.label("Git URL:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.import_url)
                                .hint_text("https://github.com/user/repo"),
                        );
                    });

                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Branch:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.import_branch)
                                .hint_text("(default branch)"),
                        );
                    });
                } else {
                    // Local directory input
                    ui.horizontal(|ui| {
                        ui.label("Directory:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.import_local_path)
                                .hint_text("/path/to/profile"),
                        );
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.import_local_path = path.display().to_string();
                            }
                        }
                    });
                }

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Subpath:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_subpath).hint_text("(root)"),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.import_name)
                            .hint_text("(auto from source)"),
                    );
                });

                ui.add_space(4.0);
                ui.checkbox(&mut self.import_force, "Force overwrite if exists");

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let can_import = if self.import_source_is_git {
                        !self.import_url.is_empty()
                    } else {
                        !self.import_local_path.is_empty()
                    };
                    if ui
                        .add_enabled(can_import, egui::Button::new("Import"))
                        .clicked()
                    {
                        if self.import_source_is_git {
                            self.do_import_from_git();
                        } else {
                            self.do_import_from_local();
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_import_dialog = false;
                    }
                });
            });

        if !open {
            self.show_import_dialog = false;
        }
    }

    fn render_create_rule_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_create_rule_dialog {
            return;
        }

        let mut open = true;
        egui::Window::new("Create Rule")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Name:");
                    ui.text_edit_singleline(&mut self.create_rule_name);
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let can_create = !self.create_rule_name.is_empty();
                    if ui
                        .add_enabled(can_create, egui::Button::new("Create"))
                        .clicked()
                    {
                        self.do_create_rule();
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_create_rule_dialog = false;
                    }
                });
            });

        if !open {
            self.show_create_rule_dialog = false;
        }
    }

    fn render_delete_rule_confirm(&mut self, ctx: &egui::Context) {
        if !self.show_delete_rule_confirm {
            return;
        }

        let rule_name = match &self.selected_rule {
            Some(name) => name.clone(),
            None => {
                self.show_delete_rule_confirm = false;
                return;
            }
        };

        let mut open = true;
        egui::Window::new("Confirm Delete")
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                ui.label(format!(
                    "Are you sure you want to delete rule '{}'?",
                    rule_name
                ));
                ui.colored_label(egui::Color32::RED, "This action cannot be undone.");

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        self.do_delete_rule(&rule_name);
                        self.show_delete_rule_confirm = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.show_delete_rule_confirm = false;
                    }
                });
            });

        if !open {
            self.show_delete_rule_confirm = false;
        }
    }

    // =========================================================================
    // Actions
    // =========================================================================

    fn do_install(&mut self, profile: &dot_agent_core::Profile) {
        let installer = Installer::new(self.base_dir.clone());
        let target = if self.install_global || self.target_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.target_path))
        };

        match installer.resolve_target(target.as_deref(), self.install_global) {
            Ok(target_dir) => {
                let opts = InstallOptions::new()
                    .force(self.force)
                    .no_prefix(self.no_prefix);
                match installer.install(profile, &target_dir, &opts) {
                    Ok(result) => {
                        self.status_message = Some((
                            format!(
                                "Installed {} files to {}",
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

    fn do_upgrade(&mut self, profile: &dot_agent_core::Profile) {
        let installer = Installer::new(self.base_dir.clone());
        let target = if self.install_global || self.target_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.target_path))
        };

        match installer.resolve_target(target.as_deref(), self.install_global) {
            Ok(target_dir) => {
                let opts = InstallOptions::new()
                    .force(self.force)
                    .no_prefix(self.no_prefix);
                match installer.upgrade(profile, &target_dir, &opts) {
                    Ok((updated, new, skipped, unchanged)) => {
                        self.status_message = Some((
                            format!(
                                "Upgraded: {} updated, {} new, {} skipped, {} unchanged",
                                updated, new, skipped, unchanged
                            ),
                            MessageType::Success,
                        ));
                    }
                    Err(e) => {
                        self.status_message =
                            Some((format!("Upgrade failed: {e}"), MessageType::Error));
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
            Ok(target_dir) => {
                match installer.diff(profile, &target_dir, &IgnoreConfig::with_defaults()) {
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
                        self.status_message =
                            Some((format!("Diff failed: {e}"), MessageType::Error));
                    }
                }
            }
            Err(e) => {
                self.status_message = Some((format!("Invalid target: {e}"), MessageType::Error));
            }
        }
    }

    fn do_remove_installed(&mut self, profile: &dot_agent_core::Profile) {
        let installer = Installer::new(self.base_dir.clone());
        let target = if self.install_global || self.target_path.is_empty() {
            None
        } else {
            Some(PathBuf::from(&self.target_path))
        };

        match installer.resolve_target(target.as_deref(), self.install_global) {
            Ok(target_dir) => {
                let opts = InstallOptions::new().force(self.force);
                match installer.remove(profile, &target_dir, &opts) {
                    Ok((removed, kept, unmerged)) => {
                        self.status_message = Some((
                            format!(
                                "Removed {} files, kept {}, unmerged {}",
                                removed, kept, unmerged
                            ),
                            MessageType::Success,
                        ));
                    }
                    Err(e) => {
                        self.status_message =
                            Some((format!("Remove failed: {e}"), MessageType::Error));
                    }
                }
            }
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
                self.status_message = Some((format!("Profile error: {e}"), MessageType::Error));
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
                        "Created profile '{}' ({} files modified)",
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

    fn do_create_profile(&mut self) {
        let name = self.create_profile_name.trim();
        if name.is_empty() {
            self.status_message =
                Some(("Profile name is required".to_string(), MessageType::Error));
            return;
        }

        match self.profile_manager.create_profile(name) {
            Ok(profile) => {
                self.status_message = Some((
                    format!("Created profile '{}'", profile.name),
                    MessageType::Success,
                ));
                self.selected_profile = Some(profile.name);
                self.show_create_profile_dialog = false;
                self.refresh();
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Failed to create profile: {e}"), MessageType::Error));
            }
        }
    }

    fn do_delete_profile(&mut self, name: &str) {
        match self.profile_manager.remove_profile(name) {
            Ok(()) => {
                self.status_message =
                    Some((format!("Deleted profile '{}'", name), MessageType::Success));
                self.selected_profile = None;
                self.refresh();
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Failed to delete profile: {e}"), MessageType::Error));
            }
        }
    }

    fn do_create_rule(&mut self) {
        let name = self.create_rule_name.trim();
        if name.is_empty() {
            self.status_message = Some(("Rule name is required".to_string(), MessageType::Error));
            return;
        }

        match self.rule_manager.create(name) {
            Ok(rule) => {
                self.status_message = Some((
                    format!("Created rule '{}'", rule.name),
                    MessageType::Success,
                ));
                self.selected_rule = Some(rule.name);
                self.show_create_rule_dialog = false;
                self.refresh();
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Failed to create rule: {e}"), MessageType::Error));
            }
        }
    }

    fn do_delete_rule(&mut self, name: &str) {
        match self.rule_manager.remove(name) {
            Ok(()) => {
                self.status_message =
                    Some((format!("Deleted rule '{}'", name), MessageType::Success));
                self.selected_rule = None;
                self.refresh();
            }
            Err(e) => {
                self.status_message =
                    Some((format!("Failed to delete rule: {e}"), MessageType::Error));
            }
        }
    }

    fn do_import_from_git(&mut self) {
        let url = self.import_url.trim();
        if url.is_empty() {
            self.status_message = Some(("Git URL is required".to_string(), MessageType::Error));
            self.show_import_dialog = false;
            return;
        }

        // Extract repo name from URL for default profile name
        let repo_name = extract_repo_name(url);

        // Create temp directory
        let temp_dir = std::env::temp_dir().join(format!("dot-agent-gui-{}", std::process::id()));

        self.status_message = Some((format!("Cloning {}...", url), MessageType::Info));

        // Build git clone command
        let mut cmd = Command::new("git");
        cmd.arg("clone");
        cmd.arg("--depth").arg("1");

        let branch = if self.import_branch.trim().is_empty() {
            None
        } else {
            Some(self.import_branch.trim().to_string())
        };

        if let Some(ref b) = branch {
            cmd.arg("--branch").arg(b);
        }

        cmd.arg(url);
        cmd.arg(&temp_dir);

        let output = match cmd.output() {
            Ok(o) => o,
            Err(e) => {
                self.status_message = Some((format!("Failed to run git: {e}"), MessageType::Error));
                self.show_import_dialog = false;
                return;
            }
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            self.status_message =
                Some((format!("git clone failed: {}", stderr), MessageType::Error));
            self.show_import_dialog = false;
            return;
        }

        // Determine import path
        let subpath = if self.import_subpath.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.import_subpath.trim()))
        };

        let import_path = if let Some(ref sub) = subpath {
            temp_dir.join(sub)
        } else {
            temp_dir.clone()
        };

        if !import_path.exists() {
            let _ = std::fs::remove_dir_all(&temp_dir);
            self.status_message = Some((
                format!("Subpath '{}' not found in repository", self.import_subpath),
                MessageType::Error,
            ));
            self.show_import_dialog = false;
            return;
        }

        // Determine profile name
        let profile_name = if self.import_name.trim().is_empty() {
            let mut parts = vec![repo_name];
            if let Some(ref sub) = subpath {
                if let Some(name) = sub.file_name() {
                    parts.push(name.to_string_lossy().to_string());
                }
            }
            if let Some(ref b) = branch {
                parts.push(b.clone());
            }
            parts.join("_")
        } else {
            self.import_name.trim().to_string()
        };

        // Import with git source info
        let subpath_str = subpath.as_ref().map(|p| p.to_string_lossy().to_string());
        let result = self.profile_manager.import_profile_from_git(
            &import_path,
            &profile_name,
            self.import_force,
            url,
            branch.as_deref(),
            None,
            subpath_str.as_deref(),
        );

        // Cleanup temp directory
        let _ = std::fs::remove_dir_all(&temp_dir);

        match result {
            Ok(profile) => {
                self.status_message = Some((
                    format!("Imported '{}' from {}", profile.name, url),
                    MessageType::Success,
                ));
                self.selected_profile = Some(profile.name);
                self.show_import_dialog = false;
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some((format!("Import failed: {e}"), MessageType::Error));
                self.show_import_dialog = false;
            }
        }
    }

    fn do_import_from_local(&mut self) {
        let source = self.import_local_path.trim();
        if source.is_empty() {
            self.status_message =
                Some(("Directory path is required".to_string(), MessageType::Error));
            self.show_import_dialog = false;
            return;
        }

        let source_path = PathBuf::from(source);

        // Determine import path with optional subpath
        let subpath = if self.import_subpath.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.import_subpath.trim()))
        };

        let import_path = if let Some(ref sub) = subpath {
            source_path.join(sub)
        } else {
            source_path.clone()
        };

        if !import_path.exists() {
            self.status_message = Some((
                format!("Directory not found: {}", import_path.display()),
                MessageType::Error,
            ));
            self.show_import_dialog = false;
            return;
        }

        // Determine profile name
        let profile_name = if self.import_name.trim().is_empty() {
            import_path
                .file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("profile"))
                .to_string_lossy()
                .to_string()
        } else {
            self.import_name.trim().to_string()
        };

        match self
            .profile_manager
            .import_profile(&import_path, &profile_name, self.import_force)
        {
            Ok(profile) => {
                self.status_message = Some((
                    format!("Imported '{}' from {}", profile.name, import_path.display()),
                    MessageType::Success,
                ));
                self.selected_profile = Some(profile.name);
                self.show_import_dialog = false;
                self.refresh();
            }
            Err(e) => {
                self.status_message = Some((format!("Import failed: {e}"), MessageType::Error));
                self.show_import_dialog = false;
            }
        }
    }
}

/// Extract repository name from Git URL
fn extract_repo_name(url: &str) -> String {
    // Handle various URL formats:
    // https://github.com/user/repo.git -> repo
    // https://github.com/user/repo -> repo
    // git@github.com:user/repo.git -> repo
    let url = url.trim_end_matches('/');
    let url = url.trim_end_matches(".git");

    if let Some(last_segment) = url.rsplit('/').next() {
        if !last_segment.is_empty() {
            return last_segment.to_string();
        }
    }

    // Fallback for git@github.com:user/repo format
    if let Some(pos) = url.rfind(':') {
        let after_colon = &url[pos + 1..];
        if let Some(repo) = after_colon.rsplit('/').next() {
            if !repo.is_empty() {
                return repo.to_string();
            }
        }
    }

    "profile".to_string()
}
