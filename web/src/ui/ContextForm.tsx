// The context as a form: the presentation chosen from a list, then its options as fields. The
// context line stays the one source of truth: every field reads its option from the line and
// writes it back, and the line itself can be edited directly.
import { createEffect, createResource, createSignal, For, on, onCleanup, Show } from 'solid-js';
import { engine } from '../app';
import { Typeset } from './Typeset';

/** What the form needs to know of a presentation, read from its file's header. */
export interface PresentationInfo {
  name: string;
  title: string;
  /** Colours from a Cartan datum (a type= option), or fixed by the presentation. */
  cartan: boolean;
  /** Objects are words of a weight ν (an algebra), or listed with a region weight μ (a 2-category). */
  weight: 'nu' | 'mu';
}

const files = import.meta.glob('../../../presentations/*.pres.toml', { query: '?raw', import: 'default' }) as Record<string, () => Promise<string>>;

/** The presentation file's header: name, title, colours and objects. */
export function presentationInfo(text: string): PresentationInfo | undefined {
  const head = text.split(/^\[/m)[0];
  const str = (k: string) => new RegExp(`^${k}\\s*=\\s*"([^"]*)"`, 'm').exec(head)?.[1];
  const name = str('name');
  if (!name) return undefined;
  return { name, title: str('title') ?? name, cartan: str('colours') === 'cartan', weight: str('objects') === 'explicit' ? 'mu' : 'nu' };
}

let cache: Promise<PresentationInfo[]> | undefined;
/** The shipped presentations, algebras first. */
export const presentations = () =>
  (cache ??= Promise.all(Object.values(files).map((f) => f())).then((ts) =>
    ts.map(presentationInfo).filter((p): p is PresentationInfo => !!p)
      .sort((a, b) => (a.weight === b.weight ? ORDER.indexOf(a.name) - ORDER.indexOf(b.name) : a.weight === 'nu' ? -1 : 1))));
/** Display order where known; others follow. */
const ORDER = ['klr', 'nilhecke', 'odd-nilhecke', 'kl', 'be2'];

/** A context line's presentation and options, in order. */
export function parseLine(line: string): { name: string; opts: [string, string][] } {
  const [name = '', ...rest] = line.trim().split(/\s+/);
  return { name, opts: rest.map((w) => { const i = w.indexOf('='); return (i < 0 ? [w, ''] : [w.slice(0, i), w.slice(i + 1)]) as [string, string]; }) };
}
const printLine = (name: string, opts: [string, string][]) => [name, ...opts.filter(([, v]) => v !== '').map(([k, v]) => `${k}=${v}`)].join(' ');
const ALIASES: Record<string, string[]> = { nu: ['nu', 'n'], Lambda: ['Lambda', 'lambda'], odd: ['odd'], type: ['type'], ring: ['ring'], mu: ['mu'] };
const get = (opts: [string, string][], k: string) => opts.find(([x]) => (ALIASES[k] ?? [k]).includes(x))?.[1] ?? '';
const set = (opts: [string, string][], k: string, v: string): [string, string][] => {
  const i = opts.findIndex(([x]) => (ALIASES[k] ?? [k]).includes(x));
  if (i < 0) return v ? [...opts, [k, v]] : opts;
  return opts.map((o, j) => (j === i ? [k, v] : o));
};

const RINGS: [string, string][] = [['QQ', 'ℚ'], ['ZZ', 'ℤ'], ['GF(2)', '𝔽₂'], ['GF(3)', '𝔽₃'], ['GF(5)', '𝔽₅'], ['QQ(q)', 'ℚ(q)']];

/** Starting options when the presentation changes: kept where they still apply, else a small example. */
function switchTo(p: PresentationInfo, from: [string, string][]): [string, string][] {
  let opts: [string, string][] = [];
  if (p.cartan) opts = set(opts, 'type', get(from, 'type') || (p.weight === 'nu' ? 'A2' : 'A1'));
  if (p.cartan && get(from, 'odd')) opts = set(opts, 'odd', get(from, 'odd'));
  const w = get(from, p.weight);
  const a1 = get(opts, 'type') === 'A1';
  opts = set(opts, p.weight, w || (p.weight === 'mu' ? (p.cartan && !a1 ? 'i' : '2') : p.cartan ? (a1 ? '2i' : '2i+j') : '3'));
  if (p.weight === 'nu' && get(from, 'Lambda')) opts = set(opts, 'Lambda', get(from, 'Lambda'));
  return set(opts, 'ring', get(from, 'ring') || 'QQ');
}

let forms = 0;

/**
 * The fields of a context line. `line` is the current line, `onLine` receives every change; the
 * error of the line as the engine reads it shows under the fields (after a short pause).
 */
export function ContextForm(props: { line: string; onLine: (l: string) => void; onSubmit?: () => void }) {
  const uid = ++forms;
  const [list] = createResource(presentations);
  const parsed = () => parseLine(props.line);
  const current = () => list()?.find((p) => p.name === parsed().name);
  const field = (k: string) => get(parsed().opts, k);
  const put = (k: string, v: string) => props.onLine(printLine(parsed().name, set(parsed().opts, k, v.trim())));
  const choose = (p: PresentationInfo) => props.onLine(printLine(p.name, switchTo(p, parsed().opts)));
  const [error, setError] = createSignal('');
  // the engine's reading of the line, a moment after the last change
  createEffect(on(() => props.line, (line) => {
    const t = setTimeout(async () => {
      const o = await engine.call('context.describe', { context: line }, { lane: 'interactive' });
      if (o.status === 'done') setError(o.response.status === 'ok' ? '' : o.response.message);
    }, 250);
    onCleanup(() => clearTimeout(t));
  }));
  const enter = (e: KeyboardEvent) => {
    if (e.key === 'Enter' && props.onSubmit) {
      e.preventDefault();
      props.onSubmit();
    }
  };
  return (
    <div class="ctx-form">
      <fieldset class="ctx-presentations">
        <legend>Presentation</legend>
        <For each={list() ?? []}>
          {(p) => (
            <label class="ctx-pres">
              <input type="radio" name={`presentation-${uid}`} value={p.name} checked={p.name === parsed().name} onChange={() => choose(p)} />
              <span class="mono">{p.name}</span>
              <span class="hint"><PresentationTitle text={p.title} />{p.weight === 'nu' ? ' · an algebra' : ' · a 2-category'}</span>
            </label>
          )}
        </For>
      </fieldset>
      <Show when={current()}>
        {(p) => (
          <div class="ctx-fields">
            <Show when={p().cartan}>
              <label>
                <span>Cartan type</span>
                <input class="mono" value={field('type')} placeholder="A2" onInput={(e) => put('type', e.currentTarget.value)} onKeyDown={enter} />
                <span class="hint">An, Bn, Cn, Dn, G2, F4, or a product such as A1xA1; colours i, j, k, …</span>
              </label>
              <label>
                <span>Odd colours</span>
                <input class="mono" value={field('odd')} placeholder="none" onInput={(e) => put('odd', e.currentTarget.value)} onKeyDown={enter} />
                <span class="hint">comma-separated, such as i</span>
              </label>
            </Show>
            <label>
              <span>Weight <Typeset text={p().weight === 'nu' ? 'nu' : 'mu'} tex={p().weight === 'nu' ? '\\nu' : '\\mu'} /></span>
              <input class="mono" value={field(p().weight)} onInput={(e) => put(p().weight, e.currentTarget.value)} onKeyDown={enter} />
              <span class="hint">{p().weight === 'nu' ? (p().cartan ? 'a sum of colours, such as 2i+j' : 'the number of strands') : 'the weight of the rightmost region, such as 2 or i-j'}</span>
            </label>
            <Show when={p().weight === 'nu'}>
              <label>
                <span>Cyclotomic <Typeset text="Lambda" tex="\Lambda" /></span>
                <input class="mono" value={field('Lambda')} placeholder="none" onInput={(e) => put('Lambda', e.currentTarget.value)} onKeyDown={enter} />
                <span class="hint">optional: a dominant weight for the finite quotient, such as 2i</span>
              </label>
            </Show>
            <label>
              <span>Ring</span>
              <select value={RINGS.some(([r]) => r === field('ring')) ? field('ring') : ''} onChange={(e) => e.currentTarget.value && put('ring', e.currentTarget.value)}>
                <For each={RINGS}>{([r, shown]) => <option value={r}>{shown}</option>}</For>
                <Show when={!RINGS.some(([r]) => r === field('ring'))}><option value="">{field('ring') || 'other'} (as written)</option></Show>
              </select>
            </label>
          </div>
        )}
      </Show>
      <Show when={error()}><div class="error" role="status">{error()}</div></Show>
    </div>
  );
}

/** A presentation's title, with the algebra's symbol in it typeset: "KLR algebra R(ν)". */
export function PresentationTitle(props: { text: string }) {
  const parts = () => props.text.split(/(\b[A-Z][A-Za-z]*(?:\([A-Za-z]+\)|_[A-Za-z0-9]+))/);
  return <For each={parts()}>{(p, i) => (i() % 2 ? <Typeset text={p} /> : <>{p}</>)}</For>;
}
