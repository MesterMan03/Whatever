import { Component, Console, Physics, Scene, Type, field } from "@whatever-engine/api";
import { EnemyNavigation } from "./EnemyNavigation.ts";
import { EnemySpawner, spawnEnemy } from "./EnemySpawner.ts";
import { ProjectileWeapon, fireProjectile } from "./ProjectileWeapon.ts";
import { emit, listen, position, resolve, run } from "./session.ts";
/** Optional development component; gameplay never calls these tools or reads their telemetry. */
export class PlaytestTools extends Component {
  static readonly type = "core:PlaytestTools";
  session = field(Type.Nullable(Type.Entity), null);
  player = field(Type.Nullable(Type.Entity), null);
  spawner = field(Type.Nullable(Type.Entity), null);
  weapon = field(Type.Nullable(Type.Entity), null);
  private lastShot = { distance: 0, reason: "none" };
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (event.type === "projectileEnd")
          this.lastShot = { distance: event.distance, reason: event.reason };
      },
    );
    const session = this.session;
    Console.register({
      name: "coinwatch",
      description: "Optional Coinwatch playtesting tools",
      handler: () =>
        "status | restart | spawning <bool> | teleport <x> <z> | enemy <x> <z> | shoot <x> <y> <z>",
      subcommands: [
        {
          name: "status",
          handler: () => {
            const state = run(session),
              player = resolve(this.player),
              spawner = resolve(this.spawner)?.getComponent(EnemySpawner);
            const enemies = Scene.findByTag("enemy");
            return JSON.stringify({
              score: state?.score ?? 0,
              coins: state?.coins ?? 0,
              kills: state?.kills ?? 0,
              dead: state?.over ?? false,
              enemies: enemies.length,
              projectiles: Scene.findByTag("projectile").length,
              nodes: resolve(spawner?.graph ?? null)?.getChildren().length ?? 0,
              lastShot: this.lastShot,
              player: player ? position(player) : null,
              states: enemies.map((e) => ({
                id: e.id,
                mode: e.getComponent(EnemyNavigation)?.mode,
                position: position(e),
              })),
            });
          },
        },
        {
          name: "restart",
          handler: () => {
            emit(session, { type: "restart" });
            return "Restarted";
          },
        },
        {
          name: "spawning",
          args: [{ name: "enabled", type: "bool", required: true }],
          handler: ({ enabled }) => {
            resolve(this.spawner)?.editComponent(EnemySpawner, (c) => {
              c.enabled = enabled === true;
            });
            return `Spawning ${enabled}`;
          },
        },
        {
          name: "teleport",
          args: [
            { name: "x", type: "float", required: true },
            { name: "z", type: "float", required: true },
          ],
          handler: ({ x, z }) => {
            const p = resolve(this.player);
            if (p) {
              Physics.body(p).teleport([Number(x), 1.05, Number(z)]);
              Physics.body(p).setLinearVelocity([0, 0, 0]);
            }
            return "Teleported";
          },
        },
        {
          name: "enemy",
          args: [
            { name: "x", type: "float", required: true },
            { name: "z", type: "float", required: true },
          ],
          handler: async ({ x, z }) => {
            const settings = resolve(this.spawner)?.getComponent(EnemySpawner);
            const world = resolve(settings?.world ?? null),
              target = resolve(settings?.target ?? null),
              graph = resolve(settings?.graph ?? null);
            const e =
              settings && session && world && target && graph
                ? await spawnEnemy({ session, world, target, graph, prefab: settings.prefab }, [
                    Number(x),
                    1,
                    Number(z),
                  ])
                : null;
            return `Spawned ${e?.id ?? "none"}`;
          },
        },
        {
          name: "shoot",
          args: [
            { name: "x", type: "float", required: true },
            { name: "y", type: "float", required: true },
            { name: "z", type: "float", required: true },
          ],
          handler: async ({ x, y, z }) => {
            const weapon = resolve(this.weapon),
              settings = weapon?.getComponent(ProjectileWeapon);
            if (!weapon || !settings?.session || !settings.world || !settings.owner)
              return "Not ready";
            const origin = position(weapon),
              d: Physics.Vec3 = [
                Number(x) - origin[0],
                Number(y) - origin[1],
                Number(z) - origin[2],
              ],
              length = Math.hypot(...d);
            if (!length) return "Choose a different point";
            await fireProjectile({
              session: settings.session,
              world: settings.world,
              owner: settings.owner,
              prefab: settings.prefab,
              speed: settings.speed,
              range: settings.range,
              damage: settings.damage,
              origin,
              direction: d.map((v) => v / length) as Physics.Vec3,
            });
            return "Fired";
          },
        },
      ],
    });
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
