import { expect, test } from "bun:test";
import { Input, PressedKeys, _updateInput, type KeyCode } from "../../runtime/src/input.ts";
import { Time, _updateTime } from "../../runtime/src/time.ts";

test("held keys support typed queries, alternatives, chords and existing includes checks", () => {
  const keys = new PressedKeys(["KeyW", "ShiftRight", "NumpadEnter"]);
  expect(keys.isPressed("KeyW")).toBe(true);
  expect(keys.includes("KeyW")).toBe(true);
  expect(keys.isPressed("KeyS")).toBe(false);
  expect(keys.includes("Enter")).toBe(false);
  expect(keys.isAnyPressed("ArrowUp", "KeyW")).toBe(true);
  expect(keys.isAnyPressed("ArrowDown", "KeyS")).toBe(false);
  expect(keys.areAllPressed("KeyW", "ShiftRight")).toBe(true);
  expect(keys.areAllPressed("KeyW", "ShiftLeft")).toBe(false);
  expect(keys.isAnyPressed()).toBe(false);
  expect(keys.areAllPressed()).toBe(true);
});

test.each([
  ["ShiftLeft", "ShiftRight", "isShiftPressed"],
  ["ControlLeft", "ControlRight", "isControlPressed"],
  ["AltLeft", "AltRight", "isAltPressed"],
  ["SuperLeft", "SuperRight", "isSuperPressed"],
] as const)("%s and %s both satisfy %s", (left, right, method) => {
  expect(new PressedKeys([left])[method]()).toBe(true);
  expect(new PressedKeys([right])[method]()).toBe(true);
  expect(new PressedKeys([left, right])[method]()).toBe(true);
  expect(new PressedKeys(["KeyW"])[method]()).toBe(false);
});

test("snapshots own immutable, iterable and serializable key data", () => {
  const source: KeyCode[] = ["KeyW", "ShiftRight", "KeyW"];
  const keys = new PressedKeys(source);
  source.splice(0, source.length, "KeyS");
  expect(keys.size).toBe(2);
  expect([...keys]).toEqual(["KeyW", "ShiftRight"]);
  expect(JSON.stringify(keys)).toBe('["KeyW","ShiftRight"]');
  expect(Object.isFrozen(keys)).toBe(true);
  expect(() => (keys.toArray() as KeyCode[]).push("KeyS")).toThrow(TypeError);
  const copy = [...keys];
  copy.push("KeyS");
  expect(keys.isPressed("KeyS")).toBe(false);
  expect(keys.isPressed("KeyW")).toBe(true);
});

test("empty snapshots expose no held keys or modifiers", () => {
  const keys = new PressedKeys();
  expect(keys.size).toBe(0);
  expect([...keys]).toEqual([]);
  expect(keys.isPressed("Space")).toBe(false);
  expect(keys.isShiftPressed()).toBe(false);
  expect(keys.isControlPressed()).toBe(false);
  expect(keys.isAltPressed()).toBe(false);
  expect(keys.isSuperPressed()).toBe(false);
});

test("shared input and timing expose read-only current-tick values and retain old snapshots", () => {
  expect(Time.tickNumber).toBe(0);
  expect(Time.deltaSeconds).toBe(0);
  expect(Input.keysPressed.size).toBe(0);
  expect(Input.mouseDelta).toEqual([0, 0]);
  expect(Input.mouseCaptured).toBe(false);

  const motion: [number, number] = [3, -2];
  _updateTime(42, 0.02);
  _updateInput(["KeyW", "ShiftRight", "ControlLeft", "AltRight", "SuperLeft"], motion, true);
  expect(Time.tickNumber).toBe(42);
  expect(Time.deltaSeconds).toBe(0.02);
  expect(Input.isPressed("KeyW")).toBe(true);
  expect(Input.isAnyPressed("ArrowUp", "KeyW")).toBe(true);
  expect(Input.areAllPressed("KeyW", "ShiftRight")).toBe(true);
  expect(Input.isShiftPressed()).toBe(true);
  expect(Input.isControlPressed()).toBe(true);
  expect(Input.isAltPressed()).toBe(true);
  expect(Input.isSuperPressed()).toBe(true);
  expect(Input.mouseCaptured).toBe(true);
  const previousKeys = Input.keysPressed;
  const previousMouse = Input.mouseDelta;
  motion[0] = 999;
  expect(Input.mouseDelta).toEqual([3, -2]);
  expect(Reflect.set(Input, "mouseCaptured", false)).toBe(false);
  expect(Reflect.set(Input, "keysPressed", new PressedKeys())).toBe(false);
  expect(Reflect.set(Input, "isPressed", () => false)).toBe(false);
  expect(Reflect.set(Time, "deltaSeconds", 999)).toBe(false);
  expect(Reflect.set(Time, "tickNumber", 999)).toBe(false);
  expect(Reflect.set(Input.mouseDelta, "0", 999)).toBe(false);

  _updateTime(43, 0.04);
  _updateInput(["KeyS"], [99, 88], false);
  expect(Time.tickNumber).toBe(43);
  expect(Time.deltaSeconds).toBe(0.04);
  expect(Input.isPressed("KeyS")).toBe(true);
  expect(Input.isPressed("KeyW")).toBe(false);
  expect(Input.isShiftPressed()).toBe(false);
  expect(Input.mouseCaptured).toBe(false);
  expect(Input.mouseDelta).toEqual([0, 0]);
  expect(previousKeys.isPressed("KeyW")).toBe(true);
  expect(previousKeys.isPressed("KeyS")).toBe(false);
  expect(previousMouse).toEqual([3, -2]);

  _updateTime(44, 0.01);
  _updateInput([], [1, 2], true);
  expect(Input.keysPressed.size).toBe(0);
  expect(Input.mouseDelta).toEqual([1, 2]);
});
