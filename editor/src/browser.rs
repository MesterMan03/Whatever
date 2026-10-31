use super::*;
use std::collections::BTreeSet;

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
impl Editor {
    pub(super) fn browser_ui(&mut self, ui: &mut egui::Ui) {
        if self.has_dialog() {
            ui.disable();
        }
        ui.horizontal(|ui| {
            for (index, label) in ["Project", "Assets", "Scripts", "Build Log"]
                .iter()
                .enumerate()
            {
                if ui
                    .selectable_value(&mut self.browser_tab, index, *label)
                    .clicked()
                {
                    self.browser_path.clear();
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.browser_tab != 3 {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.asset_search)
                            .hint_text("Search files")
                            .desired_width(190.0),
                    );
                }
            });
        });
        ui.separator();
        if self.browser_tab == 3 {
            egui::ScrollArea::vertical()
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
                    if self.browser_tab == 2 && ui.small_button("New Script").clicked() {
                        let result = self.new_script();
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
                        .filter(|part| !part.is_empty())
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
                let mut folders = BTreeSet::new();
                let mut files = Vec::new();
                for resource in &self.resources {
                    if resource.mod_id != active
                        || (self.browser_tab == 0 && !resource.scene)
                        || (self.browser_tab == 1 && (resource.scene || resource.source))
                        || (self.browser_tab == 2 && !resource.source)
                    {
                        continue;
                    }
                    let relative = resource
                        .uri
                        .split_once("://")
                        .map(|(_, path)| path)
                        .unwrap_or("");
                    if !self.asset_search.is_empty() {
                        if relative
                            .to_lowercase()
                            .contains(&self.asset_search.to_lowercase())
                        {
                            files.push((
                                relative.to_owned(),
                                resource.uri.clone(),
                                resource.path.clone(),
                                resource.scene,
                            ));
                        }
                    } else if let Some(tail) = relative.strip_prefix(&prefix) {
                        if let Some((folder, _)) = tail.split_once('/') {
                            folders.insert(folder.to_owned());
                        } else {
                            files.push((
                                tail.to_owned(),
                                resource.uri.clone(),
                                resource.path.clone(),
                                resource.scene,
                            ));
                        }
                    }
                }
                if folders.is_empty() && files.is_empty() {
                    ui.add(
                        egui::Label::new(if self.browser_tab == 0 {
                            "No scenes here. Use File > New Scene to create one."
                        } else {
                            "No matching assets in this folder."
                        })
                        .wrap(),
                    );
                }
                egui::ScrollArea::vertical()
                    .id_salt("folder_contents")
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            for folder in folders {
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(120.0, 75.0),
                                    egui::Sense::click(),
                                );
                                if response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        4.0,
                                        egui::Color32::from_rgb(44, 51, 64),
                                    );
                                }
                                folder_icon(
                                    ui,
                                    egui::Rect::from_min_size(rect.min, egui::vec2(120.0, 43.0)),
                                );
                                ui.painter().text(
                                    rect.center_bottom() - egui::vec2(0.0, 12.0),
                                    egui::Align2::CENTER_CENTER,
                                    &folder,
                                    egui::FontId::proportional(12.0),
                                    ui.visuals().text_color(),
                                );
                                if response.double_clicked() {
                                    self.browser_path = format!("{prefix}{folder}");
                                }
                            }
                            for (name, uri, path, scene) in files {
                                let (badge, color) = kind(&name);
                                let (rect, response) = ui.allocate_exact_size(
                                    egui::vec2(120.0, 75.0),
                                    egui::Sense::click_and_drag(),
                                );
                                let response = response.on_hover_text(&uri);
                                if response.hovered() {
                                    ui.painter().rect_filled(
                                        rect,
                                        4.0,
                                        egui::Color32::from_rgb(44, 51, 64),
                                    );
                                }
                                file_icon(
                                    ui,
                                    egui::Rect::from_min_size(rect.min, egui::vec2(120.0, 48.0)),
                                    badge,
                                    color,
                                );
                                tile_label(ui, rect, name.rsplit('/').next().unwrap_or(&name));
                                response.dnd_set_drag_payload(uri.clone());
                                response.context_menu(|ui| {
                                    if ui.button("Copy VFS path").clicked() {
                                        ui.ctx().copy_text(uri.clone());
                                        ui.close_menu();
                                    }
                                });
                                if response.double_clicked() {
                                    let result = if scene {
                                        self.request_scene(&path)
                                    } else if self.browser_tab == 2
                                        || matches!(
                                            path.extension().and_then(|e| e.to_str()),
                                            Some(
                                                "json"
                                                    | "txt"
                                                    | "wgsl"
                                                    | "glsl"
                                                    | "toml"
                                                    | "md"
                                                    | "ts"
                                                    | "js"
                                            )
                                        )
                                    {
                                        self.open_script(&path)
                                    } else {
                                        self.status = format!("Selected {}", display_path(&path));
                                        Ok(())
                                    };
                                    self.report(result);
                                }
                            }
                        });
                    });
            });
        });
    }
}

impl Editor {
    pub(super) fn collect_sources(&mut self, root: &Path, id: &str) -> anyhow::Result<()> {
        fn visit(root: &Path, at: &Path, out: &mut Vec<Resource>, id: &str) -> anyhow::Result<()> {
            for entry in std::fs::read_dir(at)? {
                let entry = entry?;
                let kind = entry.file_type()?;
                let path = entry.path();
                if kind.is_symlink() {
                    continue;
                }
                if kind.is_dir() {
                    if ![
                        "assets",
                        "scenes",
                        "node_modules",
                        ".git",
                        "logs",
                        "types",
                        "target",
                    ]
                    .contains(&entry.file_name().to_string_lossy().as_ref())
                    {
                        visit(root, &path, out, id)?;
                    }
                } else if path.extension().is_some_and(|s| {
                    ["ts", "js", "tsx", "jsx", "json"]
                        .iter()
                        .any(|e| s.eq_ignore_ascii_case(e))
                }) {
                    let relative = path
                        .strip_prefix(root)?
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push(Resource {
                        uri: format!("{id}://{relative}"),
                        path,
                        scene: false,
                        source: true,
                        mod_id: id.into(),
                    });
                }
            }
            Ok(())
        }
        visit(root, root, &mut self.resources, id)
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
