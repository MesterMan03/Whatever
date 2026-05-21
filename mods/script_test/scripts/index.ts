import {Engine, Window, File, Mods, Message} from "@whatever-engine/api";

const randomText = Math.random().toString(36).substring(2);

Engine.on("init", async () => {
  await File.write("test.txt", randomText);
  const content = await File.read("test.txt");
  Engine.log("info", `Content of test.txt: ${content}`);
  Window.setTitle("script-mod: " + randomText);

  const allMods = await Mods.list();
  for(const mod of allMods) {
    Message.send(mod.id, { type: "test", data: "hi" }, 1000).then(data => {
      Engine.log("info", `We got a reply from ${mod.id}: ${JSON.stringify(data)}`)
    })
  }
});

Engine.on("exit", () => {
  Engine.log("info", "Goodbye, cruel world");
});