import type { Entity } from "./components/ecs.ts";
import type { EntityReference } from "./references.ts";

export type PhysicsVec3 = [number, number, number];
/** Quaternion in xyzw order. */
export type PhysicsQuat = [number, number, number, number];
export type PhysicsEntityInput = EntityReference | string;
export type PhysicsBodyType = "dynamic" | "fixed" | "kinematicPosition" | "kinematicVelocity";
export type PhysicsLockedAxis =
  | "translationX"
  | "translationY"
  | "translationZ"
  | "rotationX"
  | "rotationY"
  | "rotationZ";
export type PhysicsJointAxis =
  | "linearX"
  | "linearY"
  | "linearZ"
  | "angularX"
  | "angularY"
  | "angularZ";
export type PhysicsCombineRule = "Average" | "Min" | "Multiply" | "Max";
export type PhysicsMotorModel = "AccelerationBased" | "ForceBased";
export interface PhysicsSpringCoefficients {
  natural_frequency: number;
  damping_ratio: number;
}
export interface PhysicsMassProperties {
  mass: number;
  center_of_mass: PhysicsVec3;
  principal_angular_inertia: PhysicsVec3;
  principal_inertia_local_frame?: PhysicsQuat;
}
export interface PhysicsGroups {
  memberships?: number;
  filter?: number;
  test_mode?: "And" | "Or";
}
/** Native Rapier 0.36.1 integration knobs. dt belongs to the engine tick clock. */
export interface PhysicsIntegrationOptions {
  min_ccd_dt?: number;
  contact_softness?: Partial<PhysicsSpringCoefficients>;
  static_contact_softness?: Partial<PhysicsSpringCoefficients>;
  warmstart_coefficient?: number;
  length_unit?: number;
  normalized_allowed_linear_error?: number;
  normalized_max_corrective_velocity?: number;
  normalized_prediction_distance?: number;
  normalized_max_linear_velocity?: number;
  num_solver_iterations?: number;
  num_internal_pgs_iterations?: number;
  num_internal_stabilization_iterations?: number;
  max_ccd_substeps?: number;
  contact_clustering?: boolean;
  contact_recycling?: boolean;
  normalized_contact_recycle_distance?: number;
  friction_in_bias_pass?: boolean;
  warmstart_joints?: boolean;
  friction_model?: "Simplified" | "Coulomb";
  /** Native tuning retained for backend completeness; soft-body ECS components are not exposed yet. */
  soft_bodies?: {
    resweep_strain?: number;
    max_extra_substeps?: number;
    contact_stiffening?: number;
    recovery?: {
      authored_velocity_margin?: boolean;
      edge_speculation?: boolean;
      inverted_cell_detection?: boolean;
      self_crossing_detection?: boolean;
      detection_motion_gating?: boolean;
      cross_body_detection?: boolean;
      self_stand_down?: boolean;
      cross_body_expel_gate?: boolean;
      edge_stand_down?: boolean;
      crossing_repulsion?: boolean;
      crossing_repulsion_guide?: boolean;
      crossing_repulsion_self_guide?: boolean;
      recovery_pace?: number;
      overlap_constraints?: boolean;
      overlap_rigid?: boolean;
      overlap_skip_self_tangled?: boolean;
      overlap_edge_stand_down?: boolean;
      overlap_constraint_pace?: number;
      overlap_patch_constraints?: "Keep" | "StandDown" | "AlongNormal";
      overlap_skin_volume?: boolean;
      overlap_kept_depth?: number;
      overlap_self_regions?: boolean;
      overlap_normal_push?: boolean;
      overlap_multi_volume?: boolean;
      overlap_split?: number;
      overlap_patience?: number;
      overlap_progress_margin?: number;
    };
  };
}
export interface PhysicsWorldOptions {
  dimension?: "3d";
  gravity?: PhysicsVec3;
  substeps?: number;
  timeScale?: number;
  rapier?: { integration?: PhysicsIntegrationOptions };
}
export interface PhysicsBodyRapierOptions {
  additional_solver_iterations?: number;
  additional_pgs_iterations?: number;
  dominance_group?: number;
  soft_ccd_prediction?: number;
  allow_fast_rotation?: boolean;
  gyroscopic_forces_enabled?: boolean;
  additional_mass?: number;
  additional_mass_properties?: PhysicsMassProperties | null;
  activation?: {
    normalized_linear_threshold?: number;
    angular_threshold?: number;
    time_until_sleep?: number;
  };
  /** A decimal u128 string, preserving all 128 bits across IPC. */
  user_data?: string;
}
export interface PhysicsBodyOptions {
  world?: PhysicsEntityInput;
  gravityScale?: number;
  linearDamping?: number;
  angularDamping?: number;
  ccd?: boolean;
  canSleep?: boolean;
  enabled?: boolean;
  lockedAxes?: PhysicsLockedAxis[];
  rapier?: PhysicsBodyRapierOptions;
}
/** All dimensions are full sizes. A capsule's height includes its two hemispherical caps. */
export type PhysicsShapeData =
  | { type: "sphere"; radius: number }
  | { type: "box"; size: PhysicsVec3; borderRadius?: number }
  | { type: "capsule"; radius: number; height: number }
  | { type: "cylinder"; radius: number; height: number }
  | { type: "cone"; radius: number; height: number }
  | { type: "plane"; normal: PhysicsVec3 }
  | { type: "convexHull"; points: PhysicsVec3[] }
  | {
      type: "triMesh";
      vertices: PhysicsVec3[];
      indices: [number, number, number][];
      flags?: number;
    }
  | {
      type: "compound";
      parts: { position?: PhysicsVec3; rotation?: PhysicsQuat; shape: PhysicsShapeData }[];
    };
