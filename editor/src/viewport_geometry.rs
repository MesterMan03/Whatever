use super::*;
use engine::ecs::{CameraComponent, Transform};
use glam::{Quat, Vec4};

/// Clip in homogeneous space before dividing by w, including near-plane crossings.
pub(super) fn project_line(
    vp: Mat4,
    rect: egui::Rect,
    a: Vec3,
    b: Vec3,
) -> Option<[egui::Pos2; 2]> {
    let a = vp * a.extend(1.0);
    let b = vp * b.extend(1.0);
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    let mut low = 0.0_f32;
    let mut high = 1.0_f32;
    for plane in [
        Vec4::new(1., 0., 0., 1.),
        Vec4::new(-1., 0., 0., 1.),
        Vec4::new(0., 1., 0., 1.),
        Vec4::new(0., -1., 0., 1.),
        Vec4::new(0., 0., 1., 0.),
        Vec4::new(0., 0., -1., 1.),
    ] {
        let left = plane.dot(a);
        let right = plane.dot(b);
        if left < 0.0 && right < 0.0 {
            return None;
        }
        if (left < 0.0) != (right < 0.0) {
            let fraction = left / (left - right);
            if left < 0.0 {
                low = low.max(fraction);
            } else {
                high = high.min(fraction);
            }
        }
    }
    if low > high {
        return None;
    }
    let screen = |p: Vec4| {
        if p.w <= 1e-6 {
            return None;
        }
        let p = p.truncate() / p.w;
        Some(egui::pos2(
            rect.left() + (p.x + 1.0) * 0.5 * rect.width(),
            rect.top() + (1.0 - p.y) * 0.5 * rect.height(),
        ))
    };
    Some([screen(a.lerp(b, low))?, screen(a.lerp(b, high))?])
}

pub(super) fn frustum_corners(
    transform: &Transform,
    camera: &CameraComponent,
    aspect: f32,
) -> [Vec3; 8] {
    let rotation = Quat::from_array(transform.rotation);
    let origin = Vec3::from(transform.position);
    std::array::from_fn(|index| {
        let distance = if index < 4 { camera.znear } else { camera.zfar };
        let height = (camera.fovy_degrees.to_radians() * 0.5).tan() * distance;
        let x = if index % 4 == 0 || index % 4 == 3 {
            -1.0
        } else {
            1.0
        };
        let y = if index % 4 < 2 { -1.0 } else { 1.0 };
        origin + rotation * Vec3::new(x * height * aspect, y * height, -distance)
    })
}

impl Editor {
    pub(super) fn scene_camera(&self) -> Option<(Transform, CameraComponent)> {
        let id = self.preview_camera?;
        if !self.world.is_active(&id) {
            return None;
        }
        Some((
            self.world.world_transform(&id)?,
            self.world.camera_components.get(&id.index)?.clone(),
        ))
    }

    pub(super) fn screen_line(&self, a: Vec3, b: Vec3) -> Option<[egui::Pos2; 2]> {
        project_line(self.view_projection(), self.viewport, a, b)
    }

    pub(super) fn draw_cameras(&self, ui: &egui::Ui) {
        let painter = ui
            .painter()
            .with_clip_rect(self.viewport.intersect(ui.clip_rect()));
        let selected = self.selected_id();
        for id in self.world.allocator.alive_entity_ids() {
            if self.preview_camera == Some(id) || !self.world.is_active(&id) {
                continue;
            }
            let Some(camera) = self.world.camera_components.get(&id.index) else {
                continue;
            };
            let Some(transform) = self.world.world_transform(&id) else {
                continue;
            };
            let mut camera = camera.clone();
            let is_selected = selected == Some(id);
            if !is_selected {
                camera.zfar = camera.zfar.min(1.2);
                camera.znear = camera.znear.min(camera.zfar * 0.5);
            }
            let aspect = self.viewport.width() / self.viewport.height().max(1.0);
            let corners = frustum_corners(&transform, &camera, aspect);
            let color = if is_selected {
                egui::Color32::from_rgb(255, 195, 105)
            } else {
                egui::Color32::from_rgba_unmultiplied(130, 177, 229, 130)
            };
            for (a, b) in [
                (0, 1),
                (1, 2),
                (2, 3),
                (3, 0),
                (4, 5),
                (5, 6),
                (6, 7),
                (7, 4),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ] {
                if let Some(points) = self.screen_line(corners[a], corners[b]) {
                    painter.line_segment(points, egui::Stroke::new(1.5_f32, color));
                }
            }
            let origin = Vec3::from(transform.position);
            let forward = Quat::from_array(transform.rotation) * Vec3::NEG_Z;
            if let Some(points) = self.screen_line(origin, origin + forward * 0.8) {
                painter.arrow(
                    points[0],
                    points[1] - points[0],
                    egui::Stroke::new(2.0_f32, color),
                );
            }
        }
    }

