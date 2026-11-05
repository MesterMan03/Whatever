import { Component, Engine, Time } from "@whatever-engine/api";
import { listen } from "./session.ts";
import { RunState } from "./RunState.ts";
/** Score/clock/lifecycle rules operate on RunState data attached to this entity. */
export class RunSession extends Component {
  static readonly type = "core:RunSession";
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    if (!this.entity.hasComponent(RunState)) this.entity.setComponent(new RunState());
    this.unsubscribe = listen(
      () => this.entity,
      (event) => {
        const state = this.entity.getComponent(RunState);
        if (!state) return;
        if (event.type === "restart") {
          this.entity.editComponent(RunState, (data) => {
            data.score = 0;
            data.coins = 0;
            data.kills = 0;
            data.over = false;
            data.elapsed = 0;
            data.generation++;
            data.readyAfter = Time.tickNumber + 2;
          });
        } else if (event.type === "death" && !state.over) {
          this.entity.editComponent(RunState, (data) => {
            data.over = true;
          });
          Engine.log(
            "info",
            `COINWATCH_GAME_OVER score=${state.score} coins=${state.coins} kills=${state.kills}`,
          );
        } else if (event.type === "award" && !state.over && Time.tickNumber > state.readyAfter) {
          this.entity.editComponent(RunState, (data) => {
            data.score += event.amount;
            if (event.kind === "coin") data.coins++;
            else data.kills++;
          });
        }
      },
    );
    Engine.log(
      "info",
      "COINWATCH_READY — WASD, mouse, Shift sprint, left click fire, R restart. F2 opens console.",
    );
  }
  override tick(dt: number): void {
    if (!this.entity.getComponent(RunState)?.over)
      this.entity.editComponent(RunState, (state) => {
        state.elapsed += dt;
      });
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
