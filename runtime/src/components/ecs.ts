import type { Component, JsonValue, QueryResult } from "../types.ts";
import {
  _send, nextReqId,
  _entityCallbacks, _entityListCallbacks,
  _componentGetCallbacks, _componentQueryCallbacks,
  _entityParentCallbacks, _entityChildrenCallbacks,
} from "../shared.ts";

export namespace BuiltInComponents {
  export const TRANSFORM_ID = "core:transform";
  export const SPRITE_RENDERER_ID = "core:sprite_renderer";
  export const TEXT_RENDERER_ID = "core:text_renderer";
  export const CAMERA_ID = "core:camera";

  /** Transform component. Returned by `getComponent("core:transform")` as a live class instance. */
  export class Transform implements Component {
    readonly id = TRANSFORM_ID;
    position: [number, number, number];
    rotation: [number, number, number, number];
    scale: [number, number, number];

    constructor(init: { position?: [number, number, number]; rotation?: [number, number, number, number]; scale?: [number, number, number] } = {}) {
      this.position = init.position ?? [0, 0, 0];
      this.rotation = init.rotation ?? [0, 0, 0, 1];
      this.scale = init.scale ?? [1, 1, 1];
    }

    // --- Position ---

    getX(): number { return this.position[0]; }
    setX(x: number): this { this.position[0] = x; return this; }
    addX(x: number): this { this.position[0] += x; return this; }
    getY(): number { return this.position[1]; }
    setY(y: number): this { this.position[1] = y; return this; }
    addY(y: number): this { this.position[1] += y; return this; }
    getZ(): number { return this.position[2]; }
    setZ(z: number): this { this.position[2] = z; return this; }
    addZ(z: number): this { this.position[2] += z; return this; }

    /** Returns a copy of the position as [x, y, z]. */
    getPosition(): [number, number, number] {
      return [this.position[0], this.position[1], this.position[2]];
    }

    /** Set all three position components at once. Chainable. */
    setPosition(x: number, y: number, z: number): this {
      this.position = [x, y, z];
      return this;
    }

    // --- Scale ---

    getScaleX(): number { return this.scale[0]; }
    setScaleX(x: number): this { this.scale[0] = x; return this; }
    getScaleY(): number { return this.scale[1]; }
    setScaleY(y: number): this { this.scale[1] = y; return this; }
    getScaleZ(): number { return this.scale[2]; }
    setScaleZ(z: number): this { this.scale[2] = z; return this; }

    /** Returns a copy of the scale tuple. */
    getScale(): [number, number, number] {
      return [this.scale[0], this.scale[1], this.scale[2]];
    }

    /** Set all three scale components. Chainable. */
    setScale(x: number, y: number, z: number): this {
      this.scale = [x, y, z];
      return this;
    }

    /** Set all three scale components to the same value. Chainable. */
    setScaleUniform(s: number): this {
      this.scale = [s, s, s];
      return this;
    }

    // --- Distance ---

    /** Euclidean distance between this transform's position and another's. */
    distance(other: Transform): number {
      const dx = this.position[0] - other.position[0];
      const dy = this.position[1] - other.position[1];
      const dz = this.position[2] - other.position[2];
      return Math.sqrt(dx * dx + dy * dy + dz * dz);
    }

    // --- Rotation: raw quaternion (xyzw) ---

    /** Returns a copy of the raw quaternion as [x, y, z, w]. */
    getRotation(): [number, number, number, number] {
      return [this.rotation[0], this.rotation[1], this.rotation[2], this.rotation[3]];
    }

    /** Set the raw quaternion directly. Chainable. */
    setRotation(x: number, y: number, z: number, w: number): this {
      this.rotation = [x, y, z, w];
      return this;
    }

    // --- Rotation: Euler angles (intrinsic XYZ / extrinsic ZYX, radians) ---

