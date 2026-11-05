import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterOrbit extends Component {
  static readonly type = "whatever_starter:StarterOrbit";
  radius = field(Type.F64, 2);
  speed = field(Type.F64, 30);
  private angle = 0;
  private x = 0;
  private z = 0;
  override start(): void {
    const pose = this.entity.getComponent("core:transform");
    this.x = pose?.position[0] ?? 0;
    this.z = pose?.position[2] ?? 0;
  }
  override tick(): void {
    this.angle += this.speed * Time.deltaSeconds * Math.PI / 180;
    this.entity.editComponent("core:transform", (pose) => {
      pose.position[0] = this.x + Math.cos(this.angle) * this.radius;
      pose.position[2] = this.z + Math.sin(this.angle) * this.radius;
    });
  }
}
