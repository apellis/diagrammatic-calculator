import { expect, test, type Page, type Route } from '@playwright/test';
import { addCell, cell, newDoc, typeCell } from './helpers';

// A recorded model: each request to the Messages API gets the next scripted turn, streamed as
// server-sent events, so the AI loop runs end to end in the page without a network call.
type Block = { type: 'text'; text: string } | { type: 'tool_use'; name: string; input: unknown };

function sse(blocks: Block[], stop: string): string {
  const ev = (type: string, data: unknown) => `event: ${type}\ndata: ${JSON.stringify({ type, ...(data as object) })}\n\n`;
  let out = ev('message_start', {
    message: { id: 'msg_test', type: 'message', role: 'assistant', model: 'claude-opus-5-5', content: [], stop_reason: null, stop_sequence: null, usage: { input_tokens: 1200, output_tokens: 1, cache_read_input_tokens: 9000, cache_creation_input_tokens: 0 } },
  });
  blocks.forEach((b, index) => {
    if (b.type === 'text') {
      out += ev('content_block_start', { index, content_block: { type: 'text', text: '' } });
      out += ev('content_block_delta', { index, delta: { type: 'text_delta', text: b.text } });
    } else {
      out += ev('content_block_start', { index, content_block: { type: 'tool_use', id: `toolu_${index}_${b.name}`, name: b.name, input: {} } });
      out += ev('content_block_delta', { index, delta: { type: 'input_json_delta', partial_json: JSON.stringify(b.input) } });
    }
    out += ev('content_block_stop', { index });
  });
  out += ev('message_delta', { delta: { stop_reason: stop, stop_sequence: null }, usage: { output_tokens: 80 } });
  out += ev('message_stop', {});
  return out;
}

const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*', 'access-control-allow-methods': 'POST, GET, OPTIONS' };

/** Anthropic's model list as the API returns it, newest first. */
const ANTHROPIC_LIST = {
  data: [
    { type: 'model', id: 'claude-opus-5-5', display_name: 'Claude Opus 5.5', created_at: '2026-09-01T00:00:00Z' },
    { type: 'model', id: 'claude-sonnet-5-5', display_name: 'Claude Sonnet 5.5', created_at: '2026-09-01T00:00:00Z' },
    { type: 'model', id: 'claude-haiku-5-5', display_name: 'Claude Haiku 5.5', created_at: '2026-08-01T00:00:00Z' },
  ],
  has_more: false, first_id: 'claude-opus-5-5', last_id: 'claude-haiku-5-5',
};
/** Answers a GET of the model list; true when it did. */
async function modelList(route: Route, body: unknown = ANTHROPIC_LIST) {
  const r = route.request();
  if (r.method() !== 'GET' || !new URL(r.url()).pathname.endsWith('/models')) return false;
  await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify(body) });
  return true;
}

async function recordedModel(page: Page, turns: (() => string | Promise<string>)[]) {
  const requests: { headers: Record<string, string>; body: Record<string, unknown> }[] = [];
  await page.route('https://api.anthropic.com/**', async (route: Route) => {
    const r = route.request();
    if (r.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    if (await modelList(route)) return;
    requests.push({ headers: await r.allHeaders(), body: r.postDataJSON() });
    const next = turns.shift();
    if (!next) return route.fulfill({ status: 500, headers: cors, body: 'no more recorded turns' });
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'text/event-stream' }, body: await next() });
  });
  return requests;
}

/** Sets the AI up from a document: the panel's link to Settings, the form there, and back. */
async function enterKey(page: Page, fill = async (p: Page) => p.getByLabel('API key').fill('sk-ant-test')) {
  await page.getByTestId('ai-toggle').click();
  await page.getByRole('button', { name: 'I understand' }).click();
  await page.getByTestId('ai-setup-link').click();
  await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
  await fill(page);
  await page.getByTestId('ai-save').click();
  await expect(page.getByTestId('ai-saved')).toBeVisible();
  await page.getByTestId('settings-back').click();
  await expect(page.getByLabel('Message to the AI')).toBeVisible();
}

/** The settings page, from the document list. */
async function openSettings(page: Page) {
  await page.goto('/');
  await page.getByTestId('open-settings').click();
  await expect(page.getByRole('heading', { name: 'Settings' })).toBeVisible();
}