    /**
     * Decompose the current quaternion into intrinsic XYZ Euler angles in radians.
     * Returns [rx, ry, rz] — pitch around X, then yaw around Y, then roll around Z.
     * Near gimbal-lock (ry ≈ ±90°) rx and rz may be unstable.
     */
    getEulerRadians(): [number, number, number] {
      const [qx, qy, qz, qw] = this.rotation;
      const rx = Math.atan2(2 * (qw * qx + qy * qz), 1 - 2 * (qx * qx + qy * qy));
      const sinPitch = Math.max(-1, Math.min(1, 2 * (qw * qy - qz * qx)));
      const ry = Math.asin(sinPitch);
      const rz = Math.atan2(2 * (qw * qz + qx * qy), 1 - 2 * (qy * qy + qz * qz));
      return [rx, ry, rz];
    }

    /**
     * Set rotation from intrinsic XYZ Euler angles in radians (equivalent to extrinsic ZYX,
     * i.e. q = Qz·Qy·Qx). Consistent with `getEulerRadians`. Chainable.
     */
    setEulerRadians(rx: number, ry: number, rz: number): this {
      const cx = Math.cos(rx / 2), sx = Math.sin(rx / 2);
      const cy = Math.cos(ry / 2), sy = Math.sin(ry / 2);
      const cz = Math.cos(rz / 2), sz = Math.sin(rz / 2);
      this.rotation = [
        sx * cy * cz - cx * sy * sz,
        cx * sy * cz + sx * cy * sz,
        cx * cy * sz - sx * sy * cz,
        cx * cy * cz + sx * sy * sz,
      ];
      return this;
    }

    // --- Rotation: Euler angles (degrees) ---

    /** Decompose the current quaternion into intrinsic XYZ Euler angles in degrees. */
    getEulerDegrees(): [number, number, number] {
      const r = this.getEulerRadians();
      return [r[0] * 180 / Math.PI, r[1] * 180 / Math.PI, r[2] * 180 / Math.PI];
    }

    /** Set rotation from intrinsic XYZ Euler angles in degrees. Chainable. */
    setEulerDegrees(x: number, y: number, z: number): this {
      return this.setEulerRadians(x * Math.PI / 180, y * Math.PI / 180, z * Math.PI / 180);
    }

    // --- Rotation: incremental world-space helpers ---

    /** Rotate by `degrees` around the world X axis. Chainable. */
    rotateX(degrees: number): this {
      const r = degrees * Math.PI / 180;
      const s = Math.sin(r / 2), c = Math.cos(r / 2);
      return this._leftMultiply(s, 0, 0, c);
    }

    /** Rotate by `degrees` around the world Y axis. Chainable. */
    rotateY(degrees: number): this {
      const r = degrees * Math.PI / 180;
      const s = Math.sin(r / 2), c = Math.cos(r / 2);
      return this._leftMultiply(0, s, 0, c);
    }

    /** Rotate by `degrees` around the world Z axis. Chainable. */
    rotateZ(degrees: number): this {
      const r = degrees * Math.PI / 180;
      const s = Math.sin(r / 2), c = Math.cos(r / 2);
      return this._leftMultiply(0, 0, s, c);
    }

    private _leftMultiply(ax: number, ay: number, az: number, aw: number): this {
      const [bx, by, bz, bw] = this.rotation;
      this.rotation = [
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
        aw * bw - ax * bx - ay * by - az * bz,
      ];
      return this;
    }
  }

  export const MESH_RENDERER_ID = "core:mesh_renderer";
  export const AMBIENT_LIGHT_ID = "core:ambient_light";
  export const DIRECTIONAL_LIGHT_ID = "core:directional_light";
  export const POINT_LIGHT_ID = "core:point_light";

  /** SpriteRenderer component. Returned by `getComponent("core:sprite_renderer")` as a live class instance. */
  export class SpriteRenderer implements Component {
    readonly id = SPRITE_RENDERER_ID;
    texture: string;
    /** VFS path to a WGSL shader that satisfies the engine shader contract. */
    shader: string;

    constructor(init: { texture?: string; shader: string }) {
      this.texture = init.texture ?? "";
      this.shader = init.shader;
    }

    getTexture(): string { return this.texture; }
    setTexture(texture: string): this { this.texture = texture; return this; }
    getShader(): string { return this.shader; }
    setShader(shader: string): this { this.shader = shader; return this; }
  }

