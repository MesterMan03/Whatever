import { Entity, _componentHydrators, _registerNativeComponent, hydrateReferences } from "./ecs.ts";
import { Engine } from "./engine.ts";
import { EntityRef, type EntityReference } from "../references.ts";
import { _pending, type Pending } from "../pending.ts";
import { _send, nextReqId } from "../shared.ts";
import type * as Options from "../physics-options.ts";
import { _requestNow } from "../transport.ts";

const callbacks = new Map<
  string,
  { resolve: (value: any) => void; reject: (error: Error) => void }
>();
/** @internal */
export function _handlePhysicsResponse(message: {
  request_id: string;
  result: unknown;
  error: string | null;
}): void {
  const callback = callbacks.get(message.request_id);
  if (!callback) return;
  callbacks.delete(message.request_id);
  message.error
    ? callback.reject(new Error(message.error))
    : callback.resolve(hydrateReferences(message.result));
}
function request<T>(command: object): Pending<T> {
  const request_id = nextReqId();
  return _pending(
    new Promise<T>((resolve, reject) => {
      callbacks.set(request_id, { resolve, reject });
      _send({ type: "PhysicsRequest", request_id, request: command });
    }),
  );
}
function query<T>(request: object): T {
  return hydrateReferences(
    _requestNow({ type: "PhysicsRequest", request_id: nextReqId(), request }).result,
  );
}
function ref(target: Options.PhysicsEntityInput): EntityRef {
  return new EntityRef(target);
}

class DefaultWorldReference extends Entity {
  constructor() {
    super("default:physics_world");
  }
  /** Obtain the real generational entity ID behind the default world's symbolic alias. */
  get(): Promise<Entity> {
    return request<Entity>({ op: "worldEntity", world: this }).get();
  }
}

export namespace Physics {
  export type Vec3 = Options.PhysicsVec3;
  export type Quat = Options.PhysicsQuat;
  export type EntityInput = Options.PhysicsEntityInput;
  export type WorldOptions = Options.PhysicsWorldOptions;
  export type BodyOptions = Options.PhysicsBodyOptions;
  export type ColliderOptions = Options.PhysicsColliderOptions;
  export type JointOptions = Options.PhysicsJointOptions;
  export type IntegrationOptions = Options.PhysicsIntegrationOptions;
  export type BodyRapierOptions = Options.PhysicsBodyRapierOptions;
  export type ColliderRapierOptions = Options.PhysicsColliderRapierOptions;
  export type JointRapierOptions = Options.PhysicsJointRapierOptions;
  export type ShapeData = Options.PhysicsShapeData;
  export type BodyState = Options.PhysicsBodyState;
  export type Status = Options.PhysicsStatusData;
  export type Event = Options.PhysicsEventData;
  export type RayHit = Options.PhysicsRayHit;
  export type RayOptions = Options.PhysicsRayOptions;
  export type Groups = Options.PhysicsGroups;
  export type MassProperties = Options.PhysicsMassProperties;
  export type Motor = Options.PhysicsMotor;
  export type Limits = Options.PhysicsLimits;
  export type JointAxis = Options.PhysicsJointAxis;
  export type LockedAxis = Options.PhysicsLockedAxis;
  export type CombineRule = Options.PhysicsCombineRule;
  export type MotorModel = Options.PhysicsMotorModel;

  export const WORLD_ID = "core:physics_world";
  export const BODY_ID = "core:rigid_body";
  export const COLLIDER_ID = "core:collider";
  export const JOINT_ID = "core:joint";
  export const BODY_STATE_ID = "core:rigid_body_state";
  export const STATUS_ID = "core:physics_status";
  /** Resolves to a lazily allocated, ordinary engine-owned world entity. */
  export const defaultWorld = new DefaultWorldReference();
  export const capabilities = Object.freeze({
    backend: "rapier",
    version: "0.36.1",
    dimensions: ["3d"] as const,
    enhancedDeterminism: true,
    parallel: false,
    simd: false,
    softBodies: false,
    javascriptPhysicsHooks: false,
  });
  export const QueryFilterFlag = Object.freeze({
    EXCLUDE_FIXED: 1,
    EXCLUDE_KINEMATIC: 2,
    EXCLUDE_DYNAMIC: 4,
    EXCLUDE_SENSORS: 8,
    EXCLUDE_SOLIDS: 16,
    ONLY_DYNAMIC: 3,
    ONLY_KINEMATIC: 5,
    ONLY_FIXED: 6,
  });
  export const CollisionType = Object.freeze({
    DYNAMIC_DYNAMIC: 0x0001,
    DYNAMIC_KINEMATIC: 0x000c,
    DYNAMIC_FIXED: 0x0002,
    KINEMATIC_KINEMATIC: 0xcc00,
    KINEMATIC_FIXED: 0x2200,
    FIXED_FIXED: 0x0020,
    ALL: 0xee2f,
    DEFAULT: 0x000f,
  });
  export const TriMeshFlag = Object.freeze({
    HALF_EDGE_TOPOLOGY: 1,
    CONNECTED_COMPONENTS: 2,
    DELETE_BAD_TOPOLOGY_TRIANGLES: 4,
    ORIENTED: 8,
    MERGE_DUPLICATE_VERTICES: 16,
    DELETE_DEGENERATE_TRIANGLES: 32,
    DELETE_DUPLICATE_TRIANGLES: 64,
    FIX_INTERNAL_EDGES: 144,
    DEFORMABLE: 256,
    FIX_INTERNAL_EDGES_TWO_SIDED: 656,
  });

