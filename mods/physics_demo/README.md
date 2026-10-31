# Rapier Physics Playground

Run the client, press **F2**, and enter `physicsdemo start`. Close the console and click
the game window to capture the mouse. The mod registers its commands at startup and
loads `scenes/playground.wscene` only when activated. Open that file in the editor
to change the floor, bean, camera hierarchy, lighting, gate, colliders and initial cubes.

- **WASD:** walk relative to the camera; push cubes and the hinged gate by walking into them.
- **Shift:** sprint.
- **Mouse:** orbit the third-person camera.
- **R:** respawn the bean if you walk off the floor.
- **Escape:** release the mouse.
- `physicsdemo cubes [count]`: drop more cubes (default 12, maximum 200 per call).
- `physicsdemo stop`: remove the scene and restore the previous camera.
- `physicsdemo start`: use the authored layout, including your editor changes.
- `physicsdemo start <seed>`: explicitly rearrange the cubes for repeatable testing.

The scene owns a physics-world entity, a 60 × 60 floor, a dynamic capsule player,
24 authored cubes, and a gate constrained by a revolute joint. The frame demonstrates
three collider entities attached to one body. The camera and visible bean are children
of the player entity; the native solver updates their parent through ECS.

The authored entities use stable scene script IDs and explicit physics-world/body
references. `scripts/scene.ts` awaits `Scene.loadScene`; it only applies cube
variation when a seed is explicitly supplied and creates additional cubes requested
by the demo command. Static scene
setup belongs in the scene file.

All gameplay and camera behavior lives in this mod. Movement sets only horizontal
velocity, preserving gravity and vertical collision response. Physics shapes use
explicit dimensions matching the visual meshes; renderer scale does not resize colliders.

`scripts/scene.ts` also runs headlessly in the server integration tests. To regenerate
the tracked OBJ geometry and shader assets, run `bun tools/generate-physics-demo.ts`.
