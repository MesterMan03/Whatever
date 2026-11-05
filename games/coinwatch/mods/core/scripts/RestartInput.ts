import { Component, Input, Type, field } from "@whatever-engine/api";
import { emit, run } from "./session.ts";
export class RestartInput extends Component {
  static readonly type = "core:RestartInput";
  session = field(Type.Nullable(Type.Entity), null);
  private held = false;
  override tick(): void {
    const pressed = Input.isPressed("KeyR");
    if ((pressed && !this.held) || (run(this.session)?.over && Input.wasMouseButtonPressed("Left")))
      emit(this.session, { type: "restart" });
    this.held = pressed;
  }
}
