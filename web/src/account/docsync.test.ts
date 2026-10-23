import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import * as Y from 'yjs';
import { DocSync } from './docsync';
import { ApiError } from './api';
import { uploadOnce } from './account';
import type { OpenDoc } from '../doc/store';
import type { LiveOptions, LiveStatus } from './live';
const mocks = vi.hoisted(() => ({ opts: null as LiveOptions | null, run: vi.fn(), update: vi.fn(), stopped: vi.fn(), saved: vi.fn(async () => {}), presence: vi.fn() }));
vi.mock('./account', () => ({ api: {}, uploadOnce: vi.fn() }));
vi.mock('../doc/store', () => ({ updateInfo: mocks.update }));
vi.mock('./sync', () => ({ CLOUD: {}, syncDoc: mocks.run }));
vi.mock('./live', () => ({ LiveProvider: class {
  role = 'editor'; constructor(opts: LiveOptions) { mocks.opts = opts; }
  start() {} stop() { mocks.stopped(); } whenSaved() { return mocks.saved(); }
  setPresence(fields: unknown) { mocks.presence(fields); }
} }));
const user = { id: 'u', name: 'Ada', email: 'ada@example.org' };
const active: DocSync[] = [];
function setup(generation = 1) {
  const cloud = { id: 'cloud', account: 'u', role: 'editor' as const, version: 1, generation };
  const open = { doc: { ydoc: new Y.Doc(), name: 'doc' }, info: { id: 'local', cloud } } as unknown as OpenDoc;
  const doc = new DocSync(open, user); active.push(doc); doc.start(); return { doc, open, cloud };
}
function status(value: LiveStatus) { mocks.opts!.onStatus!(value); }
beforeEach(() => { vi.useFakeTimers(); vi.clearAllMocks(); vi.stubGlobal('document', { addEventListener: vi.fn(), removeEventListener: vi.fn(), visibilityState: 'visible' }); });
afterEach(async () => { await Promise.all(active.splice(0).map(d => d.stop())); vi.unstubAllGlobals(); vi.useRealTimers(); });
describe('DocSync transport selection', () => {
  it('stops once even after recovery detaches the original', async () => {
    const { doc, open, cloud } = setup(0); mocks.run.mockResolvedValue(cloud);
    status('migration-required'); await vi.advanceTimersByTimeAsync(0);
    const stopping = doc.stop(); await stopping;
    open.info.cloud = undefined;
    expect(doc.stop()).toBe(stopping);
    await doc.stop();
    expect(uploadOnce).not.toHaveBeenCalled();
  });
  it('waits for pending initial upload before stop resolves', async () => {
    let resolve!: (value: any) => void;
    vi.mocked(uploadOnce).mockReturnValueOnce(new Promise(r => { resolve = r; }));
    const open = { doc: { ydoc: new Y.Doc(), name: 'local' }, info: { id: 'local' } } as unknown as OpenDoc;
    const doc = new DocSync(open, user); active.push(doc); doc.start();
    let finished = false; const stopping = doc.stop().then(() => { finished = true; });
    await Promise.resolve(); expect(finished).toBe(false);
    resolve({ id: 'new', account: 'u', role: 'owner', version: 1 }); await stopping;
    expect(doc.live).toBeNull();
  });
  it('drains pending fallback work once and saves edits made while it was running', async () => {
    const { doc, open, cloud } = setup();
    let resolve!: (value: typeof cloud) => void;
    mocks.run.mockReturnValueOnce(new Promise(r => { resolve = r; })).mockResolvedValue(cloud);
    status('unavailable'); await vi.advanceTimersByTimeAsync(0);
    open.doc.ydoc.getMap('meta').set('late', 1);
    const stopping = doc.stop(); expect(doc.stop()).toBe(stopping);
    let finished = false; void stopping.then(() => { finished = true; });
    await Promise.resolve(); expect(finished).toBe(false);
    resolve(cloud); await stopping;
    expect(mocks.run).toHaveBeenCalledTimes(2);
    open.info.cloud = undefined; await doc.stop();
    await vi.advanceTimersByTimeAsync(120000);
    expect(mocks.run).toHaveBeenCalledTimes(2); expect(uploadOnce).not.toHaveBeenCalled();
  });
  it('waits for pending role metadata before recovery can detach', async () => {
    const { doc } = setup();
    let resolve!: () => void;
    mocks.update.mockReturnValueOnce(new Promise<void>(r => { resolve = r; }));
    doc.live!.role = 'viewer'; status('view-only');
    let finished = false; const stopping = doc.stop().then(() => { finished = true; });
    await Promise.resolve(); await Promise.resolve(); expect(finished).toBe(false);
    resolve(); await stopping; expect(finished).toBe(true);
  });
  it('never starts transport for a reopened local-only recovery', async () => {
    const open = { doc: { ydoc: new Y.Doc(), name: 'recovery' }, info: { id: 'local', localOnly: true } } as unknown as OpenDoc;
    const doc = new DocSync(open, user); active.push(doc); doc.start();
    await vi.advanceTimersByTimeAsync(0); await doc.stop();
    expect(uploadOnce).not.toHaveBeenCalled(); expect(mocks.run).not.toHaveBeenCalled();
  });
  it('keeps a document editable while its first cloud upload is pending', () => {
    const open = { doc: { ydoc: new Y.Doc(), name: 'local' }, info: { id: 'local' } } as unknown as OpenDoc;
    const doc = new DocSync(open, user); active.push(doc);
    expect(doc.cloudId).toBeUndefined(); expect(doc.canEdit).toBe(true);
  });
  it('uses live by default without a periodic REST writer', async () => {
    const { doc, open } = setup();
    expect(mocks.opts!.generation).toBe(1);
    open.doc.ydoc.getMap('meta').set('local', 1); status('live');
    await vi.advanceTimersByTimeAsync(120000);
    expect(mocks.run).not.toHaveBeenCalled(); expect(doc.status()).toBe('live');
    await doc.saveNow(); expect(mocks.saved).toHaveBeenCalledTimes(1);
    doc.updatePresence({ cell: 2 }); expect(mocks.presence).toHaveBeenCalledWith({ cell: 2 });
    mocks.opts!.onPresence!([{ clientId: 12, user: { id: 'other', name: 'Other', colour: '#123' } }]);
    expect(doc.presence()).toHaveLength(1);
  });
  it('falls back to REST only for unavailable transport', async () => {
    const { doc, cloud } = setup(); mocks.run.mockResolvedValue(cloud);
    status('unavailable'); await vi.advanceTimersByTimeAsync(0);
    expect(mocks.stopped).toHaveBeenCalledTimes(1); expect(mocks.run).toHaveBeenCalledTimes(1);
    expect(doc.status()).toBe('saved');
  });
  it.each(['forbidden', 'auth-required', 'gone', 'generation-mismatch'] as const)('does not bypass %s with REST', async value => {
    const { doc } = setup(); status(value); await vi.advanceTimersByTimeAsync(120000);
    expect(mocks.run).not.toHaveBeenCalled(); expect(doc.canEdit).toBe(false);
  });
  it('legacy editor fallback is read-only', async () => {
    const { doc, cloud } = setup(0); mocks.run.mockResolvedValue(cloud);
    status('migration-required'); await vi.advanceTimersByTimeAsync(0);
    expect(doc.status()).toBe('view-only'); expect(doc.canEdit).toBe(false);
  });
  it('does not call a generation fence an ordinary save error', async () => {
    const { doc } = setup(); mocks.run.mockRejectedValue(new ApiError(409, 'generation mismatch'));
    status('unavailable'); await vi.advanceTimersByTimeAsync(0);
    expect(doc.status()).toBe('generation-mismatch'); expect(doc.canEdit).toBe(false);
  });
});