  /**
   * MeshRenderer component.  Renders arbitrary geometry loaded from a mesh file.
   *
   * Supported mesh formats (detected by file extension):
   * - `.json`        — `{"vertices":[[x,y,z,u,v],...], "indices":[...]}`
   * - `.obj`         — Wavefront OBJ (materials ignored)
   * - `.glb`/`.gltf` — glTF 2.0, first mesh/primitive, geometry only
   */
  export class MeshRenderer implements Component {
    readonly id = MESH_RENDERER_ID;
    /** VFS path to the mesh file. */
    mesh: string;
    /** VFS path to a WGSL shader that satisfies the engine shader contract. */
    shader: string;
    /** Optional VFS path to a texture.  Omit to use the 1×1 white fallback. */
    texture: string | null;

    constructor(init: { mesh?: string; shader: string; texture?: string | null }) {
      this.mesh = init.mesh ?? "";
      this.shader = init.shader;
      this.texture = init.texture ?? null;
    }

    getMesh(): string { return this.mesh; }
    setMesh(mesh: string): this { this.mesh = mesh; return this; }
    getShader(): string { return this.shader; }
    setShader(shader: string): this { this.shader = shader; return this; }
    getTexture(): string | null { return this.texture; }
    setTexture(texture: string | null): this { this.texture = texture; return this; }
  }

  /**
   * Camera component.  Attach to any entity together with `core:transform` to
   * make it usable as a scene camera.  Call `Engine.setMainCamera(entity.id)`
   * to activate it.
   *
   * The camera looks in the **-Z** direction of its local frame.
   * Position and orientation are derived from the entity's `core:transform`.
   */
  export class Camera implements Component {
    readonly id = CAMERA_ID;
    /** Vertical field-of-view in **degrees**. Default: `45`. */
    fovy_degrees: number;
    /** Near clip plane distance. Default: `0.1`. */
    znear: number;
    /** Far clip plane distance. Default: `1000`. */
    zfar: number;

    constructor(init: { fovy_degrees?: number; znear?: number; zfar?: number } = {}) {
      this.fovy_degrees = init.fovy_degrees ?? 45;
      this.znear = init.znear ?? 0.1;
      this.zfar = init.zfar ?? 1000;
    }

    getFov(): number { return this.fovy_degrees; }
    setFov(degrees: number): this { this.fovy_degrees = degrees; return this; }
    getZNear(): number { return this.znear; }
    setZNear(v: number): this { this.znear = v; return this; }
    getZFar(): number { return this.zfar; }
    setZFar(v: number): this { this.zfar = v; return this; }
  }

  /** TextRenderer component. Renders a string at the entity's world-space Transform position. */
  export class TextRenderer implements Component {
    readonly id = TEXT_RENDERER_ID;
    text: string;
    /** VFS path to a TTF/OTF font. */
    font: string;
    /** VFS path to a WGSL shader that satisfies the engine shader contract. */
    shader: string;
    font_size: number;
    /** RGBA colour, each channel in `[0.0, 1.0]`. Defaults to opaque white. */
    color: [number, number, number, number];

    constructor(init: {
      text?: string;
      font: string;
      shader: string;
      font_size?: number;
      color?: [number, number, number, number];
    }) {
      this.text = init.text ?? "";
      this.font = init.font;
      this.shader = init.shader;
      this.font_size = init.font_size ?? 24;
      this.color = init.color ?? [1, 1, 1, 1];
    }

    getText(): string { return this.text; }
    setText(text: string): this { this.text = text; return this; }
    getFont(): string { return this.font; }
    setFont(font: string): this { this.font = font; return this; }
    getFontSize(): number { return this.font_size; }
    setFontSize(size: number): this { this.font_size = size; return this; }
    getColor(): [number, number, number, number] { return [...this.color]; }
    setColor(r: number, g: number, b: number, a: number): this { this.color = [r, g, b, a]; return this; }
  }

