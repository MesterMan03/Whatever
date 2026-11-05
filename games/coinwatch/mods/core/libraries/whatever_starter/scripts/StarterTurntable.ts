import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterTurntable extends Component {
  static readonly type = "core:whatever_starter_StarterTurntable";
  speed = field(Type.F64, 20);
  tilt = field(Type.F64, 12);
  private angle = 0;
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds;
    this.entity.setEulerDegrees(this.tilt, this.angle, 0);
  }
}
