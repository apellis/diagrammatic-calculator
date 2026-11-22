import { describe, expect, it } from 'vitest';
import { exportMessage } from './export-message';

describe('export message', () => {
  it('says only that the file was exported when no copy is kept', () => {
    expect(exportMessage(true, { kind: 'none' })).toBe('Exported.');
    expect(exportMessage(false, { kind: 'none' })).toBe('Exported as entered: some cells do not parse.');
  });

  it('says when the copy with the document was saved', () => {
    expect(exportMessage(true, { kind: 'saved' })).toBe('Exported; a copy is kept with the document in your account.');
    const m = exportMessage(false, { kind: 'saved' });
    expect(m).toContain('some cells do not parse');
    expect(m).toContain('a copy is kept');
    expect(m).not.toContain('not saved');
  });

  it('says when the copy with the document failed, with the reason', () => {
    for (const canonical of [true, false]) {
      const m = exportMessage(canonical, { kind: 'failed', error: 'HTTP 500' });
      expect(m).toContain('was not saved (HTTP 500)');
      expect(m).toContain('The downloaded file is complete.');
      expect(m).not.toContain('a copy is kept');
      expect(m.includes('some cells do not parse')).toBe(!canonical);
    }
    // the plain "Exported." of a successful export never stands for a failed copy
    expect(exportMessage(true, { kind: 'failed', error: 'offline' })).not.toBe('Exported.');
  });
});
