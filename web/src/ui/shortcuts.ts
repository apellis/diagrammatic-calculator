// Keyboard shortcuts of the workspace: the main ones can be rebound on the settings page. The
// bindings are kept in this browser (only those changed from the defaults) and read here by every
// key handler, the command palette, and the shortcut sheets.
import { createSignal } from 'solid-js';
import { isMac, mod } from '../app';

/**
 * A key combination as text: modifiers then the key, joined by `+`, e.g. `Mod+K`, `Mod+Shift+S`,
 * `Alt+Enter`, `F`, `?`. `Mod` is ⌘ on a Mac and Ctrl elsewhere. A letter is written in upper case
 * and matches either case; any other key is its `KeyboardEvent.key` (`Enter`, `?`, `/`, `F2`).
 */
export type Combo = string;

export interface Action {
  id: string;
  /** What it does, as the settings page and the sheet name it. */
  name: string;
  default: Combo;
  /** Whether the shortcut also works while a cell editor or text field has the focus. */
  anywhere: boolean;
}

/** The actions whose keys can be changed, in the order the settings page lists them. */
export const ACTIONS: Action[] = [
  { id: 'palette', name: 'Command palette', default: 'Mod+K', anywhere: true },
  { id: 'ai', name: 'Show or hide the AI panel', default: 'Mod+J', anywhere: true },
  { id: 'ask', name: 'Ask the AI about the selection', default: 'Mod+L', anywhere: true },
  { id: 'branch', name: 'Start a scratch branch', default: 'Mod+B', anywhere: true },
  { id: 'export', name: 'Export the document as a file', default: 'Mod+Shift+S', anywhere: true },
  { id: 'import', name: 'Import a file as a new document', default: 'Mod+O', anywhere: true },
  { id: 'insert', name: 'Insert a cell below the active cell', default: 'Alt+Enter', anywhere: false },
  { id: 'canvas', name: 'Open the active cell in the focus canvas', default: 'F', anywhere: false },
  { id: 'keys', name: 'Keyboard shortcuts', default: '?', anywhere: false },
];
export type ActionId = (typeof ACTIONS)[number]['id'];

/** A name inside a sentence: its first letter in lower case (AI stays as it is). */
const lower = (s: string) => s[0].toLowerCase() + s.slice(1);

/** Keys that stay as they are: shown on the settings page, never offered for another action. */
export const FIXED: [string, Combo[]][] = [
  ['Undo', ['Mod+Z']],
  ['Redo', ['Mod+Shift+Z', 'Mod+Y']],
  ['Commit the cell being edited', ['Enter', 'Mod+Enter']],
  ['Stop editing, close a dialog', ['Escape']],
  ['Move between cells and controls', ['Tab', 'Shift+Tab']],
  ['Select all in the canvas', ['Mod+A']],
];

const KEY = 'diagcalc.shortcuts';
// (declared before the bindings are first read, which happens as this module loads)
const MODS = ['Mod', 'Ctrl', 'Alt', 'Shift'] as const;

function load(): Record<string, Combo> {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '{}') as unknown;
    if (!v || typeof v !== 'object' || Array.isArray(v)) return {};
    const out: Record<string, Combo> = {};
    for (const a of ACTIONS) {
      const c = (v as Record<string, unknown>)[a.id];
      if (typeof c === 'string' && valid(c)) out[a.id] = c;
    }
    return out;
  } catch {
    return {};
  }
}

const [overrides, setOverrides] = createSignal(load());

function store(o: Record<string, Combo>) {
  setOverrides(o);
  try {
    if (Object.keys(o).length) localStorage.setItem(KEY, JSON.stringify(o));
    else localStorage.removeItem(KEY);
  } catch {
    // storage blocked: the bindings last for this visit
  }
}

/** The combination bound to an action now. */
export const binding = (id: ActionId): Combo => overrides()[id] ?? ACTIONS.find((a) => a.id === id)!.default;

/** Whether an action's binding differs from its default. */
export const changed = (id: ActionId) => id in overrides();

function parse(c: Combo): { mods: Set<string>; key: string } {
  const parts = c.split('+');
  // a combination ending in "+" is the plus key itself
  const key = c.endsWith('++') || c === '+' ? '+' : parts.pop()!;
  return { mods: new Set(parts.filter((p) => p)), key };
}

function valid(c: Combo): boolean {
  const { mods, key } = parse(c);
  return !!key && [...mods].every((m) => (MODS as readonly string[]).includes(m));
}

