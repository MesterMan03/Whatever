use anyhow::{Context, bail, ensure};
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const WORLD: &str = "core:physics_world";
pub const BODY: &str = "core:rigid_body";
pub const COLLIDER: &str = "core:collider";
pub const JOINT: &str = "core:joint";
pub use crate::ecs::COMPONENT_RIGID_BODY_STATE as STATE;
pub const STATUS: &str = "core:physics_status";

pub fn vec3(value: [f32; 3]) -> anyhow::Result<Vector> {
    let value = Vector::from_array(value);
    ensure!(value.is_finite(), "vector must be finite");
    Ok(value)
}

pub fn rotation(value: [f32; 4]) -> anyhow::Result<Rotation> {
    let value = Rotation::from_array(value);
    ensure!(
        value.is_finite() && value.length_squared().is_finite() && value.length_squared() > 1.0e-12,
        "rotation must be a nonzero finite quaternion"
    );
    Ok(value.normalize())
}

pub fn pose(position: [f32; 3], quaternion: [f32; 4]) -> anyhow::Result<Pose> {
    Ok(Pose::from_parts(vec3(position)?, rotation(quaternion)?))
}

fn identity() -> [f32; 4] {
    [0.0, 0.0, 0.0, 1.0]
}
fn positive(value: f32, name: &str) -> anyhow::Result<()> {
    ensure!(
        value.is_finite() && value > 0.0,
        "{name} must be positive and finite"
    );
    Ok(())
}
fn nonnegative(value: f32, name: &str) -> anyhow::Result<()> {
    ensure!(
        value.is_finite() && value >= 0.0,
        "{name} must be nonnegative and finite"
    );
    Ok(())
}

