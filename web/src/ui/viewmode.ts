// View mode: reading a document with every editing affordance put away. Whether it is on is a
// choice of the person viewing, kept per document in this browser only: it is never written to the
// shared document, so co-editors never see it and it changes nothing they read. A view-only
// recipient is always in view mode on the document itself, whatever is stored here.

const KEY = 'diagcalc.viewMode';
/** The most documents remembered; the oldest choice is forgotten first. */
const MAX = 200;

function read(): string[] {
  try {
    const v = JSON.parse(localStorage.getItem(KEY) ?? '[]') as unknown;
    return Array.isArray(v) ? v.filter((x): x is string => typeof x === 'string') : [];
  } catch {
    return [];
  }
}

/** Whether view mode was left on for a document (by its local id) in this browser. */
export function loadViewMode(id: string): boolean {
  return read().includes(id);
}

/** Remembers whether view mode is on for a document in this browser. */
export function saveViewMode(id: string, on: boolean) {
  const ids = read().filter((x) => x !== id);
  if (on) ids.push(id);
  try {
    if (ids.length) localStorage.setItem(KEY, JSON.stringify(ids.slice(-MAX)));
    else localStorage.removeItem(KEY);
  } catch {
    // storage blocked: the choice lasts for this visit
  }
}
