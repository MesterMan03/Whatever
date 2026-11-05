import { Component, Type, field, type Physics } from "@whatever-engine/api";
import { EnemyNavigation } from "./EnemyNavigation.ts";
import { library, position, resolve } from "./session.ts";
export class EnemyAppearance extends Component {
  static readonly type = "core:EnemyAppearance";
  body = field(Type.Nullable(Type.Entity), null);
  private previous: Physics.Vec3 | null = null;
  override tick(): void {
    const body = resolve(this.body);
    if (!body) return;
    const p = position(body),
      old = this.previous;
    this.previous = p;
    if (old && Math.hypot(p[0] - old[0], p[2] - old[2]) > 0.001)
      this.entity.editComponent("core:transform", (t) => {
        t.setEulerRadians(0, Math.atan2(old[0] - p[0], old[2] - p[2]), 0);
      });
    const mode = body.getComponent(EnemyNavigation)?.mode;
    const texture =
      library +
      "textures/" +
      (mode === "chase" ? "red" : mode === "cooldown" ? "blue" : "orange") +
      ".png";
    if (this.entity.getComponent("core:mesh_renderer")?.texture !== texture)
      this.entity.editComponent("core:mesh_renderer", (m) => {
        m.texture = texture;
      });
  }
}
