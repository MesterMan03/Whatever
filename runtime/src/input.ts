/**
 * Physical keyboard codes emitted by the engine (winit 0.30 KeyCode names).
 * These describe key positions, independent of the active keyboard layout.
 */
export type KeyCode =
  | "Backquote"
  | "Backslash"
  | "BracketLeft"
  | "BracketRight"
  | "Comma"
  | "Digit0"
  | "Digit1"
  | "Digit2"
  | "Digit3"
  | "Digit4"
  | "Digit5"
  | "Digit6"
  | "Digit7"
  | "Digit8"
  | "Digit9"
  | "Equal"
  | "IntlBackslash"
  | "IntlRo"
  | "IntlYen"
  | "KeyA"
  | "KeyB"
  | "KeyC"
  | "KeyD"
  | "KeyE"
  | "KeyF"
  | "KeyG"
  | "KeyH"
  | "KeyI"
  | "KeyJ"
  | "KeyK"
  | "KeyL"
  | "KeyM"
  | "KeyN"
  | "KeyO"
  | "KeyP"
  | "KeyQ"
  | "KeyR"
  | "KeyS"
  | "KeyT"
  | "KeyU"
  | "KeyV"
  | "KeyW"
  | "KeyX"
  | "KeyY"
  | "KeyZ"
  | "Minus"
  | "Period"
  | "Quote"
  | "Semicolon"
  | "Slash"
  | "AltLeft"
  | "AltRight"
  | "Backspace"
  | "CapsLock"
  | "ContextMenu"
  | "ControlLeft"
  | "ControlRight"
  | "Enter"
  | "SuperLeft"
  | "SuperRight"
  | "ShiftLeft"
  | "ShiftRight"
  | "Space"
  | "Tab"
  | "Convert"
  | "KanaMode"
  | "Lang1"
  | "Lang2"
  | "Lang3"
  | "Lang4"
  | "Lang5"
  | "NonConvert"
  | "Delete"
  | "End"
  | "Help"
  | "Home"
  | "Insert"
  | "PageDown"
  | "PageUp"
  | "ArrowDown"
  | "ArrowLeft"
  | "ArrowRight"
  | "ArrowUp"
  | "NumLock"
  | "Numpad0"
  | "Numpad1"
  | "Numpad2"
  | "Numpad3"
  | "Numpad4"
  | "Numpad5"
  | "Numpad6"
  | "Numpad7"
  | "Numpad8"
  | "Numpad9"
  | "NumpadAdd"
  | "NumpadBackspace"
  | "NumpadClear"
  | "NumpadClearEntry"
  | "NumpadComma"
  | "NumpadDecimal"
  | "NumpadDivide"
  | "NumpadEnter"
  | "NumpadEqual"
  | "NumpadHash"
  | "NumpadMemoryAdd"
  | "NumpadMemoryClear"
  | "NumpadMemoryRecall"
  | "NumpadMemoryStore"
  | "NumpadMemorySubtract"
  | "NumpadMultiply"
  | "NumpadParenLeft"
  | "NumpadParenRight"
  | "NumpadStar"
  | "NumpadSubtract"
  | "Escape"
  | "Fn"
  | "FnLock"
  | "PrintScreen"
  | "ScrollLock"
  | "Pause"
  | "BrowserBack"
  | "BrowserFavorites"
  | "BrowserForward"
  | "BrowserHome"
  | "BrowserRefresh"
  | "BrowserSearch"
  | "BrowserStop"
  | "Eject"
  | "LaunchApp1"
  | "LaunchApp2"
  | "LaunchMail"
  | "MediaPlayPause"
  | "MediaSelect"
  | "MediaStop"
  | "MediaTrackNext"
  | "MediaTrackPrevious"
  | "Power"
  | "Sleep"
  | "AudioVolumeDown"
  | "AudioVolumeMute"
  | "AudioVolumeUp"
  | "WakeUp"
  | "Meta"
  | "Hyper"
  | "Turbo"
  | "Abort"
  | "Resume"
  | "Suspend"
  | "Again"
  | "Copy"
  | "Cut"
  | "Find"
  | "Open"
  | "Paste"
  | "Props"
  | "Select"
  | "Undo"
  | "Hiragana"
  | "Katakana"
  | "F1"
  | "F2"
  | "F3"
  | "F4"
  | "F5"
  | "F6"
  | "F7"
  | "F8"
  | "F9"
  | "F10"
  | "F11"
  | "F12"
  | "F13"
  | "F14"
  | "F15"
  | "F16"
  | "F17"
  | "F18"
  | "F19"
  | "F20"
  | "F21"
  | "F22"
  | "F23"
  | "F24"
  | "F25"
  | "F26"
  | "F27"
  | "F28"
  | "F29"
  | "F30"
  | "F31"
  | "F32"
  | "F33"
  | "F34"
  | "F35";

