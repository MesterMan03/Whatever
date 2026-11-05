import { Component, Engine, Type, Window, field } from "@whatever-engine/api";
export class GameWindow extends Component {
  static readonly type = "core:GameWindow";
  width = field(Type.I32.with({ min: 640, max: 7680 }), 1280);
  height = field(Type.I32.with({ min: 480, max: 4320 }), 720);
  override start(): void {
    if (Engine.mode === "client") Window.setSize(this.width, this.height);
  }
}
