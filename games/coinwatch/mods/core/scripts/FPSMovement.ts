import { Component, Input, Physics, Type, field } from "@whatever-engine/api";
import { playing, resolve } from "./session.ts";
export class FPSMovement extends Component {
  static readonly type = "core:FPSMovement";
  session = field(Type.Nullable(Type.Entity), null);
  look = field(Type.Nullable(Type.Entity), null);
  walkSpeed = field(Type.F64.with({ min: 0.1, max: 30 }), 5);
  sprintSpeed = field(Type.F64.with({ min: 0.1, max: 40 }), 8);
  override tick(): void {
    if (!playing(this.session) || !Input.mouseCaptured) {
      Physics.body(this.entity).setLinearVelocity([0, 0, 0], { axes: ["x", "z"] });
      return;
    }
    const q = resolve(this.look)?.getComponent("core:world_transform")?.rotation ?? [0, 0, 0, 1];
    const sin = 2 * (q[3] * q[1] + q[0] * q[2]),
      cos = 1 - 2 * (q[1] ** 2 + q[2] ** 2);
    const forward =
      Number(Input.isAnyPressed("KeyW", "ArrowUp")) -
      Number(Input.isAnyPressed("KeyS", "ArrowDown"));
    const right =
      Number(Input.isAnyPressed("KeyD", "ArrowRight")) -
      Number(Input.isAnyPressed("KeyA", "ArrowLeft"));
    const length = Math.hypot(forward, right) || 1;
    const speed = Input.isShiftPressed() ? this.sprintSpeed : this.walkSpeed;
    Physics.body(this.entity).setLinearVelocity(
      [
        ((-sin * forward + cos * right) / length) * speed,
        0,
        ((-cos * forward - sin * right) / length) * speed,
      ],
      { axes: ["x", "z"] },
    );
  }
}
