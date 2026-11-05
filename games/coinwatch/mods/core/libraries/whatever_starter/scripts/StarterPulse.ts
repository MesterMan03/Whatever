import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterPulse extends Component {
  static readonly type = "core:whatever_starter_StarterPulse";
  amplitude = field(Type.F64, 0.15);
  frequency = field(Type.F64, 1);
  private elapsed = 0;
  private base: [number, number, number] = [1, 1, 1];
  override start(): void {
    const scale = this.entity.getComponent("core:transform")?.scale;
    if (scale) this.base = [...scale];
  }
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    const factor = 1 + Math.sin(this.elapsed * this.frequency * Math.PI * 2) * this.amplitude;
    this.entity.editComponent("core:transform", (pose) => {
      pose.scale = [this.base[0] * factor, this.base[1] * factor, this.base[2] * factor];
    });
  }
}
