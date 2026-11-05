import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterSpin extends Component {
  static readonly type = "core:whatever_starter_StarterSpin";
  speed = field(Type.F64, 60);
  private angle = 0;
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds;
    this.entity.setEulerDegrees(0, this.angle, 0);
  }
}