/** Whether a key event is the combination. */
export function matches(e: KeyboardEvent, c: Combo): boolean {
  const { mods, key } = parse(c);
  if (mods.has('Mod') !== mod(e)) return false;
  // on a Mac, Ctrl is its own modifier; elsewhere it is Mod
  if (isMac && mods.has('Ctrl') !== e.ctrlKey) return false;
  if (mods.has('Alt') !== e.altKey) return false;
  const letter = /^[A-Z0-9]$/.test(key);
  // Shift is part of a symbol such as ? (Shift+/): it counts only for letters, digits, and named keys
  if ((letter || key.length > 1) && mods.has('Shift') !== e.shiftKey) return false;
  if (letter) return e.key.toUpperCase() === key || e.code === (/[0-9]/.test(key) ? `Digit${key}` : `Key${key}`);
  return e.key === key;
}

/** Whether a key event is the shortcut of an action (outside an editor unless the action works anywhere). */
export function isAction(e: KeyboardEvent, id: ActionId, inEditor = false): boolean {
  const a = ACTIONS.find((x) => x.id === id)!;
  if (inEditor && !a.anywhere) return false;
  return matches(e, binding(id));
}

/** The combination a key event makes, or null for a bare modifier. */
export function comboOf(e: KeyboardEvent): Combo | null {
  if (['Shift', 'Control', 'Alt', 'Meta', 'OS', 'Dead', 'Unidentified'].includes(e.key)) return null;
  const parts: string[] = [];
  if (mod(e)) parts.push('Mod');
  if (isMac && e.ctrlKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  // with Alt (Option on a Mac) the key may be a composed character: take the physical key then
  let key = e.key;
  if (e.altKey && /^Key[A-Z]$|^Digit[0-9]$/.test(e.code)) key = e.code.slice(-1);
  if (key.length === 1 && /[a-z]/i.test(key)) key = key.toUpperCase();
  if (key === ' ') key = 'Space';
  const letter = /^[A-Z0-9]$/.test(key);
  if (e.shiftKey && (letter || key.length > 1)) parts.push('Shift');
  parts.push(key);
  return parts.join('+');
}

const SYMBOL: Record<string, string> = { Enter: '↵', Escape: 'Esc', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', Space: 'Space', Backspace: '⌫', Delete: 'Del' };

/** A combination as shown: ⌘⇧S on a Mac, Ctrl+Shift+S elsewhere. */
export function label(c: Combo): string {
  const { mods, key } = parse(c);
  const k = isMac ? (SYMBOL[key] ?? key) : key === 'Escape' ? 'Esc' : key;
  if (isMac) {
    const m = (mods.has('Ctrl') ? '⌃' : '') + (mods.has('Alt') ? '⌥' : '') + (mods.has('Shift') ? '⇧' : '') + (mods.has('Mod') ? '⌘' : '');
    return m + k;
  }
  return [...(mods.has('Mod') || mods.has('Ctrl') ? ['Ctrl'] : []), ...(mods.has('Alt') ? ['Alt'] : []), ...(mods.has('Shift') ? ['Shift'] : []), k].join('+');
}

/** The keys of an action, as shown. */
export const keysOf = (id: ActionId) => label(binding(id));

/**
 * Why a combination cannot be given to an action, or null if it can: it is fixed, used by another
 * action, a key that types text in an editor while the action works there, or a bare key that
 * cells and the canvas use.
 */
export function problem(id: ActionId, c: Combo): string | null {
  const a = ACTIONS.find((x) => x.id === id)!;
  const { mods, key } = parse(c);
  const fixed = FIXED.find(([, cs]) => cs.includes(c));
  if (fixed) return `${label(c)} is kept for ${lower(fixed[0])}.`;
  const other = ACTIONS.find((x) => x.id !== id && binding(x.id) === c);
  if (other) return `${label(c)} is already the shortcut for ${lower(other.name)}.`;
  const plain = !mods.has('Mod') && !mods.has('Ctrl') && !mods.has('Alt');
  if (plain && a.anywhere) return `${a.name} also works while you type in a cell, so it needs ${isMac ? '⌘, ⌃, or ⌥' : 'Ctrl or Alt'}.`;
  if (plain && ['Enter', 'Tab', 'Space', 'Backspace', 'Delete'].includes(key)) return `${label(c)} is needed for moving around and editing.`;
  if (plain && key.startsWith('Arrow')) return 'The arrow keys move between cells and pieces.';
  return null;
}

/** Binds an action to a combination (its default again when `c` is null); returns why not, if refused. */
export function rebind(id: ActionId, c: Combo | null): string | null {
  const a = ACTIONS.find((x) => x.id === id)!;
  if (c !== null) {
    const p = problem(id, c);
    if (p) return p;
  }
  const o = { ...overrides() };
  if (c === null || c === a.default) delete o[id];
  else o[id] = c;
  store(o);
  return null;
}

/** Puts every shortcut back to its default. */
export const resetAll = () => store({});