export interface PhysicsColliderRapierOptions {
  friction_combine_rule?: PhysicsCombineRule;
  restitution_combine_rule?: PhysicsCombineRule;
  collision_groups?: PhysicsGroups;
  solver_groups?: PhysicsGroups;
  /** Native ActiveCollisionTypes bitmask. */
  active_collision_types?: number;
  contact_skin?: number;
  contact_force_event_threshold?: number;
  mass_properties?: PhysicsMassProperties | null;
  user_data?: string;
}
export interface PhysicsColliderOptions {
  /** Omitted: same entity. null: standalone fixed collider. Otherwise: explicit body entity. */
  body?: PhysicsEntityInput | null;
  /** Standalone colliders select a world; attached colliders inherit their body's world. */
  world?: PhysicsEntityInput;
  shape: PhysicsShapeData;
  /** Local offset from the body, or from the entity's world pose for standalone colliders. */
  position?: PhysicsVec3;
  rotation?: PhysicsQuat;
  mass?: number | null;
  density?: number | null;
  friction?: number;
  restitution?: number;
  sensor?: boolean;
  enabled?: boolean;
  events?: ("collision" | "contactForce")[];
  rapier?: PhysicsColliderRapierOptions;
}
export interface PhysicsLimits {
  min: number;
  max: number;
}
export interface PhysicsMotor {
  targetPosition?: number;
  targetVelocity?: number;
  stiffness?: number;
  damping?: number;
  maxForce?: number;
  model?: PhysicsMotorModel;
}
export interface PhysicsJointRapierOptions {
  /** Override the factory's locked degrees of freedom (particularly useful for generic joints). */
  locked_axes?: PhysicsJointAxis[] | null;
  coupled_axes?: PhysicsJointAxis[];
  limits?: Partial<Record<PhysicsJointAxis, PhysicsLimits>>;
  motors?: Partial<Record<PhysicsJointAxis, PhysicsMotor>>;
  softness?: PhysicsSpringCoefficients;
  user_data?: string;
}
export type PhysicsJointType = "fixed" | "revolute" | "prismatic" | "spherical" | "generic";
export interface PhysicsJointOptions {
  connectedBody?: PhysicsEntityInput;
  /** @deprecated Put the joint on body A and use connectedBody. */
  bodyA?: PhysicsEntityInput;
  /** @deprecated Use connectedBody. */
  bodyB?: PhysicsEntityInput;
  solver?: "impulse" | "multibody";
  /** Anchors, orientations and axes are in each body's local frame. */
  anchorA?: PhysicsVec3;
  anchorB?: PhysicsVec3;
  rotationA?: PhysicsQuat;
  rotationB?: PhysicsQuat;
  axisA?: PhysicsVec3 | null;
  axisB?: PhysicsVec3 | null;
  contactsEnabled?: boolean;
  enabled?: boolean;
  /** Friendly single-axis limits/motor for revolute and prismatic joints. */
  limits?: PhysicsLimits | null;
  motor?: PhysicsMotor | null;
  rapier?: PhysicsJointRapierOptions;
}
export interface PhysicsBodyState {
  readonly world: Entity;
  readonly position: PhysicsVec3;
  readonly rotation: PhysicsQuat;
  readonly linearVelocity: PhysicsVec3;
  readonly angularVelocity: PhysicsVec3;
  readonly sleeping: boolean;
  readonly mass: number;
  readonly centerOfMass: PhysicsVec3;
  /** This world's last completed solver tick; earlier commands may already be reflected in state. */
  readonly tickNumber: number;
}
export type PhysicsStatusData = Readonly<
  Partial<
    Record<
      "world" | "body" | "collider" | "joint",
      { readonly active: boolean; readonly error: string | null }
    >
  >
>;
export interface PhysicsRayOptions {
  maxDistance: number;
  solid?: boolean;
  groups?: PhysicsGroups;
  /** Native QueryFilterFlags bitmask. */
  flags?: number;
  excludeBody?: PhysicsEntityInput;
  excludeCollider?: PhysicsEntityInput;
}
export interface PhysicsRayHit {
  collider: Entity;
  body: Entity | null;
  distance: number;
  point: PhysicsVec3;
  normal: PhysicsVec3;
}
export type PhysicsEventData = {
  world: Entity;
  colliderA: Entity;
  colliderB: Entity;
  tickNumber: number;
  substep: number;
  phase: "step";
} & (
  | { kind: "started" | "stopped"; sensor: boolean; removed: boolean }
  | {
      kind: "contactForce";
      totalForce: PhysicsVec3;
      totalForceMagnitude: number;
      maxForceDirection: PhysicsVec3;
      maxForceMagnitude: number;
    }
);