/// Overlay the complete serializable native configuration, rejecting misspelled/unknown knobs.
/// This intentionally does not expose transient solver buffers or opaque native handles.
pub fn merge_known(base: &mut Value, patch: &Value) -> anyhow::Result<()> {
    if let (Some(base), Some(patch)) = (base.as_object_mut(), patch.as_object()) {
        for (key, value) in patch {
            let target = base
                .get_mut(key)
                .with_context(|| format!("unknown Rapier option '{key}'"))?;
            merge_known(target, value)?;
        }
    } else {
        *base = patch.clone();
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct WorldConfig {
    pub dimension: String,
    pub gravity: [f32; 3],
    pub substeps: u32,
    pub time_scale: f32,
    pub rapier: WorldRapier,
}
impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            dimension: "3d".into(),
            gravity: [0.0, -9.81, 0.0],
            substeps: 1,
            time_scale: 1.0,
            rapier: WorldRapier::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorldRapier {
    pub integration: Value,
}
impl Default for WorldRapier {
    fn default() -> Self {
        Self {
            integration: json!({}),
        }
    }
}
impl WorldConfig {
    pub fn normalize(&mut self) -> anyhow::Result<()> {
        ensure!(
            self.dimension == "3d",
            "this engine build supports 3d physics only"
        );
        vec3(self.gravity)?;
        ensure!(
            self.substeps > 0 && self.substeps <= 1024,
            "substeps must be in 1..1024"
        );
        nonnegative(self.time_scale, "timeScale")?;
        ensure!(
            self.rapier.integration.is_object(),
            "rapier.integration must be an object"
        );
        ensure!(
            self.rapier.integration.get("dt").is_none(),
            "dt is derived from the simulation tick, timeScale and substeps"
        );
        let mut defaults = serde_json::to_value(IntegrationParameters::default())?;
        defaults
            .as_object_mut()
            .context("integration defaults")?
            .remove("dt");
        merge_known(&mut defaults, &self.rapier.integration)?;
        self.rapier.integration = defaults;
        let native = self.integration()?;
        positive(native.length_unit, "length_unit")?;
        ensure!(
            native.num_solver_iterations > 0,
            "num_solver_iterations must be positive"
        );
        ensure!(
            native.max_ccd_substeps > 0,
            "max_ccd_substeps must be positive"
        );
        nonnegative(native.min_ccd_dt, "min_ccd_dt")?;
        for name in [
            "normalized_allowed_linear_error",
            "normalized_prediction_distance",
            "normalized_contact_recycle_distance",
        ] {
            nonnegative(
                self.rapier.integration[name]
                    .as_f64()
                    .context("integration scalar")? as f32,
                name,
            )?;
        }
        Ok(())
    }
    pub fn integration(&self) -> anyhow::Result<IntegrationParameters> {
        let mut value = self.rapier.integration.clone();
        value
            .as_object_mut()
            .context("integration must be an object")?
            .insert("dt".into(), json!(1.0 / 60.0));
        Ok(serde_json::from_value(value)?)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub world: Value,
    pub gravity_scale: f32,
    pub linear_damping: f32,
    pub angular_damping: f32,
    pub ccd: bool,
    pub can_sleep: bool,
    pub enabled: bool,
    pub locked_axes: Vec<String>,
    pub rapier: BodyRapier,
}
impl Default for BodyConfig {
    fn default() -> Self {
        Self {
            kind: "dynamic".into(),
            world: Value::Null,
            gravity_scale: 1.0,
            linear_damping: 0.0,
            angular_damping: 0.0,
            ccd: false,
            can_sleep: true,
            enabled: true,
            locked_axes: Vec::new(),
            rapier: BodyRapier::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct BodyRapier {
    pub additional_solver_iterations: usize,
    pub additional_pgs_iterations: usize,
    pub dominance_group: i8,
    pub soft_ccd_prediction: f32,
    pub allow_fast_rotation: bool,
    pub gyroscopic_forces_enabled: bool,
    pub additional_mass: f32,
    pub additional_mass_properties: Option<MassConfig>,
    pub activation: ActivationConfig,
    pub user_data: String,
}
impl Default for BodyRapier {
    fn default() -> Self {
        Self {
            additional_solver_iterations: 0,
            additional_pgs_iterations: 0,
            dominance_group: 0,
            soft_ccd_prediction: 0.0,
            allow_fast_rotation: false,
            gyroscopic_forces_enabled: true,
            additional_mass: 0.0,
            additional_mass_properties: None,
            activation: ActivationConfig::default(),
            user_data: "0".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ActivationConfig {
    pub normalized_linear_threshold: f32,
    pub angular_threshold: f32,
    pub time_until_sleep: f32,
}
impl Default for ActivationConfig {
    fn default() -> Self {
        let native = RigidBodyActivation::default();
        Self {
            normalized_linear_threshold: native.normalized_linear_threshold,
            angular_threshold: native.angular_threshold,
            time_until_sleep: native.time_until_sleep,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MassConfig {
    pub mass: f32,
    pub center_of_mass: [f32; 3],
    pub principal_angular_inertia: [f32; 3],
    #[serde(default = "identity")]
    pub principal_inertia_local_frame: [f32; 4],
}
impl MassConfig {
    pub fn native(&self) -> anyhow::Result<MassProperties> {
        nonnegative(self.mass, "mass")?;
        for value in self.principal_angular_inertia {
            nonnegative(value, "principal inertia")?;
        }
        Ok(MassProperties::with_principal_inertia_frame(
            vec3(self.center_of_mass)?,
            self.mass,
            vec3(self.principal_angular_inertia)?,
            rotation(self.principal_inertia_local_frame)?,
        ))
    }
}
impl BodyConfig {
    pub fn kind(&self) -> anyhow::Result<RigidBodyType> {
        Ok(match self.kind.as_str() {
            "dynamic" => RigidBodyType::Dynamic,
            "fixed" => RigidBodyType::Fixed,
            "kinematicPosition" => RigidBodyType::KinematicPositionBased,
            "kinematicVelocity" => RigidBodyType::KinematicVelocityBased,
            _ => bail!("unknown rigid body type '{}'", self.kind),
        })
    }
    pub fn locks(&self) -> anyhow::Result<LockedAxes> {
        let mut result = LockedAxes::empty();
        for name in &self.locked_axes {
            result |= match name.as_str() {
                "translationX" => LockedAxes::TRANSLATION_LOCKED_X,
                "translationY" => LockedAxes::TRANSLATION_LOCKED_Y,
                "translationZ" => LockedAxes::TRANSLATION_LOCKED_Z,
                "rotationX" => LockedAxes::ROTATION_LOCKED_X,
                "rotationY" => LockedAxes::ROTATION_LOCKED_Y,
                "rotationZ" => LockedAxes::ROTATION_LOCKED_Z,
                _ => bail!("unknown locked axis '{name}'"),
            };
        }
        Ok(result)
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        self.kind()?;
        self.locks()?;
        ensure!(
            self.gravity_scale.is_finite(),
            "gravityScale must be finite"
        );
        nonnegative(self.linear_damping, "linearDamping")?;
        nonnegative(self.angular_damping, "angularDamping")?;
        nonnegative(self.rapier.soft_ccd_prediction, "soft_ccd_prediction")?;
        nonnegative(self.rapier.additional_mass, "additional_mass")?;
        nonnegative(self.rapier.activation.time_until_sleep, "time_until_sleep")?;
        ensure!(
            self.rapier.additional_mass == 0.0 || self.rapier.additional_mass_properties.is_none(),
            "additional_mass and additional_mass_properties are mutually exclusive"
        );
        if let Some(props) = &self.rapier.additional_mass_properties {
            props.native()?;
        }
        self.rapier
            .user_data
            .parse::<u128>()
            .context("user_data must be a decimal u128 string")?;
        Ok(())
    }
    pub fn apply(&self, body: &mut RigidBody) -> anyhow::Result<()> {
        let was_sleeping = body.is_sleeping();
        body.set_body_type(self.kind()?, true);
        body.set_enabled(self.enabled);
        body.set_gravity_scale(self.gravity_scale, true);
        body.set_linear_damping(self.linear_damping);
        body.set_angular_damping(self.angular_damping);
        body.enable_ccd(self.ccd);
        body.set_locked_axes(self.locks()?, true);
        body.set_additional_solver_iterations(self.rapier.additional_solver_iterations);
        body.set_additional_pgs_iterations(self.rapier.additional_pgs_iterations);
        body.set_dominance_group(self.rapier.dominance_group);
        body.set_soft_ccd_prediction(self.rapier.soft_ccd_prediction);
        body.set_allow_fast_rotation(self.rapier.allow_fast_rotation);
        body.enable_gyroscopic_forces(self.rapier.gyroscopic_forces_enabled);
        if let Some(props) = &self.rapier.additional_mass_properties {
            body.set_additional_mass_properties(props.native()?, true);
        } else {
            body.set_additional_mass(self.rapier.additional_mass, true);
        }
        let activation = body.activation_mut();
        activation.normalized_linear_threshold = if self.can_sleep {
            self.rapier.activation.normalized_linear_threshold
        } else {
            -1.0
        };
        activation.angular_threshold = if self.can_sleep {
            self.rapier.activation.angular_threshold
        } else {
            -1.0
        };
        activation.time_until_sleep = self.rapier.activation.time_until_sleep;
        body.user_data = self.rapier.user_data.parse()?;
        if was_sleeping && self.can_sleep {
            body.sleep();
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum Shape {
    Sphere {
        radius: f32,
    },
    Box {
        size: [f32; 3],
        #[serde(default, rename = "borderRadius")]
        border_radius: f32,
    },
    Capsule {
        radius: f32,
        height: f32,
    },
    Cylinder {
        radius: f32,
        height: f32,
    },
    Cone {
        radius: f32,
        height: f32,
    },
    Plane {
        normal: [f32; 3],
    },
    ConvexHull {
        points: Vec<[f32; 3]>,
    },
    TriMesh {
        vertices: Vec<[f32; 3]>,
        indices: Vec<[u32; 3]>,
        #[serde(default)]
        flags: u16,
    },
    Compound {
        parts: Vec<ShapePart>,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShapePart {
    #[serde(default)]
    pub position: [f32; 3],
    #[serde(default = "identity")]
    pub rotation: [f32; 4],
    pub shape: Shape,
}
impl Shape {
    pub fn native(&self) -> anyhow::Result<SharedShape> {
        Ok(match self {
            Self::Sphere { radius } => {
                positive(*radius, "radius")?;
                SharedShape::ball(*radius)
            }
            Self::Box {
                size,
                border_radius,
            } => {
                for value in size {
                    positive(*value, "box size")?;
                }
                nonnegative(*border_radius, "borderRadius")?;
                ensure!(
                    size.iter().all(|x| x * 0.5 > *border_radius),
                    "borderRadius must be smaller than every half extent"
                );
                if *border_radius == 0.0 {
                    SharedShape::cuboid(size[0] * 0.5, size[1] * 0.5, size[2] * 0.5)
                } else {
                    SharedShape::round_cuboid(
                        size[0] * 0.5 - border_radius,
                        size[1] * 0.5 - border_radius,
                        size[2] * 0.5 - border_radius,
                        *border_radius,
                    )
                }
            }
            Self::Capsule { radius, height } => {
                positive(*radius, "radius")?;
                positive(*height, "height")?;
                ensure!(
                    *height >= 2.0 * radius,
                    "capsule height includes the caps and must be at least twice its radius"
                );
                SharedShape::capsule_y(height * 0.5 - radius, *radius)
            }
            Self::Cylinder { radius, height } => {
                positive(*radius, "radius")?;
                positive(*height, "height")?;
                SharedShape::cylinder(height * 0.5, *radius)
            }
            Self::Cone { radius, height } => {
                positive(*radius, "radius")?;
                positive(*height, "height")?;
                SharedShape::cone(height * 0.5, *radius)
            }
            Self::Plane { normal } => {
                let n = vec3(*normal)?;
                positive(n.length(), "plane normal length")?;
                SharedShape::halfspace(n.normalize())
            }
            Self::ConvexHull { points } => {
                let vertices = points
                    .iter()
                    .map(|p| vec3(*p))
                    .collect::<anyhow::Result<Vec<_>>>()?;
                SharedShape::convex_hull(&vertices)
                    .context("convex hull requires non-coplanar points")?
            }
            Self::TriMesh {
                vertices,
                indices,
                flags,
            } => {
                ensure!(
                    !vertices.is_empty() && !indices.is_empty(),
                    "triangle mesh cannot be empty"
                );
                ensure!(
                    indices
                        .iter()
                        .flatten()
                        .all(|idx| (*idx as usize) < vertices.len()),
                    "triangle index out of range"
                );
                let flags =
                    TriMeshFlags::from_bits(*flags).context("invalid triangle mesh flags")?;
                SharedShape::trimesh_with_flags(
                    vertices
                        .iter()
                        .map(|p| vec3(*p))
                        .collect::<anyhow::Result<Vec<_>>>()?,
                    indices.clone(),
                    flags,
                )?
            }
            Self::Compound { parts } => {
                ensure!(!parts.is_empty(), "compound cannot be empty");
                ensure!(
                    parts.iter().all(|p| !matches!(
                        p.shape,
                        Self::Compound { .. } | Self::Plane { .. } | Self::TriMesh { .. }
                    )),
                    "compound parts must be convex shapes"
                );
                SharedShape::compound(
                    parts
                        .iter()
                        .map(|p| Ok((pose(p.position, p.rotation)?, p.shape.native()?)))
                        .collect::<anyhow::Result<Vec<_>>>()?,
                )
            }
        })
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct ColliderConfig {
    pub body: Value,
    pub world: Option<Value>,
    pub shape: Shape,
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub mass: Option<f32>,
    pub density: Option<f32>,
    pub friction: f32,
    pub restitution: f32,
    pub sensor: bool,
    pub enabled: bool,
    pub events: Vec<String>,
    pub rapier: ColliderRapier,
}
impl Default for ColliderConfig {
    fn default() -> Self {
        Self {
            body: Value::Null,
            world: None,
            shape: Shape::Sphere { radius: 0.5 },
            position: [0.0; 3],
            rotation: identity(),
            mass: None,
            density: None,
            friction: 0.5,
            restitution: 0.0,
            sensor: false,
            enabled: true,
            events: Vec::new(),
            rapier: ColliderRapier::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColliderRapier {
    pub friction_combine_rule: CoefficientCombineRule,
    pub restitution_combine_rule: CoefficientCombineRule,
    pub collision_groups: Groups,
    pub solver_groups: Groups,
    pub active_collision_types: u16,
    pub contact_skin: f32,
    pub contact_force_event_threshold: f32,
    pub mass_properties: Option<MassConfig>,
    pub user_data: String,
}
impl Default for ColliderRapier {
    fn default() -> Self {
        Self {
            friction_combine_rule: CoefficientCombineRule::Average,
            restitution_combine_rule: CoefficientCombineRule::Average,
            collision_groups: Groups::default(),
            solver_groups: Groups::default(),
            active_collision_types: ActiveCollisionTypes::default().bits(),
            contact_skin: 0.0,
            contact_force_event_threshold: 0.0,
            mass_properties: None,
            user_data: "0".into(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Groups {
    pub memberships: u32,
    pub filter: u32,
    pub test_mode: InteractionTestMode,
}
impl Default for Groups {
    fn default() -> Self {
        Self {
            memberships: u32::MAX,
            filter: u32::MAX,
            test_mode: InteractionTestMode::And,
        }
    }
}
impl Groups {
    pub fn native(&self) -> InteractionGroups {
        InteractionGroups::new(
            Group::from_bits_retain(self.memberships),
            Group::from_bits_retain(self.filter),
            self.test_mode,
        )
    }
}
impl ColliderConfig {
    pub fn builder(&self) -> anyhow::Result<ColliderBuilder> {
        ensure!(
            self.mass.is_none() || self.density.is_none(),
            "mass and density are mutually exclusive"
        );
        ensure!(
            self.rapier.mass_properties.is_none()
                || (self.mass.is_none() && self.density.is_none()),
            "mass_properties cannot be combined with mass or density"
        );
        nonnegative(self.friction, "friction")?;
        nonnegative(self.restitution, "restitution")?;
        nonnegative(self.rapier.contact_skin, "contact_skin")?;
        nonnegative(
            self.rapier.contact_force_event_threshold,
            "contact_force_event_threshold",
        )?;
        let mut builder = ColliderBuilder::new(self.shape.native()?)
            .position(pose(self.position, self.rotation)?)
            .friction(self.friction)
            .restitution(self.restitution)
            .sensor(self.sensor)
            .enabled(self.enabled)
            .friction_combine_rule(self.rapier.friction_combine_rule)
            .restitution_combine_rule(self.rapier.restitution_combine_rule)
            .collision_groups(self.rapier.collision_groups.native())
            .solver_groups(self.rapier.solver_groups.native())
            .contact_skin(self.rapier.contact_skin)
            .contact_force_event_threshold(self.rapier.contact_force_event_threshold)
            .user_data(
                self.rapier
                    .user_data
                    .parse::<u128>()
                    .context("user_data must be a decimal u128 string")?,
            );
        builder.active_collision_types =
            ActiveCollisionTypes::from_bits(self.rapier.active_collision_types)
                .context("invalid active_collision_types bits")?;
        let mut events = ActiveEvents::empty();
        for event in &self.events {
            events |= match event.as_str() {
                "collision" => ActiveEvents::COLLISION_EVENTS,
                "contactForce" => ActiveEvents::CONTACT_FORCE_EVENTS,
                _ => bail!("unknown physics event '{event}'"),
            };
        }
        builder.active_events = events;
        if let Some(mass) = self.mass {
            nonnegative(mass, "mass")?;
            builder = builder.mass(mass);
        }
        if let Some(density) = self.density {
            nonnegative(density, "density")?;
            builder = builder.density(density);
        }
        if let Some(properties) = &self.rapier.mass_properties {
            builder = builder.mass_properties(properties.native()?);
        }
        Ok(builder)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct JointConfig {
    #[serde(rename = "type")]
    pub kind: String,
    pub body_a: Value,
    #[serde(rename = "connectedBody", alias = "bodyB")]
    pub body_b: Value,
    pub solver: String,
    pub anchor_a: [f32; 3],
    pub anchor_b: [f32; 3],
    pub rotation_a: [f32; 4],
    pub rotation_b: [f32; 4],
    pub axis_a: Option<[f32; 3]>,
    pub axis_b: Option<[f32; 3]>,
    pub contacts_enabled: bool,
    pub enabled: bool,
    pub limits: Option<Limits>,
    pub motor: Option<Motor>,
    pub rapier: JointRapier,
}
impl Default for JointConfig {
    fn default() -> Self {
        Self {
            kind: "fixed".into(),
            body_a: Value::Null,
            body_b: Value::Null,
            solver: "impulse".into(),
            anchor_a: [0.0; 3],
            anchor_b: [0.0; 3],
            rotation_a: identity(),
            rotation_b: identity(),
            axis_a: None,
            axis_b: None,
            contacts_enabled: false,
            enabled: true,
            limits: None,
            motor: None,
            rapier: JointRapier::default(),
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub min: f32,
    pub max: f32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase", deny_unknown_fields)]
pub struct Motor {
    pub target_position: f32,
    pub target_velocity: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub max_force: f32,
    pub model: MotorModel,
}
impl Default for Motor {
    fn default() -> Self {
        Self {
            target_position: 0.0,
            target_velocity: 0.0,
            stiffness: 0.0,
            damping: 0.0,
            max_force: f32::MAX,
            model: MotorModel::AccelerationBased,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JointRapier {
    pub locked_axes: Option<Vec<String>>,
    pub coupled_axes: Vec<String>,
    pub limits: std::collections::BTreeMap<String, Limits>,
    pub motors: std::collections::BTreeMap<String, Motor>,
    pub softness: SpringCoefficients<f32>,
    pub user_data: String,
}
impl Default for JointRapier {
    fn default() -> Self {
        Self {
            locked_axes: None,
            coupled_axes: Vec::new(),
            limits: Default::default(),
            motors: Default::default(),
            softness: GenericJoint::default().softness,
            user_data: "0".into(),
        }
    }
}
pub fn joint_axis(name: &str) -> anyhow::Result<JointAxis> {
    Ok(match name {
        "linearX" => JointAxis::LinX,
        "linearY" => JointAxis::LinY,
        "linearZ" => JointAxis::LinZ,
        "angularX" => JointAxis::AngX,
        "angularY" => JointAxis::AngY,
        "angularZ" => JointAxis::AngZ,
        _ => bail!("unknown joint axis '{name}'"),
    })
}
fn axes(names: &[String]) -> anyhow::Result<JointAxesMask> {
    let mut mask = JointAxesMask::empty();
    for name in names {
        mask |= JointAxesMask::from(joint_axis(name)?);
    }
    Ok(mask)
}
fn set_limits(joint: &mut GenericJoint, axis: JointAxis, limits: &Limits) -> anyhow::Result<()> {
    ensure!(
        limits.min.is_finite() && limits.max.is_finite() && limits.min <= limits.max,
        "joint limits require finite min <= max"
    );
    joint.set_limits(axis, [limits.min, limits.max]);
    Ok(())
}
fn set_motor(joint: &mut GenericJoint, axis: JointAxis, motor: &Motor) -> anyhow::Result<()> {
    ensure!(
        motor.target_position.is_finite() && motor.target_velocity.is_finite(),
        "motor targets must be finite"
    );
    nonnegative(motor.stiffness, "motor stiffness")?;
    nonnegative(motor.damping, "motor damping")?;
    nonnegative(motor.max_force, "motor maxForce")?;
    joint.set_motor(
        axis,
        motor.target_position,
        motor.target_velocity,
        motor.stiffness,
        motor.damping,
    );
    joint.set_motor_model(axis, motor.model);
    joint.set_motor_max_force(axis, motor.max_force);
    Ok(())
}
impl JointConfig {
    pub fn native(&self) -> anyhow::Result<GenericJoint> {
        ensure!(
            matches!(self.solver.as_str(), "impulse" | "multibody"),
            "solver must be impulse or multibody"
        );
        let (locked, main) = match self.kind.as_str() {
            "fixed" => (JointAxesMask::LOCKED_FIXED_AXES, None),
            "revolute" => (JointAxesMask::LOCKED_REVOLUTE_AXES, Some(JointAxis::AngX)),
            "prismatic" => (JointAxesMask::LOCKED_PRISMATIC_AXES, Some(JointAxis::LinX)),
            "spherical" => (JointAxesMask::LOCKED_SPHERICAL_AXES, None),
            "generic" => (JointAxesMask::empty(), None),
            _ => bail!("unknown joint type '{}'", self.kind),
        };
        let mut joint = GenericJoint::new(locked);
        joint.set_local_frame1(pose(self.anchor_a, self.rotation_a)?);
        joint.set_local_frame2(pose(self.anchor_b, self.rotation_b)?);
        if main.is_some() {
            if let Some(axis) = self.axis_a {
                ensure!(
                    self.rotation_a == identity(),
                    "axisA and rotationA are alternative joint-frame definitions"
                );
                let axis = vec3(axis)?;
                positive(axis.length(), "axisA length")?;
                joint.set_local_axis1(axis.normalize());
            }
            if let Some(axis) = self.axis_b {
                ensure!(
                    self.rotation_b == identity(),
                    "axisB and rotationB are alternative joint-frame definitions"
                );
                let axis = vec3(axis)?;
                positive(axis.length(), "axisB length")?;
                joint.set_local_axis2(axis.normalize());
            }
        } else {
            ensure!(
                self.axis_a.is_none() && self.axis_b.is_none(),
                "axes are for revolute/prismatic joints; use rotationA/B to orient generic frames"
            );
        }
        joint.set_contacts_enabled(self.contacts_enabled);
        joint.set_enabled(self.enabled);
        if let Some(names) = &self.rapier.locked_axes {
            joint.locked_axes = axes(names)?;
        }
        joint.coupled_axes = axes(&self.rapier.coupled_axes)?;
        if let Some(limits) = &self.limits {
            let axis = main.context("use rapier.limits per axis for this joint type")?;
            ensure!(
                !self
                    .rapier
                    .limits
                    .keys()
                    .any(|name| joint_axis(name).ok() == Some(axis)),
                "limits and rapier.limits cannot configure the same axis"
            );
            set_limits(&mut joint, axis, limits)?;
        }
        if let Some(motor) = &self.motor {
            let axis = main.context("use rapier.motors per axis for this joint type")?;
            ensure!(
                !self
                    .rapier
                    .motors
                    .keys()
                    .any(|name| joint_axis(name).ok() == Some(axis)),
                "motor and rapier.motors cannot configure the same axis"
            );
            set_motor(&mut joint, axis, motor)?;
        }
        for (axis, limits) in &self.rapier.limits {
            set_limits(&mut joint, joint_axis(axis)?, limits)?;
        }
        for (axis, motor) in &self.rapier.motors {
            set_motor(&mut joint, joint_axis(axis)?, motor)?;
        }
        nonnegative(
            self.rapier.softness.natural_frequency,
            "joint softness natural_frequency",
        )?;
        nonnegative(
            self.rapier.softness.damping_ratio,
            "joint softness damping_ratio",
        )?;
        joint.softness = self.rapier.softness;
        joint.user_data = self
            .rapier
            .user_data
            .parse::<u128>()
            .context("user_data must be a decimal u128 string")?;
        Ok(joint)
    }
}
