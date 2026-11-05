use super::*;

fn kind(name: &str) -> (&'static str, egui::Color32) {
    let (label, color) = match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "wgsl" => ("SHADER", [177, 135, 235]),
        "ttf" | "otf" => ("Aa", [229, 177, 117]),
        "json" | "toml" => ("{ }", [231, 191, 98]),
        "txt" | "md" => ("TEXT", [177, 190, 204]),
        "gltf" | "glb" | "obj" | "blend" => ("3D", [113, 194, 222]),
        "png" | "jpg" | "jpeg" => ("IMAGE", [126, 202, 151]),
        "wscene" => ("SCENE", [133, 177, 245]),
        "wprefab" => ("PREFAB", [100, 211, 180]),
        "wasset" => ("LIBRARY", [222, 177, 92]),
        "ts" | "js" => ("CODE", [130, 174, 238]),
        _ => ("FILE", [155, 165, 180]),
    };
    (label, egui::Color32::from_rgb(color[0], color[1], color[2]))
}
fn folder_icon(ui: &egui::Ui, rect: egui::Rect) {
    let r = egui::Rect::from_center_size(rect.center(), egui::vec2(30.0, 21.0));
    ui.painter().rect_filled(
        egui::Rect::from_min_size(r.min - egui::vec2(0.0, 4.0), egui::vec2(14.0, 8.0)),
        2.0,
        egui::Color32::from_rgb(195, 149, 68),
    );
    ui.painter()
        .rect_filled(r, 3.0, egui::Color32::from_rgb(228, 181, 88));
}
fn file_icon(ui: &egui::Ui, rect: egui::Rect, badge: &str, color: egui::Color32) {
    let painter = ui.painter();
    let r = egui::Rect::from_center_size(rect.center(), egui::vec2(32.0, 39.0));
    let fold = 8.0;
    painter.add(egui::Shape::convex_polygon(
        vec![
            r.left_top(),
            r.right_top() - egui::vec2(fold, 0.0),
            r.right_top() + egui::vec2(0.0, fold),
            r.right_bottom(),
            r.left_bottom(),
        ],
        color.gamma_multiply(0.12),
        egui::Stroke::new(1.3_f32, color),
    ));
    painter.line_segment(
        [
            r.right_top() - egui::vec2(fold, 0.0),
            r.right_top() + egui::vec2(-fold, fold),
        ],
        egui::Stroke::new(1.0_f32, color),
    );
    painter.line_segment(
        [
            r.right_top() + egui::vec2(-fold, fold),
            r.right_top() + egui::vec2(0.0, fold),
        ],
        egui::Stroke::new(1.0_f32, color),
    );
    let center = r.center() + egui::vec2(0.0, 4.0);
    if matches!(badge, "3D" | "SCENE") {
        let points = [
            [0.0, -10.0],
            [10.0, -5.0],
            [0.0, 0.0],
            [-10.0, -5.0],
            [10.0, 7.0],
            [0.0, 12.0],
            [-10.0, 7.0],
        ];
        for (a, b) in [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (1, 4),
            (4, 5),
            (5, 6),
            (6, 3),
            (2, 5),
        ] {
            painter.line_segment(
                [
                    center + egui::vec2(points[a][0], points[a][1]),
                    center + egui::vec2(points[b][0], points[b][1]),
                ],
                egui::Stroke::new(1.3_f32, color),
            );
        }
    } else if badge == "IMAGE" {
        painter.circle_filled(center + egui::vec2(6.0, -6.0), 2.5, color);
        painter.add(egui::Shape::line(
            vec![
                center + egui::vec2(-11.0, 8.0),
                center + egui::vec2(-3.0, -3.0),
                center + egui::vec2(2.0, 3.0),
                center + egui::vec2(6.0, 0.0),
                center + egui::vec2(11.0, 8.0),
            ],
            egui::Stroke::new(1.5_f32, color),
        ));
    } else if matches!(badge, "TEXT" | "FILE") {
        for (y, width) in [(-5.0, 20.0), (1.0, 20.0), (7.0, 13.0)] {
            painter.line_segment(
                [
                    center + egui::vec2(-10.0, y),
                    center + egui::vec2(width - 10.0, y),
                ],
                egui::Stroke::new(1.5_f32, color),
            );
        }
    } else {
        painter.text(
            center,
            egui::Align2::CENTER_CENTER,
            match badge {
                "SHADER" => "<>",
                "CODE" => "TS",
                other => other,
            },
            egui::FontId::monospace(14.0),
            color,
        );
    }
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum BrowserTab {
    #[default]
    Assets,
    Log,
}

impl Editor {
    pub(super) fn reference_allowed(&self, namespace: &str) -> bool {
        let owner = self
            .document
            .as_ref()
            .and_then(|doc| {
                let path = engine::project::portable_path(&doc.path);
                self.mod_roots
                    .iter()
                    .filter(|(_, root)| path.starts_with(engine::project::portable_path(root)))
                    .max_by_key(|(_, root)| root.components().count())
                    .map(|(id, _)| id)
            })
            .or_else(|| self.mod_roots.get(self.active_mod).map(|(id, _)| id));
        owner.is_some_and(|id| {
            id == namespace
                || self
                    .mod_dependencies
                    .get(id)
                    .is_some_and(|deps| deps.iter().any(|dep| dep == namespace))
        })
    }

    pub(super) fn browser_ui(&mut self, ui: &mut egui::Ui) {
        if self.has_dialog() {
            ui.disable();
        }
        ui.set_min_height(ui.available_height());
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.browser_tab, BrowserTab::Assets, "Assets");
            ui.selectable_value(&mut self.browser_tab, BrowserTab::Log, "Log");
            if self.browser_tab == BrowserTab::Assets {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.asset_search)
                            .hint_text("Search files")
                            .desired_width(190.0),
                    );
                });
            } else if ui.small_button("Clear").clicked() {
                self.logs.clear();
            }
        });
        ui.add_space(4.0);
        ui.separator();
        if self.browser_tab == BrowserTab::Log {
            egui::ScrollArea::vertical()
                .id_salt("editor_log")
                .auto_shrink([false, false])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    for line in &self.logs {
                        ui.monospace(line);
                    }
                });
            return;
        }
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(150.0, ui.available_height()),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("mod_browser")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for (index, (id, _)) in self.mod_roots.iter().enumerate() {
                                if ui
                                    .add_sized(
                                        [140.0, 25.0],
                                        egui::SelectableLabel::new(self.active_mod == index, id),
                                    )
                                    .clicked()
                                {
                                    self.active_mod = index;
                                    self.browser_path.clear();
                                }
                            }
                        });
                },
            );
            ui.separator();
            ui.vertical(|ui| {
                let active = self
                    .mod_roots
                    .get(self.active_mod)
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    if ui.small_button("Mod Settings").clicked() {
                        let result = self.edit_mod();
                        self.report(result);
                    }
                    ui.menu_button("Create", |ui| {
                        if ui.button("Folder").clicked() {
                            self.begin_asset_action(
                                &active,
                                &self.browser_path.clone(),
                                super::asset_actions::AssetOperation::Folder,
                            );
                            ui.close_menu();
                        }
                        if ui.button("Script").clicked() {
                            let result = self.new_script();
                            self.report(result);
                            ui.close_menu();
                        }
                        if ui.button("Scene").clicked() {
                            let result = self.action(Action::NewScene);
                            self.report(result);
                            ui.close_menu();
                        }
                        if ui.button("Prefab from selection…").clicked() {
                            let result = self.action(Action::SavePrefab);
                            self.report(result);
                            ui.close_menu();
                        }
                        if ui.button("Import asset library…").clicked() {
                            let result = self.action(Action::ImportLibrary);
                            self.report(result);
                            ui.close_menu();
                        }
                    });
                    if ui
                        .small_button("Refresh")
                        .on_hover_text("Refresh files and component types")
                        .clicked()
                    {
                        let result = self.refresh_resources();
                        self.report(result);
                    }
                    if ui.small_button(&active).clicked() {
                        self.browser_path.clear();
                    }
                    let mut path = String::new();
                    for part in self
                        .browser_path
                        .clone()
                        .split('/')
                        .filter(|v| !v.is_empty())
                    {
                        ui.label("/");
                        if !path.is_empty() {
                            path.push('/');
                        }
                        path.push_str(part);
                        if ui.small_button(part).clicked() {
                            self.browser_path = path.clone();
                        }
                    }
                });
                let prefix = if self.browser_path.is_empty() {
                    String::new()
                } else {
                    format!("{}/", self.browser_path)
                };
                let query = self.asset_search.to_lowercase();
                let items: Vec<_> = self
                    .resources
                    .iter()
                    .filter(|r| r.mod_id == active)
                    .filter_map(|r| {
                        let name = if query.is_empty() {
                            let tail = r.relative.strip_prefix(&prefix)?;
                            if tail.contains('/') {
                                return None;
                            }
                            tail.to_owned()
                        } else {
                            if !r.relative.to_lowercase().contains(&query) {
                                return None;
                            }
                            r.relative.clone()
                        };
                        Some((
                            name,
                            r.relative.clone(),
                            r.path.clone(),
                            r.uri.clone(),
                            r.directory,
                            r.scene,
                        ))
                    })
                    .collect();
                egui::ScrollArea::vertical()
                    .id_salt("folder_contents")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if items.is_empty() {
                            ui.weak(
                                "No matching files. Use Create to add a folder, script, or scene.",
                            );
                        }
                        ui.horizontal_wrapped(|ui| {
                            for (name, relative, path, uri, directory, scene) in items {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(120.0, 75.0),
                                    egui::Sense::click_and_drag(),
                                );
                                if response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        4.0,
                                        egui::Color32::from_rgb(44, 51, 64),
                                    );
                                }
                                if directory {
                                    folder_icon(
                                        ui,
                                        egui::Rect::from_min_size(
                                            rect.min,
                                            egui::vec2(120.0, 43.0),
                                        ),
                                    );
                                } else {
                                    let (badge, color) = kind(&name);
                                    file_icon(
                                        ui,
                                        egui::Rect::from_min_size(
                                            rect.min,
                                            egui::vec2(120.0, 48.0),
                                        ),
                                        badge,
                                        color,
                                    );
                                }
                                tile_label(ui, rect, name.rsplit('/').next().unwrap_or(&name));
                                let response = response.on_hover_text(if uri.is_empty() {
                                    display_path(&path)
                                } else {
                                    uri.clone()
                                });
                                if !directory && !uri.is_empty() {
                                    response.dnd_set_drag_payload(uri.clone());
                                }
                                response.context_menu(|ui| {
                                    for (label, operation) in [
                                        ("Rename…", super::asset_actions::AssetOperation::Rename),
                                        ("Move…", super::asset_actions::AssetOperation::Move),
                                        ("Delete…", super::asset_actions::AssetOperation::Delete),
                                    ] {
                                        if ui
                                            .add_enabled(
                                                self.live.is_none() && self.job.is_none(),
                                                egui::Button::new(label),
                                            )
                                            .clicked()
                                        {
                                            self.begin_asset_action(&active, &relative, operation);
                                            ui.close_menu();
                                        }
                                    }
                                    if !uri.is_empty() && ui.button("Copy VFS path").clicked() {
                                        ui.ctx().copy_text(uri.clone());
                                        ui.close_menu();
                                    }
                                    if ui.button("Copy file path").clicked() {
                                        ui.ctx().copy_text(display_path(&path));
                                        ui.close_menu();
                                    }
                                });
                                if response.double_clicked() {
                                    if directory {
                                        self.browser_path = relative;
                                    } else {
                                        let result = if path
                                            .extension()
                                            .is_some_and(|e| e == "wprefab")
                                        {
                                            self.place_prefab(&uri)
                                        } else if path.extension().is_some_and(|e| e == "wasset") {
                                            self.start_library_import(path.clone(), None, false)
                                        } else if scene {
                                            self.request_scene(&path)
                                        } else {
                                            if matches!(
                                                path.extension().and_then(|v| v.to_str()),
                                                Some(
                                                    "ts" | "js"
                                                        | "tsx"
                                                        | "jsx"
                                                        | "mts"
                                                        | "cts"
                                                        | "mjs"
                                                        | "cjs"
                                                        | "json"
                                                        | "toml"
                                                        | "wgsl"
                                                        | "glsl"
                                                        | "txt"
                                                        | "md"
                                                )
                                            ) {
                                                self.open_script(&path)
                                            } else {
                                                self.status =
                                                    format!("Selected {}", display_path(&path));
                                                Ok(())
                                            }
                                        };
                                        self.report(result);
                                    }
                                }
                            }
                        });
                    });
            });
        });
    }

    pub(super) fn collect_mod_files(
        &mut self,
        root: &Path,
        manifest: &engine::mods::ModManifest,
    ) -> anyhow::Result<()> {
        fn visit(
            root: &Path,
            at: &Path,
            manifest: &engine::mods::ModManifest,
            out: &mut Vec<Resource>,
        ) -> anyhow::Result<()> {
            for entry in std::fs::read_dir(at)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_symlink()
                    || [
                        ".git",
                        ".whatever",
                        "node_modules",
                        "logs",
                        "target",
                        "dist",
                        "types",
                        ".gitkeep",
                        ".gitignore",
                        ".wasset-import.json",
                    ]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                {
                    continue;
                }
                let relative = path
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let extension = path.extension().and_then(|v| v.to_str()).unwrap_or("");
                out.push(Resource {
                    uri: engine::project::resource_uri(manifest, &relative).unwrap_or_default(),
                    path: path.clone(),
                    scene: extension == "wscene",
                    source: matches!(
                        extension,
                        "ts" | "js" | "tsx" | "jsx" | "mts" | "cts" | "mjs" | "cjs"
                    ),
                    mod_id: manifest.meta.id.clone(),
                    relative,
                    directory: kind.is_dir(),
                });
                if kind.is_dir() {
                    visit(root, &path, manifest, out)?;
                }
            }
            Ok(())
        }
        visit(root, root, manifest, &mut self.resources)
    }
}
fn tile_label(ui: &egui::Ui, rect: egui::Rect, name: &str) {
    let color = ui.visuals().text_color();
    let mut job = egui::text::LayoutJob::simple_singleline(
        name.to_owned(),
        egui::FontId::proportional(12.0),
        color,
    );
    job.wrap.max_width = 116.0;
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let galley = ui.fonts(|fonts| fonts.layout_job(job));
    ui.painter().galley(
        egui::pos2(rect.center().x - galley.size().x * 0.5, rect.top() + 50.0),
        galley,
        color,
    );
}
#[cfg(test)]
mod layout_tests {
    use super::*;
    #[test]
    fn browser_splitter_resizes_in_the_full_workspace_for_both_tabs() {
        let mut editor = Editor::new(None, None);
        editor.project = Some(Project {
            version: 1,
            name: "Layout test".into(),
            id: "layout_test".into(),
            mods: vec![],
            build: Default::default(),
            path: PathBuf::from("layout-test/whatever.wproject"),
        });
        let ctx = editor.ctx.clone();
        let frame = |editor: &mut Editor, events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| editor.ui(ctx),
            );
        };
        for tab in [BrowserTab::Assets, BrowserTab::Log] {
            editor.browser_tab = tab;
            frame(&mut editor, vec![]);
            let before = editor.viewport.bottom();
            let start = egui::pos2(600.0, before + 1.0);
            let end = start - egui::vec2(0.0, 100.0);
            frame(&mut editor, vec![egui::Event::PointerMoved(start)]);
            frame(
                &mut editor,
                vec![egui::Event::PointerButton {
                    pos: start,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: Default::default(),
                }],
            );
            frame(&mut editor, vec![egui::Event::PointerMoved(end)]);
            frame(
                &mut editor,
                vec![egui::Event::PointerButton {
                    pos: end,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: Default::default(),
                }],
            );
            frame(&mut editor, vec![]);
            assert!(
                editor.viewport.bottom() < before - 80.0,
                "browser splitter did not move: {before} -> {}",
                editor.viewport.bottom()
            );
        }
    }
    #[test]
    fn switching_assets_and_empty_or_populated_log_preserves_panel_height() {
        let mut editor = Editor::new(None, None);
        let ctx = editor.ctx.clone();
        let mut sizes = Vec::new();
        for tab in [
            BrowserTab::Assets,
            BrowserTab::Log,
            BrowserTab::Assets,
            BrowserTab::Log,
            BrowserTab::Assets,
        ] {
            editor.browser_tab = tab;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 700.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    let response = egui::TopBottomPanel::bottom("test_browser")
                        .resizable(true)
                        .default_height(280.0)
                        .min_height(130.0)
                        .show(ctx, |ui| editor.browser_ui(ui));
                    sizes.push(response.response.rect.height());
                    egui::CentralPanel::default().show(ctx, |_| {});
                },
            );
            if tab == BrowserTab::Log {
                editor.logs = vec!["Log line".into(); 40];
            }
        }
        for height in sizes {
            assert!(
                (height - 280.0).abs() < 1.0,
                "panel height changed to {height}"
            );
        }
    }
    #[test]
    fn tile_captions_do_not_advance_wrapped_rows() {
        let ctx = egui::Context::default();
        let mut rects = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 400.0),
                )),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for _ in 0..5 {
                            let (rect, _) = ui.allocate_exact_size(
                                egui::vec2(120.0, 75.0),
                                egui::Sense::click_and_drag(),
                            );
                            tile_label(ui, rect, "a long script filename.ts");
                            rects.push(rect);
                        }
                    });
                });
            },
        );
        assert_eq!(rects[0].top(), rects[1].top());
        assert_eq!(rects[1].top(), rects[2].top());
        assert!(rects[3].top() > rects[0].bottom());
        assert_eq!(rects[3].top(), rects[4].top());
    }
}
