import {
  BuiltInComponents,
  Scene,
  Window,
  type CommandSpec,
  type Entity,
} from "@whatever-engine/api";
let textEntity: Entity | undefined;
export const presentationCommands: CommandSpec[] = [
  {
    name: "window",
    description: "Window configuration probes",
    subcommands: [
      {
        name: "resize",
        args: [
          { name: "width", type: "int", required: true },
          { name: "height", type: "int", required: true },
        ],
        handler: ({ width, height }) => {
          if ((width as number) < 1 || (height as number) < 1)
            throw new Error("Dimensions must be positive");
          Window.setSize(width as number, height as number);
          return `Window size ${width}×${height}`;
        },
      },
      {
        name: "mode",
        subcommands: (["windowed", "borderless", "fullscreen"] as const).map((mode) => ({
          name: mode,
          handler: () => {
            Window.setMode(mode);
            return mode;
          },
        })),
      },
      {
        name: "title",
        args: [{ name: "title", type: "string", required: true }],
        handler: ({ title }) => {
          Window.setTitle(title as string);
          return "Title updated";
        },
      },
    ],
  },
  {
    name: "suggest",
    description: "Deterministic custom autocomplete callback probe",
    args: [
      {
        name: "value",
        type: "string",
        required: true,
        suggest: (current) => {
          let hash = 2166136261;
          for (const char of current) hash = Math.imul(hash ^ char.charCodeAt(0), 16777619);
          return [(hash >>> 0).toString(16).padStart(8, "0")];
        },
      },
    ],
    handler: ({ value }) => `Suggestion accepted: ${value}`,
  },
  {
    name: "text",
    description: "Spawn or update the text rendering probe",
    args: [
      {
        name: "text",
        type: "string",
        required: true,
        suggest: async () => {
          const text = textEntity
            ? await textEntity.getComponent("core:text_renderer").get()
            : null;
          return text ? [`"${text.getText()}"`] : [];
        },
      },
    ],
    handler: async ({ text }) => {
      const component = textEntity
        ? await textEntity.getComponent("core:text_renderer").get()
        : null;
      if (component && textEntity) {
        component.setText(text as string);
        textEntity.setComponent(component);
        return "Text updated";
      }
      textEntity = await Scene.spawnText(text as string, [5, 2, 0], {
        shader: "core://shaders/sprite.wgsl",
        font: "core://fonts/default.ttf",
        font_size: 50,
        color: [0.5, 0.2, 0.2, 0.8],
      }).get();
      const transform = await textEntity.getComponent("core:transform").get();
      if (transform) {
        transform.rotateZ(-15);
        textEntity.setComponent(transform);
      }
      return `Spawned text ${textEntity.id}`;
    },
  },
];
