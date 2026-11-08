import type { RunSession } from "./RunSession.ts";
import { Component, Engine, Type, field } from "@whatever-engine/api";
import { resolve, run } from "./session.ts";
/** A clear world-space score view above enemy height, independent of the player body. */
export class GameOverView extends Component {
  static readonly type = "core:GameOverView";
  session: RunSession | null = null;
  playCamera = field(Type.Nullable(Type.Entity), null);
  private showing = false;
  override tick(): void {
    if (Engine.mode !== "client") return;
    const over = run(this.session)?.over ?? false;
    if (over === this.showing) return;
    this.showing = over;
    const camera = over ? this.entity : resolve(this.playCamera);
    if (camera) Engine.setMainCamera(camera);
  }
}
