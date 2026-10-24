// Display of the calculator's text as typeset mathematics.
//
// Text is entered and edited as plain text in a fixed-width font (`x_1^2 psi_1 e(iij)`); wherever
// it is only shown, it is typeset (x₁² ψ₁ e(iij)) in the reading font. `toTex` reads the text the
// way the engine does, token by token, and writes LaTeX for KaTeX; it never changes what a term
// says, only how it looks. The mapping is listed in docs/conventions.md (Display).
import katex from 'katex';

const GREEK = new Set([
  'alpha', 'beta', 'gamma', 'delta', 'epsilon', 'zeta', 'eta', 'theta', 'iota', 'kappa', 'lambda', 'mu', 'nu', 'xi',
  'pi', 'rho', 'sigma', 'tau', 'upsilon', 'phi', 'chi', 'psi', 'omega',
  'Gamma', 'Delta', 'Theta', 'Lambda', 'Xi', 'Pi', 'Sigma', 'Upsilon', 'Phi', 'Psi', 'Omega',
]);

/** Rings as the context names them, and as they are written in mathematics. */
const RINGS: [RegExp, string][] = [
  [/^QQ\b/, '\\mathbb{Q}'],
  [/^ZZ\b/, '\\mathbb{Z}'],
  [/^GF\b/, '\\mathbb{F}'],
];

type Kind = 'operand' | 'operator' | 'open' | 'none';

/** One script argument after `_` or `^`: a braced group, a parenthesized group, a signed number, or a word. */
function script(src: string, i: number): [string, number] {
  if (src[i] === '{' || src[i] === '(') {
    const close = src[i] === '{' ? '}' : ')';
    let depth = 0;
    let j = i;
    for (; j < src.length; j++) {
      if (src[j] === src[i]) depth++;
      else if (src[j] === close && --depth === 0) break;
    }
    const inner = toTex(src.slice(i + 1, j));
    return [src[i] === '(' ? `(${inner})` : inner, j + 1];
  }
  const m = /^-?[0-9]+|^[A-Za-z][A-Za-z0-9]*/.exec(src.slice(i));
  if (!m) return ['', i];
  return [m[0].startsWith('-') ? m[0] : /^[0-9]/.test(m[0]) ? m[0] : word(m[0], false), i + m[0].length];
}

/** An identifier: Greek names as letters, trailing digits as a subscript, longer names upright or italic. */
function word(w: string, call: boolean): string {
  const m = /^([A-Za-z]+)([0-9]*)$/.exec(w) ?? [w, w, ''];
  const [, base, digits] = m;
  let t: string;
  if (GREEK.has(base)) t = `\\${base}`;
  else if (base === 'Qbar') t = '\\bar{Q}';
  else if (base.length === 1) t = base;
  else if (call && base !== 'e') t = `\\operatorname{${base}}`;
  else t = `\\mathit{${base}}`;
  return digits ? `${t}_{${digits}}` : t;
}

