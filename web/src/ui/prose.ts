// Prose from presentation files typeset for reading: the certificate references and hypotheses
// that a result's badge shows (`Q_ab != 0`, `t_ij = 1, s_ij^pq = 0`). The files keep their plain
// text; only the display changes (docs/conventions.md, Display). Short formulas are found by
// shape: a name of one or two letters with a subscript or superscript, or an equation or
// inequation between such names and numbers. Lean declaration names are set as code, and a hyphen
// between two capitalised names (Brundan-Ellis, Kac-Moody) becomes an en dash.
import { render, toTex } from './tex';

const SCRIPT = String.raw`(?:\{[^{}]*\}|\([^()]*\)|[A-Za-z0-9]{1,3})`;
const ATOM = String.raw`(?:[A-Za-z]{1,2}(?:_${SCRIPT})?(?:\^${SCRIPT})?(?:\([^()]{0,24}\))?|-?[0-9]+)`;
const FORMULA = new RegExp(String.raw`(?<![\w.])${ATOM}(?:\s*(?:!=|<=|>=|=)\s*${ATOM})?(?![\w.])`, 'g');
const LEAN = /(?<![\w.])[A-Z][A-Za-z0-9]*(?:\.[A-Za-z_][A-Za-z0-9_']*)+/g;

const escape = (t: string) => t.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** KaTeX's HTML for one formula found in prose, or null to leave it as text. */
function formula(src: string): string | null {
  if (!/[_^]/.test(src) && !src.includes('!=')) return null;
  const parts = src.split(/\s*(!=|<=|>=|=)\s*/);
  const ops: Record<string, string> = { '!=': ' \\neq ', '<=': ' \\leq ', '>=': ' \\geq ', '=': ' = ' };
  const tex = parts.map((p, i) => (i % 2 ? ops[p] : toTex(p))).join('');
  return render(tex);
}

/** HTML for prose with its formulas typeset; everything else escaped. */
export function proseHtml(text: string): string {
  let out = '';
  let at = 0;
  // Lean names first, so that their parts are never read as formulas
  const pieces: { start: number; end: number; html: string }[] = [];
  for (const m of text.matchAll(LEAN)) pieces.push({ start: m.index!, end: m.index! + m[0].length, html: `<code>${escape(m[0])}</code>` });
  for (const m of text.matchAll(FORMULA)) {
    const start = m.index!;
    const end = start + m[0].length;
    if (pieces.some((p) => start < p.end && p.start < end)) continue;
    const html = formula(m[0]);
    if (html) pieces.push({ start, end, html });
  }
  pieces.sort((a, b) => a.start - b.start);
  const plain = (t: string) => escape(t.replace(/\b([A-Z][a-z]+)-(?=[A-Z][a-z])/g, '$1–'));
  for (const p of pieces) {
    out += plain(text.slice(at, p.start)) + p.html;
    at = p.end;
  }
  return out + plain(text.slice(at));
}
