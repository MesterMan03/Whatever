import {Engine, Window, File, Mods, Message, Console, Entity, Scene, BuiltInComponents} from "@whatever-engine/api";

const randomText = Math.random().toString(36).substring(2);

Engine.on("init", async ({ mod_id: selfId }) => {
  await File.write("test.txt", randomText);
  const content = await File.read("test.txt");
  Engine.log("info", `Content of test.txt: ${content}`);
  Window.setTitle("script-mod: " + randomText);

  const allMods = await Mods.list();
  for(const mod of allMods.filter(x => x.id !== selfId)) {
    Message.send(mod.id, { type: "test", data: "hi" }, 1000).then(data => {
      Engine.log("info", `We got a reply from ${mod.id}: ${JSON.stringify(data)}`)
    }).catch(() => {
      Engine.log("warn", `Aww, no reply from ${mod.id} :(`);
    })
  }
});

Engine.on("exit", () => {
  Engine.log("info", "Goodbye, cruel world");
});

Console.register({
  name: "window",
  description: "Change window parameters (size, fullscreen)",
  subcommands: [{
    name: "resize",
    description: "Resize the window (format: WIDTHxHEIGHT)",
    args: [{
      name: "size",
      description: "New size of the window",
      type: "string",
      required: true
    }],
    handler: (args) => {
      const size = args["size"] as string;
      const parts = size.split("x");
      if(parts.length !== 2) {
        return "incorrect format";
      }
      const widthStr = parts[0];
      const heightStr = parts[1];
      if(widthStr == null || heightStr == null) {
        return "incorrect format";
      }
      let width: number, height: number;
      try {
        width = parseInt(widthStr, 10);
        height = parseInt(heightStr, 10);
      } catch {
        return "invalid numbers";
      }
      Window.setSize(width, height);
      return `set window size to ${width}x${height}`;
    }
  }, {
    name: "mode",
    description: "Set window mode",
    subcommands: [{
      name: "windowed",
      handler: (_) => {
        Window.setMode("windowed");
        return "ok";
      }
    }, {
      name: "borderless",
      handler: (_) => {
        Window.setMode("borderless");
        return "ok";
      }
    }, {
      name: "fullscreen",
      handler: (_) => {
        Window.setMode("fullscreen");
        return "ok";
      }
    }]
  }]
});

Console.register({
  name: "stupidsuggest",
  description: "Yes",
  args: [{
    name: "stupid",
    type: "string",
    required: true,
    suggest: (current) => {
      const hasher = new Bun.SHA256()
      hasher.update(current);
      const hash = hasher.digest("hex");
      // split hash into 8 character chunks
      return hash.split(/(.{1,8})/g).filter(x => x).slice(0, 10);
    }
  }],
  handler: (_) => "this literally does nothing lmao"
});

let textEntity: Entity | null = null;
Console.register({
  name: "showtext",
  args: [{
    name: "text",
    type: "string",
    required: true,
    suggest: async (_) => {
      const component = textEntity ? await textEntity.getComponent("core:text_renderer") : null;
      if(component == null) {
        return [];
      }
      return [`"${component.getText()}"`];
    }
  }],
  handler: async (args) => {
    const text = args["text"] as string;
    if(textEntity != null) {
      const component = await textEntity.getComponent("core:text_renderer");
      if(component != null) {
        component.setText(text);
        textEntity.setComponent("core:text_renderer", component);
        return "updated text";
      } else {
        const newComponent = new BuiltInComponents.TextRenderer({ text, shader: "core://shaders/sprite.wgsl", font: "core://fonts/default.ttf", font_size: 50, color: [0.5, 0.2, 0.2, 0.8] });
        textEntity.setComponent("core:text_renderer", newComponent);
        return "added text component";
      }
    }
    textEntity = await Scene.spawnText(text, [5, 2, 0], {
      shader: "core://shaders/sprite.wgsl",
      font: "core://fonts/default.ttf",
      font_size: 50,
      color: [0.5,0.2,0.2,0.8]
    });
    const transform = await textEntity.getComponent("core:transform");
    if(transform) {
      transform.rotateZ(-15);
      textEntity.setComponent(transform);
    }
    return "spawned text";
  }
});