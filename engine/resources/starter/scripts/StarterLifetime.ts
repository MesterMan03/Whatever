import { Component, Time, Type, field } from "@whatever-engine/api";

export class StarterLifetime extends Component {
  static readonly type = "whatever_starter:StarterLifetime";
  seconds = field(Type.F64, 3);
  private elapsed = 0;
  override tick(): void {
    this.elapsed += Time.deltaSeconds;
    if (this.elapsed >= this.seconds) this.entity.destroy();
  }
}
