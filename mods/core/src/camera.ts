import { BuiltInComponents, Engine, Entity, Scene } from "@whatever-engine/api";
import Camera = BuiltInComponents.Camera;

// --- Camera controller state -------------------------------------------------

/** Horizontal rotation (radians). 0 = looking toward −Z. */
let yaw = 0;
/** Vertical rotation (radians). Clamped to (−1.5, 1.5) to avoid gimbal flip. */
let pitch = -0.3;

const MOUSE_SENSITIVITY = 0.002;
const MOVE_SPEED = 5.0; // world units per second

// --- Scene entities ----------------------------------------------------------

let character: Entity;
let cameraEntity: Entity;

Engine.on("init", async () => {
    character = await Scene.spawnSprite("asset_mod://humoros.png", [0, 0, 0], [1, 1, 1]);
    cameraEntity = await Scene.createEntity();
    await cameraEntity.move([0, 0, 0]);
    cameraEntity.setComponent(new Camera());
    cameraEntity.setParent(character);
    Engine.setMainCamera(cameraEntity.id);
});

// --- Tick: camera controller -------------------------------------------------

Engine.on("tick", async ({ mouse_delta, keys_pressed, delta_seconds }) => {
    // Entities aren't ready until after init resolves.
    if (!character || !cameraEntity) return;

    const [dx, dy] = mouse_delta;

    // Update look angles from mouse delta.
    // dx > 0 = mouse moved right → camera turns right → yaw decreases
    // (positive yaw = counter-clockwise = left; subtract to go right).
    yaw   -= dx * MOUSE_SENSITIVITY;
    pitch  = Math.max(-1.5, Math.min(1.5, pitch - dy * MOUSE_SENSITIVITY));

    // ---- Character: movement + yaw ------------------------------------------

    const charTransform = await character.getComponent("core:transform");
    if (!charTransform) return;

    // World-space forward/right vectors derived from yaw.
    // With yaw=0 the camera looks toward −Z, so forward is (0, 0, −1) and
    // right is (+1, 0, 0).  General case:
    //   forward = (−sin(yaw), 0, −cos(yaw))
    //   right   = cross(forward, Y) = (cos(yaw), 0, −sin(yaw))
    const fwdX =  -Math.sin(yaw), fwdZ = -Math.cos(yaw);
    const rgtX =   Math.cos(yaw), rgtZ = -Math.sin(yaw);

    let moveX = 0, moveY = 0, moveZ = 0;

    if (keys_pressed.includes("KeyW"))     { moveX += fwdX; moveZ += fwdZ; }
    if (keys_pressed.includes("KeyS"))     { moveX -= fwdX; moveZ -= fwdZ; }
    if (keys_pressed.includes("KeyD"))     { moveX += rgtX; moveZ += rgtZ; }
    if (keys_pressed.includes("KeyA"))     { moveX -= rgtX; moveZ -= rgtZ; }
    if (keys_pressed.includes("Space"))    { moveY += 1; }
    if (keys_pressed.includes("ShiftLeft") ||
        keys_pressed.includes("ShiftRight")) { moveY -= 1; }

    // Normalise the horizontal component so diagonal movement isn't faster.
    const hLen = Math.sqrt(moveX * moveX + moveZ * moveZ);
    if (hLen > 1) { moveX /= hLen; moveZ /= hLen; }

    charTransform.position[0] += moveX * MOVE_SPEED * delta_seconds;
    charTransform.position[1] += moveY * MOVE_SPEED * delta_seconds;
    charTransform.position[2] += moveZ * MOVE_SPEED * delta_seconds;

    // Character yaw is the horizontal rotation; the camera inherits this
    // through the parent-child relationship.
    charTransform.setEulerRadians(0, yaw, 0);

    character.setComponent(charTransform);

    // ---- Camera: pitch (local X-axis rotation) ------------------------------

    // The camera entity is a child of the character, so it already inherits
    // the yaw.  We only need to apply the vertical pitch in local space.
    const camTransform = await cameraEntity.getComponent("core:transform")
        ?? new BuiltInComponents.Transform();
    camTransform.setEulerRadians(pitch, 0, 0);
    cameraEntity.setComponent(camTransform);
});