  /** AmbientLight component. Multiple ambient lights are additive. */
  export class AmbientLight implements Component {
    readonly id = AMBIENT_LIGHT_ID;
    /** RGB colour, each channel in `[0.0, 1.0]`. Defaults to white. */
    color: [number, number, number];
    /** Multiplier for the colour. Defaults to `0.1`. */
    intensity: number;

    constructor(init: { color?: [number, number, number]; intensity?: number } = {}) {
      this.color = init.color ?? [1, 1, 1];
      this.intensity = init.intensity ?? 0.1;
    }

    getColor(): [number, number, number] { return [...this.color]; }
    setColor(r: number, g: number, b: number): this { this.color = [r, g, b]; return this; }
    getIntensity(): number { return this.intensity; }
    setIntensity(v: number): this { this.intensity = v; return this; }
  }

  /** DirectionalLight component. Simulates an infinitely distant light source (e.g. the sun). */
  export class DirectionalLight implements Component {
    readonly id = DIRECTIONAL_LIGHT_ID;
    /** Normalised direction the light travels *towards* (world space). */
    direction: [number, number, number];
    /** RGB colour, each channel in `[0.0, 1.0]`. Defaults to white. */
    color: [number, number, number];
    /** Multiplier for the colour. Defaults to `1.0`. */
    intensity: number;

    constructor(init: { direction?: [number, number, number]; color?: [number, number, number]; intensity?: number } = {}) {
      this.direction = init.direction ?? [0, -1, 0];
      this.color = init.color ?? [1, 1, 1];
      this.intensity = init.intensity ?? 1.0;
    }

    getDirection(): [number, number, number] { return [...this.direction]; }
    setDirection(x: number, y: number, z: number): this { this.direction = [x, y, z]; return this; }
    getColor(): [number, number, number] { return [...this.color]; }
    setColor(r: number, g: number, b: number): this { this.color = [r, g, b]; return this; }
    getIntensity(): number { return this.intensity; }
    setIntensity(v: number): this { this.intensity = v; return this; }
  }

  /**
   * PointLight component. Emits light in all directions from the entity's `core:transform` position.
   * Attenuation is quadratic, falling off to zero at `range` units.
   */
  export class PointLight implements Component {
    readonly id = POINT_LIGHT_ID;
    /** RGB colour, each channel in `[0.0, 1.0]`. Defaults to white. */
    color: [number, number, number];
    /** Multiplier for the colour. Defaults to `1.0`. */
    intensity: number;
    /** Distance (in world units) at which the light reaches zero. Defaults to `10`. */
    range: number;

    constructor(init: { color?: [number, number, number]; intensity?: number; range?: number } = {}) {
      this.color = init.color ?? [1, 1, 1];
      this.intensity = init.intensity ?? 1.0;
      this.range = init.range ?? 10;
    }

    getColor(): [number, number, number] { return [...this.color]; }
    setColor(r: number, g: number, b: number): this { this.color = [r, g, b]; return this; }
    getIntensity(): number { return this.intensity; }
    setIntensity(v: number): this { this.intensity = v; return this; }
    getRange(): number { return this.range; }
    setRange(v: number): this { this.range = v; return this; }
  }
}

interface _ComponentRegistry {
  [BuiltInComponents.TRANSFORM_ID]: BuiltInComponents.Transform;
  [BuiltInComponents.SPRITE_RENDERER_ID]: BuiltInComponents.SpriteRenderer;
  [BuiltInComponents.TEXT_RENDERER_ID]: BuiltInComponents.TextRenderer;
  [BuiltInComponents.CAMERA_ID]: BuiltInComponents.Camera;
  [BuiltInComponents.MESH_RENDERER_ID]: BuiltInComponents.MeshRenderer;
  [BuiltInComponents.AMBIENT_LIGHT_ID]: BuiltInComponents.AmbientLight;
  [BuiltInComponents.DIRECTIONAL_LIGHT_ID]: BuiltInComponents.DirectionalLight;
  [BuiltInComponents.POINT_LIGHT_ID]: BuiltInComponents.PointLight;
}

