use super::{
    GpuDirectionalLight, GpuLightingData, GpuPointLight, MeshUpdateTarget, RenderContext, Renderer,
    SpriteUpdateTarget,
};
use crate::ecs::World;
use crate::script::RenderCommand;
use crate::vfs::Vfs;

pub fn apply_render_command(
    cmd: &RenderCommand,
    renderer: &mut Renderer,
    world: &World,
    vfs: &dyn Vfs,
) {
    match cmd {
        RenderCommand::UpsertSprite { entity_idx } => {
            let entity_id = world.allocator.live_id(*entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(sprite) = world.sprite_renderers.get(entity_idx) else {
                return;
            };
            let target = SpriteUpdateTarget {
                entity_idx: *entity_idx,
                transform: &world_transform,
                sprite,
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            if let Err(e) = renderer.scene.update_sprite(vfs, target, ctx) {
                tracing::warn!("update_sprite error (entity {}): {e}", entity_idx);
            }
            // Pre-warm pipeline so the first render frame has no stall.
            renderer.get_or_build_pipeline(vfs, &sprite.shader, false);
        }
        RenderCommand::RemoveSprite { entity_idx } => {
            renderer.scene.remove_sprite(*entity_idx);
        }
        RenderCommand::UpsertText { entity_idx } => {
            let entity_id = world.allocator.live_id(*entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(text_comp) = world.text_renderers.get(entity_idx) else {
                return;
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            if let Err(e) =
                renderer
                    .text
                    .upsert_text(ctx, vfs, *entity_idx, &world_transform, text_comp)
            {
                tracing::warn!("upsert_text error (entity {}): {e}", entity_idx);
            }
        }
        RenderCommand::RemoveText { entity_idx } => {
            renderer.text.remove_text(*entity_idx);
        }
        RenderCommand::UpsertMesh { entity_idx } => {
            let entity_id = world.allocator.live_id(*entity_idx);
            let Some(entity_id) = entity_id else { return };
            let Some(world_transform) = world.world_transform(&entity_id) else {
                return;
            };
            let Some(mesh_comp) = world.mesh_renderers.get(entity_idx) else {
                return;
            };
            let target = MeshUpdateTarget {
                entity_idx: *entity_idx,
                transform: &world_transform,
                mesh_renderer: mesh_comp,
            };
            let ctx = RenderContext {
                device: &renderer.ctx.device,
                queue: &renderer.ctx.queue,
                bgl: &renderer.texture_bind_group_layout,
            };
            // Clone shader path before the mutable borrow of renderer.scene.
            let shader_path = mesh_comp.shader.clone();
            if let Err(e) = renderer.scene.update_mesh(vfs, target, ctx) {
                tracing::warn!("update_mesh error (entity {}): {e}", entity_idx);
            }
            // Pre-warm pipeline cache so the first render frame has no stall.
            renderer.get_or_build_pipeline(vfs, &shader_path, true);
        }
        RenderCommand::RemoveMesh { entity_idx } => {
            renderer.scene.remove_mesh(*entity_idx);
        }
    }
}

pub fn collect_lights(world: &World) -> GpuLightingData {
    use bytemuck::Zeroable;

    let mut data = GpuLightingData::zeroed();

    // Additive ambient: sum all ambient light components.
    for al in world.ambient_lights.values() {
        let c = glam::Vec3::from(al.color) * al.intensity;
        data.ambient_color = (glam::Vec3::from(data.ambient_color) + c).to_array();
        data.ambient_intensity = 1.0;
    }

    // Directional lights (up to 4).
    let mut dir_count = 0u32;
    for dl in world.directional_lights.values().take(4) {
        data.dir_lights[dir_count as usize] = GpuDirectionalLight {
            direction: dl.direction,
            _pad0: 0.0,
            color: dl.color,
            intensity: dl.intensity,
        };
        dir_count += 1;
    }
    data.dir_light_count = dir_count;

    // Point lights (up to 8) — position comes from core:transform.
    let mut pt_count = 0u32;
    for (idx, pl) in world.point_lights.iter().take(8) {
        let position = if let Some(t) = world.transforms.get(idx) {
            t.position
        } else {
            [0.0, 0.0, 0.0]
        };
        data.point_lights[pt_count as usize] = GpuPointLight {
            position,
            range: pl.range,
            color: pl.color,
            intensity: pl.intensity,
        };
        pt_count += 1;
    }
    data.point_light_count = pt_count;

    data
}