/**
 * Immutable snapshot of the keys held at a delivered tick, not key-down edges.
 * Safe to retain across awaits; later ticks never change this snapshot.
 */
export class PressedKeys implements Iterable<KeyCode> {
  readonly #held: Set<KeyCode>;
  readonly #codes: readonly KeyCode[];

  constructor(keys: readonly KeyCode[] = []) {
    this.#held = new Set(keys);
    this.#codes = Object.freeze([...this.#held]);
    Object.freeze(this);
  }

  /** Number of distinct held keys. */
  get size(): number {
    return this.#held.size;
  }

  /** Whether this physical key is held. Key names support editor autocomplete. */
  isPressed(key: KeyCode): boolean {
    return this.#held.has(key);
  }

  /** Compatibility alias for existing `keys_pressed.includes(key)` checks. */
  includes(key: KeyCode): boolean {
    return this.isPressed(key);
  }

  /** Whether at least one supplied key is held; false with no arguments. */
  isAnyPressed(...keys: KeyCode[]): boolean {
    return keys.some((key) => this.isPressed(key));
  }

  /** Whether every supplied key is held; true with no arguments. */
  areAllPressed(...keys: KeyCode[]): boolean {
    return keys.every((key) => this.isPressed(key));
  }

  /** Whether either Shift key is held. */
  isShiftPressed(): boolean {
    return this.isAnyPressed("ShiftLeft", "ShiftRight");
  }

  /** Whether either Control key is held. */
  isControlPressed(): boolean {
    return this.isAnyPressed("ControlLeft", "ControlRight");
  }

  /** Whether either Alt key is held (including the physical AltGr position). */
  isAltPressed(): boolean {
    return this.isAnyPressed("AltLeft", "AltRight");
  }

  /** Whether either Super key (Windows / Command) is held. */
  isSuperPressed(): boolean {
    return this.isAnyPressed("SuperLeft", "SuperRight");
  }

  /** Read-only array of held key codes; use a spread to make a mutable copy. */
  toArray(): readonly KeyCode[] {
    return this.#codes;
  }

  /** Serialize as an array of key codes. */
  toJSON(): readonly KeyCode[] {
    return this.#codes;
  }

  [Symbol.iterator](): IterableIterator<KeyCode> {
    return this.#codes[Symbol.iterator]();
  }
}

let currentKeys = new PressedKeys();
let currentMouseDelta: readonly [number, number] = Object.freeze([0, 0]);
let currentMouseCaptured = false;

/** @internal Refresh before any component lifecycle hooks or public tick handlers. */
export function _updateInput(
  keys: readonly KeyCode[],
  mouseDelta: readonly [number, number],
  mouseCaptured: boolean,
): void {
  currentKeys = new PressedKeys(keys);
  currentMouseDelta = Object.freeze(mouseCaptured ? [mouseDelta[0], mouseDelta[1]] : [0, 0]);
  currentMouseCaptured = mouseCaptured;
}

/** Read-only input for the latest engine tick, available throughout the mod. */
export const Input = Object.freeze({
  /** Immutable held-key snapshot; retain this value to preserve input across awaits. */
  get keysPressed(): PressedKeys {
    return currentKeys;
  },
  /** Relative motion for this tick only, in pixels; zero while uncaptured. */
  get mouseDelta(): readonly [number, number] {
    return currentMouseDelta;
  },
  /** Whether the player owns mouse capture for this tick. */
  get mouseCaptured(): boolean {
    return currentMouseCaptured;
  },
  /** Whether this physical key is held in the latest tick. */
  isPressed(key: KeyCode): boolean {
    return currentKeys.isPressed(key);
  },
  /** Whether any supplied key is held; false with no arguments. */
  isAnyPressed(...keys: KeyCode[]): boolean {
    return currentKeys.isAnyPressed(...keys);
  },
  /** Whether every supplied key is held; true with no arguments. */
  areAllPressed(...keys: KeyCode[]): boolean {
    return currentKeys.areAllPressed(...keys);
  },
  /** Whether either Shift key is held. */
  isShiftPressed(): boolean {
    return currentKeys.isShiftPressed();
  },
  /** Whether either Control key is held. */
  isControlPressed(): boolean {
    return currentKeys.isControlPressed();
  },
  /** Whether either Alt key is held (including the physical AltGr position). */
  isAltPressed(): boolean {
    return currentKeys.isAltPressed();
  },
  /** Whether either Super key (Windows / Command) is held. */
  isSuperPressed(): boolean {
    return currentKeys.isSuperPressed();
  },
});