// Accepted data shapes for setComponent — plain objects and class instances both satisfy this.
interface _ComponentSetRegistry {
  [BuiltInComponents.TRANSFORM_ID]: { position: [number, number, number]; rotation: [number, number, number, number]; scale: [number, number, number] };
  [BuiltInComponents.SPRITE_RENDERER_ID]: { texture: string; shader: string };
  [BuiltInComponents.TEXT_RENDERER_ID]: { text: string; font: string; shader: string; font_size?: number; color?: [number, number, number, number]; };
  [BuiltInComponents.CAMERA_ID]: { fovy_degrees?: number; znear?: number; zfar?: number };
  [BuiltInComponents.MESH_RENDERER_ID]: { mesh: string; shader: string; texture?: string | null };
  [BuiltInComponents.AMBIENT_LIGHT_ID]: { color?: [number, number, number]; intensity?: number };
  [BuiltInComponents.DIRECTIONAL_LIGHT_ID]: { direction?: [number, number, number]; color?: [number, number, number]; intensity?: number };
  [BuiltInComponents.POINT_LIGHT_ID]: { color?: [number, number, number]; intensity?: number; range?: number };
}

// Converts raw component JSON into the appropriate class instance for built-in types.
const _componentHydrators: Record<string, (data: any) => any> = {
  "core:transform": (data) => new BuiltInComponents.Transform(data),
  "core:sprite_renderer": (data) => new BuiltInComponents.SpriteRenderer(data),
  "core:text_renderer": (data) => new BuiltInComponents.TextRenderer(data),
  "core:camera": (data) => new BuiltInComponents.Camera(data),
  "core:mesh_renderer": (data) => new BuiltInComponents.MeshRenderer(data),
  "core:ambient_light": (data) => new BuiltInComponents.AmbientLight(data),
  "core:directional_light": (data) => new BuiltInComponents.DirectionalLight(data),
  "core:point_light": (data) => new BuiltInComponents.PointLight(data),
};

function _setComponentImpl<K extends keyof _ComponentSetRegistry>(entity_id: string, component_type: K, data: _ComponentSetRegistry[K]): void;
function _setComponentImpl(entity_id: string, component_type: string, data: JsonValue): void;
function _setComponentImpl(entity_id: string, component_type: string, data: any): void {
  // Destructure to exclude `id` without mutating the caller's object.
  const { id: _id, ...rest } = data;
  _send({ type: "ComponentSet", entity_id, component_type, data: rest });
}

function _getComponentImpl<K extends keyof _ComponentRegistry>(entity_id: string, component_type: K): Promise<_ComponentRegistry[K] | null>;
function _getComponentImpl<T extends Component = Component>(entity_id: string, component_type: string): Promise<T | null>;
function _getComponentImpl(entity_id: string, component_type: string): Promise<any> {
  return new Promise((resolve, reject) => {
    const request_id = nextReqId();
    _componentGetCallbacks.set(request_id, { resolve, reject });
    _send({ type: "ComponentGet", request_id, entity_id, component_type });
  }).then((data) => {
    if (data === null) return null;
    const hydrator = _componentHydrators[component_type];
    return hydrator ? hydrator(data) : data;
  });
}

/** A live entity in the scene. Wraps an entity ID and provides component access. */
export class Entity {
  constructor(public readonly id: string) {}

  /** Destroy this entity and all its components. Fire-and-forget. */
  destroy(): void {
    _send({ type: "EntityDestroy", entity_id: this.id });
  }

  /** Set a component on this entity. Fire-and-forget. */
  setComponent<K extends keyof _ComponentSetRegistry>(component_type: K, data: _ComponentSetRegistry[K]): void;
  setComponent(component: Component): void;
  setComponent(component_type: string, data: JsonValue): void;
  setComponent(component_type_or_component: string | Component, data?: any): void {
    if (typeof component_type_or_component === "object") {
      _setComponentImpl(this.id, component_type_or_component.id, component_type_or_component as any);
    } else {
      _setComponentImpl(this.id, component_type_or_component, data);
    }
  }