test('an AI what-if on a branch: edits there, reads the values, proposes accepting; one undo takes it back', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await addCell(page, 'watch a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 0');
  const def = (await cell(page, 0).getAttribute('data-cell'))!;
  const watch = (await cell(page, 1).getAttribute('data-cell'))!;

  const requests = await recordedModel(page, [
    () => sse([
      { type: 'text', text: 'I will try the slide on a branch.' },
      { type: 'tool_use', name: 'branch_create', input: { name: 'slide the dot up' } },
      { type: 'tool_use', name: 'cell_update', input: { id: def, text: 'def a := x_2 psi_1 e(iij) + e(iij)', note: 'slide the dot up through the crossing' } },
      { type: 'tool_use', name: 'workspace_read', input: { cells: [def, watch] } },
    ], 'tool_use'),
    () => sse([
      { type: 'text', text: 'On the branch the definition has 2 terms and the watch is unchanged.' },
      { type: 'tool_use', name: 'simplify', input: { term: 'x_2 psi_1 e(iij) + e(iij) - psi_1 x_1 e(iij)' } },
      { type: 'tool_use', name: 'branch_accept', input: { note: 'the slide is an equality; keep the expanded form' } },
    ], 'tool_use'),
    () => sse([{ type: 'text', text: 'The difference simplifies to 0, so the slide is an equality. Accept the branch to keep it.' }], 'end_turn'),
  ]);

  await enterKey(page);
  await page.getByLabel('Message to the AI').fill('What if I slide the dot up through the crossing?');
  await page.keyboard.press('Enter');

  const chat = page.getByTestId('chat');
  await expect(chat).toContainText('The difference simplifies to 0');
  await expect(chat.locator('.ai-tool')).toHaveCount(5);
  await expect(chat.locator('.ai-tool').nth(2)).toContainText('read 2 cells');
  await expect(chat.locator('.ai-tool').nth(3)).toContainText('simplify: 0');
  // the workspace shows the branch, with the edit made there
  await expect(page.getByTestId('branch')).toContainText('slide the dot up');
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_2 psi_1 e(iij) + e(iij)');

  // what the model received: the tool result for workspace_read carries the watch's value
  const third = requests[1].body.messages as { role: string; content: { type: string; content?: string }[] }[];
  const read = third.at(-1)!.content.find((b) => b.type === 'tool_result' && b.content?.includes('"cells"'))!;
  // (it waited for the branch's values: the definition is written out, the watch's value is unchanged)
  const cells = JSON.parse(read.content!).cells;
  expect(cells[0]).toMatchObject({ id: def, kind: 'def', status: 'fresh', terms: 2 });
  expect(cells[1]).toMatchObject({ id: watch, kind: 'watch', status: 'fresh', terms: 1, normal_form: 'psi_1 x_1 e(iij)' });
  // the request: browser opt-in header, cached system prompt and tool list, adaptive thinking
  expect(requests[0].headers['anthropic-dangerous-direct-browser-access']).toBe('true');
  expect(requests[0].headers['authorization']).toBe('Bearer sk-ant-test');
  const body = requests[0].body as { model: string; system: { cache_control?: unknown }[]; tools: { name: string; cache_control?: unknown; eager_input_streaming?: boolean }[]; thinking: unknown };
  expect(body.model).toBe('claude-opus-5-5');
  expect(body.system[0].cache_control).toEqual({ type: 'ephemeral' });
  expect(body.tools.at(-1)!.cache_control).toEqual({ type: 'ephemeral' });
  expect(body.tools.every((t) => t.eager_input_streaming)).toBe(true);
  expect(body.thinking).toEqual({ type: 'adaptive', display: 'summarized' });

  // the person accepts the proposal; the document now has the change, and one undo takes it back
  await page.getByTestId('proposal').getByRole('button', { name: 'Accept' }).click();
  await expect(page.getByTestId('branch')).toHaveCount(0);
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := x_2 psi_1 e(iij) + e(iij)');
  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(cell(page, 0).locator('.head .text')).toHaveAttribute('data-text', 'a := psi_1 x_1 e(iij)');

  // the activity timeline (part of the document, so shared) records that a request was made,
  // without its text, and the AI's steps with their notes
  await page.getByTestId('activity-tab').click();
  const tl = page.getByTestId('timeline');
  await expect(tl).toContainText('Asked the AI.');
  await expect(tl).not.toContainText('What if I slide the dot up');
  // Phase 7 keeps exploratory branch activity private, even when its final change is accepted.
  await expect(tl).not.toContainText('slide the dot up through the crossing');
  await expect(tl).toContainText('accepted: the slide is an equality');
});

