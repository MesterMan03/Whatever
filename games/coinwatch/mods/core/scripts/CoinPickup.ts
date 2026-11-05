import { Component, Type, field } from "@whatever-engine/api";
import { distance } from "./navigation.ts";
import { emit, listen, playing, position, resolve } from "./session.ts";
export class CoinPickup extends Component {
  static readonly type = "core:CoinPickup";
  session = field(Type.Nullable(Type.Entity), null);
  collector = field(Type.Nullable(Type.Entity), null);
  radius = field(Type.F64.with({ min: 0.2, max: 5 }), 1.1);
  score = field(Type.I32.with({ min: 1, max: 10000 }), 10);
  respawnSeconds = field(Type.F64.with({ min: 0, max: 300 }), 15);
  private collected = false;
  private cooldown = 0;
  private scale: [number, number, number] = [1, 1, 1];
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    this.scale = [...(this.entity.getComponent("core:transform")?.scale ?? [1, 1, 1])];
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (event.type === "restart") this.reset();
      },
    );
  }
  private reset(): void {
    this.collected = false;
    this.cooldown = 0;
    this.entity.editComponent("core:transform", (t) => {
      t.scale = [...this.scale];
    });
  }
  override tick(dt: number): void {
    if (!playing(this.session)) return;
    if (this.collected) {
      if (this.respawnSeconds > 0 && (this.cooldown -= dt) <= 0) this.reset();
      return;
    }
    const collector = resolve(this.collector);
    if (!collector) return;
    const p = position(this.entity),
      target = position(collector);
    if (distance(p, target) <= this.radius && Math.abs(p[1] - target[1]) < 1.5) {
      this.collected = true;
      this.cooldown = this.respawnSeconds;
      this.entity.editComponent("core:transform", (t) => {
        t.scale = [0, 0, 0];
      });
      emit(this.session, { type: "award", amount: this.score, kind: "coin" });
    }
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
