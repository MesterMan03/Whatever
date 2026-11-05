import { Audio, Component, Engine, Type, field } from "@whatever-engine/api";
import { library, listen } from "./session.ts";
export class SoundEffects extends Component {
  static readonly type = "core:SoundEffects";
  session = field(Type.Nullable(Type.Entity), null);
  enabled = true;
  volume = field(Type.F64.with({ min: 0, max: 1 }), 0.3);
  private unsubscribe: (() => void) | null = null;
  override start(): void {
    this.unsubscribe = listen(
      () => this.session,
      (event) => {
        if (!this.enabled || Engine.mode !== "client") return;
        const path =
          event.type === "award"
            ? event.kind === "coin"
              ? "coin"
              : "hit"
            : event.type === "fired"
              ? "laser"
              : event.type === "death"
                ? "error"
                : null;
        if (path)
          void Audio.load({
            path: `${library}audio/${path}.wav`,
            play: true,
            volume: this.volume * (path === "laser" ? 0.6 : 1),
          }).catch((error) => Engine.log("warn", `Game audio: ${String(error)}`));
      },
    );
  }
  override destroy(): void {
    this.unsubscribe?.();
  }
}