test('a refused key and a refusal are shown as plain messages', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  let n = 0;
  await page.route('https://api.anthropic.com/**', async (route) => {
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    if (await modelList(route)) return;
    if (n++ === 0) {
      return route.fulfill({ status: 401, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ type: 'error', error: { type: 'authentication_error', message: 'invalid x-api-key' } }) });
    }
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'text/event-stream' }, body: sse([{ type: 'text', text: 'I cannot help with that.' }], 'refusal') });
  });
  await enterKey(page);
  await page.getByLabel('Message to the AI').fill('hello');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat').locator('.error')).toContainText('Anthropic refused the key');
  await page.getByLabel('Message to the AI').fill('hello again');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat').locator('.error').nth(1)).toContainText('declined');
});

test('the key stays for the session only by default; a reply shows its mathematics typeset and its code fixed-width', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await recordedModel(page, [
    () => sse([{ type: 'text', text: 'They differ by a sign:\n\\[\na=\\psi_1e(iji),\\qquad b=-a.\n\\]\n\n- \\(a^2=a\\), by the new `check idempotent(a)`.\n- **Orthogonal** too.' }], 'end_turn'),
  ]);
  await enterKey(page);
  expect(await page.evaluate(() => [sessionStorage.getItem('diagcalc.ai.key.anthropic'), localStorage.getItem('diagcalc.ai.key.anthropic')])).toEqual(['sk-ant-test', null]);
  await page.getByLabel('Message to the AI').fill('compare');
  await page.keyboard.press('Enter');
  const reply = page.getByTestId('chat').locator('.ai-text');
  await expect(reply.locator('.katex-display')).toHaveCount(1);
  await expect(reply.locator('li .katex')).toHaveCount(1);
  await expect(reply.locator('code')).toHaveText('check idempotent(a)');
  await expect(reply.locator('strong')).toHaveText('Orthogonal');
  await expect(reply).not.toContainText('\\[');
});

test('a multi-step AI request undoes as one step; pinned watches sit at the top of the outline', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def e := e(iij)');
  await recordedModel(page, [
    () => sse([
      { type: 'tool_use', name: 'cell_add', input: { text: 'watch e e - e', note: 'is e idempotent?' } },
      { type: 'tool_use', name: 'cell_add', input: { text: 'check idempotent(e)', note: 'ask for a verdict too' } },
      { type: 'tool_use', name: 'watch_pin', input: { id: 'wt2' } },
      { type: 'tool_use', name: 'milestone_write', input: { title: 'e is idempotent', summary: 'e e - e = 0 and the check holds.' } },
    ], 'tool_use'),
    () => sse([{ type: 'text', text: 'Added a watch and a check; e is idempotent.' }], 'end_turn'),
  ]);
  await enterKey(page);
  await page.getByLabel('Message to the AI').fill('Is e idempotent?');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat')).toContainText('e is idempotent.');
  await expect(page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(3);
  await expect(cell(page, 2).getByTestId('summary')).toHaveText('holds');
  await expect(page.getByTestId('pinned').locator('.t')).toHaveAttribute('data-text', /e e - e/);
  await page.getByTestId('activity-tab').click();
  await expect(page.getByTestId('timeline').locator('.tl-milestone')).toContainText('e e - e = 0 and the check holds.');

  await page.getByRole('button', { name: 'Undo' }).click();
  await expect(page.locator('.cell[data-cell]:not([data-cell="new"])')).toHaveCount(1);
  await expect(page.getByTestId('pinned')).toHaveCount(0);
});

/** Opens a cell (its tools show) without starting to edit it. */
async function expand(page: Page, i: number) {
  const head = cell(page, i).locator('.head');
  if ((await head.getAttribute('aria-expanded')) === 'true') return;
  await head.focus();
  await page.keyboard.press('Enter');
  await expect(head).toHaveAttribute('aria-expanded', 'true');
}

