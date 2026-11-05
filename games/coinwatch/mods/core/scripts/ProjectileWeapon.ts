import {
  BuiltInComponents as C,
  Component,
  Engine,
  Input,
  Prefab,
  Scene,
  Type,
  field,
  type Physics,
} from "@whatever-engine/api";
import { Projectile } from "./Projectile.ts";
import { emit, playing, run, type Ref } from "./session.ts";
export type ShotOptions = {
  session: Ref;
  world: Ref;
  owner: Ref;
  prefab: string;
  speed: number;
  range: number;
  damage: number;
  origin: Physics.Vec3;
  direction: Physics.Vec3;
};
export async function fireProjectile(options: ShotOptions): Promise<void> {
  if (!playing(options.session)) return;
  const generation = run(options.session)?.generation;
  const instance = await Prefab.spawn(options.prefab);
  if (!playing(options.session) || generation !== run(options.session)?.generation) {
    instance.root.destroy();
    return;
  }
  instance.root
    .setComponent(new C.Transform({ position: options.origin, scale: [0.13, 0.13, 0.13] }))
    .addTag("projectile");
  const projectile = new Projectile();
  projectile.session = options.session;
  projectile.world = options.world;
  projectile.owner = options.owner;
  projectile.direction = [...options.direction];
  projectile.speed = options.speed;
  projectile.range = options.range;
  projectile.damage = options.damage;
  instance.root.setComponent(projectile);
  emit(options.session, { type: "fired" });
}
export class ProjectileWeapon extends Component {
  static readonly type = "core:ProjectileWeapon";
  session = field(Type.Nullable(Type.Entity), null);
  world = field(Type.Nullable(Type.Entity), null);
  owner = field(Type.Nullable(Type.Entity), null);
  prefab = field(Type.Asset, "core://prefabs/pulse.wprefab");
  speed = field(Type.F64.with({ min: 1, max: 200 }), 30);
  range = field(Type.F64.with({ min: 1, max: 200 }), 40);
  damage = field(Type.F64.with({ min: 1, max: 1000 }), 1);
  fireInterval = field(Type.F64.with({ min: 0.05, max: 5 }), 0.22);
  maxProjectiles = field(Type.I32.with({ min: 1, max: 100 }), 40);
  private timer = 0;
  private pending = false;
  override tick(dt: number): void {
    this.timer -= dt;
    if (
      !playing(this.session) ||
      !Input.mouseCaptured ||
      this.timer > 0 ||
      this.pending ||
      !this.session ||
      !this.world ||
      !this.owner
    )
      return;
    if (!Input.isMouseButtonDown("Left") && !Input.wasMouseButtonPressed("Left")) return;
    if (Scene.findByTag("projectile").length >= this.maxProjectiles) return;
    const pose = this.entity.getComponent("core:world_transform");
    if (!pose) return;
    const [x, y, z, w] = pose.rotation;
    const direction: Physics.Vec3 = [
      -2 * (x * z + w * y),
      2 * (w * x - y * z),
      -(1 - 2 * (x * x + y * y)),
    ];
    this.timer = this.fireInterval;
    this.pending = true;
    void fireProjectile({
      session: this.session,
      world: this.world,
      owner: this.owner,
      prefab: this.prefab,
      speed: this.speed,
      range: this.range,
      damage: this.damage,
      origin: [...pose.position],
      direction,
    })
      .catch((error) => Engine.log("error", `Projectile spawn failed: ${String(error)}`))
      .finally(() => {
        this.pending = false;
      });
  }
}