  /** Component data only: adding it through ECS creates/configures a native world. */
  export class World {
    readonly id = WORLD_ID;
    dimension: "3d" = "3d";
    gravity: Vec3 = [0, -9.81, 0];
    substeps = 1;
    timeScale = 1;
    rapier: NonNullable<WorldOptions["rapier"]> = {};
    constructor(options: WorldOptions = {}) {
      Object.assign(this, options);
      delete (this as unknown as Record<string, unknown>).bodyB;
    }
  }
  export class RigidBody {
    readonly id = BODY_ID;
    type: Options.PhysicsBodyType = "dynamic";
    world?: EntityReference;
    gravityScale = 1;
    linearDamping = 0;
    angularDamping = 0;
    ccd = false;
    canSleep = true;
    enabled = true;
    lockedAxes: LockedAxis[] = [];
    rapier: BodyRapierOptions = {};
    constructor(options: BodyOptions & { type?: Options.PhysicsBodyType } = {}) {
      Object.assign(this, options);
      if (options.world !== undefined) this.world = ref(options.world);
    }
    static dynamic(options: BodyOptions = {}): RigidBody {
      return new RigidBody({ ...options, type: "dynamic" });
    }
    static fixed(options: BodyOptions = {}): RigidBody {
      return new RigidBody({ ...options, type: "fixed" });
    }
    static kinematicPosition(options: BodyOptions = {}): RigidBody {
      return new RigidBody({ ...options, type: "kinematicPosition" });
    }
    static kinematicVelocity(options: BodyOptions = {}): RigidBody {
      return new RigidBody({ ...options, type: "kinematicVelocity" });
    }
  }
  export class Collider {
    readonly id = COLLIDER_ID;
    body?: EntityReference | null;
    world?: EntityReference;
    shape: ShapeData;
    position: Vec3 = [0, 0, 0];
    rotation: Quat = [0, 0, 0, 1];
    mass?: number | null;
    density?: number | null;
    friction = 0.5;
    restitution = 0;
    sensor = false;
    enabled = true;
    events: ("collision" | "contactForce")[] = [];
    rapier: ColliderRapierOptions = {};
    constructor(options: ColliderOptions) {
      this.shape = options.shape;
      Object.assign(this, options);
      if (options.body !== undefined) this.body = options.body === null ? null : ref(options.body);
      if (options.world !== undefined && options.world !== null) this.world = ref(options.world);
      else this.world = undefined;
    }
  }
  export class Joint {
    readonly id = JOINT_ID;
    type: Options.PhysicsJointType = "fixed";
    bodyA?: EntityReference;
    connectedBody: EntityReference;
    /** @deprecated Use connectedBody. */
    get bodyB(): EntityReference {
      return this.connectedBody;
    }
    set bodyB(value: EntityReference) {
      this.connectedBody = ref(value);
    }
    solver: "impulse" | "multibody" = "impulse";
    anchorA: Vec3 = [0, 0, 0];
    anchorB: Vec3 = [0, 0, 0];
    rotationA: Quat = [0, 0, 0, 1];
    rotationB: Quat = [0, 0, 0, 1];
    axisA?: Vec3 | null;
    axisB?: Vec3 | null;
    contactsEnabled = false;
    enabled = true;
    limits?: Limits | null;
    motor?: Motor | null;
    rapier: JointRapierOptions = {};
    constructor(options: JointOptions & { type?: Options.PhysicsJointType }) {
      this.bodyA = options.bodyA ? ref(options.bodyA) : undefined;
      this.connectedBody = ref(options.connectedBody ?? options.bodyB!);
      Object.assign(this, options);
      this.bodyA = options.bodyA ? ref(options.bodyA) : undefined;
      this.connectedBody = ref(options.connectedBody ?? options.bodyB!);
    }
    static fixed(options: JointOptions): Joint {
      return new Joint({ ...options, type: "fixed" });
    }
    static revolute(options: JointOptions): Joint {
      return new Joint({ ...options, type: "revolute" });
    }
    static prismatic(options: JointOptions): Joint {
      return new Joint({ ...options, type: "prismatic" });
    }
    static spherical(options: JointOptions): Joint {
      return new Joint({ ...options, type: "spherical" });
    }
    static generic(options: JointOptions): Joint {
      return new Joint({ ...options, type: "generic" });
    }
  }
  export const Shape = {
    sphere(radius: number): ShapeData {
      return { type: "sphere", radius };
    },
    box(options: { size: Vec3; borderRadius?: number }): ShapeData {
      return { type: "box", ...options };
    },
    capsule(options: { radius: number; height: number }): ShapeData {
      return { type: "capsule", ...options };
    },
    cylinder(options: { radius: number; height: number }): ShapeData {
      return { type: "cylinder", ...options };
    },
    cone(options: { radius: number; height: number }): ShapeData {
      return { type: "cone", ...options };
    },
    plane(normal: Vec3 = [0, 1, 0]): ShapeData {
      return { type: "plane", normal };
    },
    convexHull(points: Vec3[]): ShapeData {
      return { type: "convexHull", points };
    },
    triMesh(vertices: Vec3[], indices: [number, number, number][], flags = 0): ShapeData {
      return { type: "triMesh", vertices, indices, flags };
    },
    compound(parts: Extract<ShapeData, { type: "compound" }>["parts"]): ShapeData {
      return { type: "compound", parts };
    },
  };
  /** Native operations on an existing body component. Does not add or enable a component. */
  export class BodyHandle implements Pending<void> {
    constructor(private readonly entity: EntityRef) {}
    private command(action: string, options: object = {}): this {
      query<void>({ op: "body", entity: this.entity, action, ...options });
      return this;
    }
    /** Wait for the commands queued on this handle so far, including their validation errors. */
    get(): Promise<void> {
      return Promise.resolve();
    }
    state(): BodyState {
      return query({ op: "state", entity: this.entity });
    }
    applyImpulse(vector: Vec3, options: { point?: Vec3; wake?: boolean } = {}): this {
      return this.command("applyImpulse", { vector, ...options });
    }
    addForce(vector: Vec3, options: { point?: Vec3; wake?: boolean } = {}): this {
      return this.command("addForce", { vector, ...options });
    }
    applyTorqueImpulse(vector: Vec3, wake = true): this {
      return this.command("applyTorqueImpulse", { vector, wake });
    }
    addTorque(vector: Vec3, wake = true): this {
      return this.command("addTorque", { vector, wake });
    }
    resetForces(wake = true): this {
      return this.command("resetForces", { wake });
    }
    resetTorques(wake = true): this {
      return this.command("resetTorques", { wake });
    }
    /** Select axes to preserve velocity on the others (e.g. steer horizontally without cancelling gravity). */
    setLinearVelocity(
      vector: Vec3,
      options: { axes?: ("x" | "y" | "z")[]; wake?: boolean } = {},
    ): this {
      return this.command("setLinearVelocity", { vector, ...options });
    }
    setAngularVelocity(vector: Vec3, wake = true): this {
      return this.command("setAngularVelocity", { vector, wake });
    }
    teleport(position: Vec3, options: { rotation?: Quat; wake?: boolean } = {}): this {
      return this.command("teleport", { vector: position, ...options });
    }
    setKinematicTarget(position: Vec3, rotation?: Quat): this {
      return this.command("setKinematicTarget", { vector: position, rotation });
    }
    sleep(): this {
      return this.command("sleep");
    }
    wake(): this {
      return this.command("wake");
    }
  }
  export class WorldHandle {
    constructor(private readonly entity: EntityRef) {}
    /** Sees all earlier commands in this tick, without advancing physics time. Direction is normalized. */
    castRay(origin: Vec3, direction: Vec3, options: RayOptions): RayHit | null {
      const { excludeBody, excludeCollider, ...rest } = options;
      return query({
        op: "castRay",
        world: this.entity,
        origin,
        direction,
        ...rest,
        excludeBody: excludeBody === undefined ? undefined : ref(excludeBody),
        excludeCollider: excludeCollider === undefined ? undefined : ref(excludeCollider),
      });
    }
  }
  export function body(entity: EntityInput): BodyHandle {
    return new BodyHandle(ref(entity));
  }
  export function world(entity: EntityInput = defaultWorld): WorldHandle {
    return new WorldHandle(ref(entity));
  }
  export function onEvent(handler: (event: Event) => void | Promise<void>): void {
    Engine.on("physics", ({ event }) => handler(event));
  }
}

