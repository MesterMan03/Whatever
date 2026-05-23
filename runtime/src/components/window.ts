import { _send } from "../shared.ts";

export type WindowMode = "windowed" | "borderless" | "fullscreen";

/** Window management. */
export const Window = {
  /** Set the title of the active window. */
  setTitle(title: string): void {
    _send({ type: "SetWindowTitle", title });
  },

  /** Request a new inner window size in physical pixels. Has no effect in fullscreen modes. */
  setSize(width: number, height: number): void {
    _send({ type: "SetWindowSize", width, height });
  },

  /**
   * Set the window display mode.
   * - `"windowed"` — normal window
   * - `"borderless"` — borderless fullscreen (windowed fullscreen)
   * - `"fullscreen"` — exclusive fullscreen using the monitor's preferred video mode;
   *   falls back to borderless if no exclusive mode is available
   */
  setMode(mode: WindowMode): void {
    _send({ type: "SetWindowMode", mode });
  },
};
