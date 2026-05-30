/**
 * shader_test mod
 *
 * Verification mod for the custom shader + mesh renderer rework.
 * Registers a `spawnmesh` console command that lets you spawn any mesh
 * asset with any shader asset at a given world position.
 *
 * Usage from the dev console:
 *   spawnmesh <mesh> <shader> [x] [y] [z]
 *
 * Examples:
 *   spawnmesh quad.json         solid_red         0 0 -3
 *   spawnmesh triangle.json     uv_debug          2 0 -3
 *   spawnmesh cube.obj          checkerboard     -2 0 -3
 *   spawnmesh triangle.glb      tint_orange       0 0 -6
 *   spawnmesh quad.json         sprite            0 0 -3  ← default sprite shader
 *
 * Shader names are resolved as "shader_test://shaders/<name>.wgsl".
 * The special name "sprite" resolves to "core://shaders/sprite.wgsl".
 *
 * A second command `clearspawned` destroys all entities spawned by this mod.
 */

import { Engine, Scene, Console, BuiltInComponents } from "@whatever-engine/api";

// Track spawned entities so `clearspawned` can clean them up.
const spawned: { destroy(): void }[] = [];

Engine.on("init", () => {
  Engine.log("info", "[shader_test] mod loaded — use `spawnmesh` to test shaders");
});

function getShaderPath(shaderName: string): string {
  if(shaderName === "sprite") {
    return "core://shaders/sprite.wgsl";
  }
  if(shaderName === "mesh_lit") {
    return "core://shaders/mesh_lit.wgsl";
  }
  return `shader_test://shaders/${shaderName}.wgsl`;
}

// ---------------------------------------------------------------------------
// spawnmesh command
// ---------------------------------------------------------------------------

Console.register({
  name: "spawnmesh",
  description: "Spawn a mesh with a given shader for shader/mesh testing.",
  args: [
    {
      name: "mesh",
      type: "string",
      required: true,
      description:
        'Mesh filename inside shader_test://meshes/ (e.g. "quad.json", "triangle.glb")',
      suggest: (current) => {
        const meshes = ["quad.json", "triangle.json", "epicness.obj", "triangle.glb"];
        return meshes.filter((m) => m.startsWith(current));
      }
    },
    {
      name: "shader",
      type: "string",
      required: true,
      description:
        'Shader name inside shader_test://shaders/ without .wgsl extension, ' +
        'or "sprite" for the default sprite shader',
      suggest: (current) => {
        const shaders = [
          "solid_red",
          "uv_debug",
          "tint_orange",
          "checkerboard",
          "normal_map_debug",
          "sprite",
          "mesh_lit"
        ];
        return shaders.filter((s) => s.startsWith(current));
      }
    },
    {
      name: "x",
      type: "float",
      required: false,
      description: "World X position (default 0)",
    },
    {
      name: "y",
      type: "float",
      required: false,
      description: "World Y position (default 0)",
    },
    {
      name: "z",
      type: "float",
      required: false,
      description: "World Z position (default -3)",
    },
  ],
  handler: async (args) => {
    const meshName   = args["mesh"]   as string;
    const shaderName = args["shader"] as string;
    const x = (args["x"] as number | undefined) ?? 0;
    const y = (args["y"] as number | undefined) ?? 0;
    const z = (args["z"] as number | undefined) ?? -3;

    const meshPath =
      `shader_test://meshes/${meshName}`;

    const shaderPath = getShaderPath(shaderName);

    const entity = await Scene.spawnMesh(
      meshPath,
      shaderPath,
      [x, y, z],
    );

    spawned.push(entity);
    return (
      `Spawned mesh '${meshName}' with shader '${shaderName}' ` +
      `at (${x}, ${y}, ${z})  [entity ${entity.id}]`
    );
  },
});

// ---------------------------------------------------------------------------
// spawnsprite command — test custom shader on a sprite_renderer
// ---------------------------------------------------------------------------

Console.register({
  name: "spawnsprite_shader",
  description: "Spawn a sprite with a custom shader for comparison testing.",
  args: [
    {
      name: "texture",
      type: "string",
      required: true,
      description: "VFS texture path (e.g. asset_mod://humoros.png)",
    },
    {
      name: "shader",
      type: "string",
      required: true,
      description:
        'Shader name inside shader_test://shaders/ without .wgsl, or "sprite"',
    },
  ],
  handler: async (args) => {
    const texturePath = args["texture"] as string;
    const shaderName  = args["shader"]  as string;
    const shaderPath = getShaderPath(shaderName);

    const entity = await Scene.createEntity();
    entity.setComponent("core:transform", {
      position: [0, 0, -3],
      rotation: [0, 0, 0, 1],
      scale: [2, 1, 2],
    });
    entity.setComponent(
      new BuiltInComponents.SpriteRenderer({ texture: texturePath, shader: shaderPath })
    );

    spawned.push(entity);
    return (
      `Spawned sprite with texture '${texturePath}' and shader '${shaderName}' ` +
      `[entity ${entity.id}]`
    );
  },
});

