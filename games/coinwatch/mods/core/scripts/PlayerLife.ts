import { Component, Physics, Type, field } from "@whatever-engine/api";
import { emit, listen, playing, position } from "./session.ts";
/** Respawn/fall handling; contact damage lives on the damaging entity. */
export class PlayerLife extends Component {
  static readonly type = "core:PlayerLife";
  session = field(Type.Nullable(Type.Entity), null);
  spawn = field(Type.Tuple(Type.F64, Type.F64, Type.F64), [0, 1.05, 9]);
  fallHeight = field(Type.F64, -8);
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (event.type === "death" || event.type === "restart") {
          Physics.body(this.entity).teleport([...this.spawn]);
          Physics.body(this.entity).setLinearVelocity([0, 0, 0]);
        }
      },
    );
  }
  override tick(): void {
    if (playing(this.session) && position(this.entity)[1] < this.fallHeight)
      emit(this.session, { type: "death" });
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
