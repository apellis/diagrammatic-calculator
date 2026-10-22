// Things the whole app shares: the engine, the result cache, and file helpers.
import { Engine } from './engine/engine';
import { ResultCache } from './eval/cache';
import wasmUrl from './engine/pkg/diagcalc_wasm_bg.wasm?url';

export const engine = new Engine();
export const cache = new ResultCache();
/** The engine build: results cached by one build are never read by another. */
export const engineId = engine.version.then((v) => `${v} ${wasmUrl}`);

/** Downloads text as a file. */
export function download(name: string, text: string, type = 'text/plain') {
  const a = document.createElement('a');
  a.href = URL.createObjectURL(new Blob([text], { type }));
  a.download = name;
  a.click();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}

/** Lets the person pick a file and returns its name and text. */
export function pickFile(accept: string): Promise<{ name: string; text: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = accept;
    input.onchange = async () => {
      const f = input.files?.[0];
      resolve(f ? { name: f.name, text: await f.text() } : null);
    };
    input.click();
  });
}

export const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
export const mod = (e: KeyboardEvent) => (isMac ? e.metaKey : e.ctrlKey);

export function ago(t: number): string {
  const s = (Date.now() - t) / 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return new Date(t).toLocaleDateString();
}
