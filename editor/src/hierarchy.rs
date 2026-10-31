use super::*;

pub(super) fn entity_paths(
    items: &[SceneEntity],
    prefix: &mut Vec<usize>,
    out: &mut Vec<Vec<usize>>,
) {
    for (index, item) in items.iter().enumerate() {
        prefix.push(index);
        out.push(prefix.clone());
        entity_paths(&item.children, prefix, out);
        prefix.pop();
    }
}

#[derive(Clone, Default)]
pub(super) struct DroppedEntity {
    pub path: Vec<usize>,
    pub position: egui::Pos2,
}

enum TreeEvent {
    Select(Vec<usize>),
    Command(Vec<usize>, Action),
    Move(Vec<usize>, Vec<usize>, usize),
}

fn drop_target(
    path: &[usize],
    child_count: usize,
    rect: egui::Rect,
    y: f32,
) -> (Vec<usize>, usize) {
    if y < rect.top() + 7.0 {
        (path[..path.len() - 1].to_vec(), path[path.len() - 1])
    } else if y > rect.bottom() - 7.0 {
        (path[..path.len() - 1].to_vec(), path[path.len() - 1] + 1)
    } else {
        (path.to_vec(), child_count)
    }
}

// Native event loops can deliver a whole fast drag between two rendered frames.
// Recover its endpoints instead of requiring an intermediate button-down frame.
fn pointer_gesture(
    ctx: &egui::Context,
    start: &mut Option<egui::Pos2>,
) -> Option<(egui::Pos2, egui::Pos2)> {
    ctx.input(|input| {
        let mut end = None;
        for event in &input.events {
            if let egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                ..
            } = event
            {
                if *pressed {
                    *start = Some(*pos);
                } else {
                    end = Some(*pos);
                }
            }
        }
        let gesture = start
            .zip(end)
            .filter(|(start, end)| start.distance(*end) > 6.0);
        if !input.pointer.primary_down() {
            *start = None;
        }
        gesture
    })
}

fn gesture_move(
    items: &[SceneEntity],
    hits: &[(egui::Rect, Vec<usize>)],
    root: egui::Rect,
    start: egui::Pos2,
    end: egui::Pos2,
) -> Option<TreeEvent> {
    let (_, source) = hits.iter().find(|(rect, _)| rect.contains(start))?;
    let (parent, index) =
        if let Some((rect, path)) = hits.iter().find(|(rect, _)| rect.contains(end)) {
            drop_target(path, entity_at(items, path)?.children.len(), *rect, end.y)
        } else if root.contains(end) {
            (Vec::new(), items.len())
        } else {
            return None;
        };
    Some(TreeEvent::Move(source.clone(), parent, index))
}