  /** Remove a component from this entity. Fire-and-forget. */
  removeComponent(component_type: string): void {
    _send({ type: "ComponentRemove", entity_id: this.id, component_type });
  }

  /** Get a component's data. Returns `null` if the component is not set. */
  getComponent<K extends keyof _ComponentRegistry>(component_type: K): Promise<_ComponentRegistry[K] | null>;
  getComponent<T extends Component = Component>(component_type: string): Promise<T | null>;
  getComponent(component_type: string): Promise<any> {
    return _getComponentImpl(this.id, component_type);
  }

  /**
   * Convenience: update the `position` field of this entity's `core:transform`
   * while preserving existing rotation and scale.
   */
  async move(position: [number, number, number]): Promise<void> {
    const existing = await this.getComponent("core:transform");
    this.setComponent("core:transform", {
      position,
      rotation: existing?.rotation ?? [0, 0, 0, 1],
      scale: existing?.scale ?? [1, 1, 1],
    });
  }

  /**
   * Attach this entity to a parent. Pass `null` to detach from the current
   * parent. Fire-and-forget.
   *
   * Transform components are interpreted as *local* — i.e. relative to the
   * parent's world-space transform. If the entity has no parent its local
   * transform equals its world-space transform.
   */
  setParent(parent: Entity | string | null): void {
    const parent_id = parent === null ? null : (typeof parent === "string" ? parent : parent.id);
    _send({ type: "EntitySetParent", entity_id: this.id, parent_id });
  }

  /** Return the parent `Entity`, or `null` if this entity has no parent. */
  getParent(): Promise<Entity | null> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityParentCallbacks.set(request_id, {
        resolve: (id) => resolve(id !== null ? new Entity(id) : null),
        reject,
      });
      _send({ type: "EntityGetParent", request_id, entity_id: this.id });
    });
  }

  /** Return all direct children of this entity. */
  getChildren(): Promise<Entity[]> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityChildrenCallbacks.set(request_id, {
        resolve: (ids) => resolve(ids.map((id) => new Entity(id))),
        reject,
      });
      _send({ type: "EntityGetChildren", request_id, entity_id: this.id });
    });
  }
}

