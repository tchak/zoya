export const $$Task = {
  of(value: unknown) {
    return Object.freeze({ $task: true, run: () => Promise.resolve(value) });
  },
  map(task: { run: () => Promise<unknown> }, f: (x: unknown) => unknown) {
    return Object.freeze({
      $task: true,
      run: () => task.run().then(f),
    });
  },
  and_then(
    task: { run: () => Promise<unknown> },
    f: (x: unknown) => { run: () => Promise<unknown> },
  ) {
    return Object.freeze({
      $task: true,
      run: () => task.run().then((v) => f(v).run()),
    });
  },
  delay(ms: number) {
    return Object.freeze({
      $task: true,
      run: () => new Promise<unknown[]>((r) => setTimeout(() => r([]), ms)),
    });
  },
};
