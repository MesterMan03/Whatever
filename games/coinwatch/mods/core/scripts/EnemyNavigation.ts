import { Component, Physics, Type, field } from "@whatever-engine/api";
import { advanceHunt, distance, nearest, route, type HuntState, type Node } from "./navigation.ts";
import { readGraph } from "./NavNode.ts";
import { id, playing, position, resolve } from "./session.ts";
/** Path selection and pursuit only. Appearance, damage, spawning and scoring are separate. */
export class EnemyNavigation extends Component {
  static readonly type = "core:EnemyNavigation";
  session = field(Type.Nullable(Type.Entity), null);
  world = field(Type.Nullable(Type.Entity), null);
  graph = field(Type.Nullable(Type.Entity), null);
  target = field(Type.Nullable(Type.Entity), null);
  roamSpeed = field(Type.F64.with({ min: 0.1, max: 20 }), 1.6);
  chaseSpeed = field(Type.F64.with({ min: 0.1, max: 20 }), 3.2);
  chaseSeconds = field(Type.F64.with({ min: 0.1, max: 120 }), 20);
  cooldownSeconds = field(Type.F64.with({ min: 0.1, max: 120 }), 10);
  sightRange = field(Type.F64.with({ min: 1, max: 100 }), 22);
  sightFov = field(Type.F64.with({ min: 1, max: 360 }), 160);
  sightInterval = field(Type.F64.with({ min: 0.05, max: 3 }), 0.25);
  waypointRadius = field(Type.F64.with({ min: 0.1, max: 3 }), 0.7);
  repathSeconds = field(Type.F64.with({ min: 0.1, max: 5 }), 0.75);
  mode = field(Type.Enum(["roam", "chase", "cooldown"]), "roam");
  private hunt: HuntState = { mode: "roam", remaining: 0 };
  private path: Node[] = [];
  private pathTimer = 0;
  private sightTimer = 0;
  private checking = false;
  private visible = false;
  private active = true;
  private facing: Physics.Vec3 = [0, 0, -1];
  override tick(dt: number): void {
    const graph = resolve(this.graph),
      targetEntity = resolve(this.target);
    if (!playing(this.session) || !graph || !targetEntity || !this.world) {
      Physics.body(this.entity).setLinearVelocity([0, 0, 0], { axes: ["x", "z"] });
      return;
    }
    const p = position(this.entity),
      target = position(targetEntity);
    this.sightTimer -= dt;
    this.pathTimer -= dt;
    if (this.sightTimer <= 0 && !this.checking && this.hunt.mode !== "cooldown") {
      this.sightTimer = this.sightInterval;
      void this.checkSight(p, target).catch(() => {
        this.visible = false;
      });
    }
    const old = this.hunt.mode;
    this.hunt = advanceHunt(this.hunt, dt, this.visible, this.chaseSeconds, this.cooldownSeconds);
    this.mode = this.hunt.mode;
    if (old !== this.mode) {
      this.path = [];
      this.pathTimer = 0;
      this.visible = false;
    }
    if (!this.path.length || (this.mode === "chase" && this.pathTimer <= 0)) {
      const all = readGraph(graph),
        start = nearest(all, p);
      const goal =
        this.mode === "chase" ? nearest(all, target) : all[Math.floor(Math.random() * all.length)];
      this.path = start && goal ? route(all, start.key, goal.key) : [];
      if (
        this.path.length > 1 &&
        (this.mode === "chase" || distance(p, this.path[0].position) < this.waypointRadius)
      )
        this.path.shift();
      this.pathTimer = this.repathSeconds;
    }
    while (this.path.length && distance(p, this.path[0].position) < this.waypointRadius)
      this.path.shift();
    const destination = this.mode === "chase" && this.visible ? target : this.path[0]?.position;
    const dx = destination ? destination[0] - p[0] : 0,
      dz = destination ? destination[2] - p[2] : 0;
    const length = Math.hypot(dx, dz);
    const speed = this.mode === "chase" ? this.chaseSpeed : this.roamSpeed;
    this.facing = length > 0.05 ? [dx / length, 0, dz / length] : this.facing;
    Physics.body(this.entity).setLinearVelocity(
      length > 0.05 ? [this.facing[0] * speed, 0, this.facing[2] * speed] : [0, 0, 0],
      { axes: ["x", "z"] },
    );
  }
  private async checkSight(p: Physics.Vec3, target: Physics.Vec3): Promise<void> {
    const world = resolve(this.world);
    this.visible = false;
    if (!world) return;
    const delta = target.map((v, i) => v - p[i]) as Physics.Vec3,
      length = Math.hypot(...delta);
    if (length > this.sightRange || length < 0.01) return;
    const direction = delta.map((v) => v / length) as Physics.Vec3;
    if (
      this.facing[0] * direction[0] + this.facing[2] * direction[2] <
      Math.cos((this.sightFov * Math.PI) / 360)
    )
      return;
    this.checking = true;
    try {
      const hit = await Physics.world(world)
        .castRay(p, direction, {
          maxDistance: length + 0.2,
          excludeBody: this.entity,
          flags: Physics.QueryFilterFlag.EXCLUDE_SENSORS,
        })
        .get();
      if (this.active && playing(this.session))
        this.visible = hit?.body?.id === id(this.target) || hit?.collider.id === id(this.target);
    } finally {
      this.checking = false;
    }
  }
  override destroy(): void {
    this.active = false;
  }
}
