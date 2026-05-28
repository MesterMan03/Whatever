use crate::ecs::{CameraComponent, Transform};
use glam::{Mat4, Quat, Vec3};

/// Compute a combined view-projection matrix from an entity's world-space
/// `Transform` and its `CameraComponent`.
///
/// The camera looks in the **-Z** direction of its local frame (right-hand
/// convention, same as wgpu / glam defaults).  `up` is the local +Y axis
/// rotated into world space.
pub fn view_proj_from_entity(
    transform: &Transform,
    cam: &CameraComponent,
    aspect: f32,
) -> Mat4 {
    let position = Vec3::from(transform.position);
    let [qx, qy, qz, qw] = transform.rotation;
    let rotation = Quat::from_xyzw(qx, qy, qz, qw);
    let forward = rotation * Vec3::NEG_Z;
    let up = rotation * Vec3::Y;

    let view = Mat4::look_at_rh(position, position + forward, up);
    let fovy = cam.fovy_degrees.to_radians();
    let proj = Mat4::perspective_rh(fovy, aspect, cam.znear, cam.zfar);
    proj * view
}
