import { Component, Type, field } from "@whatever-engine/api";
import { onContact } from "./contacts.ts";
import { emit, id, playing } from "./session.ts";
export class ContactDamage extends Component {
  static readonly type = "core:ContactDamage";
  session = field(Type.Nullable(Type.Entity), null);
  target = field(Type.Nullable(Type.Entity), null);
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    const self = this.entity.id;
    this.unsubscribe = onContact((event) => {
      if (event.kind !== "started" || !playing(this.session)) return;
      const a = event.colliderA.id,
        b = event.colliderB.id,
        target = id(this.target);
      if ((a === self && b === target) || (b === self && a === target))
        emit(this.session, { type: "death" });
    });
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
