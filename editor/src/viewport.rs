use super::*;
use engine::ecs::{EntityId, Transform};
use glam::{Quat, Vec2};

/// Welcome and hidden/collapsed views have no scene rectangle to send to the GPU.
pub(super) fn physical_viewport(
    rect: egui::Rect,
    pixels_per_point: f32,
    surface: [u32; 2],
) -> Option<[u32; 4]> {
    if !rect.is_finite()
        || !rect.is_positive()
        || !pixels_per_point.is_finite()
        || pixels_per_point <= 0.0
    {
        return None;
    }
    let physical =
        egui::Rect::from_min_max(rect.min * pixels_per_point, rect.max * pixels_per_point);
    if !physical.is_finite() {
        return None;
    }
    let clipped = physical.intersect(egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(surface[0] as f32, surface[1] as f32),
    ));
    if !clipped.is_positive() {
        return None;
    }
    let x = clipped.min.x.floor() as u32;
    let y = clipped.min.y.floor() as u32;
    let width = (clipped.max.x.ceil() as u32).saturating_sub(x);
    let height = (clipped.max.y.ceil() as u32).saturating_sub(y);
    (width > 0 && height > 0).then_some([x, y, width, height])
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Tool {
    #[default]
    Select,
    Move,
    Rotate,
    Scale,
}

