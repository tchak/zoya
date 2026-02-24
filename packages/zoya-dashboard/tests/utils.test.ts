import { describe, it, expect } from 'vitest';
import { groupByModule } from '../src/utils';

describe('groupByModule', () => {
  it('returns empty array for empty input', () => {
    const result = groupByModule([] as { module: string }[], (a, b) =>
      a.module.localeCompare(b.module),
    );
    expect(result).toEqual([]);
  });

  it('groups items by module', () => {
    const items = [
      { name: 'b', module: 'math' },
      { name: 'a', module: 'math' },
      { name: 'c', module: 'io' },
    ];
    const result = groupByModule(items, (a, b) => a.name.localeCompare(b.name));
    expect(result).toEqual([
      ['io', [{ name: 'c', module: 'io' }]],
      [
        'math',
        [
          { name: 'a', module: 'math' },
          { name: 'b', module: 'math' },
        ],
      ],
    ]);
  });

  it('puts root module (empty string) first', () => {
    const items = [
      { name: 'z', module: 'beta' },
      { name: 'a', module: '' },
      { name: 'b', module: 'alpha' },
    ];
    const result = groupByModule(items, (a, b) => a.name.localeCompare(b.name));
    expect(result[0][0]).toBe('');
    expect(result[1][0]).toBe('alpha');
    expect(result[2][0]).toBe('beta');
  });

  it('sorts items within groups using sort function', () => {
    const items = [
      { name: 'c', module: '' },
      { name: 'a', module: '' },
      { name: 'b', module: '' },
    ];
    const result = groupByModule(items, (a, b) => a.name.localeCompare(b.name));
    expect(result).toEqual([
      [
        '',
        [
          { name: 'a', module: '' },
          { name: 'b', module: '' },
          { name: 'c', module: '' },
        ],
      ],
    ]);
  });
});
