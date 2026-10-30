/** A recorded operation's result. Calling get() waits; it never flushes or repeats work. */
export interface Pending<T> {
  get(): Promise<T>;
}

export function _pending<T>(result: Promise<T>): Pending<T> {
  // Recording a query need not observe it. Preserve rejection for get() without
  // creating an unhandled rejection when a recorded result is intentionally ignored.
  void result.catch(() => {});
  return { get: () => result };
}
