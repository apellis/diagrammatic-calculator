// Light, dark, or whatever the system uses: kept on this device, applied to the root element.
import { createSignal } from 'solid-js';

export type Theme = 'light' | 'system' | 'dark';
const KEY = 'diagcalc.theme';

export function loadTheme(): Theme {
  try {
    const t = localStorage.getItem(KEY);
    return t === 'light' || t === 'dark' ? t : 'system';
  } catch {
    return 'system';
  }
}

/** Sets the theme on the page (styles.css reads `data-theme`; none means the system's). */
export function applyTheme(t: Theme) {
  if (t === 'system') delete document.documentElement.dataset.theme;
  else document.documentElement.dataset.theme = t;
}

export function saveTheme(t: Theme) {
  try {
    if (t === 'system') localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, t);
  } catch {
    // storage blocked: the choice lasts for this visit
  }
  applyTheme(t);
}

// Strand patterns: dashes on strands of different colours, so colour is not the only cue. Kept on
// this device; styles.css reads `data-patterns` on the root element for drawings in cells, and the
// canvas reads the signal.
const PATTERNS_KEY = 'diagcalc.patterns';
const [patterns, setPatternsSignal] = createSignal(loadPatterns());
export { patterns };

function loadPatterns(): boolean {
  try {
    return localStorage.getItem(PATTERNS_KEY) === '1';
  } catch {
    return false;
  }
}

export function applyPatterns(on = patterns()) {
  if (on) document.documentElement.dataset.patterns = '';
  else delete document.documentElement.dataset.patterns;
}

export function savePatterns(on: boolean) {
  try {
    localStorage.setItem(PATTERNS_KEY, on ? '1' : '0');
  } catch {
    // storage blocked: the choice lasts for this visit
  }
  setPatternsSignal(on);
  applyPatterns(on);
}
