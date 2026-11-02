mod components;
mod entity;
mod lifecycle;
mod references;
mod registry;
mod tags;
mod world;
pub use references::{referenced_entity, resolve_references};
pub use registry::{ComponentDefinition, ComponentField, FieldEditor, is_native_component};
pub use tags::{MAX_ENTITY_TAGS, TagQuery, validate_tag, validate_tags};

#[allow(unused_imports)]
pub use components::{
    AmbientLight, COMPONENT_AMBIENT_LIGHT, COMPONENT_DIRECTIONAL_LIGHT, COMPONENT_MESH_RENDERER,
    COMPONENT_POINT_LIGHT, COMPONENT_SPRITE_RENDERER, COMPONENT_TEXT_RENDERER, COMPONENT_TRANSFORM,
    CameraComponent, DirectionalLight, MeshRenderer, PointLight, SpriteRenderer, TextRenderer,
    Transform,
};
pub use components::{COMPONENT_RIGID_BODY_STATE, RigidBodyState};
pub use entity::EntityId;
pub use world::World;

#[cfg(test)]
pub(crate) use registry::register_test_component;

mod snapshot;
pub use snapshot::{EntityDelta, EntityMetadata, EntitySnapshot, SceneDelta, ScenePublisher};