test('claims link to the operations behind them, comments on cells reach the AI, milestones show their key diagrams', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await addCell(page, 'watch a');
  await expect(cell(page, 1).getByTestId('summary')).toHaveText('1 term, degree 0');
  const def = (await cell(page, 0).getAttribute('data-cell'))!;
  const watch = (await cell(page, 1).getAttribute('data-cell'))!;
  // the first answer is held until the person has commented, so the comment arrives mid-request
  let release!: () => void;
  const held = new Promise<void>((r) => (release = r));
  const requests = await recordedModel(page, [
    async () => {
      await held;
      return sse([{ type: 'text', text: 'Checking.' }, { type: 'tool_use', name: 'simplify', input: { term: 'x_2 psi_1 e(iij) + e(iij) - psi_1 x_1 e(iij)' } }], 'tool_use');
    },
    () => sse([{ type: 'tool_use', name: 'milestone_write', input: { title: 'Slide checked', summary: 'The slide is an **equality**.', cells: [watch] } }], 'tool_use'),
    () => sse([{ type: 'text', text: 'The difference simplifies to 0 [op1], so the slide holds; the milestone keeps the watch [op2].' }], 'end_turn'),
    () => sse([{ type: 'text', text: 'Noted the comment on the watch.' }], 'end_turn'),
  ]);
  await enterKey(page);
  await page.getByLabel('Message to the AI').fill('Does the slide hold?');
  await page.keyboard.press('Enter');

  await expand(page, 0);
  await cell(page, 0).getByTestId('comment-ai').click();
  await page.getByLabel(`Comment on ${def} for the AI`).fill('Keep the dot on the left strand.');
  await page.keyboard.press('Enter');
  await expect(cell(page, 0).getByTestId('cell-comments')).toContainText('(waiting): Keep the dot on the left strand.');
  await expect(page.getByTestId('comments-waiting')).toContainText('waits for the AI to finish its step');
  release();
  const chat = page.getByTestId('chat');
  await expect(chat).toContainText('the milestone keeps the watch');
  await expect(cell(page, 0).getByTestId('cell-comments')).toContainText('(sent)');
  await expect(chat.getByTestId('comment-on')).toHaveText(`comment on ${def}`);
  // it went with the results of the first step, as the person's instruction tied to the cell
  const after = requests[1].body.messages as { role: string; content: { type: string; text?: string; content?: string }[] }[];
  const blocks = after.at(-1)!.content;
  expect(blocks.map((b) => b.type)).toEqual(['tool_result', 'text']);
  expect(JSON.parse(blocks[0].content!)).toMatchObject({ ref: 'op1', status: 'ok' });
  expect(blocks[1].text).toBe(`[comment on cell ${def}] Keep the dot on the left strand.\n[the cell's text: "def a := psi_1 x_1 e(iij)"]`);
  expect((requests[0].body.system as { text: string }[])[0].text).toContain('end it with the reference of the tool call');

  // a claim's reference shows the call and its result
  await chat.locator('.claim-ref[data-ref="op1"]').click();
  const claim = chat.getByTestId('claim');
  await expect(claim).toContainText('op1 simplify');
  await expect(claim).toContainText('x_2 psi_1 e(iij) + e(iij) - psi_1 x_1 e(iij)');
  await expect(claim.getByTestId('claim-result')).toContainText('"ref": "op1"');
  await chat.locator('.claim-ref[data-ref="op2"]').click();
  await expect(claim).toContainText('op2 milestone_write');
  await expect(claim).toContainText('milestone written with 1 diagram');

  // the milestone shows its key diagram, frozen as a snapshot
  await page.getByTestId('activity-tab').click();
  const milestone = page.getByTestId('timeline').locator('.tl-milestone');
  await expect(milestone.locator('strong')).toHaveText('equality');
  const diagram = milestone.getByTestId('milestone-diagram');
  await expect(diagram).toContainText(`Slide checked (${watch})`);
  await expect(diagram.locator('svg').first()).toBeVisible();
  await diagram.getByRole('button', { name: 'Open' }).click();
  const snap = page.getByRole('dialog', { name: `Snapshot Slide checked (${watch})` });
  await expect(snap).toContainText('watch');
  await page.keyboard.press('Escape');
  await expect(snap).toHaveCount(0);

  // a comment while the AI is idle starts a request of its own
  await page.getByRole('button', { name: 'Chat', exact: true }).click();
  await expand(page, 1);
  await cell(page, 1).getByTestId('comment-ai').click();
  await page.getByLabel(`Comment on ${watch} for the AI`).fill('Pin this one.');
  await page.getByTestId('comment-send').click();
  await expect(chat).toContainText('Noted the comment on the watch.');
  const last = requests[3].body.messages as { role: string; content: { type: string; text?: string }[] }[];
  expect(last.at(-1)!.content).toEqual([{ type: 'text', text: `[comment on cell ${watch}] Pin this one.\n[the cell's text: "watch a"]` }]);
});

