import { describe, it, expect, beforeEach } from 'vitest';
import '../src/index';
import { $$run, $$enqueue } from '../src/zoya';

describe('$$run', () => {
  beforeEach(() => {
    // Clean up any test functions from globalThis
    delete (globalThis as Record<string, unknown>)['$test_pkg$my_fn'];
    delete (globalThis as Record<string, unknown>)['$test_pkg$add'];
  });

  it('calls a globalThis function by qualified path', async () => {
    (globalThis as Record<string, unknown>)['$test_pkg$my_fn'] = () => 42;
    const result = await $$run('test_pkg::my_fn');
    expect(result).toEqual({ value: 42, jobs: [] });
  });

  it('passes converted arguments', async () => {
    (globalThis as Record<string, unknown>)['$test_pkg$add'] = (
      a: number,
      b: number,
    ) => a + b;
    const result = await $$run('test_pkg::add', 3, 4);
    expect(result).toEqual({ value: 7, jobs: [] });
  });

  it('panics when function is not found', async () => {
    await expect($$run('test_pkg::missing')).rejects.toThrow(
      'function not found: test_pkg::missing',
    );
  });

  it('panics on arity mismatch', async () => {
    (globalThis as Record<string, unknown>)['$test_pkg$add'] = (
      a: number,
      b: number,
    ) => a + b;
    await expect($$run('test_pkg::add', 1)).rejects.toThrow(
      'arity mismatch for test_pkg::add: expected 2 arguments, got 1',
    );
  });

  it('collects enqueued jobs', async () => {
    (globalThis as Record<string, unknown>)['$test_pkg$my_fn'] = () => {
      $$enqueue({ name: 'deploy', args: [1, 2] });
      $$enqueue({ name: 'notify', args: ['done'] });
      return 'ok';
    };
    const result = await $$run('test_pkg::my_fn');
    expect(result).toEqual({
      value: 'ok',
      jobs: [
        { name: 'deploy', args: [1, 2] },
        { name: 'notify', args: ['done'] },
      ],
    });
  });

  it('drains jobs between runs', async () => {
    (globalThis as Record<string, unknown>)['$test_pkg$my_fn'] = () => {
      $$enqueue({ name: 'job1' });
      return 'first';
    };
    const r1 = await $$run('test_pkg::my_fn');
    expect(r1.jobs).toHaveLength(1);

    (globalThis as Record<string, unknown>)['$test_pkg$my_fn'] = () => 'second';
    const r2 = await $$run('test_pkg::my_fn');
    expect(r2.jobs).toHaveLength(0);
  });
});