// ---------------------------------------------------------------------------
// spawnlight command
// ---------------------------------------------------------------------------

Console.register({
  name: "spawnlight",
  description: "Spawn a light source for testing.",
  subcommands: [
    {
      name: "ambient",
      description: "Spawn a uniform ambient fill light.",
      args: [
        { name: "r", type: "float", required: true,  description: "Red channel (0–1)" },
        { name: "g", type: "float", required: true,  description: "Green channel (0–1)" },
        { name: "b", type: "float", required: true,  description: "Blue channel (0–1)" },
        { name: "intensity", type: "float", required: false, description: "Intensity multiplier (default 0.1)" },
      ],
      handler: async (args) => {
        const color: [number, number, number] = [args["r"] as number, args["g"] as number, args["b"] as number];
        const intensity = (args["intensity"] as number | undefined) ?? 0.1;
        const entity = await Scene.addAmbientLight(color, intensity);
        spawned.push(entity);
        return `Spawned ambient light color=(${color}) intensity=${intensity} [entity ${entity.id}]`;
      },
    },
    {
      name: "dir",
      description: "Spawn a directional light (infinitely distant, like the sun).",
      args: [
        { name: "dx", type: "float", required: true,  description: "Direction X" },
        { name: "dy", type: "float", required: true,  description: "Direction Y" },
        { name: "dz", type: "float", required: true,  description: "Direction Z" },
        { name: "r",  type: "float", required: false, description: "Red channel (0–1, default 1)" },
        { name: "g",  type: "float", required: false, description: "Green channel (0–1, default 1)" },
        { name: "b",  type: "float", required: false, description: "Blue channel (0–1, default 1)" },
        { name: "intensity", type: "float", required: false, description: "Intensity multiplier (default 1)" },
      ],
      handler: async (args) => {
        const direction: [number, number, number] = [args["dx"] as number, args["dy"] as number, args["dz"] as number];
        const color: [number, number, number] = [
          (args["r"] as number | undefined) ?? 1,
          (args["g"] as number | undefined) ?? 1,
          (args["b"] as number | undefined) ?? 1,
        ];
        const intensity = (args["intensity"] as number | undefined) ?? 1.0;
        const entity = await Scene.addDirectionalLight(direction, color, intensity);
        spawned.push(entity);
        return `Spawned directional light dir=(${direction}) color=(${color}) intensity=${intensity} [entity ${entity.id}]`;
      },
    },
    {
      name: "point",
      description: "Spawn a point light at a world position.",
      args: [
        { name: "x", type: "float", required: true,  description: "World X position" },
        { name: "y", type: "float", required: true,  description: "World Y position" },
        { name: "z", type: "float", required: true,  description: "World Z position" },
        { name: "r", type: "float", required: false, description: "Red channel (0–1, default 1)" },
        { name: "g", type: "float", required: false, description: "Green channel (0–1, default 1)" },
        { name: "b", type: "float", required: false, description: "Blue channel (0–1, default 1)" },
        { name: "intensity", type: "float", required: false, description: "Intensity multiplier (default 1)" },
        { name: "range",     type: "float", required: false, description: "Attenuation radius in world units (default 10)" },
      ],
      handler: async (args) => {
        const position: [number, number, number] = [args["x"] as number, args["y"] as number, args["z"] as number];
        const color: [number, number, number] = [
          (args["r"] as number | undefined) ?? 1,
          (args["g"] as number | undefined) ?? 1,
          (args["b"] as number | undefined) ?? 1,
        ];
        const intensity = (args["intensity"] as number | undefined) ?? 1.0;
        const range     = (args["range"]     as number | undefined) ?? 10;
        const entity = await Scene.addPointLight(position, color, intensity, range);
        spawned.push(entity);
        return `Spawned point light at (${position}) color=(${color}) intensity=${intensity} range=${range} [entity ${entity.id}]`;
      },
    },
  ],
});

// ---------------------------------------------------------------------------
// clearspawned command
// ---------------------------------------------------------------------------

Console.register({
  name: "clearspawned",
  description: "Destroy all entities spawned by shader_test.",
  args: [],
  handler: async () => {
    const count = spawned.length;
    for (const e of spawned) {
      e.destroy();
    }
    spawned.length = 0;
    return `Destroyed ${count} spawned entity/entities.`;
  },
});