/** LaTeX for a term, an equation, or the text of a cell after its kind. */
export function toTex(src: string): string {
  let out = '';
  let last: Kind = 'none';
  let spaced = false;
  let lastWide = false;
  const emit = (t: string, kind: Kind, wide = false) => {
    // a space between two factors stays visible only next to a long name, where it separates words
    if (spaced && last === 'operand' && kind === 'operand' && (wide || lastWide)) out += '\\,';
    out += t;
    last = kind;
    lastWide = wide;
    spaced = false;
  };
  for (let i = 0; i < src.length; ) {
    const rest = src.slice(i);
    let m: RegExpExecArray | null;
    if ((m = /^\s+/.exec(rest))) {
      spaced = true;
      i += m[0].length;
    } else if (rest.startsWith(':=')) {
      emit(' \\coloneqq ', 'operator');
      i += 2;
    } else if (/^\(x\)(?=\s|$)/.test(rest) && spaced) {
      emit(' \\otimes ', 'operator');
      i += 3;
    } else if ((m = /^for\b/.exec(rest)) && spaced && last !== 'none') {
      emit('\\quad\\text{for}\\quad ', 'operator');
      i += 3;
    } else if ((m = /^[A-Za-z][A-Za-z']*[0-9]*/.exec(rest))) {
      const w = m[0].replace(/'/g, '');
      const call = src[i + m[0].length] === '(';
      const t = word(w, call);
      emit(t, 'operand', t.startsWith('\\mathit') || t.startsWith('\\operatorname'));
      i += m[0].length;
    } else if ((m = /^[0-9]+/.exec(rest))) {
      emit(m[0], 'operand');
      i += m[0].length;
    } else if (rest[0] === '_' || rest[0] === '^') {
      const [arg, j] = script(src, i + 1);
      out += `${rest[0]}{${arg}}`;
      i = j;
    } else {
      const c = rest[0];
      const map: Record<string, [string, Kind]> = {
        '⊗': [' \\otimes ', 'operator'], '∘': [' \\circ ', 'operator'], 'π': ['\\pi ', 'operand'], '−': ['-', 'operator'],
        '*': [' \\cdot ', 'operator'], ',': [',\\,', 'operator'], '(': ['(', 'open'], ')': [')', 'operand'],
        '{': ['\\{', 'open'], '}': ['\\}', 'operand'], '\\': ['\\backslash ', 'operator'], '#': ['\\#', 'operand'],
        '$': ['\\$', 'operand'], '%': ['\\%', 'operand'], '&': ['\\&', 'operand'], '~': ['\\sim ', 'operator'],
        '"': ["''", 'operand'],
      };
      const [t, kind] = map[c] ?? [c, /[+\-=/<>|]/.test(c) ? 'operator' : 'operand'];
      emit(t, kind);
      i += 1;
    }
  }
  return out;
}

/** LaTeX for the options of a context: `type=A2 nu=2i+j ring=QQ(q)`, without the presentation's name. */
export function contextTex(options: string): string {
  return options
    .trim()
    .split(/\s+/)
    .filter(Boolean)
    .map((o) => {
      const [k, v = ''] = o.split('=');
      if (k === 'type') return `\\text{type }${v.replace(/^([A-Z])([0-9]+)$/, '$1_{$2}')}`;
      if (k === 'ring') return ringTex(v);
      return `${GREEK.has(k) ? `\\${k}` : k.length === 1 ? k : `\\mathrm{${k}}`} = ${toTex(v)}`;
    })
    .join(',\\quad ');
}

/** A ring as written in mathematics: ℚ, ℤ[I], 𝔽₅, ℚ(q). */
export function ringTex(r: string): string {
  for (const [re, t] of RINGS) {
    const m = re.exec(r);
    if (m) {
      const rest = r.slice(m[0].length);
      if (t === '\\mathbb{F}') return `\\mathbb{F}_{${rest.replace(/^\((.*)\)$/, '$1')}}`;
      return t + toTex(rest);
    }
  }
  return toTex(r);
}

/**
 * KaTeX settings for text from documents (which anyone with edit access may write): no commands
 * that make links, classes, styles, or HTML attributes (`trust: false`, also KaTeX's default), and
 * bounds on sizes (`\rule{1000em}{1000em}` would cover the page) and on macro expansion.
 */
export const KATEX_SAFE = { trust: false, maxSize: 20, maxExpand: 200 } as const;

const cache = new Map<string, string | null>();

/** KaTeX's HTML for some LaTeX, or null when KaTeX cannot read it (the caller shows the text). */
export function render(tex: string, display = false): string | null {
  const key = `${display ? 'D' : 'I'}${tex}`;
  let html = cache.get(key);
  if (html === undefined) {
    try {
      html = katex.renderToString(tex, { ...KATEX_SAFE, displayMode: display, throwOnError: true, strict: 'ignore' });
    } catch {
      html = null;
    }
    if (cache.size > 4000) cache.clear();
    cache.set(key, html);
  }
  return html;
}