fn rows(
    ui: &mut egui::Ui,
    items: &[SceneEntity],
    prefix: &[usize],
    selected: &[usize],
    events: &mut Vec<TreeEvent>,
    dragging: &mut Option<Vec<usize>>,
    hits: &mut Vec<(egui::Rect, Vec<usize>)>,
) {
    for (index, entity) in items.iter().enumerate() {
        let mut path = prefix.to_vec();
        path.push(index);
        let id = ui.make_persistent_id((&path, "node"));
        let mut expanded = ui
            .ctx()
            .data_mut(|data| data.get_temp::<bool>(id).unwrap_or(true));
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(ui.available_width(), 26.0),
            egui::Sense::click_and_drag(),
        );
        hits.push((rect.intersect(ui.clip_rect()), path.clone()));
        if path == selected {
            ui.painter()
                .rect_filled(rect, 3.0, egui::Color32::from_rgb(49, 72, 104));
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, 3.0, egui::Color32::from_rgb(40, 45, 55));
        }
        let x = rect.left() + 8.0 + prefix.len() as f32 * 15.0;
        let arrow =
            egui::Rect::from_min_size(egui::pos2(x, rect.top() + 3.0), egui::vec2(18.0, 20.0));
        let mut toggle = false;
        if !entity.children.is_empty() {
            let click = ui.interact(arrow, id.with("arrow"), egui::Sense::click());
            if click.clicked() {
                expanded = !expanded;
                toggle = true;
            }
            let center = arrow.center();
            let points = if expanded {
                vec![
                    center + egui::vec2(-4.0, -2.0),
                    center + egui::vec2(4.0, -2.0),
                    center + egui::vec2(0.0, 3.0),
                ]
            } else {
                vec![
                    center + egui::vec2(-2.0, -4.0),
                    center + egui::vec2(-2.0, 4.0),
                    center + egui::vec2(3.0, 0.0),
                ]
            };
            ui.painter().add(egui::Shape::convex_polygon(
                points,
                egui::Color32::GRAY,
                egui::Stroke::new(0.0_f32, egui::Color32::TRANSPARENT),
            ));
        }
        let icon = egui::Rect::from_center_size(
            egui::pos2(x + 28.0, rect.center().y),
            egui::vec2(10.0, 10.0),
        );
        let stroke = egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(130, 177, 229));
        if entity.components.keys().any(|kind| kind.ends_with("light")) {
            ui.painter().circle_stroke(icon.center(), 4.0, stroke);
        } else {
            ui.painter().rect_stroke(icon, 1.0, stroke);
            ui.painter()
                .line_segment([icon.left_top(), icon.right_bottom()], stroke);
        }
        let name = if entity.name.is_empty() {
            entity.script_id.as_deref().unwrap_or("Entity")
        } else {
            &entity.name
        };
        ui.painter().text(
            egui::pos2(x + 44.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            name,
            egui::FontId::proportional(13.0),
            egui::Color32::from_gray(if entity.enabled { 217 } else { 95 }),
        );
        if response.clicked() && !toggle {
            events.push(TreeEvent::Select(path.clone()));
        }
        if response.drag_started() {
            *dragging = Some(path.clone());
        }
        response.context_menu(|ui| {
            ui.menu_button("Create", |ui| {
                for &(label, action) in menus::PRESETS {
                    if ui.button(label).clicked() {
                        events.push(TreeEvent::Command(path.clone(), action));
                        ui.close_menu();
                    }
                }
            });
            for (label, action) in [
                ("Copy (Ctrl+C)", Action::Copy),
                ("Cut (Ctrl+X)", Action::Cut),
                ("Paste (Ctrl+V)", Action::Paste),
                ("Duplicate (Ctrl+D)", Action::Duplicate),
                ("Rename (F2)", Action::Rename),
                ("Frame selected", Action::Frame),
                ("Delete", Action::Delete),
            ] {
                if ui.button(label).clicked() {
                    events.push(TreeEvent::Command(path.clone(), action));
                    ui.close_menu();
                }
            }
        });
        if let Some(source) = dragging.as_ref().filter(|_| {
            ui.ctx()
                .pointer_hover_pos()
                .is_some_and(|point| rect.contains(point))
        }) {
            let y = ui
                .ctx()
                .pointer_hover_pos()
                .map(|point| point.y)
                .unwrap_or(rect.center().y);
            let (parent, position) = drop_target(&path, entity.children.len(), rect, y);
            let valid = !parent.starts_with(source.as_slice());
            let color = if valid {
                egui::Color32::from_rgb(110, 176, 255)
            } else {
                egui::Color32::from_rgb(228, 106, 116)
            };
            if parent == path {
                ui.painter()
                    .rect_stroke(rect, 3.0, egui::Stroke::new(1.5_f32, color));
            } else {
                let y = if position == index {
                    rect.top()
                } else {
                    rect.bottom()
                };
                ui.painter().line_segment(
                    [
                        egui::pos2(rect.left() + 5.0, y),
                        egui::pos2(rect.right(), y),
                    ],
                    egui::Stroke::new(2.0_f32, color),
                );
            }
            if ui.ctx().input(|input| input.pointer.any_released()) {
                events.push(TreeEvent::Move(source.clone(), parent, position));
            }
        }
        ui.ctx().data_mut(|data| data.insert_temp(id, expanded));
        if expanded {
            rows(
                ui,
                &entity.children,
                &path,
                selected,
                events,
                dragging,
                hits,
            );
        }
    }
}

