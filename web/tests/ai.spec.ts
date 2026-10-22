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

async function recordedModel(page: Page, turns: (() => string)[]) {
  const requests: { headers: Record<string, string>; body: Record<string, unknown> }[] = [];
  await page.route('https://api.anthropic.com/**', async (route: Route) => {
    const r = route.request();
    if (r.method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
    requests.push({ headers: await r.allHeaders(), body: r.postDataJSON() });
    const next = turns.shift();
    if (!next) return route.fulfill({ status: 500, headers: cors, body: 'no more recorded turns' });
    await route.fulfill({ status: 200, headers: { ...cors, 'content-type': 'text/event-stream' }, body: next() });
  });
  return requests;
}

async function enterKey(page: Page) {
  await page.getByTestId('ai-toggle').click();
  await page.getByRole('button', { name: 'I understand' }).click();
  await page.getByLabel('API key').fill('sk-ant-test');
  await page.getByTestId('ai-save').click();
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

  // the activity timeline lists the request and the AI's steps with their notes
  await page.getByTestId('activity-tab').click();
  const tl = page.getByTestId('timeline');
  await expect(tl).toContainText('What if I slide the dot up');
  await expect(tl).toContainText('slide the dot up through the crossing');
  await expect(tl).toContainText('accepted: the slide is an equality');
});

test('a refused key and a refusal are shown as plain messages', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  let n = 0;
  await page.route('https://api.anthropic.com/**', async (route) => {
    if (route.request().method() === 'OPTIONS') return route.fulfill({ status: 204, headers: cors });
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

test('No AI keeps the document away from the AI, and the key stays for the session only by default', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := x_1 e(iij)');
  await enterKey(page);
  expect(await page.evaluate(() => [sessionStorage.getItem('diagcalc.ai.key.anthropic'), localStorage.getItem('diagcalc.ai.key.anthropic')])).toEqual(['sk-ant-test', null]);
  await page.getByText('No AI for this document').click();
  await expect(page.getByLabel('Message to the AI')).toBeDisabled();
});

test('a multi-step AI request undoes as one step; pinned watches sit at the top of the outline', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def e := e(iij)');
  await recordedModel(page, [
    () => sse([
      { type: 'tool_use', name: 'cell_add', input: { text: 'watch e e - e', note: 'is e idempotent?' } },
      { type: 'tool_use', name: 'cell_add', input: { text: 'check idempotent(e)', note: 'ask for a verdict too' } },
      { type: 'tool_use', name: 'watch_pin', input: { id: 'w2' } },
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

test('Cmd-L in the canvas asks about the selection, which travels with the message as references', async ({ page }) => {
  await newDoc(page);
  await typeCell(page, 'def a := psi_1 x_1 e(iij)');
  await expect(cell(page, 0).getByTestId('summary')).toHaveText('1 term');
  const requests = await recordedModel(page, [() => sse([{ type: 'text', text: 'That is a dot below a crossing.' }], 'end_turn')]);
  await enterKey(page);
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
  await newDoc(page);
  await page.getByTestId('ai-toggle').click();
  await page.getByRole('button', { name: 'I understand' }).click();
  await page.getByLabel('Provider').selectOption('compatible');
  await page.route('http://localhost:11434/**', (route) => route.abort('failed'));
  await page.getByRole('button', { name: 'Check the connection' }).click();
  await expect(page.getByTestId('ai-check')).toContainText('refuses calls made from a browser');
  await expect(page.getByTestId('ai-check')).toContainText('MCP');
  await page.unroute('http://localhost:11434/**');
  await page.route('http://localhost:11434/**', (route) => route.fulfill({ status: 200, headers: cors, contentType: 'application/json', body: '{"data":[]}' }));
  await page.getByRole('button', { name: 'Check the connection' }).click();
  await expect(page.getByTestId('ai-check')).toContainText('accepts calls from this page');
});