test('Cmd-L in the canvas asks about the selection, which travels with the message as references', async ({ page }) => {
  await newDoc(page);
  const requests = await recordedModel(page, [() => sse([{ type: 'text', text: 'That is a dot below a crossing.' }], 'end_turn')]);
  await enterKey(page);
  // back from Settings the message box has the focus; the empty document's new cell is open
  await page.getByLabel('Cell text').click();
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  await cell(page, 0).getByTestId('open-canvas').click();
  await expect(page.locator('.canvas .frame svg.diagram').first()).toBeVisible();
  await page.locator('.canvas').focus();
  await page.keyboard.press('Tab');
  await page.keyboard.press('ControlOrMeta+l');
  await expect(page.getByTestId('ai-selection')).toContainText('summand 1, 1 piece');
  await expect(page.getByLabel('Message to the AI')).toBeFocused();
  await page.keyboard.type('What is this?');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat')).toContainText('a dot below a crossing');
  const msgs = requests[0].body.messages as { content: string }[];
  expect(msgs[0].content).toContain('[selection: {"version":');
  expect(msgs[0].content).toContain('"word":"psi_1 x_1 e(iij)"');
  expect(msgs[0].content).toContain('"layers":[1]');
});

test('the browser check for an OpenAI-compatible server says whether this page can reach it', async ({ page }) => {
  let calls = 0;
  await page.route('http://localhost:11434/**', (route) => (calls++, route.abort('failed')));
  await openSettings(page);
  await page.getByLabel('Provider').selectOption('compatible');
  // no URL yet: no default, nothing called, no error
  await expect(page.getByLabel('Server URL')).toHaveValue('');
  await expect(page.getByTestId('models-error')).toHaveCount(0);
  await page.getByRole('button', { name: 'Check the connection' }).click();
  await expect(page.getByTestId('ai-check')).toContainText('Enter the server URL first');
  // the model menu is empty until a list is fetched
  const model = page.getByLabel('Model', { exact: true });
  await expect(model).toBeDisabled();
  await expect(model).toContainText('Enter the server URL to load the models');
  // an unusable URL is marked when the field is left, and the refresh button refuses it
  const url = page.getByLabel('Server URL');
  await url.fill('localhost:11434');
  await url.blur();
  await expect(url).toHaveClass(/invalid/);
  await expect(page.getByTestId('url-problem')).toContainText('http:// or https://');
  await url.fill('');
  await expect(page.getByTestId('url-problem')).toHaveCount(0);
  await page.getByTestId('models-refresh').click();
  await expect(page.getByTestId('url-problem')).toContainText('Enter the server URL first');
  expect(calls).toBe(0);
  // a usable URL fetches the list when the field is left; an unreachable server says so
  await url.fill('http://localhost:11434/v1');
  await url.blur();
  await expect(url).not.toHaveClass(/invalid/);
  await expect(page.getByTestId('models-error')).toContainText('could not reach');
  await expect(model).toContainText('No models');
  expect(calls).toBe(1);
  await page.getByRole('button', { name: 'Check the connection' }).click();
  await expect(page.getByTestId('ai-check')).toContainText('refuses calls made from a browser');
  await expect(page.getByTestId('ai-check')).toContainText('MCP');
  await page.unroute('http://localhost:11434/**');
  await page.route('http://localhost:11434/**', (route) => route.fulfill({ status: 200, headers: cors, contentType: 'application/json', body: '{"data":[]}' }));
  await page.getByRole('button', { name: 'Check the connection' }).click();
  await expect(page.getByTestId('ai-check')).toContainText('accepts calls from this page');
  await expect(page.getByTestId('ai-check')).toHaveClass(/ok/);
  // a server that answers fills the menu
  await page.unroute('http://localhost:11434/**');
  await page.route('http://localhost:11434/**', (route) => route.fulfill({ status: 200, headers: cors, contentType: 'application/json', body: '{"data":[{"id":"llama-test"}]}' }));
  await page.getByTestId('models-refresh').click();
  await expect(model).toBeEnabled();
  await expect(model).toHaveValue('llama-test');
});

