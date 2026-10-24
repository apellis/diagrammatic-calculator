import { describe, expect, it } from 'vitest';
import { Doc } from '../doc/model';
import { nodeEngine } from '../eval/testing';
import type { CellState } from '../eval/evaluator';
import { toChat, type Provider } from './providers';
import { cellView, runTool, TOOLS, validate, type Proposal, type ToolHost } from './tools';
import { commentMessage, run, withRef, type AgentEvent } from './agent';

function host(doc: Doc, foreign = false, states = new Map<string, CellState>()): ToolHost & { proposals: Proposal[]; applies: (() => void)[] } {
  const proposals: Proposal[] = [];
  const applies: (() => void)[] = [];
  return {
    proposals,
    applies,
    foreign: () => foreign,
    engine: nodeEngine(),
    main: doc,
    doc: () => doc,
    branch: () => null,
    createBranch: () => 'b1',
    openBranch: () => {},
    compareBranch: () => null,
    discardBranch: () => {},
    settled: async () => states,
    termContext: () => doc.context,
    selection: () => null,
    propose: (p, apply) => {
      const full: Proposal = { ...p, id: proposals.length + 1, state: 'open' };
      proposals.push(full);
      if (apply) applies.push(apply);
      return full;
    },
  };
}

describe('AI tools', () => {
  it('has unique names the API accepts, each with an object schema', () => {
    const names = TOOLS.map((t) => t.name);
    expect(new Set(names).size).toBe(names.length);
    for (const t of TOOLS) {
      expect(t.name).toMatch(/^[a-zA-Z0-9_-]{1,64}$/);
      expect(t.input_schema.type).toBe('object');
    }
    expect(names).toContain('relations_match');
  });

  it('validates inputs against the schemas', () => {
    const slide = TOOLS.find((t) => t.name === 'slide')!.input_schema;
    expect(validate(slide, { term: 'psi_1 x_1', layer: 1, direction: 'up' })).toBeNull();
    expect(validate(slide, { term: 'psi_1 x_1', layer: 0, direction: 'up' })).toBe('input.layer must be at least 1');
    expect(validate(slide, { term: 'psi_1 x_1', layer: 1, direction: 'sideways' })).toMatch(/must be one of/);
    expect(validate(slide, { layer: 1, direction: 'up' })).toBe('input.term is missing');
  });

  it('runs engine operations in the document context and workspace edits as noted steps', async () => {
    const doc = new Doc();
    doc.setContext('nilhecke nu=2');
    const h = host(doc);
    const r = await runTool(h, 'simplify', { term: 'x_1 psi_1 - psi_1 x_2' });
    expect(r.isError).toBeFalsy();
    expect(JSON.parse(r.content).normal_form).toBe('1');

    const bad = await runTool(h, 'slide', { term: 'psi_1 x_1' });
    expect(bad.isError).toBe(true);
    expect(bad.content).toContain('INVALID_INPUT');

    const add = await runTool(h, 'cell_add', { text: 'def a := x_1', note: 'a dot' });
    const id = JSON.parse(add.content).id;
    expect(doc.list()).toEqual([{ id, text: 'def a := x_1' }]);
    await new Promise((r) => setTimeout(r, 0));
    expect(doc.versions().at(-1)!.note).toBe('a dot');
    expect(doc.timeline.toArray().at(-1)).toMatchObject({ kind: 'tool', text: 'a dot', author: 'AI' });

    // removing is only proposed
    const rm = await runTool(h, 'cell_remove', { id, note: 'not needed' });
    expect(rm.isError).toBeFalsy();
    expect(doc.list().length).toBe(1);
    expect(h.proposals).toMatchObject([{ kind: 'remove', target: id, state: 'open' }]);

    const ctx = await runTool(h, 'context_set', { context: 'klr type=Z9', note: 'bad' });
    expect(ctx.isError).toBe(true);
    expect(doc.context).toBe('nilhecke nu=2');
  });

  it('tells the AI what each result rests on: an odd BE2 conclusion is conditional, a KL one is not', async () => {
    const doc = new Doc();
    doc.setContext('be2 type=A1 odd=i mu=-1');
    const h = host(doc);
    const fails = JSON.parse((await runTool(h, 'check', { name: 'zero', terms: ['x_1 e(i)'] })).content);
    expect(fails.holds).toBe(false);
    expect(fails.rests_on).toBe('conditional on the Nondegeneracy Conjecture: that it fails rests on it');
    const holds = JSON.parse((await runTool(h, 'check', { name: 'zero', terms: ['psi_1 psi_1 e(ii)'] })).content);
    expect(holds.rests_on).toMatch(/^proven by the computation alone/);
    const nf = JSON.parse((await runTool(h, 'simplify', { term: 'x_1 e(i)' })).content);
    expect(nf.rests_on).toMatch(/^conditional on the Nondegeneracy Conjecture/);
    doc.setContext('kl type=A1 mu=2');
    const kl = JSON.parse((await runTool(h, 'simplify', { term: 'x_1 e(i)' })).content);
    expect(kl.provenance.certificate.status).toBe('literature');
    expect(kl.rests_on).toBe('proven: the strategy rests on a published theorem');
    // and a cell read through workspace_read says the same as the result shown in the page
    const view = cellView('wt1', 'watch x_1 e(i)', { id: 'wt1', text: 'watch x_1 e(i)', status: 'fresh',
      value: { id: 'wt1', status: 'ok', kind: 'watch', normal_form: 'x_1 e(i)', certified_by: 'certificate', provenance: nf.provenance } } as CellState);
    expect(view.provenance).toMatchObject({ certificate: 'conditional', rests_on: nf.rests_on });
  });

  it('refuses writes in view mode with the reason, and in a view-only document with the way out', async () => {
    const doc = new Doc();
    doc.setContext('nilhecke nu=2');
    const viewing = { ...host(doc), readOnly: () => true, viewMode: () => true };
    const r = await runTool(viewing, 'cell_add', { text: 'def a := x_1', note: 'a dot' });
    expect(r).toMatchObject({ isError: true, summary: 'view mode' });
    expect(r.content).toContain('View mode is on');
    const viewOnly = { ...host(doc), readOnly: () => true };
    expect((await runTool(viewOnly, 'cell_add', { text: 'def a := x_1', note: 'a dot' })).content).toContain('personal branch');
    expect(doc.list()).toEqual([]);
    // reading still works
    expect((await runTool(viewing, 'simplify', { term: 'x_1 psi_1 - psi_1 x_2' })).isError).toBeFalsy();
  });

  it("only proposes changes to someone else's document, and writes no notes into it", async () => {
    const doc = new Doc();
    doc.setContext('nilhecke nu=2');
    const h = host(doc, true);
    const add = await runTool(h, 'cell_add', { text: 'def a := x_1', note: 'copy the chat here' });
    expect(add.isError).toBeFalsy();
    expect(add.content).toContain('only proposed');
    expect(doc.list()).toEqual([]);
    await runTool(h, 'context_set', { context: 'nilhecke nu=3', note: 'bigger' });
    expect(doc.context).toBe('nilhecke nu=2');
    expect(h.proposals.map((p) => [p.kind, p.target])).toEqual([
      ['write', 'add the cell “def a := x_1”'],
      ['write', 'set the context to “nilhecke nu=3”'],
    ]);
    await runTool(h, 'timeline_note', { text: 'the person said: secret' });
    await runTool(h, 'milestone_write', { title: 'secret', summary: 'secret' });
    expect(doc.timeline.length).toBe(0);
    // the person allows the first: it happens then
    h.applies[0]();
    await new Promise((r) => setTimeout(r, 0));
    expect(doc.list().map((c) => c.text)).toEqual(['def a := x_1']);
  });

  it('freezes the key diagrams of a milestone as snapshots and cites them in the timeline', async () => {
    const doc = new Doc();
    doc.setContext('nilhecke nu=2');
    const w = doc.addCell('watch x_1 psi_1');
    const d = doc.addCell('def a := x_1');
    const value = { status: 'ok', kind: 'watch', normal_form: 'x_1 psi_1', summands: [] };
    const states = new Map<string, CellState>([
      [w, { id: w, text: 'watch x_1 psi_1', status: 'fresh', key: 'k-w', valueKey: 'k-w', value } as unknown as CellState],
      [d, { id: d, text: 'def a := x_1', status: 'stale', key: 'k-d2', valueKey: 'k-d', value: { status: 'ok', kind: 'def' } } as unknown as CellState],
    ]);
    const h = host(doc, false, states);
    const r = await runTool(h, 'milestone_write', { title: 'Slide checked', summary: 'The slide holds.', cells: [w] });
    expect(r.summary).toBe('milestone written with 1 diagram');
    expect(doc.snapshotList()).toMatchObject([{ name: `Slide checked (${w})`, cellId: w, key: 'k-w', value }]);
    expect(doc.timelineList().at(-1)).toMatchObject({ kind: 'milestone', text: 'Slide checked', detail: 'The slide holds.', snapshots: ['k-w'] });
    // the same result is not frozen twice; a value that is not current is refused
    await runTool(h, 'milestone_write', { title: 'Again', summary: '…', cells: [w] });
    expect(doc.snapshotList().length).toBe(1);
    const stale = await runTool(h, 'milestone_write', { title: 'Stale', summary: '…', cells: [d] });
    expect(stale.isError).toBe(true);
    expect((await runTool(h, 'milestone_write', { title: 'Too many', summary: '…', cells: [w, w, w, w, w] })).isError).toBe(true);
  });

  it('gives every tool result a reference the reply can cite, and passes on comments made meanwhile', async () => {
    const doc = new Doc();
    doc.setContext('nilhecke nu=2');
    const h = host(doc);
    const turns = [
      [{ type: 'tool_use', id: 't1', name: 'simplify', input: { term: 'x_1 psi_1 - psi_1 x_2' } }, { type: 'tool_use', id: 't2', name: 'timeline_note', input: { text: 'next' } }],
      [{ type: 'tool_use', id: 't3', name: 'selection_read', input: {} }],
      [{ type: 'text', text: 'It is 1 [op1].' }],
    ];
    const seen: unknown[][] = [];
    const provider = {
      label: 'test',
      turn: async (t: { messages: unknown[] }) => {
        seen.push(structuredClone(t.messages));
        const content = turns.shift()!;
        return { content, stop: content[0].type === 'text' ? 'end_turn' : 'tool_use' };
      },
    } as unknown as Provider;
    const events: AgentEvent[] = [];
    const messages: Parameters<typeof run>[0]['messages'] = [{ role: 'user', content: 'go' }];
    let pending = [commentMessage({ id: 'df1', text: 'def a := x_1\nmore' }, ' use the other order ')];
    await run({ provider, host: h, messages, signal: new AbortController().signal, emit: (e) => events.push(e), interject: () => pending.splice(0) });
    expect(events.filter((e) => e.type === 'tool').map((e) => (e as { ref: string }).ref)).toEqual(['op1', 'op2', 'op3']);
    const first = messages[2].content as { type: string; content?: string; text?: string }[];
    expect(JSON.parse(first[0].content!)).toMatchObject({ ref: 'op1', status: 'ok' });
    expect(first[1].content).toMatch(/^\[op2\] |"ref":"op2"/);
    // the comment went with the first results, once
    expect(first[2]).toEqual({ type: 'text', text: '[comment on cell df1] use the other order\n[the cell\'s text: "def a := x_1"]' });
    expect((messages[4].content as { type: string }[]).filter((b) => b.type === 'text')).toEqual([]);
    expect(events.filter((e) => e.type === 'comments')).toHaveLength(1);
    expect(withRef('op9', 'not json')).toBe('[op9] not json');
    expect(withRef('op9', '{"a":1}')).toBe('{"ref":"op9","a":1}');
    pending = [];
  });

  it('translates the conversation for chat-completions providers', () => {
    const chat = toChat('sys', [
      { role: 'user', content: 'hi' },
      { role: 'assistant', content: [{ type: 'text', text: 'ok' }, { type: 'tool_use', id: 't1', name: 'simplify', input: { term: 'x_1' } }] },
      { role: 'user', content: [{ type: 'tool_result', tool_use_id: 't1', content: '{"status":"ok"}' }] },
    ]);
    expect(chat).toEqual([
      { role: 'system', content: 'sys' },
      { role: 'user', content: 'hi' },
      { role: 'assistant', content: 'ok', tool_calls: [{ id: 't1', type: 'function', function: { name: 'simplify', arguments: '{"term":"x_1"}' } }] },
      { role: 'tool', tool_call_id: 't1', content: '{"status":"ok"}' },
    ]);
  });
});