_componentHydrators[Physics.WORLD_ID] = (data) => new Physics.World(data);
_componentHydrators[Physics.BODY_ID] = (data) => new Physics.RigidBody(data);
_componentHydrators[Physics.COLLIDER_ID] = (data) => new Physics.Collider(data);
_componentHydrators[Physics.JOINT_ID] = (data) => new Physics.Joint(data);

declare module "./ecs.ts" {
  interface ComponentRegistry {
    "core:physics_world": Physics.World;
    "core:rigid_body": Physics.RigidBody;
    "core:collider": Physics.Collider;
    "core:joint": Physics.Joint;
    "core:rigid_body_state": Physics.BodyState;
    "core:physics_status": Physics.Status;
  }
  interface ComponentSetRegistry {
    "core:physics_world": Physics.WorldOptions;
    "core:rigid_body": Physics.BodyOptions & { type?: Options.PhysicsBodyType };
    "core:collider": Physics.ColliderOptions;
    "core:joint": Physics.JointOptions & { type?: Options.PhysicsJointType };
  }
}

_registerNativeComponent(Physics.World, Physics.WORLD_ID);
_registerNativeComponent(Physics.RigidBody, Physics.BODY_ID);
_registerNativeComponent(Physics.Collider, Physics.COLLIDER_ID);
_registerNativeComponent(Physics.Joint, Physics.JOINT_ID);
