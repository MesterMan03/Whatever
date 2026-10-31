use crate::ecs::{EntityId, Transform, World};
use std::{collections::HashMap, sync::Arc, time::Instant};

#[derive(Clone)]
struct Pose {
    id: EntityId,
    parent: Option<EntityId>,
    active: bool,
    transform: Transform,
}
struct Sample {
    at: Instant,
    poses: HashMap<u32, Pose>,
    camera: Option<EntityId>,
}
/// Two adjacent simulation publications, even when the window skips rendering several ticks.
#[derive(Clone, Default)]
pub(crate) struct MotionHistory {
    previous: Option<Arc<Sample>>,
    current: Option<Arc<Sample>>,
}
impl MotionHistory {
    pub fn capture(&mut self, world: &World, camera: Option<EntityId>, at: Instant) {
        self.previous = self.current.take();
        if self
            .previous
            .as_ref()
            .is_some_and(|sample| sample.camera != camera)
        {
            self.previous = None;
        }
        self.current = Some(Arc::new(Sample {
            at,
            camera,
            poses: world
                .transforms
                .iter()
                .filter_map(|(&index, transform)| {
                    let id = world.allocator.live_id(index)?;
                    Some((
                        index,
                        Pose {
                            id,
                            parent: world.get_parent(&id),
                            active: world.is_active(&id),
                            transform: transform.clone(),
                        },
                    ))
                })
                .collect(),
        }));
    }
    pub fn sample(&self, world: &World, now: Instant) -> HashMap<u32, Transform> {
        let mut result = world.transforms.clone();
        let (Some(previous), Some(current)) = (&self.previous, &self.current) else {
            return result;
        };
        let duration = current
            .at
            .saturating_duration_since(previous.at)
            .as_secs_f32();
        if duration <= 0.0 {
            return result;
        }
        let alpha =
            (now.saturating_duration_since(current.at).as_secs_f32() / duration).clamp(0.0, 1.0);
        for (&index, to) in &current.poses {
            let Some(from) = previous.poses.get(&index) else {
                continue;
            };
            if from.id != to.id || from.parent != to.parent || from.active != to.active {
                continue;
            }
            let rotation = |value: [f32; 4]| {
                let q = glam::Quat::from_array(value);
                if q.is_finite() && q.length_squared() > 1e-10 {
                    q.normalize()
                } else {
                    glam::Quat::IDENTITY
                }
            };
            result.insert(
                index,
                Transform {
                    position: glam::Vec3::from(from.transform.position)
                        .lerp(to.transform.position.into(), alpha)
                        .to_array(),
                    rotation: rotation(from.transform.rotation)
                        .slerp(rotation(to.transform.rotation), alpha)
                        .to_array(),
                    scale: glam::Vec3::from(from.transform.scale)
                        .lerp(to.transform.scale.into(), alpha)
                        .to_array(),
                },
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    #[test]
    fn frames_interpolate_local_hierarchies_and_stop_at_latest_pose() {
        let mut world = World::new();
        let rig = world.create_entity();
        let camera = world.create_entity();
        world.transforms.insert(rig.index, Transform::default());
        world.transforms.insert(
            camera.index,
            Transform {
                position: [0.0, 0.0, 2.0],
                ..Default::default()
            },
        );
        assert!(world.set_parent(camera, Some(rig)));
        let mut history = MotionHistory::default();
        let at = Instant::now();
        history.capture(&world, Some(camera), at);
        world.transforms.get_mut(&rig.index).expect("rig").position = [10.0, 0.0, 0.0];
        world.transforms.get_mut(&rig.index).expect("rig").rotation =
            glam::Quat::from_rotation_y(std::f32::consts::PI / 2.0).to_array();
        history.capture(&world, Some(camera), at + Duration::from_millis(20));
        let sampled = history.sample(&world, at + Duration::from_millis(30));
        assert!((sampled[&rig.index].position[0] - 5.0).abs() < 1e-5);
        let mut rendered = world.clone();
        rendered.transforms = sampled;
        let pose = rendered.world_transform(&camera).expect("pose");
        assert!((pose.position[0] - (5.0 + 2.0_f32.sqrt())).abs() < 1e-4);
        assert_eq!(world.transforms[&rig.index].position[0], 10.0);
        assert_eq!(
            history.sample(&world, at + Duration::from_secs(2))[&rig.index].position[0],
            10.0
        );
    }
    #[test]
    fn a_slow_renderer_still_receives_adjacent_tick_poses() {
        let mut world = World::new();
        let id = world.create_entity();
        let mut history = MotionHistory::default();
        let at = Instant::now();
        for n in 0..=4 {
            world.transforms.insert(
                id.index,
                Transform {
                    position: [n as f32, 0.0, 0.0],
                    ..Default::default()
                },
            );
            history.capture(&world, None, at + Duration::from_millis(n * 10));
        }
        let rendered = history.sample(&world, at + Duration::from_millis(45));
        assert!((rendered[&id.index].position[0] - 3.5).abs() < 1e-5);
    }
    #[test]
    fn new_generations_reparenting_and_camera_cuts_are_immediate() {
        let mut w = World::new();
        let old = w.create_entity();
        w.transforms.insert(old.index, Transform::default());
        let mut history = MotionHistory::default();
        let at = Instant::now();
        history.capture(&w, None, at);
        w.destroy_entity(old);
        let new = w.create_entity();
        w.transforms.insert(
            new.index,
            Transform {
                position: [50.0, 0.0, 0.0],
                ..Default::default()
            },
        );
        history.capture(&w, None, at + Duration::from_millis(20));
        assert_eq!(
            history.sample(&w, at + Duration::from_millis(21))[&new.index].position[0],
            50.0
        );
        let parent = w.create_entity();
        assert!(w.set_parent(new, Some(parent)));
        w.transforms.get_mut(&new.index).expect("pose").position[0] = 80.0;
        history.capture(&w, None, at + Duration::from_millis(40));
        assert_eq!(
            history.sample(&w, at + Duration::from_millis(41))[&new.index].position[0],
            80.0
        );
        w.transforms.get_mut(&new.index).expect("pose").position[0] = 100.0;
        history.capture(&w, Some(new), at + Duration::from_millis(60));
        assert_eq!(
            history.sample(&w, at + Duration::from_millis(61))[&new.index].position[0],
            100.0
        );
    }
}