    pub(super) fn ring_world(&self, axis: usize) -> Option<Vec<Vec3>> {
        let transform = self.edit_pose()?;
        let q = Quat::from_array(transform.rotation);
        let origin = Vec3::from(transform.position);
        let radius = (self.camera_position().distance(origin) * 0.13).max(0.1);
        let u = q * [Vec3::Y, Vec3::Z, Vec3::X][axis];
        let v = q * [Vec3::Z, Vec3::X, Vec3::Y][axis];
        Some(
            (0..=96)
                .map(|index| {
                    let angle = index as f32 / 96.0 * std::f32::consts::TAU;
                    origin + (u * angle.cos() + v * angle.sin()) * radius
                })
                .collect(),
        )
    }

    pub(super) fn ring_segments(&self, axis: usize) -> Vec<[egui::Pos2; 2]> {
        self.ring_world(axis)
            .unwrap_or_default()
            .windows(2)
            .filter_map(|pair| self.screen_line(pair[0], pair[1]))
            .collect()
    }

    pub(super) fn rotation_vector(
        &self,
        point: egui::Pos2,
        origin: Vec3,
        normal: Vec3,
    ) -> Option<Vec3> {
        let inverse = self.view_projection().inverse();
        let ndc = glam::Vec2::new(
            (point.x - self.viewport.left()) / self.viewport.width() * 2.0 - 1.0,
            1.0 - (point.y - self.viewport.top()) / self.viewport.height() * 2.0,
        );
        let near = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 0.0));
        let far = inverse.project_point3(Vec3::new(ndc.x, ndc.y, 1.0));
        let direction = (far - near).normalize_or_zero();
        let denominator = direction.dot(normal);
        if denominator.abs() < 0.08 {
            return None;
        }
        let distance = (origin - near).dot(normal) / denominator;
        if !distance.is_finite() || distance < 0.0 {
            return None;
        }
        (near + direction * distance - origin).try_normalize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn camera_frustum_matches_the_renderer_projection_after_rotation() {
        let transform = Transform {
            position: [3.0, 4.0, -2.0],
            rotation: (Quat::from_rotation_y(0.8) * Quat::from_rotation_x(-0.2)).to_array(),
            scale: [2.0, 3.0, 4.0],
        };
        let camera = CameraComponent {
            fovy_degrees: 68.0,
            znear: 0.3,
            zfar: 90.0,
        };
        let aspect = 1.6;
        let vp = engine::renderer::view_proj_from_entity(&transform, &camera, aspect);
        for (index, corner) in frustum_corners(&transform, &camera, aspect)
            .into_iter()
            .enumerate()
        {
            let p = vp.project_point3(corner);
            assert!((p.x.abs() - 1.0).abs() < 1e-3);
            assert!((p.y.abs() - 1.0).abs() < 1e-3);
            assert!((p.z - if index < 4 { 0.0 } else { 1.0 }).abs() < 1e-3);
        }
    }
    #[test]
    fn overlay_lines_clip_near_plane_and_screen_edges_without_leaking() {
        let rect = egui::Rect::from_min_size(egui::pos2(100.0, 80.0), egui::vec2(600.0, 400.0));
        let vp = Mat4::perspective_rh(1.0, 1.5, 0.1, 100.0);
        for (a, b) in [
            (Vec3::new(-100.0, 0.0, -2.0), Vec3::new(100.0, 0.0, -2.0)),
            (Vec3::new(0.0, 0.0, 1.0), Vec3::new(0.2, 0.2, -3.0)),
        ] {
            let points = project_line(vp, rect, a, b).expect("visible segment");
            for point in points {
                assert!(rect.expand(0.01).contains(point));
                assert!(point.is_finite());
            }
        }
        assert!(
            project_line(vp, rect, Vec3::new(0.0, 0.0, 1.0), Vec3::new(1.0, 0.0, 2.0)).is_none()
        );
    }
}
