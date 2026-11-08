import type { RunSession } from "./RunSession.ts";
import { Component, Type, field } from "@whatever-engine/api";
import { run } from "./session.ts";
export class Scoreboard extends Component {
  static readonly type = "core:Scoreboard";
  session: RunSession | null = null;
  private lastText = "";
  override tick(): void {
    const state = run(this.session);
    if (!state) return;
    const text = state.over
      ? `GAME OVER\nSCORE  ${state.score}\n${state.coins} coins  /  ${state.kills} sentinels\nCLICK / R to try again`
      : `COINWATCH\nSCORE  ${state.score}\nCOINS ${state.coins}   KILLS ${state.kills}\nSURVIVED ${Math.floor(state.elapsed)}s\nWASD / SHIFT / LEFT CLICK`;
    if (text === this.lastText) return;
    this.lastText = text;
    this.entity.editComponent("core:text_renderer", (c) => {
      c.text = text;
      c.color = state.over ? [1, 0.35, 0.3, 1] : [1, 0.91, 0.55, 1];
    });
  }
}
