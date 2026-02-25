export interface TaskValue {
  readonly $task: true;
  run(): Promise<unknown>;
}

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
  all(tasks: { run: () => Promise<unknown> }[]) {
    return Object.freeze({
      $task: true,
      run: () => Promise.all(tasks.map((t) => t.run())),
    });
  },
  tap(task: { run: () => Promise<unknown> }, f: (x: unknown) => void) {
    return Object.freeze({
      $task: true,
      run: () =>
        task.run().then((v) => {
          f(v);
          return v;
        }),
    });
  },
  zip(a: { run: () => Promise<unknown> }, b: { run: () => Promise<unknown> }) {
    return Object.freeze({
      $task: true,
      run: () => Promise.all([a.run(), b.run()]),
    });
  },
  zip3(
    a: { run: () => Promise<unknown> },
    b: { run: () => Promise<unknown> },
    c: { run: () => Promise<unknown> },
  ) {
    return Object.freeze({
      $task: true,
      run: () => Promise.all([a.run(), b.run(), c.run()]),
    });
  },
  zip4(
    a: { run: () => Promise<unknown> },
    b: { run: () => Promise<unknown> },
    c: { run: () => Promise<unknown> },
    d: { run: () => Promise<unknown> },
  ) {
    return Object.freeze({
      $task: true,
      run: () => Promise.all([a.run(), b.run(), c.run(), d.run()]),
    });
  },
  delay(ms: number) {
    return Object.freeze({
      $task: true,
      run: () => new Promise<unknown[]>((r) => setTimeout(() => r([]), ms)),
    });
  },
};
