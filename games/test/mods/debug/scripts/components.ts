import { Component, Type, field } from "@whatever-engine/api";

/** Attach in the editor or spawn with `dev render component`. */
export class Spinner extends Component {
  static readonly type = "debug:spinner";

  speed = field(Type.F64.with({ min: -720, max: 720 }), 90, {
    label: "Rotation speed",
    tooltip: "Degrees per simulation second around the local Y axis.",
  });

  private angle = 0;

  override tick(deltaSeconds: number): void {
    this.angle += this.speed * deltaSeconds;
    const entity = this.entity;
    if (entity.hasComponent("core:transform")) entity.setEulerDegrees(0, this.angle, 0);
  }
}
