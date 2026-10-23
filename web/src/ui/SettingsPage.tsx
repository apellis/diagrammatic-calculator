// Settings for this device: the appearance, and the AI provider with its key.
import { createSignal, For, Show } from 'solid-js';
import { AISettingsForm } from './AISettingsForm';
import { askBeforeContext, setAskBeforeContext } from '../ai/tools';
import { ACTIONS, FIXED, changed, comboOf, keysOf, label, rebind, resetAll, type ActionId } from './shortcuts';
import { SignOutEverywhere } from './Account';
import { loadTheme, patterns, savePatterns, saveTheme, type Theme } from './theme';

const THEMES: { id: Theme; name: string }[] = [
  { id: 'light', name: 'Light' },
  { id: 'system', name: 'System' },
  { id: 'dark', name: 'Dark' },
];

/** `from`: the route the settings were opened from, returned to by Back (the document list if none). */
export function SettingsPage(props: { from?: string }) {
  return (
    <div class="doclist settings">
      <div class="row">
        <button class="ghost" onClick={() => (location.hash = props.from || '#/')} data-testid="settings-back">← Back{props.from?.startsWith('#/doc/') ? ' to the document' : ''}</button>
      </div>
      <h1>Settings</h1>
      <p class="muted">These settings are kept in this browser and apply to every document.</p>
      <section>
        <h3>Appearance</h3>
        <ThemeSwitch />
        <label class="row" style={{ 'margin-top': '12px' }}>
          <input type="checkbox" checked={patterns()} onChange={(e) => savePatterns(e.currentTarget.checked)} data-testid="patterns" />
          Strand patterns: dashes on strands of different colours, in cells and on the canvas, so colour is not the only cue
        </label>
      </section>
      <section>
        <h3>AI assistant</h3>
        <p class="muted">The assistant uses your own key with the provider you choose. Every message goes from this browser straight to that provider.</p>
        <AISettingsForm />
        <label class="row" style={{ 'margin-top': '12px' }}>
          <input type="checkbox" checked={askBeforeContext()} onChange={(e) => setAskBeforeContext(e.currentTarget.checked)} data-testid="ask-context" />
          Ask before the AI changes a document's context (on documents of others it always asks)
        </label>
      </section>
      <Shortcuts />
      <SignOutEverywhere />
    </div>
  );
}

/** A three-position slider: Light, System, Dark. Arrow keys move it. */
function ThemeSwitch() {
  const [theme, setTheme] = createSignal(loadTheme());
  const pick = (t: Theme) => {
    setTheme(t);
    saveTheme(t);
  };
  const at = () => THEMES.findIndex((x) => x.id === theme());
  const step = (d: number) => pick(THEMES[Math.min(THEMES.length - 1, Math.max(0, at() + d))].id);
  return (
    <div class="theme-switch" role="radiogroup" aria-label="Theme" data-testid="theme-switch" data-theme-choice={theme()}
      onKeyDown={(e) => {
        if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') step(-1);
        else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') step(1);
        else return;
        e.preventDefault();
        (e.currentTarget.querySelector('[aria-checked="true"]') as HTMLElement | null)?.focus();
      }}>
      <div class="knob" style={{ transform: `translateX(${at() * 100}%)` }} aria-hidden="true" />
      <For each={THEMES}>
        {(t) => (
          <button role="radio" aria-checked={theme() === t.id} tabindex={theme() === t.id ? 0 : -1} onClick={() => pick(t.id)}>
            {t.name}
          </button>
        )}
      </For>
    </div>
  );
}

/**
 * The keyboard shortcuts: every one listed, and the main ones rebindable. "Change" listens for the
 * next key combination; Esc cancels.
 */
function Shortcuts() {
  const [listening, setListening] = createSignal<ActionId | null>(null);
  const [error, setError] = createSignal('');
  const listen = (id: ActionId) => {
    setError('');
    setListening(listening() === id ? null : id);
  };
  const onKey = (id: ActionId, e: KeyboardEvent) => {
    if (listening() !== id) return;
    if (e.key === 'Tab') return;
    e.preventDefault();
    e.stopPropagation();
    if (e.key === 'Escape') return setListening(null);
    const c = comboOf(e);
    if (!c) return; // a modifier on its own: wait for the key
    const why = rebind(id, c);
    setError(why ?? '');
    if (!why) setListening(null);
  };
  return (
    <section data-testid="shortcuts">
      <h3>Keyboard shortcuts</h3>
      <p class="muted">The main shortcuts of a document can be changed. Choose Change, then press the new keys; Esc keeps the old ones.</p>
      <table class="kv keys">
        <thead><tr><th>Action</th><th>Keys</th><th /></tr></thead>
        <tbody>
          <For each={ACTIONS}>
            {(a) => (
              <tr data-action={a.id}>
                <td>{a.name}</td>
                <td><kbd data-testid={`keys-${a.id}`}>{keysOf(a.id)}</kbd></td>
                <td class="row">
                  <button onClick={() => listen(a.id)} onKeyDown={(e) => onKey(a.id, e)} onBlur={() => listening() === a.id && setListening(null)}
                    aria-pressed={listening() === a.id} data-testid={`rebind-${a.id}`}>
                    {listening() === a.id ? 'Press the new keys…' : 'Change'}
                  </button>
                  <Show when={changed(a.id)}>
                    <button class="ghost" onClick={() => { rebind(a.id, null); setError(''); }} title={`Back to ${label(a.default)}`}>Reset</button>
                  </Show>
                </td>
              </tr>
            )}
          </For>
          <For each={FIXED}>
            {([name, combos]) => (
              <tr class="fixed"><td>{name}</td><td><kbd>{combos.map(label).join(', ')}</kbd></td><td class="hint">fixed</td></tr>
            )}
          </For>
        </tbody>
      </table>
      <Show when={error()}><div class="error small" role="alert" data-testid="rebind-error">{error()}</div></Show>
      <div class="row" style={{ 'margin-top': '8px' }}>
        <button onClick={() => { resetAll(); setError(''); setListening(null); }} data-testid="shortcuts-reset">Reset all shortcuts</button>
        <span class="hint">The canvas's own keys are listed with ? in the canvas.</span>
      </div>
    </section>
  );
}
