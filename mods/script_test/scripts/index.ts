import { engine } from "@whatever/api";

const randomText= Math.random().toString(36).substring(2);

engine.on("init", async () => {
  await engine.writeFile("test.txt", randomText);
  const content = await engine.readFile("test.txt");
  engine.log("info", `Content of test.txt: ${content}`);
  engine.setWindowTitle("script-mod: " + randomText);
});

engine.on("exit", () => {
  engine.log("info", "Goodbye, cruel world");
});