/** Entity and component management. */
export const Scene = {
  /** Create a new entity and return it. */
  createEntity(): Promise<Entity> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityCallbacks.set(request_id, { resolve: (id) => resolve(new Entity(id)), reject });
      _send({ type: "EntityCreate", request_id });
    });
  },

  /** Destroy an entity and all its components. Fire-and-forget. */
  destroyEntity(entity_id: string): void {
    _send({ type: "EntityDestroy", entity_id });
  },

  /** Return all living entities. */
  listEntities(): Promise<Entity[]> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityListCallbacks.set(request_id, { resolve: (ids) => resolve(ids.map((id) => new Entity(id))), reject });
      _send({ type: "EntityListRequest", request_id });
    });
  },

  /** Set a component on an entity by ID. Fire-and-forget. */
  setComponent: _setComponentImpl,

  /** Remove a component from an entity. Fire-and-forget. */
  removeComponent(entity_id: string, component_type: string): void {
    _send({ type: "ComponentRemove", entity_id, component_type });
  },

  /** Get a component's data by entity ID. Returns `null` if the component is not set. */
  getComponent: _getComponentImpl,

  /** Query all entities that have every listed component type. */
  query(component_types: string[]): Promise<QueryResult<Entity>[]> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _componentQueryCallbacks.set(request_id, {
        resolve: (results) => resolve(results.map((r) => ({
          entity: new Entity(r.entity_id),
          components: Object.fromEntries(
            Object.entries(r.components).map(([k, v]) => {
              const hydrator = _componentHydrators[k];
              return [k, hydrator ? hydrator(v) : v];
            })
          ),
        }))),
        reject,
      });
      _send({ type: "ComponentQuery", request_id, component_types });
    });
  },

  /**
   * Convenience: create an entity and attach `core:transform` + `core:sprite_renderer`.
   * The sprite becomes visible as soon as both components are set.
   */
  async spawnSprite(
    texture: string,
    shader: string,
    position: [number, number, number],
    scale: [number, number, number] = [1, 1, 1],
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:transform", { position, rotation: [0, 0, 0, 1], scale });
    entity.setComponent("core:sprite_renderer", { texture, shader });
    return entity;
  },

  /**
   * Convenience: create an entity and attach `core:transform` + `core:text_renderer`.
   * The text becomes visible as soon as both components are set.
   */
  async spawnText(
    text: string,
    position: [number, number, number],
    options: { shader: string; font: string; font_size?: number; color?: [number, number, number, number]; },
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:transform", { position, rotation: [0, 0, 0, 1], scale: [1, 1, 1] });
    entity.setComponent("core:text_renderer", {
      text,
      shader: options.shader,
      font: options.font,
      font_size: options.font_size ?? 24,
      color: options.color ?? [1, 1, 1, 1]
    });
    return entity;
  },

  /**
   * Convenience: create an entity and attach `core:transform` + `core:mesh_renderer`.
   * The mesh becomes visible as soon as both components are set.
   */
  async spawnMesh(
    mesh: string,
    shader: string,
    position: [number, number, number],
    options: { texture?: string | null; scale?: [number, number, number] } = {},
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:transform", {
      position,
      rotation: [0, 0, 0, 1],
      scale: options.scale ?? [1, 1, 1],
    });
    entity.setComponent("core:mesh_renderer", {
      mesh,
      shader,
      texture: options.texture ?? null,
    });
    return entity;
  },

  /**
   * Convenience: move an entity by updating the `position` field of its `core:transform`
   * while preserving existing rotation and scale.
   */
  async moveEntity(entity_id: string, position: [number, number, number]): Promise<void> {
    const existing = await Scene.getComponent(entity_id, "core:transform");
    Scene.setComponent(entity_id, "core:transform", {
      position,
      rotation: existing?.rotation ?? [0, 0, 0, 1],
      scale: existing?.scale ?? [1, 1, 1],
    });
  },

  /**
   * Attach `entity_id` to `parent_id`. Pass `null` as `parent_id` to detach.
   * Fire-and-forget.
   */
  setParent(entity_id: string, parent_id: string | null): void {
    _send({ type: "EntitySetParent", entity_id, parent_id });
  },

  /** Return the parent entity ID, or `null` if the entity has no parent. */
  getParent(entity_id: string): Promise<string | null> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityParentCallbacks.set(request_id, { resolve, reject });
      _send({ type: "EntityGetParent", request_id, entity_id });
    });
  },

  /** Return the IDs of all direct children of the given entity. */
  getChildren(entity_id: string): Promise<string[]> {
    return new Promise((resolve, reject) => {
      const request_id = nextReqId();
      _entityChildrenCallbacks.set(request_id, { resolve, reject });
      _send({ type: "EntityGetChildren", request_id, entity_id });
    });
  },

  /**
   * Convenience: create an entity and attach `core:ambient_light`.
   * Multiple ambient lights are additive.
   */
  async addAmbientLight(
    color: [number, number, number] = [1, 1, 1],
    intensity: number = 0.1,
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:ambient_light", { color, intensity });
    return entity;
  },

  /**
   * Convenience: create an entity and attach `core:directional_light`.
   * @param direction Normalised direction the light travels towards (world space).
   */
  async addDirectionalLight(
    direction: [number, number, number],
    color: [number, number, number] = [1, 1, 1],
    intensity: number = 1.0,
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:directional_light", { direction, color, intensity });
    return entity;
  },

  /**
   * Convenience: create an entity with `core:transform` + `core:point_light`.
   * The light is emitted from `position` in all directions.
   */
  async addPointLight(
    position: [number, number, number],
    color: [number, number, number] = [1, 1, 1],
    intensity: number = 1.0,
    range: number = 10,
  ): Promise<Entity> {
    const entity = await Scene.createEntity();
    entity.setComponent("core:transform", { position, rotation: [0, 0, 0, 1], scale: [1, 1, 1] });
    entity.setComponent("core:point_light", { color, intensity, range });
    return entity;
  },
};