impl Editor {
    pub(super) fn hierarchy_ui(&mut self, ui: &mut egui::Ui) {
        if self.has_dialog() || self.live_waiting() {
            ui.disable();
        }
        let mut events = Vec::new();
        let mut hits = Vec::new();
        if let Some(doc) = &self.document {
            rows(
                ui,
                &doc.scene.entities,
                &[],
                &doc.selected,
                &mut events,
                &mut self.dragging_entity,
                &mut hits,
            );
            let (rect, _) = ui.allocate_exact_size(
                egui::vec2(ui.available_width(), ui.available_height().max(50.0)),
                egui::Sense::hover(),
            );
            let root_drop = ui
                .ctx()
                .pointer_hover_pos()
                .is_some_and(|point| rect.contains(point));
            if self.dragging_entity.is_some() && root_drop {
                ui.painter().rect_stroke(
                    rect.shrink(5.0),
                    4.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::from_rgb(110, 176, 255)),
                );
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Move to scene root",
                    egui::FontId::proportional(12.0),
                    egui::Color32::GRAY,
                );
            }
            if let Some(source) = self
                .dragging_entity
                .as_ref()
                .filter(|_| root_drop && ui.ctx().input(|input| input.pointer.any_released()))
            {
                events.push(TreeEvent::Move(
                    source.clone(),
                    Vec::new(),
                    doc.scene.entities.len(),
                ));
            }
            if !self.input_blocked()
                && let Some((start, end)) = pointer_gesture(ui.ctx(), &mut self.hierarchy_press)
                && ui.ctx().layer_id_at(start) == Some(ui.layer_id())
            {
                if let Some((_, path)) = hits.iter().find(|(rect, _)| rect.contains(start)) {
                    ui.ctx().data_mut(|state| {
                        state.insert_temp(
                            egui::Id::new("entity_drop"),
                            DroppedEntity {
                                path: path.clone(),
                                position: end,
                            },
                        )
                    });
                }
                events.retain(|event| matches!(event, TreeEvent::Command(..)));
                if let Some(event) = gesture_move(&doc.scene.entities, &hits, rect, start, end) {
                    events.push(event);
                }
            }
        } else {
            ui.add_space(25.0);
            ui.weak("Open a scene from the Project browser\nor create one from File > New Scene.");
        }
        if ui.ctx().input(|input| input.pointer.any_released()) {
            self.dragging_entity = None;
        }
        if self.input_blocked() || self.live_waiting() {
            self.dragging_entity = None;
            return;
        }
        for event in events {
            let result = match event {
                TreeEvent::Select(path) => self.select(path),
                TreeEvent::Command(path, action) => {
                    self.select(path).and_then(|()| self.action(action))
                }
                TreeEvent::Move(source, parent, index) => (|| {
                    self.commit_inspector()?;
                    self.document
                        .as_mut()
                        .context("scene")?
                        .move_entity(&source, &parent, index)?;
                    self.sync_selection();
                    self.preview_dirty = true;
                    Ok(())
                })(),
            };
            self.report(result);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_gestures_reparent_and_reorder_rows() {
        let context = egui::Context::default();
        let items = vec![
            SceneEntity {
                name: "Root".into(),
                children: vec![SceneEntity::default()],
                ..Default::default()
            },
            SceneEntity {
                name: "Other".into(),
                ..Default::default()
            },
        ];
        for (source, target, expected_parent, expected_index) in [
            (egui::pos2(100.0, 39.0), egui::pos2(100.0, 65.0), vec![1], 0),
            (egui::pos2(100.0, 65.0), egui::pos2(100.0, 2.0), vec![], 0),
        ] {
            let mut dragging = None;
            let mut press = None;
            let mut events = Vec::new();
            let pointer = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            for input_events in [
                vec![],
                vec![egui::Event::PointerMoved(source)],
                vec![pointer(source, true)],
                vec![egui::Event::PointerMoved(source + egui::vec2(15.0, 0.0))],
                vec![egui::Event::PointerMoved(target)],
                vec![pointer(target, false)],
            ] {
                let _ = context.run(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(260.0, 400.0),
                        )),
                        events: input_events,
                        ..Default::default()
                    },
                    |ctx| {
                        egui::CentralPanel::default()
                            .frame(egui::Frame::none())
                            .show(ctx, |ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                let mut hits = Vec::new();
                                rows(ui, &items, &[], &[], &mut events, &mut dragging, &mut hits);
                                if let Some((start, end)) = pointer_gesture(ctx, &mut press) {
                                    events.clear();
                                    if let Some(event) =
                                        gesture_move(&items, &hits, egui::Rect::NOTHING, start, end)
                                    {
                                        events.push(event);
                                    }
                                }
                            });
                    },
                );
            }
            assert!(events.iter().any(|event|matches!(event,TreeEvent::Move(_,parent,index) if *parent==expected_parent&&*index==expected_index)),"pointer release must produce the expected hierarchy move");
            // Exercise press, movement and release delivered in one native event-loop batch.
            let mut coalesced = None;
            let _ = context.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(260.0, 400.0),
                    )),
                    events: vec![
                        egui::Event::PointerMoved(source),
                        pointer(source, true),
                        egui::Event::PointerMoved(target),
                        pointer(target, false),
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::none())
                        .show(ctx, |ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            let mut hits = Vec::new();
                            rows(ui, &items, &[], &[], &mut Vec::new(), &mut None, &mut hits);
                            let (start, end) = pointer_gesture(ctx, &mut press)
                                .expect("coalesced pointer gesture");
                            coalesced =
                                gesture_move(&items, &hits, egui::Rect::NOTHING, start, end);
                        });
                },
            );
            assert!(
                matches!(coalesced,Some(TreeEvent::Move(_,parent,index)) if parent==expected_parent&&index==expected_index)
            );
        }
    }
}

