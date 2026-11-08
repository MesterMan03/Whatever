import type { RunSession } from "./RunSession.ts";
import {
  BuiltInComponents as C,
  Component,
  Engine,
  Input,
  Type,
  field,
} from "@whatever-engine/api";
import { clamp } from "./navigation.ts";
import { listen, playing, resolve } from "./session.ts";
export class MouseLook extends Component {
  static readonly type = "core:MouseLook";
  session: RunSession | null = null;
  camera = field(Type.Nullable(Type.Entity), null);
  sensitivity = field(Type.F64.with({ min: 0.0001, max: 0.02 }), 0.002);
  pitchLimit = field(Type.F64.with({ min: 0.1, max: 1.56 }), 1.45);
  private yaw = 0;
  private pitch = 0;
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    const camera = resolve(this.camera);
    if (Engine.mode === "client" && camera) Engine.setMainCamera(camera);
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (event.type === "restart" || event.type === "death") {
          this.yaw = 0;
          this.pitch = event.type === "death" ? 0.12 : 0;
          this.apply();
        }
      },
    );
  }
  private apply(): void {
    this.entity.editComponent("core:transform", (t) => {
      t.setEulerRadians(0, this.yaw, 0);
    });
    resolve(this.camera)?.editComponent("core:transform", (t) => {
      t.setEulerRadians(this.pitch, 0, 0);
    });
  }
  override tick(): void {
    if (!playing(this.session) || !Input.mouseCaptured) return;
    this.yaw -= Input.mouseDelta[0] * this.sensitivity;
    this.pitch = clamp(
      this.pitch - Input.mouseDelta[1] * this.sensitivity,
      -this.pitchLimit,
      this.pitchLimit,
    );
    this.apply();
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
