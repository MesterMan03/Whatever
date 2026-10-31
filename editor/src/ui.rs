use super::*;

impl Editor {
    pub(super) fn theme(&self) {
        let mut style = (*self.ctx.style()).clone();
        let mut visuals = egui::Visuals::dark();
        visuals.override_text_color = Some(egui::Color32::from_rgb(214, 219, 227));
        visuals.panel_fill = egui::Color32::from_rgb(29, 32, 38);
        visuals.window_fill = egui::Color32::from_rgb(34, 37, 44);
        visuals.extreme_bg_color = egui::Color32::from_rgb(21, 24, 29);
        visuals.faint_bg_color = egui::Color32::from_rgb(37, 41, 49);
        visuals.selection.bg_fill = egui::Color32::from_rgb(50, 79, 116);
        visuals.selection.stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(170, 208, 255));
        visuals.widgets.noninteractive.bg_stroke =
            egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(53, 58, 68));
        visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(43, 48, 57);
        visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(55, 66, 82);
        style.visuals = visuals;
        style.spacing.item_spacing = egui::vec2(8.0, 7.0);
        style.spacing.button_padding = egui::vec2(10.0, 5.0);
        style.spacing.text_edit_width = 180.0;
        self.ctx.set_style(style);
    }
    pub(super) fn ui(&mut self, ctx: &egui::Context) {
        self.dialogs(ctx);
        self.preferences_ui(ctx);
        self.mod_editor_ui(ctx);
        if self.input_blocked() {
            ctx.data_mut(|state| {
                state.remove::<hierarchy::DroppedEntity>(egui::Id::new("entity_drop"))
            });
            self.gizmo_drag = None;
            self.viewport_press = None;
            self.hierarchy_press = None;
            self.dragging_entity = None;
            self.dragging_entity = None;
        }
        if self.project.is_none() {
            self.viewport = egui::Rect::NOTHING;
            self.welcome_ui(ctx);
            return;
        }
        self.entity_shortcuts(ctx);

        ctx.data_mut(|state| {
            state.remove_temp::<hierarchy::DroppedEntity>(egui::Id::new("entity_drop"))
        });
        #[cfg(not(any(windows, target_os = "macos")))]
        egui::TopBottomPanel::top("menu").show(ctx, |ui| {
            egui::menu::bar(ui, |ui| {
                for (name, items) in menus::GROUPS {
                    ui.menu_button(*name, |ui| {
                        for (_, label, action) in *items {
                            if ui.button(*label).clicked() {
                                let result = self.action(*action);
                                self.report(result);
                                ui.close_menu();
                            }
                        }
                    });
                }
            });
        });
        #[cfg(not(any(windows, target_os = "macos")))]
        if !ctx.wants_keyboard_input() {
            for (key, action) in [
                (egui::Key::S, Action::Save),
                (egui::Key::O, Action::OpenProject),
                (egui::Key::B, Action::Build),
                (egui::Key::Z, Action::Undo),
                (egui::Key::Y, Action::Redo),
            ] {
                if ctx.input(|input| input.modifiers.command && input.key_pressed(key)) {
                    let result = self.action(action);
                    self.report(result);
                }
            }
        }
        egui::TopBottomPanel::top("toolbar")
            .exact_height(45.0)
            .show(ctx, |ui| {
                if self.has_dialog() {
                    ui.disable();
                }
                ui.horizontal_centered(|ui| {
                    ui.add_space(6.0);
                    ui.label(
                        egui::RichText::new("W")
                            .strong()
                            .size(22.0)
                            .color(egui::Color32::from_rgb(129, 174, 248)),
                    );
                    let project = self
                        .project
                        .as_ref()
                        .map(|p| p.name.as_str())
                        .unwrap_or("Whatever Editor");
                    ui.strong(project);
                    if let Some(doc) = &self.document {
                        ui.weak("/");
                        let name = doc.path.file_name().unwrap_or_default().to_string_lossy();
                        ui.label(format!("{name}{}", if self.unsaved() { " *" } else { "" }));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                self.project.is_some() && self.job.is_none(),
                                egui::Button::new("Build Game"),
                            )
                            .on_hover_text(
                                "Package the engine, mod runtime, launcher and selected mods",
                            )
                            .clicked()
                        {
                            let result = self.action(Action::Build);
                            self.report(result);
                        }
                        ui.separator();
                        if self.player.is_some() {
                            if ui.button("■  Stop").clicked() {
                                self.stop();
                            }
                        } else if ui
                            .add_enabled(
                                self.project.is_some() && self.job.is_none(),
                                egui::Button::new("▶  Play"),
                            )
                            .on_hover_text(
                                "Play in the viewport. Hold Shift to open a separate window.",
                            )
                            .clicked()
                        {
                            let result = self.play();
                            self.report(result);
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("status")
            .exact_height(26.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.job.is_some() {
                        ui.spinner();
                        ui.add(egui::Label::new(&self.job_label).truncate());
                    } else {
                        ui.add(egui::Label::new(&self.status).truncate());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(project) = &self.project {
                            ui.weak("Project")
                                .on_hover_text(display_path(&project.path));
                        }
                    });
                });
            });
        egui::TopBottomPanel::bottom("project_browser")
            .resizable(true)
            .default_height(220.0)
            .min_height(130.0)
            .show(ctx, |ui| self.browser_ui(ui));
        egui::SidePanel::left("hierarchy")
            .resizable(true)
            .default_width(260.0)
            .width_range(210.0..=450.0)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.strong("Hierarchy");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("+", |ui| {
                            for &(label, action) in menus::PRESETS {
                                if ui.button(label).clicked() {
                                    let result = self.action(action);
                                    self.report(result);
                                    ui.close_menu();
                                }
                            }
                        });
                    });
                });
                ui.separator();
                self.hierarchy_focused |= ui.rect_contains_pointer(ui.max_rect())
                    && ui.input(|i| i.pointer.primary_pressed());
                egui::ScrollArea::vertical()
                    .drag_to_scroll(false)
                    .show(ui, |ui| self.hierarchy_ui(ui));
            });
        egui::SidePanel::right("inspector")
            .resizable(true)
            .default_width(345.0)
            .width_range(290.0..=550.0)
            .show(ctx, |ui| {
                ui.strong("Inspector");
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_salt("inspector_scroll")
                    .drag_to_scroll(false)
                    .show(ui, |ui| {
                        ui.set_min_width(ui.available_width());
                        self.inspector_ui(ui);
                    });
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                self.viewport_ui(ui);
            });
    }
    pub(super) fn viewport_overlay(&self, ui: &mut egui::Ui) {
        let painter = ui.painter();
        let rect = self.viewport;
        let position = self.camera_position();
        let vp = Mat4::perspective_rh(
            45_f32.to_radians(),
            rect.width() / rect.height().max(1.0),
            0.01,
            100000.0,
        ) * Mat4::look_at_rh(position, self.target, Vec3::Y);
        let project = |point: Vec3| -> Option<egui::Pos2> {
            let clip = vp * point.extend(1.0);
            if clip.w < 0.01 {
                return None;
            }
            let ndc = clip.truncate() / clip.w;
            Some(egui::pos2(
                rect.left() + (ndc.x + 1.0) * 0.5 * rect.width(),
                rect.top() + (1.0 - ndc.y) * 0.5 * rect.height(),
            ))
        };
        for index in -20..=20 {
            for (start, end) in [
                (
                    Vec3::new(index as f32, 0.0, -20.0),
                    Vec3::new(index as f32, 0.0, 20.0),
                ),
                (
                    Vec3::new(-20.0, 0.0, index as f32),
                    Vec3::new(20.0, 0.0, index as f32),
                ),
            ] {
                if let (Some(a), Some(b)) = (project(start), project(end)) {
                    painter.line_segment(
                        [a, b],
                        egui::Stroke::new(
                            1.0_f32,
                            egui::Color32::from_white_alpha(if index == 0 { 45 } else { 15 }),
                        ),
                    );
                }
            }
        }
        if let Some(doc) = &self.document {
            let mut paths = Vec::new();
            hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
            for (index, id) in self.world.allocator.alive_entity_ids().enumerate() {
                if let Some(transform) = self.world.world_transform(&id)
                    && let Some(point) = project(Vec3::from(transform.position))
                {
                    let selected = paths.get(index) == Some(&doc.selected);
                    let color = if selected {
                        egui::Color32::from_rgb(255, 184, 95)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(130, 177, 229, 100)
                    };
                    painter.circle_stroke(
                        point,
                        if selected { 7.0 } else { 3.0 },
                        egui::Stroke::new(1.5_f32, color),
                    );
                    if selected && self.tool == viewport::Tool::Select {
                        for (axis, color) in [
                            (Vec3::X, egui::Color32::from_rgb(229, 95, 104)),
                            (Vec3::Y, egui::Color32::from_rgb(113, 211, 135)),
                            (Vec3::Z, egui::Color32::from_rgb(99, 158, 245)),
                        ] {
                            if let Some(end) = project(Vec3::from(transform.position) + axis) {
                                painter
                                    .line_segment([point, end], egui::Stroke::new(2.0_f32, color));
                            }
                        }
                    }
                }
            }
        }
        painter.text(
            rect.left_top() + egui::vec2(16.0, 12.0),
            egui::Align2::LEFT_TOP,
            "SCENE",
            egui::FontId::monospace(11.0),
            egui::Color32::from_gray(170),
        );
        painter.text(
            rect.left_bottom() + egui::vec2(16.0, -12.0),
            egui::Align2::LEFT_BOTTOM,
            "Drag to orbit   ·   MMB pan   ·   Scroll zoom   ·   F frame",
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(145),
        );
        if self.project.is_none() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Create or open a project from the File menu",
                egui::FontId::proportional(20.0),
                egui::Color32::GRAY,
            );
        }
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            let mut open = true;
            egui::Modal::new(egui::Id::new("project_settings")).show(ctx,|ui|{ui.set_width(460.0);ui.horizontal(|ui| {ui.heading("Project & Build Settings");if ui.button("Close").clicked(){open=false;}});
            if let Some(mut project)=self.project.clone(){
                let mut changed=false;ui.heading("Game branding");ui.label("Game ID");changed|=ui.text_edit_singleline(&mut project.id).changed();ui.label("Display name / executable name");changed|=ui.text_edit_singleline(&mut project.name).changed();
                ui.horizontal(|ui|{ui.label("Version");changed|=ui.text_edit_singleline(&mut project.build.version).changed();});
                ui.horizontal(|ui|{ui.label("Company");changed|=ui.text_edit_singleline(&mut project.build.company).changed();});
                ui.separator();ui.label("Executable icon");
                ui.horizontal(|ui|{ui.label(project.build.icon.as_deref().and_then(|path|Path::new(path).file_name()).map(|name|name.to_string_lossy().into_owned()).unwrap_or("Whatever icon".into()));
                    if ui.button("Choose…").clicked()&&let Some(path)=self.file_dialog("Choose game icon").add_filter("Game icon",&["png","ico"]).pick_file(){
                        let result=(||->anyhow::Result<String>{let root=project.path.parent().context("project directory")?;let directory=root.join("branding");std::fs::create_dir_all(&directory)?;
                            let extension=path.extension().unwrap_or_default().to_string_lossy();let target=directory.join(format!("game-icon.{extension}"));
                            if path.canonicalize()?!=target.canonicalize().unwrap_or(target.clone()){std::fs::copy(&path,&target)?;}
                            Ok(format!("branding/game-icon.{extension}"))})();match result{Ok(path)=>{project.build.icon=Some(path);changed=true;},Err(error)=>self.status=format!("{error:#}")}
                    }
                    if project.build.icon.is_some()&&ui.button("Reset").clicked(){project.build.icon=None;changed=true;}
                });
                ui.weak("PNG or ICO. Build embeds multiple icon sizes and game metadata into the launcher.");
                ui.separator();ui.heading("Build");ui.label("Exclude mods from the final build");
                for (id,_) in &self.mod_roots { if id=="core" {continue;} let mut excluded=project.build.exclude_mods.contains(id);if ui.checkbox(&mut excluded,id).changed(){project.build.exclude_mods.retain(|item|item!=id);if excluded{project.build.exclude_mods.push(id.clone());}changed=true;} }
                ui.weak("Player and mod runtime are included automatically.");
                ui.horizontal(|ui|{ui.add(egui::Label::new(self.build_destination.as_ref().map(|path|display_path(path)).unwrap_or("Choose a destination on first Build".into())).truncate());
                    if ui.button("Browse…").clicked(){self.build_destination=self.file_dialog("Choose game build folder").pick_folder();}});
                ui.weak("An empty folder or a previous Whatever build can be used. Source mods are protected.");
                if changed{match project.save(){Ok(())=>{if let Some(window)=&self.window{window.set_title(&format!("{} — Whatever Editor",project.name));}self.project=Some(project);},Err(error)=>self.status=format!("{error:#}")}}
                ui.add_space(10.0);if ui.button("Build Game").clicked(){let result=self.action(Action::Build);self.report(result);}
            }
        });
            self.show_settings = open && !ctx.input(|i| i.key_pressed(egui::Key::Escape));
        }
        if self.show_new_project || self.show_new_mod {
            let is_project = self.show_new_project;
            let mut open = true;
            let mut create = false;
            egui::Modal::new(egui::Id::new("create_project_mod")).show(ctx, |ui| {
                ui.set_width(420.0);
                ui.horizontal(|ui| {
                    ui.heading(if is_project {
                        "New Project"
                    } else {
                        "Create Mod"
                    });
                    if ui.button("Cancel").clicked() {
                        open = false;
                    }
                });
                ui.label("Name");
                ui.text_edit_singleline(&mut self.draft_name);
                ui.label(if is_project { "Game ID" } else { "Mod ID" });
                ui.text_edit_singleline(&mut self.draft_id);
                if is_project {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                self.draft_directory
                                    .as_ref()
                                    .map(|path| display_path(path))
                                    .unwrap_or("Choose a parent directory".into()),
                            )
                            .truncate(),
                        );
                        if ui.button("Browse…").clicked() {
                            self.draft_directory = self
                                .file_dialog("Choose project parent directory")
                                .pick_folder();
                        }
                    });
                }
                ui.add_space(10.0);
                create = ui
                    .add_enabled(
                        !is_project || self.draft_directory.is_some(),
                        egui::Button::new(if is_project {
                            "Create Project"
                        } else {
                            "Create Mod"
                        }),
                    )
                    .clicked();
            });
            if create {
                let result = (|| -> anyhow::Result<()> {
                    ensure!(
                        engine::sandbox::valid_storage_id(&self.draft_id),
                        "use a lowercase ID containing letters, digits, underscores or hyphens"
                    );
                    if is_project {
                        if !self.confirm_leave()? {
                            return Ok(());
                        }
                        let directory = self
                            .draft_directory
                            .as_ref()
                            .context("project parent directory")?
                            .join(&self.draft_id);
                        let project = engine::project::create_project(
                            &directory,
                            &self.draft_id,
                            &self.draft_name,
                            &engine::project::default_core()?,
                        )?;
                        self.set_project(project)?;
                    } else {
                        self.save_if_dirty()?;
                        let mut project = self.project.clone().context("open a project")?;
                        let root = project
                            .path
                            .parent()
                            .context("project directory")?
                            .join("mods")
                            .join(&self.draft_id);
                        engine::project::create_mod(
                            &root,
                            &engine::project::ModOptions::new(&self.draft_id, &self.draft_name),
                        )?;
                        project.add_mod(&root)?;
                        self.set_project(project)?;
                        self.open_scene(&root.join("scenes/main.wscene"))?;
                    }
                    open = false;
                    Ok(())
                })();
                self.report(result);
            }
            self.show_new_project = is_project && open;
            self.show_new_mod = !is_project && open;
        }
    }
}
