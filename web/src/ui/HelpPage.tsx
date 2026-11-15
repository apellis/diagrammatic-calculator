// Help: first steps, then a short reference of the language, cells, contexts, the canvas, results,
// files, and keys. The full reference is docs/language.md; this page is what a reader needs while
// working, written to match it.
import { For, type JSX } from 'solid-js';
import { ACTIONS, FIXED, keysOf, label } from './shortcuts';

const DOCS = 'https://github.com/apellis/diagrammatic-calculator/blob/master/docs';

const SECTIONS: [string, string][] = [
  ['help-start', 'Getting started'],
  ['help-terms', 'Writing terms'],
  ['help-cells', 'Cells'],
  ['help-context', 'Contexts'],
  ['help-results', 'Reading results'],
  ['help-canvas', 'Drawing and the canvas'],
  ['help-files', 'Saving, files, and sharing'],
  ['help-keys', 'Keyboard shortcuts'],
  ['help-more', 'Further reading'],
];

/** A two-column table of `code` and its meaning. */
function Table(p: { head: [string, string]; rows: [string, JSX.Element | string][] }) {
  return (
    <table class="help-table">
      <thead><tr><th>{p.head[0]}</th><th>{p.head[1]}</th></tr></thead>
      <tbody>
        <For each={p.rows}>{([c, m]) => <tr><td><code>{c}</code></td><td>{m}</td></tr>}</For>
      </tbody>
    </table>
  );
}