impl Editor {
    pub(super) fn entity_shortcuts(&mut self, ctx: &egui::Context) {
        if self.input_blocked() || ctx.wants_keyboard_input() {
            return;
        }
        if ctx.input(|i| i.pointer.primary_pressed()) {
            self.hierarchy_focused = ctx
                .pointer_hover_pos()
                .is_some_and(|p| p.x < self.viewport.left());
        }
        if !self.hierarchy_focused {
            return;
        }
        for event in ctx.input(|i| i.events.clone()) {
            let action = match event {
                egui::Event::Copy => Some(Action::Copy),
                egui::Event::Cut => Some(Action::Cut),
                egui::Event::Paste(text) => {
                    self.entity_clipboard = Some(text);
                    Some(Action::Paste)
                }
                egui::Event::Key {
                    key: egui::Key::Delete,
                    pressed: true,
                    ..
                } => Some(Action::Delete),
                egui::Event::Key {
                    key: egui::Key::F2,
                    pressed: true,
                    ..
                } => Some(Action::Rename),
                egui::Event::Key {
                    key: egui::Key::D,
                    pressed: true,
                    modifiers,
                    ..
                } if modifiers.command => Some(Action::Duplicate),
                _ => None,
            };
            if let Some(action) = action {
                let result = self.action(action);
                self.report(result);
            }
        }
    }
}

#[cfg(test)]
mod modal_tests {
    use super::*;
    #[test]
    fn settings_modal_blocks_row_selection_and_entity_shortcuts() {
        let path = std::env::temp_dir().join(format!(
            "whatever-modal-{}-{}.wscene",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::write(&path,r#"{"version":1,"entities":[{"script_id":"a","name":"A"},{"script_id":"b","name":"B"}]}"#).expect("fixture");
        let mut editor = Editor::new(None, None);
        editor.document = Some(Document::open(&path).expect("open"));
        std::fs::remove_file(&path).expect("cleanup");
        editor.document.as_mut().expect("doc").selected = vec![0];
        editor.hierarchy_focused = true;
        let ctx = editor.ctx.clone();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1200.0, 800.0));
        let mut hits = Vec::new();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    rows(
                        ui,
                        &editor.document.as_ref().expect("doc").scene.entities,
                        &[],
                        &[0],
                        &mut Vec::new(),
                        &mut None,
                        &mut hits,
                    );
                });
            },
        );
        let point = hits[1].0.center();
        editor.show_preferences = true;
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                        egui::Event::Key {
                            key: egui::Key::Delete,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        },
                        egui::Event::Key {
                            key: egui::Key::F2,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    editor.preferences_ui(ctx);
                    editor.entity_shortcuts(ctx);
                    egui::CentralPanel::default().show(ctx, |ui| editor.hierarchy_ui(ui));
                },
            );
        }
        assert_eq!(editor.document.as_ref().expect("doc").selected, vec![0]);
        assert_eq!(
            editor.document.as_ref().expect("doc").scene.entities.len(),
            2
        );
        assert!(!editor.rename_requested);
        editor.show_preferences = false;
        for _ in 0..2 {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| editor.hierarchy_ui(ui));
                },
            );
        }
        for pressed in [true, false] {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        egui::Event::PointerMoved(point),
                        egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| editor.hierarchy_ui(ui));
                },
            );
        }
        assert_eq!(editor.document.as_ref().expect("doc").selected, vec![1]);
    }
}
