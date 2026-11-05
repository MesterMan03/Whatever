import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterBob extends Component {
  static readonly type = "whatever_starter:StarterBob";
  amplitude = field(Type.F64, 0.3);
  frequency = field(Type.F64, 1);
  private elapsed = 0;
  private origin = 0;
  override start(): void {
    this.origin = this.entity.getComponent("core:transform")?.position[1] ?? 0;
  }
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    this.entity.editComponent("core:transform", (pose) => {
      pose.position[1] = this.origin + Math.sin(this.elapsed * this.frequency * Math.PI * 2) * this.amplitude;
    });
  }
}
