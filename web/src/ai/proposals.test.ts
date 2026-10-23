import { describe, expect, it } from 'vitest';
import * as Y from 'yjs';
import { Doc, type JsonValue } from '../doc/model';
import { nodeEngine } from '../eval/testing';
import { prepareDecision, sharedProposals } from './proposals';
import type { Proposal, ToolHost } from './tools';

function setup() {
  const doc = new Doc(); doc.setContext('nilhecke nu=2');
  const cell = doc.addCell('def a := x_1');
  const p: Proposal = { id: 1, key: 'p-1', kind: 'write', target: cell, note: 'square it', state: 'open', base: doc.inputs(), operation: { name: 'cell_update', args: { id: cell, text: 'def a := x_1^2', note: 'square it' } } };
  doc.propose({ id: p.key!, author: 'alice', time: 1, title: p.note, payload: JSON.parse(JSON.stringify(p)) as JsonValue, status: 'pending' });
  const host: ToolHost = { main: doc, engine: nodeEngine(), doc: () => doc, branch: () => null, createBranch: () => '', openBranch: () => {}, compareBranch: () => null, discardBranch: () => {}, settled: async () => new Map(), termContext: () => doc.context, selection: () => null, propose: () => p, foreign: () => true };
  return { doc, p, host, cell };
}

describe('serializable shared proposals', () => {
  it('another editor prepares the change without changing the live document or needing a callback', async () => {
    const { doc, p, host, cell } = setup();
    const prepared = await prepareDecision(doc, p, host, true, 'bob');
    expect(doc.list()[0].text).toBe('def a := x_1');
    expect(doc.proposals.get(p.key!)?.status).toBe('pending');
    doc.undoManager.clear();
    Y.applyUpdate(doc.ydoc, prepared.update, doc.localOrigin);
    expect(doc.list()[0]).toEqual({ id: cell, text: 'def a := x_1^2' });
    expect(doc.proposals.get(p.key!)).toMatchObject({ status: 'accepted', decidedBy: 'bob' });
    expect(sharedProposals(doc)[0].state).toBe('accepted');
    doc.undo(); expect(doc.list()[0].text).toBe('def a := x_1');
  });
  it('refuses a stale target and an already-decided proposal', async () => {
    const { doc, p, host, cell } = setup();
    doc.updateCell(cell, 'def a := x_2');
    await expect(prepareDecision(doc, p, host, true, 'bob')).rejects.toThrow(/changed/);
    const rejection = await prepareDecision(doc, p, host, false, 'bob');
    Y.applyUpdate(doc.ydoc, rejection.update);
    await expect(prepareDecision(doc, p, host, true, 'bob')).rejects.toThrow(/decided/);
  });
  it('skips malformed shared payloads rather than executing them', () => {
    const { doc } = setup();
    doc.proposals.set('bad', { id: 'bad', author: 'x', time: 0, title: 'x', payload: { kind: 'eval', operation: 'bad' }, status: 'pending' });
    expect(sharedProposals(doc)).toHaveLength(1);
  });
});
