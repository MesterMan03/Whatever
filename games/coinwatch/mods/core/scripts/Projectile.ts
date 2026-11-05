import { Component, Physics, Type, field } from "@whatever-engine/api";
import { sweepLength } from "./navigation.ts";
import { emit, playing, position, resolve, run } from "./session.ts";
/** One entity owns one swept projectile and its lifetime. */
export class Projectile extends Component {
  static readonly type = "core:Projectile";
  session = field(Type.Nullable(Type.Entity), null);
  world = field(Type.Nullable(Type.Entity), null);
  owner = field(Type.Nullable(Type.Entity), null);
  direction = field(Type.Tuple(Type.F64, Type.F64, Type.F64), [0, 0, -1]);
  speed = field(Type.F64.with({ min: 1, max: 200 }), 30);
  range = field(Type.F64.with({ min: 1, max: 200 }), 40);
  damage = field(Type.F64.with({ min: 1, max: 1000 }), 1);
  private travelled = 0;
  private generation = -1;
  private active = true;
  private origin: Physics.Vec3 | null = null;
  override async tick(dt: number): Promise<void> {
    if (!this.active) return;
    const state = run(this.session),
      world = resolve(this.world);
    if (!world || !state) return;
    if (this.generation === -1) this.generation = state.generation;
    if (!playing(this.session) || state.generation !== this.generation) {
      this.end();
      return;
    }
    this.origin ??= position(this.entity);
    const step = sweepLength(this.speed, dt, this.travelled, this.range);
    const hit = await Physics.world(world)
      .castRay(this.origin, this.direction, {
        maxDistance: step,
        excludeBody: resolve(this.owner) ?? undefined,
        flags: Physics.QueryFilterFlag.EXCLUDE_SENSORS,
      })
      .get();
    if (!this.active) return;
    if (!playing(this.session) || run(this.session)?.generation !== this.generation) {
      this.end();
      return;
    }
    if (hit) {
      emit(this.session, {
        type: "damage",
        target: hit.body?.id ?? hit.collider.id,
        amount: this.damage,
      });
      emit(this.session, {
        type: "projectileEnd",
        distance: this.travelled + hit.distance,
        reason: "collider",
      });
      this.end();
    } else {
      this.travelled += step;
      this.origin = this.origin.map((v, i) => v + this.direction[i] * step) as Physics.Vec3;
      if (this.travelled >= this.range - 0.00001) {
        emit(this.session, { type: "projectileEnd", distance: this.travelled, reason: "range" });
        this.end();
      } else
        this.entity.editComponent("core:transform", (t) => {
          t.position = [...this.origin!];
        });
    }
  }
  private end(): void {
    this.active = false;
    this.entity.destroy();
  }
  override destroy(): void {
    this.active = false;
  }
}
