import type { RunSession } from "./RunSession.ts";
import { Component, Type, field } from "@whatever-engine/api";
import { emit, listen, playing } from "./session.ts";
export class Health extends Component {
  static readonly type = "core:Health";
  session: RunSession | null = null;
  hitPoints = field(Type.F64.with({ min: 1, max: 1000 }), 1);
  scoreOnKill = field(Type.I32.with({ min: 0, max: 10000 }), 50, {
    label: "Score on Kill",
    tooltip: "Score gained on a kill",
  });
  private remaining = 1;
  private dead = false;
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    this.remaining = this.hitPoints;
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (event.type === "restart") {
          this.dead = true;
          this.entity.destroy();
        }
        if (
          event.type !== "damage" ||
          event.target !== this.entity.id ||
          this.dead ||
          !playing(this.session)
        )
          return;
        this.remaining -= event.amount;
        if (this.remaining <= 0) {
          this.dead = true;
          emit(this.session, { type: "award", amount: this.scoreOnKill, kind: "kill" });
          this.entity.destroy();
        }
      },
    );
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