test('the OpenAI check makes a small Responses call, showing a spinner while it waits', async ({ page }) => {
  const bodies: Record<string, unknown>[] = [];
  await page.route('https://api.openai.com/**', async (route) => {
    const r = route.request();
    if (r.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    if (await modelList(route, { object: 'list', data: [{ id: 'gpt-test', created: 1, object: 'model' }] })) return;
    bodies.push(r.postDataJSON());
    await new Promise((done) => setTimeout(done, 700));
    if (bodies.length === 1) {
      return route.fulfill({ status: 404, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ error: { message: 'The model gpt-test does not exist.' } }) });
    }
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ id: 'resp', status: 'incomplete', output: [] }) });
  });
  await openSettings(page);
  await page.getByLabel('Provider').selectOption('openai');
  await page.getByLabel('API key').fill('sk-test');
  await page.getByLabel('API key').blur();
  await expect(page.getByLabel('Model', { exact: true })).toHaveValue('gpt-test');
  const run = page.getByTestId('ai-check-run');
  await run.click();
  await expect(run).toContainText('Checking…');
  await expect(run).toBeDisabled();
  await expect(page.getByTestId('ai-check')).toContainText('OpenAI answered 404: The model gpt-test does not exist.');
  await expect(run).toHaveText('Check the connection');
  await run.click();
  await expect(page.getByTestId('ai-check')).toContainText('OpenAI accepts calls from this page with gpt-test.');
  expect(bodies[0]).toMatchObject({ model: 'gpt-test', max_output_tokens: 16, store: false });
});

test('the model list comes from the provider, and the refresh button fetches it again', async ({ page }) => {
  await page.goto('/');
  let anthropicLists = 0;
  await page.route('https://api.anthropic.com/**', async (route) => {
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    anthropicLists++;
    const body = anthropicLists === 1 ? ANTHROPIC_LIST : { ...ANTHROPIC_LIST, data: [{ type: 'model', id: 'claude-opus-6', display_name: 'Claude Opus 6', created_at: '2026-10-20T00:00:00Z' }, ...ANTHROPIC_LIST.data] };
    if (!(await modelList(route, body))) await route.fulfill({ status: 500, headers: cors });
  });
  await page.route('https://api.openai.com/**', async (route) => {
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    await modelList(route, {
      object: 'list',
      data: [
        { id: 'gpt-old', created: 100, object: 'model' },
        { id: 'text-embedding-3-large', created: 300, object: 'model' },
        { id: 'gpt-new', created: 200, object: 'model' },
      ],
    });
  });
  await openSettings(page);
  const model = page.getByLabel('Model', { exact: true });
  // before a key, the built-in Anthropic list
  await expect(model.locator('option')).toContainText(['Claude Opus 5.5', 'Claude Sonnet 5.5']);
  await page.getByLabel('API key').fill('sk-ant-test');
  await page.getByLabel('API key').blur();
  await expect.poll(() => anthropicLists).toBe(1);
  await expect(model.locator('option')).toHaveText(['Claude Opus 5.5', 'Claude Sonnet 5.5', 'Claude Haiku 5.5', 'Other…']);
  await expect(model).toHaveValue('claude-opus-5-5');
  await page.getByTestId('models-refresh').click();
  await expect(model.locator('option').first()).toHaveText('Claude Opus 6');
  expect(anthropicLists).toBe(2);
  // OpenAI: chat models only, newest first, and the newest is chosen
  await page.getByLabel('Provider').selectOption('openai');
  // no OpenAI key yet: a name is typed
  await expect(page.locator('input[aria-label="Model"]')).toBeVisible();
  await page.getByLabel('API key').fill('sk-test');
  await page.getByLabel('API key').blur();
  await expect(page.getByLabel('Model', { exact: true }).locator('option')).toHaveText(['gpt-new', 'gpt-old', 'Other…']);
  await expect(page.getByLabel('Model', { exact: true })).toHaveValue('gpt-new');
  // Other… lets one type a name the list does not have
  await page.getByLabel('Model', { exact: true }).selectOption({ label: 'Other…' });
  await page.locator('input[aria-label="Model"]').fill('gpt-custom');
  await page.getByTestId('ai-save').click();
  await page.getByTestId('settings-back').click();
  await page.getByTestId('open-settings').click();
  await expect(page.getByLabel('Provider')).toHaveValue('openai');
  await expect(page.getByLabel('Model', { exact: true })).toHaveValue('gpt-custom');
});

