import { describe, expect, it } from 'vitest';
import { parseLine } from './contextLine';

describe('parseLine', () => {
  it('keeps a quoted value with spaces as one option', () => {
    expect(parseLine('klrw type=A2 red="i@0, 2j@5/2" theta_ij=1/2')).toEqual({
      name: 'klrw',
      opts: [['type', 'A2'], ['red', '"i@0, 2j@5/2"'], ['theta_ij', '1/2']],
    });
  });
  it('reads a plain line as before', () => {
    expect(parseLine('  klr type=A2 nu=2i+j ')).toEqual({ name: 'klr', opts: [['type', 'A2'], ['nu', '2i+j']] });
  });
});
