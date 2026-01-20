use std::path::PathBuf;

use eframe::egui;

use dot_agent_core::installer::Installer;
use dot_agent_core::profile::ProfileManager;
use dot_agent_core::{DotAgentError, Result};

pub fn run() -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([400.0, 300.0]),
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

struct DotAgentApp {
    base_dir: PathBuf,
    manager: ProfileManager,
    selected_profile: Option<String>,
    target_path: String,
    install_global: bool,
    no_prefix: bool,
    force: bool,
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

        let manager = ProfileManager::new(base_dir.clone());

        Self {
            base_dir,
            manager,
            selected_profile: None,
            target_path: String::new(),
            install_global: false,
            no_prefix: false,
            force: false,
            status_message: None,
        }
    }

    fn refresh_profiles(&mut self) {
        self.manager = ProfileManager::new(self.base_dir.clone());
    }
}

impl eframe::App for DotAgentApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.heading("dot-agent");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Refresh").clicked() {
                        self.refresh_profiles();
                        self.status_message =
                            Some(("Profiles refreshed".to_string(), MessageType::Info));
                    }
                });
            });
            ui.add_space(8.0);
        });

        egui::SidePanel::left("profiles")
            .resizable(true)
            .default_width(200.0)
            .show(ctx, |ui| {
                ui.add_space(8.0);
                ui.heading("Profiles");
                ui.separator();

                match self.manager.list_profiles() {
                    Ok(profiles) => {
                        if profiles.is_empty() {
                            ui.label("No profiles found");
                        } else {
                            for profile in profiles {
                                let selected =
                                    self.selected_profile.as_ref() == Some(&profile.name);
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
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let mut clear_status = false;
            if let Some((msg, msg_type)) = &self.status_message {
                let color = match msg_type {
                    MessageType::Success => egui::Color32::GREEN,
                    MessageType::Error => egui::Color32::RED,
                    MessageType::Info => egui::Color32::LIGHT_BLUE,
                };
                ui.horizontal(|ui| {
                    ui.colored_label(color, msg.clone());
                    if ui.small_button("x").clicked() {
                        clear_status = true;
                    }
                });
                ui.separator();
            }
            if clear_status {
                self.status_message = None;
            }

            if let Some(profile_name) = &self.selected_profile.clone() {
                ui.heading(format!("Profile: {profile_name}"));
                ui.add_space(8.0);

                match self.manager.get_profile(profile_name) {
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
                            if ui.button("Install").clicked() {
                                let installer = Installer::new(self.base_dir.clone());
                                let target = if self.install_global || self.target_path.is_empty() {
                                    None
                                } else {
                                    Some(PathBuf::from(&self.target_path))
                                };

                                match installer
                                    .resolve_target(target.as_deref(), self.install_global)
                                {
                                    Ok(target_dir) => {
                                        match installer.install(
                                            &profile,
                                            &target_dir,
                                            self.force,
                                            false,
                                            self.no_prefix,
                                            None,
                                        ) {
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
                                                self.status_message = Some((
                                                    format!("Install failed: {e}"),
                                                    MessageType::Error,
                                                ));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.status_message = Some((
                                            format!("Invalid target: {e}"),
                                            MessageType::Error,
                                        ));
                                    }
                                }
                            }

                            if ui.button("Diff").clicked() {
                                let installer = Installer::new(self.base_dir.clone());
                                let target = if self.install_global || self.target_path.is_empty() {
                                    None
                                } else {
                                    Some(PathBuf::from(&self.target_path))
                                };

                                match installer
                                    .resolve_target(target.as_deref(), self.install_global)
                                {
                                    Ok(target_dir) => match installer.diff(&profile, &target_dir) {
                                        Ok(result) => {
                                            self.status_message = Some((
                                                format!(
                                                    "Modified: {}, Missing: {}, Unchanged: {}",
                                                    result.modified,
                                                    result.missing,
                                                    result.unchanged
                                                ),
                                                MessageType::Info,
                                            ));
                                        }
                                        Err(e) => {
                                            self.status_message = Some((
                                                format!("Diff failed: {e}"),
                                                MessageType::Error,
                                            ));
                                        }
                                    },
                                    Err(e) => {
                                        self.status_message = Some((
                                            format!("Invalid target: {e}"),
                                            MessageType::Error,
                                        ));
                                    }
                                }
                            }
                        });
                    }
                    Err(e) => {
                        ui.colored_label(egui::Color32::RED, format!("Error loading profile: {e}"));
                    }
                }
            } else {
                ui.vertical_centered(|ui| {
                    ui.add_space(100.0);
                    ui.heading("Select a profile from the sidebar");
                    ui.label("Or create a new profile using the CLI:");
                    ui.code("dot-agent profile add <name>");
                });
            }
        });
    }
}
