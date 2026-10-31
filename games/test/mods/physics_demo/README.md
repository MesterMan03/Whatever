# Physics Playground

Run `physicsdemo start` in the F2 console. The scene authors the floor, bean,
camera, lighting, a Gate parent with frame/door, a disabled Base Cube and a Cube
Spawn Point marker. Initial cubes and `physicsdemo cubes [count]` clone that
base template, enable the clones and vary placement, rotation, scale and mass.
Move the marker in the editor to move the spawning region; change the template's
mesh/material/body settings to change all future cubes. No cube instances are prebaked.

- WASD walks; Shift sprints; mouse orbits; R respawns; Escape releases the mouse.
- `physicsdemo start [seed]` makes procedural placement repeatable when a seed is given.
- `physicsdemo cubes [count]` drops more cubes (default 12, maximum 200).
- `physicsdemo stop` removes the scene and restores the previous camera.

The door owns its revolute joint and connects to the frame through Connected Body.
Frame colliders explicitly attach to its shared body. Physics dimensions remain
independent of renderer scale. All gameplay and spawning behavior belongs to this mod.
If core's initial_scene points to this playground, the mod adopts that preloaded
instance and starts automatically. Otherwise it remains opt-in through the console.

The authored scene and behavior also run in the headless server integration tests.
To regenerate the OBJ meshes/shaders, run `bun tools/generate-physics-demo.ts`.