/** `from`: the route the page was opened from, returned to by Back (the document list if none). */
export function HelpPage(props: { from?: string }) {
  // the router owns the hash, so the contents scroll instead of following #anchors
  const go = (id: string) => document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
  return (
    <div class="doclist about help">
      <div class="row">
        <button class="ghost" onClick={() => (location.hash = props.from || '#/')} data-testid="help-back">← Back</button>
      </div>
      <nav class="help-contents" aria-label="Help contents">
        <For each={SECTIONS}>{([id, title]) => <button class="link" onClick={() => go(id)}>{title}</button>}</For>
      </nav>

      <section class="getting-started" id="help-start">
        <h1>Getting started</h1>
        <ol>
          <li>Open an example from the document list to make your own editable copy, or choose New document and set its context (the algebra you work in).</li>
          <li>Add an Element cell to name an expression, a Watch cell to evaluate one, or a Check cell to test an equality or property.</li>
          <li>Use Type to enter an expression or Draw to edit its string diagram. Open Commands (⋯, {keysOf('palette')}) for document actions and keyboard shortcuts.</li>
          <li>Document edits save automatically in this browser. Use Export beside a document for a <code>.dcw</code> workspace backup, then Import to open it as a new document. For versions and snapshots, use Export history (<code>.dch</code>) in the document's Commands.</li>
        </ol>
        <p class="muted">A first worksheet: in the context <code>nilhecke nu=2</code>, watch <code>psi_1 x_1 - x_2 psi_1</code>. The answer <code>1</code> is the nilHecke relation.</p>
      </section>

      <section id="help-terms">
        <h2>Writing terms</h2>
        <p>A term is built from the generators of the presentation. Pieces written next to each other are stacked, <strong>read from right to left</strong>: the rightmost piece is at the bottom of the picture, as when composing maps. Strands are numbered from 1 at the left.</p>
        <Table head={['Write', 'Meaning']} rows={[
          ['psi_1 x_1', 'a dot on strand 1 with a crossing of strands 1 and 2 above it'],
          ['3 x_1 + 1/2 x_2', 'sums, differences, and coefficients; fractions with /'],
          ['x_1^3, x_1^{2+1}', 'powers; braces hold a small calculation'],
          ['sum_{f=0..2} x_1^{f} x_2^{2-f}', 'a finite sum over f'],
          ['e(iji)', 'the colours along the bottom, from left to right; left out, every colouring is summed'],
          ['e(i) (x) psi_1 e(ij)', 'side by side, first on the left (also ⊗)'],
          ['1_i*, e(i j*)', 'downward strands, in the 2-categories kl and be2'],
          ['e(j@0 i@1/4)', 'a loading in klrw: each colour at a rational position'],
          ['# a remark', 'a comment to the end of the line'],
        ]} />
        <p>Errors are shown below the editor as you type, and text that does not make sense is never saved.</p>
      </section>

      <section id="help-cells">
        <h2>Cells</h2>
        <p>A document is a list of cells. Each starts with a word saying what to do; a term typed with no word becomes an Element with a fresh name. Typing <code>/</code> in an empty cell offers the kinds.</p>
        <Table head={['Cell', 'What it does']} rows={[
          ['def a := psi_1 x_1', 'Element: names a term for later cells'],
          ['watch a - b', 'Watch: shows the normal form, updated when earlier cells change'],
          ['check equal(a, b)', <>Check: tests <code>equal</code>, <code>zero</code>, <code>idempotent</code>, <code>orthogonal</code>, <code>homogeneous</code>, <code>central</code>, or <code>primitive</code></>],
          ['solve c psi_1 = psi_1 x_1 psi_1 for c', 'Solve: finds the listed unknown constants (needs a field)'],
          ['note """Text with $\\psi_1$."""', 'Note: paragraphs with mathematics between $…$ or $$…$$'],
          ['watch t  view=diagrams', 'Options after two spaces: view=diagrams draws the result; strategy=… chooses how equality is decided'],
        ]} />
        <p>Enter saves a cell and Escape cancels; in a note, Enter starts a new line and {label('Mod+Enter')} saves. Click a typeset cell to edit it. The dot beside a cell is green when its result is current, amber while it is computing, and red when it failed or stopped at a limit.</p>
      </section>

      <section id="help-context">
        <h2>Contexts</h2>
        <p>The context says which algebra a document computes in. It is chosen with a form when the document is made and shown in the chip at the top; click the chip to change it. As text it is a presentation name followed by <code>key=value</code> settings.</p>
        <Table head={['Presentation', 'Algebra']} rows={[
          ['nilhecke nu=3', 'the nilHecke algebra on three strands'],
          ['klr type=A2 nu=2i+j', 'a KLR (quiver Hecke) algebra; add Lambda=2i for its cyclotomic quotient'],
          ['klrw type=A2 theta_ij=1/2', 'a weighted KLR algebra (Webster), with ghost strands; red="i@0" adds red strands'],
          ['kl type=A1 mu=2', 'the Khovanov–Lauda 2-category, at the weight mu of the rightmost region'],
          ['be2 type=A1 odd=i mu=-1', 'the super Kac–Moody 2-category of Brundan–Ellis'],
          ['odd-nilhecke nu=2', 'the odd nilHecke algebra'],
        ]} />
        <Table head={['Setting', 'Meaning']} rows={[
          ['type=B3, type=A1xA1', 'the Cartan type (A, B, C, D, F₄, G₂, and products)'],
          ['ring=QQ(q), ZZ, GF(5)', 'the coefficients; QQ by default'],
          ['scalars=t', 'free constants usable as coefficients'],
          ['dotcap=3, degcap=4', 'compute up to a bound; the result says so'],
          ['Q_ij="u - v"', 'replace a parameter of the presentation'],
        ]} />
      </section>

      <section id="help-results">
        <h2>Reading results</h2>
        <p>Results are exact and typeset; click one to see and copy its text. Each result says how it was obtained: the equality strategy, what it rests on, and any truncation cap (nothing is truncated silently). Expand a cell for its details, and add <code>view=diagrams</code> to see a result drawn.</p>
        <Table head={['Mark', 'Meaning']} rows={[
          ['(none)', 'proven: by the relations alone, or by a strategy checked in Lean or resting on a published theorem'],
          ['conditional', 'rests on an open conjecture, which the details name (for be2, “Assuming nondegeneracy”)'],
          ['assumed', 'rests on a representation assumed faithful'],
          ['unproven', 'whether the representation used is faithful is not known'],
        ]} />
      </section>

      <section id="help-canvas">
        <h2>Drawing and the canvas</h2>
        <p>Choose Draw in a new Element to build a diagram by pointing, or press {keysOf('canvas')} on an Element or Watch cell to open it in the focus canvas, where relations are applied to selected pieces. Press <code>?</code> on the canvas for its keys.</p>
        <Table head={['Key', 'On the canvas']} rows={[
          ['Tab, Shift+Tab', 'select the next or previous piece'],
          ['S, X', 'select a strand; add the piece under the ring to the selection'],
          ['R, then a digit', 'apply one of the listed relations'],
          ['I, then a digit', 'set the insertion point and insert a generator'],
          ['↑ ↓, ← →', 'slide a selected dot; move between summands'],
          ['F, 0, +, -', 'fit, reset the zoom, zoom in, zoom out'],
        ]} />
        <p>In weighted KLR documents, ghost strands are drawn dashed; Settings › Appearance hides them without changing any element.</p>
      </section>

      <section id="help-files">
        <h2>Saving, files, and sharing</h2>
        <ul>
          <li>Every edit is saved in this browser. Undo and redo cover every step; the History tab of the outline keeps versions and named snapshots.</li>
          <li>A <code>.dcw</code> file is the workspace as text: the line <code>diagcalc-workspace/1</code>, the context, then one cell per line. A <code>.dch</code> file also keeps the history.</li>
          <li>Commands › Save to a file on disk writes the <code>.dcw</code> after every step, in browsers that allow it.</li>
          <li>A scratch branch ({keysOf('branch')}) is a copy to try something; compare and accept it, or discard it.</li>
          <li>Signed in, documents are kept in your account, can be shared, and can be opened read-only in view mode ({keysOf('view')}).</li>
        </ul>
      </section>

      <section id="help-keys">
        <h2>Keyboard shortcuts</h2>
        <p>These can be changed in <a href="#/settings">Settings</a>.</p>
        <table class="help-table">
          <tbody>
            <For each={ACTIONS}>{(a) => <tr><td><kbd>{keysOf(a.id)}</kbd></td><td>{a.name}</td></tr>}</For>
            <For each={FIXED}>{([name, combos]) => <tr><td><kbd>{combos.map(label).join(', ')}</kbd></td><td>{name}</td></tr>}</For>
          </tbody>
        </table>
      </section>

      <section id="help-more">
        <h2>Further reading</h2>
        <ul>
          <li><a href={`${DOCS}/language.md`} target="_blank" rel="noopener">The input language</a>: a tutorial and the full reference, with every symbol, generator, option, and error message.</li>
          <li><a href={`${DOCS}/conventions.md`} target="_blank" rel="noopener">Conventions</a>: how each notation is read, its signs, and how it is drawn and typeset.</li>
          <li><a href={`${DOCS}/klrw.md`} target="_blank" rel="noopener">Weighted KLR algebras</a>: loadings, ghosts, red strands, and the basis.</li>
        </ul>
      </section>
    </div>
  );
}
