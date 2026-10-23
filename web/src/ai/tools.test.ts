import { describe, expect, it } from 'vitest';
import { Doc } from '../doc/model';
import { nodeEngine } from '../eval/testing';
import { toChat } from './providers';
import { runTool, TOOLS, validate, type Proposal, type ToolHost } from './tools';

function host(doc: Doc, foreign = false): ToolHost & { proposals: Proposal[]; applies: (() => void)[] } {
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
    settled: async () => new Map(),
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