test('OpenAI goes through the Responses API: a tool call, its result sent back, then the answer', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  const bodies: Record<string, unknown>[] = [];
  await page.route('https://api.openai.com/**', async (route) => {
    const r = route.request();
    if (r.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    if (await modelList(route, { object: 'list', data: [{ id: 'gpt-test', created: 1, object: 'model' }] })) return;
    expect(new URL(r.url()).pathname).toBe('/v1/responses');
    const body = r.postDataJSON();
    bodies.push(body);
    const output = bodies.length === 1
      ? [{ type: 'reasoning', id: 'rs_1', summary: [] }, { type: 'function_call', id: 'fc_1', call_id: 'call_1', name: 'workspace_read', arguments: '{}' }]
      : [{ type: 'message', id: 'msg_1', role: 'assistant', content: [{ type: 'output_text', text: 'The workspace has one definition.', annotations: [] }] }];
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'application/json' }, body: JSON.stringify({ id: 'resp', status: 'completed', output, usage: { input_tokens: 10, output_tokens: 5 } }) });
  });
  await enterKey(page, async (p) => {
    await p.getByLabel('Provider').selectOption('openai');
    await p.getByLabel('API key').fill('sk-test');
    await p.getByLabel('API key').blur();
    await expect(p.getByLabel('Model', { exact: true })).toHaveValue('gpt-test');
  });
  await page.getByLabel('Message to the AI').fill('what is here?');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat')).toContainText('The workspace has one definition.');
  expect(bodies).toHaveLength(2);
  expect(bodies[0]).toMatchObject({ model: 'gpt-test', store: false });
  expect(bodies[0]).not.toHaveProperty('reasoning_effort');
  expect((bodies[0].tools as { type: string; name: string }[])[0]).toMatchObject({ type: 'function', name: 'workspace_read' });
  const input = bodies[1].input as Record<string, unknown>[];
  expect(input.some((x) => x.type === 'function_call' && x.call_id === 'call_1')).toBe(true);
  expect(input.some((x) => x.type === 'function_call_output' && x.call_id === 'call_1')).toBe(true);
});

test('with "Ask before the AI changes a document\'s context" on, its context change waits for the person', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await recordedModel(page, [
    () => sse([{ type: 'tool_use', name: 'context_set', input: { context: 'nilhecke nu=2', note: 'try the nilHecke algebra' } }], 'tool_use'),
    () => sse([{ type: 'text', text: 'Proposed the nilHecke context.' }], 'end_turn'),
    () => sse([{ type: 'tool_use', name: 'context_set', input: { context: 'nilhecke nu=3', note: 'three strands' } }], 'tool_use'),
    () => sse([{ type: 'text', text: 'Changed the context.' }], 'end_turn'),
  ]);
  await enterKey(page);
  await page.goto(page.url().replace(/#.*/, '#/settings'));
  await page.getByTestId('ask-context').check();
  await page.goBack();
  const chip = page.getByTestId('context').locator('.ctx');
  await expect(chip).toHaveAttribute('data-text', /^klr/);

  await page.getByLabel('Message to the AI').fill('Try nilHecke.');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat')).toContainText('Proposed the nilHecke context.');
  const proposal = page.getByTestId('proposal');
  await expect(proposal).toContainText('set the context to “nilhecke nu=2”');
  await expect(chip).toHaveAttribute('data-text', /^klr/);
  await proposal.getByRole('button', { name: 'Allow' }).click();
  await expect(chip).toHaveAttribute('data-text', /^nilhecke nu=2/);

  // off again: the AI changes it straight away
  await page.goto(page.url().replace(/#.*/, '#/settings'));
  await page.getByTestId('ask-context').uncheck();
  await page.goBack();
  await page.getByLabel('Message to the AI').fill('Three strands.');
  await page.keyboard.press('Enter');
  await expect(page.getByTestId('chat')).toContainText('Changed the context.');
  await expect(chip).toHaveAttribute('data-text', /^nilhecke nu=3/);
  await expect(page.getByTestId('proposal')).toHaveCount(0);
});
