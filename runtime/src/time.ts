let currentTickNumber = 0;
let currentDeltaSeconds = 0;

/** @internal Refresh before any component lifecycle hooks or public tick handlers. */
export function _updateTime(tickNumber: number, deltaSeconds: number): void {
  currentTickNumber = tickNumber;
  currentDeltaSeconds = deltaSeconds;
}

/** Read-only simulation timing for the latest engine tick, available throughout the mod. */
export const Time = Object.freeze({
  /** Engine tick number; zero before the first tick. */
  get tickNumber(): number {
    return currentTickNumber;
  },
  /** Simulation interval for this tick only, in seconds; zero before the first tick. */
  get deltaSeconds(): number {
    return currentDeltaSeconds;
  },
});
