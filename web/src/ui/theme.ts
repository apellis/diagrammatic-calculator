// Light, dark, or whatever the system uses: kept on this device, applied to the root element.
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
