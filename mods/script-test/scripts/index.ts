import { engine } from "@whatever/api";

engine.on("init", () => {
  engine.setWindowTitle("script-mod: Hello, World!");
});

engine.on("exit", () => {
  engine.log("info", "Goodbye, cruel world");
});