#[derive(Clone)]
pub(super) struct Snap {
    pub enabled: bool,
    pub move_units: f32,
    pub rotate: f32,
    pub scale: f32,
}
impl Default for Snap {
    fn default() -> Self {
        Self {
            enabled: false,
            move_units: 1.0,
            rotate: 15.0,
            scale: 0.1,
        }
    }
}
impl Snap {
    fn step(&self, tool: Tool, m: egui::Modifiers) -> f32 {
        let index =
            usize::from(m.shift) | usize::from(m.ctrl || m.command) << 1 | usize::from(m.alt) << 2;
        let values = match tool {
            Tool::Rotate => [self.rotate, 45.0, 15.0, 10.0, 5.0, 1.0, 0.5, 0.1],
            Tool::Scale => [self.scale, 0.5, 0.1, 0.05, 0.025, 0.01, 0.005, 0.001],
            _ => [self.move_units, 1.0, 0.5, 0.25, 0.1, 0.05, 0.01, 0.001],
        };
        if index == 0 && !self.enabled {
            0.0
        } else {
            values[index]
        }
    }
}
fn quantize(value: f32, step: f32) -> f32 {
    if step > 0.0 {
        (value / step).round() * step
    } else {
        value
    }
}
fn matrix(t: &Transform) -> Mat4 {
    Mat4::from_scale_rotation_translation(
        Vec3::from(t.scale),
        Quat::from_array(t.rotation),
        Vec3::from(t.position),
    )
}
fn line_distance(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let d = b - a;
    let fraction = ((point - a).dot(d) / d.length_sq().max(1.0)).clamp(0.0, 1.0);
    point.distance(a + d * fraction)
}
impl Editor {
    pub(super) fn view_projection(&self) -> Mat4 {
        if let Some((pose, camera)) = self.scene_camera() {
            return engine::renderer::view_proj_from_entity(
                &pose,
                &camera,
                self.viewport.width() / self.viewport.height().max(1.0),
            );
        }
        Mat4::perspective_rh(
            45_f32.to_radians(),
            self.viewport.width() / self.viewport.height().max(1.0),
            0.01,
            100000.0,
        ) * Mat4::look_at_rh(self.camera_position(), self.target, Vec3::Y)
    }
    pub(super) fn screen_point(&self, point: Vec3) -> Option<egui::Pos2> {
        let clip = self.view_projection() * point.extend(1.0);
        if clip.w < 0.01 {
            return None;
        }
        let p = clip.truncate() / clip.w;
        Some(egui::pos2(
            self.viewport.left() + (p.x + 1.0) * 0.5 * self.viewport.width(),
            self.viewport.top() + (1.0 - p.y) * 0.5 * self.viewport.height(),
        ))
    }
    pub(super) fn selected_id(&self) -> Option<EntityId> {
        let doc = self.document.as_ref()?;
        if self.live.is_some() {
            return entity_at(&doc.scene.entities, &doc.selected)?
                .script_id
                .as_deref()
                .and_then(EntityId::parse)
                .filter(|id| self.world.is_alive(id));
        }
        let mut paths = Vec::new();
        hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
        self.world
            .allocator
            .alive_entity_ids()
            .nth(paths.iter().position(|path| *path == doc.selected)?)
    }
    fn axes(&self) -> Option<(egui::Pos2, [(egui::Pos2, Vec3); 3])> {
        let transform = self.edit_pose()?;
        let origin = Vec3::from(transform.position);
        let center = self.screen_point(origin)?;
        let size = (self.camera_position().distance(origin) * 0.13).max(0.1);
        let rotation = Quat::from_array(transform.rotation);
        let mut axes = [(center, Vec3::ZERO); 3];
        for (index, axis) in [Vec3::X, Vec3::Y, Vec3::Z].into_iter().enumerate() {
            let direction = rotation * axis;
            axes[index] = (
                self.screen_point(origin + direction * size)
                    .unwrap_or(center),
                direction,
            );
        }
        Some((center, axes))
    }
    fn handle_at(&self, point: egui::Pos2) -> Option<usize> {
        if self.tool == Tool::Select {
            return None;
        }
        if self.tool == Tool::Rotate {
            return (0..3)
                .filter_map(|axis| {
                    let distance = self
                        .ring_segments(axis)
                        .iter()
                        .map(|points| line_distance(point, points[0], points[1]))
                        .fold(f32::INFINITY, f32::min);
                    (distance < 8.0).then_some((axis, distance))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(axis, _)| axis);
        }
        let (center, axes) = self.axes()?;
        if point.distance(center) < 8.0 {
            return Some(3);
        }
        axes.iter()
            .enumerate()
            .filter_map(|(index, (end, _))| {
                let distance = line_distance(point, center, *end);
                (distance < 8.0 && end.distance(center) > 12.0).then_some((index, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(index, _)| index)
    }
    fn pick(&mut self, point: egui::Pos2) -> Option<Vec<usize>> {
        let doc = self.document.as_ref()?;
        let mut paths = Vec::new();
        hierarchy::entity_paths(&doc.scene.entities, &mut Vec::new(), &mut paths);
        let ndc = Vec2::new(
            (point.x - self.viewport.left()) / self.viewport.width() * 2.0 - 1.0,
            1.0 - (point.y - self.viewport.top()) / self.viewport.height() * 2.0,
        );
        let inverse = self.view_projection().inverse();
        let near = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        let direction = (far - near).normalize();
        let mut nearest: Option<(f32, Vec<usize>)> = None;
        let entities: Vec<_> = if self.live.is_some() {
            paths
                .into_iter()
                .filter_map(|path| {
                    let id = entity_at(&doc.scene.entities, &path)?
                        .script_id
                        .as_deref()
                        .and_then(EntityId::parse)?;
                    Some((path, id))
                })
                .collect()
        } else {
            paths
                .into_iter()
                .zip(self.world.allocator.alive_entity_ids())
                .collect()
        };
        for (path, id) in entities {
            if !self.world.is_active(&id) {
                continue;
            }
            let Some(transform) = self.world.world_transform(&id) else {
                continue;
            };
            let distance = if let Some(mesh) = self.world.mesh_renderers.get(&id.index) {
                if !self.mesh_cache.contains_key(&mesh.mesh)
                    && let Ok(data) = engine::renderer::load_mesh_from_vfs(&self.vfs, &mesh.mesh)
                {
                    self.mesh_cache.insert(mesh.mesh.clone(), data);
                }
                let Some(mesh) = self.mesh_cache.get(&mesh.mesh) else {
                    continue;
                };
                let model = matrix(&transform);
                if model.determinant().abs() < 1e-8 {
                    continue;
                }
                let local = model.inverse();
                let origin = local.transform_point3(near);
                let ray = local.transform_vector3(direction);
                mesh.indices
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .filter_map(|indices| {
                        triangle(
                            origin,
                            ray,
                            Vec3::from(mesh.vertices[indices[0] as usize].position),
                            Vec3::from(mesh.vertices[indices[1] as usize].position),
                            Vec3::from(mesh.vertices[indices[2] as usize].position),
                        )
                    })
                    .min_by(f32::total_cmp)
            } else {
                self.screen_point(Vec3::from(transform.position))
                    .filter(|screen| screen.distance(point) < 11.0)
                    .map(|_| near.distance(Vec3::from(transform.position)))
            };
            if let Some(distance) = distance
                && nearest.as_ref().is_none_or(|(old, _)| distance < *old)
            {
                nearest = Some((distance, path));
            }
        }
        nearest.map(|(_, path)| path)
    }
    fn begin_drag(&mut self, start: egui::Pos2) -> anyhow::Result<()> {
        let Some(axis) = self.handle_at(start) else {
            return Ok(());
        };
        self.commit_inspector()?;
        let id = self.selected_id().context("select an entity")?;
        let world = self.edit_pose().context("entity transform")?;
        let baseline = self
            .world
            .transforms
            .get(&id.index)
            .cloned()
            .unwrap_or_default();
        let parent = self
            .world
            .get_parent(&id)
            .and_then(|id| self.world.world_transform(&id))
            .unwrap_or_default();
        let (baseline, parent) = if self.component_handle.is_some() {
            (world.clone(), Transform::default())
        } else {
            (baseline, parent)
        };
        let normal = Quat::from_array(world.rotation) * [Vec3::X, Vec3::Y, Vec3::Z][axis.min(2)];
        let origin = Vec3::from(world.position);
        let plane_vector = self.rotation_vector(start, origin, normal);
        let start_vector = plane_vector
            .or_else(|| {
                self.ring_world(axis.min(2))?
                    .into_iter()
                    .filter_map(|p| {
                        self.screen_point(p)
                            .map(|screen| (p, screen.distance(start)))
                    })
                    .min_by(|a, b| a.1.total_cmp(&b.1))
                    .and_then(|(p, _)| (p - origin).try_normalize())
            })
            .unwrap_or(Vec3::Y);
        let radius = (self.camera_position().distance(origin) * 0.13).max(0.1);
        let tangent = self
            .screen_point(
                origin + start_vector * radius + normal.cross(start_vector) * radius * 0.01,
            )
            .zip(self.screen_point(origin + start_vector * radius))
            .map(|(a, b)| (a - b) * 100.0)
            .unwrap_or(egui::vec2(50.0, -50.0));
        self.gizmo_drag = Some(Drag {
            start,
            axis,
            baseline,
            world,
            parent,
            checkpointed: false,
            start_vector,
            plane_rotation: plane_vector.is_some(),
            tangent,
            angle: 0.0,
            last_angle: 0.0,
        });
        Ok(())
    }
    fn update_drag(&mut self, position: egui::Pos2) -> anyhow::Result<()> {
        let Some(drag) = self.gizmo_drag.clone() else {
            return Ok(());
        };
        let step = self.snap.step(self.tool, self.ctx.input(|i| i.modifiers));
        let delta = position - drag.start;
        if delta.length() < 1.0 && !drag.checkpointed {
            return Ok(());
        }
        let mut value = drag.baseline.clone();
        let baseline_world = Vec3::from(drag.world.position);
        let pixels_per_unit = self.viewport.height()
            / (self.camera_position().distance(baseline_world).max(0.1) * 0.83);
        let axis = if drag.axis < 3 {
            Quat::from_array(drag.world.rotation) * [Vec3::X, Vec3::Y, Vec3::Z][drag.axis]
        } else {
            (self.camera_position() - self.target).normalize()
        };
        match self.tool {
            Tool::Move => {
                let movement = if drag.axis < 3 {
                    let center = self
                        .screen_point(baseline_world)
                        .context("gizmo behind camera")?;
                    let end = self
                        .screen_point(baseline_world + axis)
                        .context("axis behind camera")?;
                    let screen = end - center;
                    axis * quantize(delta.dot(screen) / screen.length_sq().max(1.0), step)
                } else {
                    let forward = (self.target - self.camera_position()).normalize();
                    let right = forward.cross(Vec3::Y).normalize();
                    let up = right.cross(forward);
                    right * quantize(delta.x / pixels_per_unit, step)
                        - up * quantize(delta.y / pixels_per_unit, step)
                };
                anyhow::ensure!(
                    Vec3::from(drag.parent.scale).abs().min_element() > 1e-6,
                    "parent scale must be nonzero"
                );
                value.position = matrix(&drag.parent)
                    .inverse()
                    .transform_point3(baseline_world + movement)
                    .to_array();
            }
            Tool::Rotate => {
                let raw = if drag.plane_rotation {
                    self.rotation_vector(position, baseline_world, axis)
                        .map(|vector| {
                            axis.dot(drag.start_vector.cross(vector))
                                .atan2(drag.start_vector.dot(vector))
                        })
                        .unwrap_or(drag.last_angle)
                } else {
                    delta.dot(drag.tangent) / drag.tangent.length_sq().max(1.0)
                };
                let angle = if drag.plane_rotation {
                    drag.angle
                        + (raw - drag.last_angle + std::f32::consts::PI)
                            .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI
                } else {
                    raw
                };
                let rotation =
                    Quat::from_axis_angle(axis, quantize(angle.to_degrees(), step).to_radians())
                        * Quat::from_array(drag.world.rotation);
                value.rotation = (Quat::from_array(drag.parent.rotation).inverse() * rotation)
                    .normalize()
                    .to_array();
                if let Some(drag) = self.gizmo_drag.as_mut() {
                    drag.angle = angle;
                    drag.last_angle = raw;
                }
            }
            Tool::Scale => {
                let factor = (1.0 + quantize(((delta.x - delta.y) * 0.01).exp() - 1.0, step))
                    .clamp(0.001, 100.0);
                if drag.axis < 3 {
                    value.scale[drag.axis] = drag.baseline.scale[drag.axis] * factor;
                } else {
                    value.scale = drag.baseline.scale.map(|value| value * factor);
                }
            }
            Tool::Select => return Ok(()),
        }
        let checkpoint = !drag.checkpointed;
        if self.component_handle.is_some() {
            self.apply_handle_pose(&value, checkpoint)?;
            if let Some(drag) = self.gizmo_drag.as_mut() {
                drag.checkpointed = true;
            }
            return Ok(());
        }
        let doc = self.document.as_mut().context("open a scene")?;
        if checkpoint {
            doc.checkpoint();
        }
        entity_mut(&mut doc.scene.entities, &doc.selected)
            .context("selected entity")?
            .components
            .insert("core:transform".into(), serde_json::to_value(value)?);
        if let Some(drag) = self.gizmo_drag.as_mut() {
            drag.checkpointed = true;
        }
        self.sync_selection();
        self.preview_dirty = true;
        Ok(())
    }
    pub(super) fn viewport_ui(&mut self, ui: &mut egui::Ui) {
        if self.has_dialog() {
            ui.disable();
        }
        ui.horizontal(|ui| {
            ui.add_space(7.0);
            ui.selectable_value(&mut self.view, live::View::Scene, "Scene");
            if self.preview_camera.is_some() {
                if ui.small_button("Exit camera view").clicked() {
                    self.preview_camera = None;
                }
            } else if let Some(id) = self.selected_id()
                && self.world.camera_components.contains_key(&id.index)
                && ui.small_button("Look through camera").clicked()
            {
                self.preview_camera = Some(id);
            }
            if ui
                .add_enabled(
                    self.player.is_some(),
                    egui::Button::new("Play").selected(self.view == live::View::Play),
                )
                .clicked()
            {
                self.view = live::View::Play;
            }
            if self.live.is_some() {
                ui.colored_label(
                    egui::Color32::from_rgb(242, 188, 94),
                    "LIVE · edits are temporary",
                );
            }
        });
        if self.view == live::View::Play && self.player.is_some() {
            ui.horizontal(|ui|{ui.weak(if self.player_detached{"Player input is in the separate game window"}else{"Click Play view to capture the mouse · Escape releases it · F2 opens the console"});});
            ui.separator();
            self.viewport = ui.available_rect_before_wrap();
            ui.allocate_rect(self.viewport, egui::Sense::hover());
            return;
        }
        ui.horizontal(|ui| {
            for (tool, label) in [
                (Tool::Select, "Select (Q)"),
                (Tool::Move, "Move (W)"),
                (Tool::Rotate, "Rotate (E)"),
                (Tool::Scale, "Scale (R)"),
            ] {
                let allowed=tool!=Tool::Scale || self.component_handle.is_none_or(|h|h==super::component_handles::ComponentHandle::Collider);
                if ui.add_enabled(allowed,egui::Button::new(label).selected(self.tool==tool)).clicked(){self.tool=tool;}
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center),|ui|{
                let value=match self.tool{Tool::Rotate=>&mut self.snap.rotate,Tool::Scale=>&mut self.snap.scale,_=>&mut self.snap.move_units};
                ui.add(egui::DragValue::new(value).range(0.001..=10000.0).speed(0.01));
                ui.checkbox(&mut self.snap.enabled,"Snap").on_hover_text("Shift: coarse | Ctrl: medium | Alt: fine\nShift+Ctrl: quarter | Shift+Alt: precise | Ctrl+Alt: finer | all three: finest\nMove: world units, Rotate: degrees, Scale: ratio increments. No modifiers uses this manual value when enabled.");
            });
        });
        if let Some(handle) = self.component_handle {
            ui.horizontal(|ui| {
                ui.colored_label(
                    egui::Color32::from_rgb(86, 229, 221),
                    match handle {
                        super::component_handles::ComponentHandle::Collider => {
                            "Editing collider shape"
                        }
                        super::component_handles::ComponentHandle::JointA => {
                            "Editing joint owner anchor"
                        }
                        super::component_handles::ComponentHandle::JointB => {
                            "Editing joint connected anchor"
                        }
                    },
                );
                if ui.small_button("Done (Esc)").clicked() {
                    self.component_handle = None;
                }
            });
        }
        ui.separator();
        self.viewport = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(self.viewport, egui::Sense::click_and_drag());
        ui.set_clip_rect(ui.clip_rect().intersect(self.viewport));
        let ctx = ui.ctx().clone();
        if self.input_blocked() || self.live_waiting() {
            self.viewport_overlay(ui);
            self.draw_component_handle(ui);
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.component_handle = None;
        }
        if response.hovered() && !ctx.wants_keyboard_input() {
            for (key, tool) in [
                (egui::Key::Q, Tool::Select),
                (egui::Key::W, Tool::Move),
                (egui::Key::E, Tool::Rotate),
                (egui::Key::R, Tool::Scale),
            ] {
                if ctx.input(|input| input.key_pressed(key))
                    && (tool != Tool::Scale
                        || self.component_handle.is_none_or(|h| {
                            h == super::component_handles::ComponentHandle::Collider
                        }))
                {
                    self.tool = tool;
                }
            }
            if ctx.input(|input| input.key_pressed(egui::Key::F)) {
                self.frame_selected();
            }
        }
        if response.dragged_by(egui::PointerButton::Secondary) {
            let delta = ctx.input(|input| input.pointer.delta());
            self.orbit[0] -= delta.x * 0.006;
            self.orbit[1] = (self.orbit[1] + delta.y * 0.006).clamp(-1.5, 1.5);
        }
        if response.dragged_by(egui::PointerButton::Middle) {
            let delta = ctx.input(|input| input.pointer.delta());
            let forward = (self.target - self.camera_position()).normalize();
            let right = forward.cross(Vec3::Y).normalize();
            let up = right.cross(forward);
            self.target += (-right * delta.x + up * delta.y) * self.distance * 0.002;
        }
        if response.hovered() {
            self.distance = (self.distance
                * (-ctx.input(|input| input.smooth_scroll_delta.y) * 0.002).exp())
            .clamp(0.1, 10000.0);
        }
        let (press, release, position, down) = ctx.input(|input| {
            let mut press = None;
            let mut release = None;
            for event in &input.events {
                if let egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    ..
                } = event
                {
                    if *pressed {
                        press = Some(*pos);
                    } else {
                        release = Some(*pos);
                    }
                }
            }
            (
                press,
                release,
                input.pointer.hover_pos(),
                input.pointer.primary_down(),
            )
        });
        if let Some(start) = press.filter(|point| self.viewport.contains(*point))
            && ctx.layer_id_at(start) == Some(ui.layer_id())
        {
            self.viewport_press = Some(start);
            let result = self.begin_drag(start);
            self.report(result);
        }
        if self.gizmo_drag.is_some()
            && let Some(position) = position
        {
            let result = self.update_drag(position);
            self.report(result);
        }
        if let Some(end) = release {
            if self.gizmo_drag.is_none()
                && self
                    .viewport_press
                    .is_some_and(|start| start.distance(end) < 6.0)
                && self.viewport.contains(end)
            {
                let path = self.pick(end).unwrap_or_default();
                let result = self.select(path);
                self.report(result);
            }
            self.gizmo_drag = None;
            self.viewport_press = None;
        } else if !down {
            self.gizmo_drag = None;
            self.viewport_press = None;
        }
        ui.set_clip_rect(ui.clip_rect().intersect(self.viewport));
        self.viewport_overlay(ui);
        self.draw_component_handle(ui);
        let hovered = self.gizmo_drag.as_ref().map(|d| d.axis).or_else(|| {
            ctx.pointer_hover_pos()
                .filter(|p| self.viewport.contains(*p))
                .and_then(|p| self.handle_at(p))
        });
        if self.tool == Tool::Rotate {
            let painter = ui
                .painter()
                .with_clip_rect(self.viewport.intersect(ui.clip_rect()));
            for axis in 0..3 {
                let color = [
                    egui::Color32::from_rgb(238, 96, 100),
                    egui::Color32::from_rgb(108, 222, 137),
                    egui::Color32::from_rgb(103, 158, 255),
                ][axis];
                for points in self.ring_segments(axis) {
                    painter.line_segment(
                        points,
                        egui::Stroke::new(
                            if hovered == Some(axis) {
                                4.0_f32
                            } else {
                                2.5_f32
                            },
                            color,
                        ),
                    );
                }
            }
            return;
        }
        if self.tool != Tool::Select
            && let Some((center, axes)) = self.axes()
        {
            for (index, (end, _)) in axes.iter().enumerate() {
                let highlighted = hovered == Some(index);
                let end = if highlighted {
                    center + (*end - center) * 1.07
                } else {
                    *end
                };
                let color = [
                    egui::Color32::from_rgb(238, 96, 100),
                    egui::Color32::from_rgb(108, 222, 137),
                    egui::Color32::from_rgb(103, 158, 255),
                ][index];
                ui.painter().line_segment(
                    [center, end],
                    egui::Stroke::new(if highlighted { 5.0_f32 } else { 3.0_f32 }, color),
                );
                if self.tool == Tool::Scale {
                    ui.painter().rect_filled(
                        egui::Rect::from_center_size(
                            end,
                            egui::Vec2::splat(if highlighted { 12.0 } else { 9.0 }),
                        ),
                        1.0,
                        color,
                    );
                } else {
                    ui.painter()
                        .circle_filled(end, if highlighted { 8.0 } else { 5.0 }, color);
                }
            }
            ui.painter().rect_filled(
                egui::Rect::from_center_size(center, egui::vec2(10.0, 10.0)),
                2.0,
                egui::Color32::WHITE,
            );
            if self.tool == Tool::Rotate {
                ui.painter().circle_stroke(
                    center,
                    30.0,
                    egui::Stroke::new(1.0_f32, egui::Color32::GRAY),
                );
            }
        }
    }
}

#[derive(Clone)]
pub(super) struct Drag {
    start: egui::Pos2,
    pub(super) axis: usize,
    baseline: Transform,
    world: Transform,
    parent: Transform,
    checkpointed: bool,
    start_vector: Vec3,
    plane_rotation: bool,
    tangent: egui::Vec2,
    angle: f32,
    last_angle: f32,
}
fn triangle(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    let e1 = b - a;
    let e2 = c - a;
    let p = direction.cross(e2);
    let determinant = e1.dot(p);
    if determinant.abs() < 1e-7 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let offset = origin - a;
    let u = offset.dot(p) * inverse;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = offset.cross(e1);
    let v = direction.dot(q) * inverse;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let distance = e2.dot(q) * inverse;
    (distance > 0.0).then_some(distance)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn rotated_editor() -> Editor {
        let mut editor = Editor::new(None, None);
        let parent = Transform {
            position: [0.0, 0.0, 0.0],
            rotation: Quat::from_rotation_y(0.7).to_array(),
            scale: [1.0, 1.0, 1.0],
        };
        let child = Transform {
            position: [0.0, 0.0, 0.0],
            rotation: Quat::from_rotation_z(0.9).to_array(),
            scale: [1.0, 1.0, 1.0],
        };
        let scene=SceneDocument::parse(&serde_json::to_vec(&serde_json::json!({"version":1,"entities":[{"components":{"core:transform":parent},"children":[{"components":{"core:transform":child}}]}]})).expect("JSON")).expect("scene");
        scene.instantiate(&mut editor.world).expect("world");
        editor.document = Some(Document::live(scene));
        editor.document.as_mut().expect("doc").selected = vec![0, 0];
        editor.viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 700.0));
        editor
    }
    #[test]
    fn every_transform_tool_uses_the_objects_rotated_world_axes() {
        let mut editor = rotated_editor();
        let rotation = Quat::from_rotation_y(0.7) * Quat::from_rotation_z(0.9);
        for tool in [Tool::Move, Tool::Rotate, Tool::Scale] {
            editor.tool = tool;
            let (_, axes) = editor.axes().expect("axes");
            for (index, (_, direction)) in axes.iter().enumerate() {
                assert!(direction.distance(rotation * [Vec3::X, Vec3::Y, Vec3::Z][index]) < 1e-5);
            }
        }
    }
    #[test]
    fn rotated_move_drag_changes_position_along_the_displayed_axis() {
        let mut editor = rotated_editor();
        editor.tool = Tool::Move;
        let (center, axes) = editor.axes().expect("axes");
        let axis = 1;
        let start = center + (axes[axis].0 - center) * 0.8;
        editor.begin_drag(start).expect("start");
        assert_eq!(editor.gizmo_drag.as_ref().expect("drag").axis, axis);
        let before = Vec3::from(editor.edit_pose().expect("pose").position);
        editor
            .update_drag(start + (axes[axis].0 - center).normalized() * 40.0)
            .expect("drag");
        let movement = Vec3::from(editor.edit_pose().expect("pose").position) - before;
        assert!(movement.length() > 0.01);
        assert!(movement.normalize().dot(axes[axis].1) > 0.999);
    }
    #[test]
    fn rotation_ring_samples_lie_in_the_plane_of_the_rotated_axis() {
        let mut editor = rotated_editor();
        editor.tool = Tool::Rotate;
        let transform = editor.edit_pose().expect("pose");
        let q = Quat::from_array(transform.rotation);
        let origin = Vec3::from(transform.position);
        for axis in 0..3 {
            let normal = q * [Vec3::X, Vec3::Y, Vec3::Z][axis];
            for point in editor.ring_world(axis).expect("ring") {
                assert!((point - origin).dot(normal).abs() < 1e-4);
            }
        }
    }
    #[test]
    fn welcome_and_returning_home_have_no_scene_viewport() {
        let mut editor = Editor::new(None, None);
        let ctx = editor.ctx.clone();
        for previous_viewport in [
            egui::Rect::NOTHING,
            egui::Rect::from_min_max(egui::pos2(200.0, 100.0), egui::pos2(900.0, 600.0)),
        ] {
            editor.viewport = previous_viewport;
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ctx| editor.ui(ctx),
            );
            assert!(physical_viewport(editor.viewport, 1.0, [1200, 800]).is_none());
        }
    }
    #[test]
    fn scene_rectangles_are_finite_nonempty_and_clipped_to_the_surface() {
        let rect = egui::Rect::from_min_max(egui::pos2(-10.0, 20.0), egui::pos2(500.0, 300.0));
        assert_eq!(
            physical_viewport(rect, 2.0, [800, 500]),
            Some([0, 40, 800, 460])
        );
        for rect in [
            egui::Rect::NOTHING,
            egui::Rect::EVERYTHING,
            egui::Rect::ZERO,
            egui::Rect::from_min_size(egui::pos2(800.0, 500.0), egui::vec2(100.0, 100.0)),
        ] {
            assert!(physical_viewport(rect, 1.0, [800, 500]).is_none());
        }
        for ppp in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            assert!(physical_viewport(rect, ppp, [800, 500]).is_none());
        }
        assert!(physical_viewport(rect, 1.0, [0, 0]).is_none());
    }
    #[test]
    fn snapping_supports_free_manual_and_all_modifier_combinations() {
        let mut snap = Snap::default();
        let mut modifiers = egui::Modifiers::default();
        assert_eq!(snap.step(Tool::Move, modifiers), 0.0);
        snap.enabled = true;
        snap.move_units = 2.0;
        assert_eq!(snap.step(Tool::Move, modifiers), 2.0);
        for (mask, step) in [1.0, 0.5, 0.25, 0.1, 0.05, 0.01, 0.001]
            .into_iter()
            .enumerate()
        {
            let mask = mask + 1;
            modifiers.shift = mask & 1 != 0;
            modifiers.ctrl = mask & 2 != 0;
            modifiers.alt = mask & 4 != 0;
            assert_eq!(snap.step(Tool::Move, modifiers), step);
            assert!(snap.step(Tool::Rotate, modifiers) > 0.0);
            assert!(snap.step(Tool::Scale, modifiers) > 0.0);
        }
        assert!((quantize(-0.34, 0.1) + 0.3).abs() < 1e-6);
        assert_eq!(quantize(0.34, 0.0), 0.34);
    }
    #[test]
    fn picking_uses_triangle_surfaces_and_preserves_world_ray_distance_under_scaling() {
        let t = Transform {
            position: [2.0, 0.0, 0.0],
            scale: [2.0, 3.0, 4.0],
            ..Default::default()
        };
        let inverse = matrix(&t).inverse();
        let origin = inverse.transform_point3(Vec3::new(2.0, 0.0, 10.0));
        let direction = inverse.transform_vector3(-Vec3::Z);
        let hit = triangle(
            origin,
            direction,
            Vec3::new(-1.0, -1.0, 0.0),
            Vec3::new(1.0, -1.0, 0.0),
            Vec3::new(0.0, 1.0, 0.0),
        )
        .expect("surface hit");
        assert!((hit - 10.0).abs() < 0.001);
        assert!(
            triangle(
                Vec3::new(5.0, 5.0, 10.0),
                -Vec3::Z,
                Vec3::ZERO,
                Vec3::X,
                Vec3::Y
            )
            .is_none()
        );
    }
}
