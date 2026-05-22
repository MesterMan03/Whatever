import { _send } from "../shared.ts";

/** Window management. */
export const Window = {
  /** Set the title of the active window. */
  setTitle(title: string): void {
    _send({ type: "SetWindowTitle", title });
  },
};
