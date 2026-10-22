// Settings for this device: the appearance, and the AI provider with its key.
import { createSignal, For } from 'solid-js';
import { AISettingsForm } from './AISettingsForm';
import { loadTheme, saveTheme, type Theme } from './theme';

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
      </section>
      <section>
        <h3>AI assistant</h3>
        <p class="muted">The assistant uses your own key with the provider you choose. Every message goes from this browser straight to that provider.</p>
        <AISettingsForm />
      </section>
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
