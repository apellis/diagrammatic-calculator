// The AI provider, model and key: set on the settings page, read by the AI panel when it sends.
import { createEffect, createSignal, For, Show } from 'solid-js';
import {
  ANTHROPIC_MODELS,
  BUDGETS,
  checkProvider,
  listModels,
  loadKey,
  loadModels,
  loadSettings,
  saveKey,
  saveModels,
  saveSettings,
  hasServerUrl,
  type AISettings,
  type ModelOption,
  type ProviderId,
} from '../ai/providers';

/** The model menu's last entry, which switches to typing a model name. */
const OTHER = '__other__';

export function AISettingsForm(props: { onDirtyChange?: (dirty: boolean) => void } = {}) {
  const [settings, setSettings] = createSignal<AISettings>(loadSettings());
  const stored = loadKey(settings().provider);
  const [key, setKey] = createSignal(stored.key);
  const [remember, setRemember] = createSignal(stored.remembered);
  const snapshot = () => JSON.stringify([settings(), key(), remember()]);
  const [baseline, setBaseline] = createSignal(snapshot());
  const dirty = () => snapshot() !== baseline();
  createEffect(() => props.onDirtyChange?.(dirty()));
  const [saved, setSaved] = createSignal(false);
  const [check, setCheck] = createSignal<{ ok: boolean; message: string } | null>(null);
  const [checking, setChecking] = createSignal(false);
  const runCheck = async () => {
    setCheck(null);
    setChecking(true);
    try {
      setCheck(await checkProvider(key(), settings()));
    } finally {
      setChecking(false);
    }
  };
  /** The provider's model list (as last fetched), whether it is being fetched, and why it could not be. */
  const [models, setModels] = createSignal<ModelOption[]>(loadModels(settings())?.models ?? []);
  const [listing, setListing] = createSignal(false);
  const [listError, setListError] = createSignal('');
  const [other, setOther] = createSignal(false);
  /** Why the server URL cannot be used, shown once the person has left the field or asked for the list. */
  const [urlProblem, setUrlProblem] = createSignal('');
  const compatible = () => settings().provider === 'compatible';
  /** For a compatible server: whether its URL can be used; if not, says why. */
  const urlOk = () => {
    if (!compatible() || hasServerUrl(settings())) return setUrlProblem(''), true;
    setUrlProblem(settings().baseUrl.trim() ? 'Enter a URL starting with http:// or https://, such as https://my-server.example/v1.' : 'Enter the server URL first.');
    return false;
  };

  const saveAll = () => {
    saveSettings(settings());
    saveKey(settings().provider, key(), remember());
    setBaseline(snapshot());
    setSaved(true);
  };
  const setProvider = (p: ProviderId) => {
    const s = { ...settings(), provider: p, model: p === 'anthropic' ? 'claude-opus-5-5' : '' };
    setSettings(s);
    const k = loadKey(p);
    setKey(k.key);
    setRemember(k.remembered);
    setCheck(null);
    setUrlProblem('');
    showModels();
  };

  /** The models offered: the fetched list, else the built-in one for Anthropic; the chosen one is always there. */
  const options = () => {
    const list = models().length ? models() : settings().provider === 'anthropic' ? ANTHROPIC_MODELS : [];
    const m = settings().model;
    return m && !list.some((x) => x.id === m) ? [{ id: m, name: m }, ...list] : list;
  };
  /** Fetches the provider's model list with the key entered, keeps it, and picks one if none is chosen. */
  const refreshModels = async () => {
    const s = settings();
    // a compatible server is asked only once its URL is entered; until then there is nothing to say
    if (s.provider === 'compatible' && !hasServerUrl(s)) return setListError('');
    if (!key() && s.provider !== 'compatible') return setListError('Enter the key first: the list comes from the provider.');
    setListing(true);
    setListError('');
    try {
      const list = await listModels(key(), s);
      if (settings().provider !== s.provider) return;
      saveModels(s, list);
      setModels(list);
      if (list.length && !list.some((x) => x.id === settings().model)) setSettings({ ...settings(), model: list[0].id });
      if (!list.length) {
        setListError('The provider listed no models; type a model name.');
        setOther(true);
      }
    } catch (e) {
      setListError((e as Error).message);
    } finally {
      setListing(false);
    }
  };
  /**
   * Shows the kept list for the current provider, fetching it when there is none yet. A compatible
   * server's list is known only for its URL once fetched; it is fetched when the URL is entered.
   */
  const showModels = () => {
    setModels(!compatible() || hasServerUrl(settings()) ? (loadModels(settings())?.models ?? []) : []);
    setListError('');
    setOther(false);
    if (!models().length && key() && !compatible()) void refreshModels();
  };
  /** Leaving the URL field: an empty field says nothing; a usable URL fetches the list; anything else is marked. */
  const leaveUrl = () => {
    if (settings().baseUrl.trim() && urlOk()) void refreshModels();
  };
  /** What the empty model menu of a compatible server says. */
  const noModels = () =>
    listing() ? 'Loading the models…'
    : listError() ? 'No models: see below'
    : hasServerUrl(settings()) ? 'Refresh to load the models'
    : 'Enter the server URL to load the models';
  if (key() && !models().length) void refreshModels();

  return (
    <div class="ai-settings" onInput={() => setSaved(false)} onChange={() => setSaved(false)}>
      <p class="hint">Changes here apply only after Save AI settings. Keys are never saved automatically.</p>
      <label>Provider
        <select value={settings().provider} onChange={(e) => setProvider(e.currentTarget.value as ProviderId)} aria-label="Provider">
          <option value="anthropic">Anthropic</option>
          <option value="openai">OpenAI</option>
          <option value="compatible">OpenAI-compatible server</option>
        </select>
      </label>
      <Show when={settings().provider === 'compatible'}>
        <label>Server URL
          <input value={settings().baseUrl} placeholder="https://my-server.example/v1" aria-label="Server URL" classList={{ invalid: !!urlProblem() }} aria-invalid={!!urlProblem()}
            onInput={(e) => {
              // the list and model belong to the server they came from
              setSettings({ ...settings(), baseUrl: e.currentTarget.value, model: '' });
              setModels([]);
              setListError('');
              setOther(false);
              setUrlProblem('');
            }}
            onBlur={leaveUrl} onKeyDown={(e) => e.key === 'Enter' && leaveUrl()} />
        </label>
        <Show when={urlProblem()}><div class="error small" data-testid="url-problem">{urlProblem()}</div></Show>
      </Show>
      <label>Model
        <span class="row model-pick">
          <Show when={options().length || other() || !compatible()} fallback={
            <select aria-label="Model" disabled><option selected>{noModels()}</option></select>
          }>
          <Show when={options().length && !other()} fallback={
            <input value={settings().model} placeholder="model name" onInput={(e) => setSettings({ ...settings(), model: e.currentTarget.value })} aria-label="Model" />
          }>
            <select aria-label="Model" onChange={(e) => {
              if (e.currentTarget.value === OTHER) return setOther(true);
              setSettings({ ...settings(), model: e.currentTarget.value });
            }}>
              {/* each option says whether it is chosen, so the choice survives the list changing */}
              <Show when={!settings().model}><option value="" selected>Choose a model</option></Show>
              <For each={options()}>{(m) => <option value={m.id} selected={m.id === settings().model}>{m.name}</option>}</For>
              <option value={OTHER}>Other…</option>
            </select>
          </Show>
          </Show>
          <button class="ghost refresh" classList={{ spinning: listing() }} disabled={listing()} onClick={(e) => {
              e.preventDefault();
              if (!urlOk()) return;
              setOther(false);
              void refreshModels();
            }}
            title="Fetch the list of models again from the provider" aria-label="Refresh the model list" data-testid="models-refresh">↻</button>
        </span>
      </label>
      <Show when={listError()}><div class="error small" data-testid="models-error">{listError()}</div></Show>
      <Show when={settings().provider === 'anthropic'}>
        <label title="Higher effort thinks longer and costs more">Effort
          <select value={settings().effort} onChange={(e) => setSettings({ ...settings(), effort: e.currentTarget.value as AISettings['effort'] })} aria-label="Effort">
            <For each={['low', 'medium', 'high', 'xhigh', 'max']}>{(x) => <option value={x}>{x}</option>}</For>
          </select>
        </label>
      </Show>
      <label title="A request stops when it has used this many tokens, so a long loop cannot use up your credit">Limit per request
        <select value={settings().budget} onChange={(e) => setSettings({ ...settings(), budget: Number(e.currentTarget.value) })} aria-label="Token limit per request" data-testid="ai-budget">
          <For each={BUDGETS}>{(b) => <option value={b}>{b ? `${b.toLocaleString('en')} tokens` : 'no limit'}</option>}</For>
        </select>
      </label>
      <label>Key <input type="password" value={key()} onInput={(e) => setKey(e.currentTarget.value)} onChange={() => key() && void refreshModels()} placeholder={settings().provider === 'compatible' ? 'only if the server needs one' : 'your API key'} aria-label="API key" autocomplete="off" /></label>
      <label class="row hint"><input type="checkbox" checked={remember()} onChange={(e) => setRemember(e.currentTarget.checked)} /> Remember on this device (otherwise for this session only)</label>
      <div class="row">
        <button class="primary" onClick={saveAll} data-testid="ai-save">Save AI settings</button>
        <Show when={dirty()}><span class="hint" role="status" data-testid="ai-dirty">Unsaved AI settings</span></Show>
        <Show when={saved() && !dirty()}><span class="hint" data-testid="ai-saved">Saved.</span></Show>
        <button onClick={() => void runCheck()} disabled={checking()} data-testid="ai-check-run">
          <Show when={checking()} fallback="Check the connection"><span class="spinner" aria-hidden="true" /> Checking…</Show>
        </button>
      </div>
      <Show when={check()}>
        {(c) => (
          <div class="check-result" classList={{ ok: c().ok, bad: !c().ok }} role="status" data-testid="ai-check">
            <span class="mark" aria-hidden="true">{c().ok ? '✓' : '!'}</span>
            <span>{c().message}</span>
          </div>
        )}
      </Show>
      <div class="hint">The key is sent only to the provider. Your calls are billed to it.</div>
    </div>
  );
}
