// The map pane: one map of a complex, chain map or module cell, beside the document. The pane
// takes a half or a third of the width; the document stays usable on the other side. It shows
// the map's matrix with every entry that a failed check involves marked in red, the failures
// themselves, and the chosen entry: its text, and its diagram on an embedded canvas, where the
// usual gestures edit it. Every change is an ordinary edit of the cell, so it can be undone, and
// nothing is thrown away while the cell does not check (docs/complexes.md, "Editing maps").
import { createEffect, createSignal, For, type JSX, Show } from 'solid-js';
import { EntryShown, type EntryEdit, type MapData, type MapRef, Proj, summandText } from './Complexes';

export type PaneWidth = 'half' | 'third';

/** The canvas the pane embeds, made by the page (it holds the engine, the document and the context). */
export type PaneCanvas = (p: { cellId: string; term: string; title: string; onWrite: (term: string) => void; onClose: () => void }) => JSX.Element;

const isZero = (e: string | null | undefined) => e == null || e.trim() === '' || e.trim() === '0';
const refKey = (r: MapRef) =>
  r.map === 'differential' ? `d${JSON.stringify(r.from)}a${r.axis ?? 0}` : r.map === 'component' ? `F${JSON.stringify(r.at)}` : r.map === 'augmentation' ? 'eps' : 'phi';

export function MapPane(props: {
  /** The cell's ID and name, and which of its maps. */
  cellId: string;
  cell: string;
  mapRef: MapRef;
  /** The map as it stands now; null when the cell no longer has it (its terms changed, or it was removed). */
  data: MapData | null;
  /** The engine could not read the cell: what it said. */
  error?: string;
  readOnly?: boolean;
  width: PaneWidth;
  onWidth: (w: PaneWidth) => void;
  onEdit: (e: EntryEdit) => void;
  onClose: () => void;
  canvas: PaneCanvas;
}) {
  const [at, setAt] = createSignal<{ row: number; col: number } | null>(null);
  // the entry chosen: as picked, else the first one marked in red, else the first nonzero one
  const chosen = () => {
    const d = props.data;
    if (!d) return null;
    const a = at();
    if (a && a.row < d.m.length && a.col < (d.m[0]?.length ?? 0)) return a;
    const bad = [...d.flags.entries][0]?.split(',').map(Number);
    if (bad) return { row: bad[0], col: bad[1] };
    for (let i = 0; i < d.m.length; i++) for (let j = 0; j < d.m[i].length; j++) if (!isZero(d.m[i][j])) return { row: i, col: j };
    return { row: 0, col: 0 };
  };
  const entry = () => { const c = chosen(); return c ? props.data?.m[c.row]?.[c.col] ?? null : null; };
  const [draft, setDraft] = createSignal('');
  // the text field follows the entry, also when the canvas or an undo changes it
  createEffect(() => setDraft(isZero(entry()) ? '' : entry()!));
  const write = (text: string) => {
    const d = props.data;
    const c = chosen();
    if (!d || !c || props.readOnly) return;
    const t = text.trim();
    const next = t === '' || t === '0' ? null : t;
    if (next === (isZero(entry()) ? null : entry()!.trim())) return;
    props.onEdit({ ref: props.mapRef, row: c.row, col: c.col, rows: d.m.length, cols: d.m[0]?.length ?? 0, text: next });
  };
  const place = () => {
    const d = props.data;
    const c = chosen();
    return d && c ? { src: d.src[c.col], tgt: d.tgt[c.row] } : undefined;
  };
  const degree = () => { const p = place(); return p ? (p.src.shift ?? 0) - (p.tgt.shift ?? 0) : 0; };
  const bad = (i: number, j: number) => !!props.data?.flags.entries.has(`${i},${j}`);
  const onKey = (ev: KeyboardEvent) => {
    if (ev.key !== 'Escape' || ev.defaultPrevented) return;
    const t = ev.target as HTMLElement;
    if (t.closest('.canvas') || t.tagName === 'INPUT') return;
    ev.preventDefault();
    props.onClose();
  };
  return (
    <aside class="map-pane" data-testid="map-pane" aria-label={`Editing ${props.data?.title ?? 'a map'} in ${props.cell}`} onKeyDown={onKey}>
      <div class="map-pane-bar">
        <strong class="map-pane-title">Editing {props.data?.title ?? 'a map'} in {props.cell}</strong>
        <span class="grow" />
        <div class="map-pane-widths" role="group" aria-label="Width of the pane">
          <button class="ghost" classList={{ on: props.width === 'half' }} aria-pressed={props.width === 'half'} onClick={() => props.onWidth('half')} title="The pane takes half the width">½</button>
          <button class="ghost" classList={{ on: props.width === 'third' }} aria-pressed={props.width === 'third'} onClick={() => props.onWidth('third')} title="The pane takes a third of the width">⅓</button>
        </div>
        <button class="map-pane-close" data-testid="map-pane-close" onClick={() => props.onClose()} title="Close the pane and go back to the document alone (Esc)" aria-label="Close the map pane">✕ Close</button>
      </div>
      <div class="map-pane-body">
        <p class="hint map-pane-intro">
          This pane edits one map of the cell {props.cell}; the document stays open beside it. Pick an entry in the matrix, then type its new
          value or change its diagram below. Every change is written into the cell and can be undone. Entries a failed check involves are
          marked in red; they are kept as written until you change them.
        </p>
        <Show when={props.error}>
          <div class="cx-error" role="alert"><span class="badge bad">cannot be checked</span> <span class="error">{props.error}</span></div>
        </Show>
        <Show when={props.data} fallback={<div class="hint" data-testid="map-pane-gone">The cell no longer has this map (its terms changed). Close the pane, or open another map from the cell.</div>}>
          {(d) => (
            <>
              <Show when={d().failures.length}>
                <ul class="cx-failures" data-testid="map-pane-failures">
                  <For each={d().failures}>{(f) => <li><b>{f.location}</b>: {f.detail}</li>}</For>
                </ul>
              </Show>
              <table class="cx-matrix map-pane-matrix" aria-label={d().title}>
                <thead><tr><th /><For each={d().src}>{(s) => <th><Proj s={s} d={d().display} /></th>}</For></tr></thead>
                <tbody>
                  <For each={d().m}>{(r, i) => (
                    <tr>
                      <th><Proj s={d().tgt[i()]} d={d().display} /></th>
                      <For each={r}>{(e, j) => (
                        <td>
                          <button class="cx-entry-btn" data-testid="map-pane-entry"
                            classList={{ 'cx-bad-entry': bad(i(), j()), on: chosen()?.row === i() && chosen()?.col === j() }}
                            data-bad={bad(i(), j()) ? 'true' : undefined} aria-pressed={chosen()?.row === i() && chosen()?.col === j()}
                            title={`row ${i() + 1}, column ${j() + 1}${bad(i(), j()) ? ': a check fails at this entry' : ''}`}
                            onClick={() => setAt({ row: i(), col: j() })}>
                            <EntryShown e={e} d={d().display} />
                          </button>
                        </td>
                      )}</For>
                    </tr>
                  )}</For>
                </tbody>
              </table>
              <Show when={chosen()}>
                {(c) => (
                  <div class="map-pane-entry">
                    <div class="hint">
                      Row {c().row + 1}, column {c().col + 1}: from {summandText(place()!.src)} to {summandText(place()!.tgt)}, an element of
                      e({place()!.tgt.object}) A e({place()!.src.object}) of degree {degree()}.
                    </div>
                    <div class="row">
                      <input class="mono grow" data-testid="map-pane-input" value={draft()} disabled={props.readOnly} placeholder="0"
                        aria-label="The entry's text; empty or 0 for zero" onInput={(ev) => setDraft(ev.currentTarget.value)}
                        onKeyDown={(ev) => {
                          if (ev.key === 'Enter') { ev.preventDefault(); write(draft()); }
                          else if (ev.key === 'Escape') { ev.preventDefault(); setDraft(isZero(entry()) ? '' : entry()!); }
                        }} />
                      <button class="primary" disabled={props.readOnly || draft().trim() === (isZero(entry()) ? '' : entry()!.trim())} onClick={() => write(draft())} data-testid="map-pane-write">Write</button>
                      <button disabled={props.readOnly || isZero(entry())} onClick={() => write('')} title="Make this entry zero">Set to 0</button>
                    </div>
                    <div class="hint">Enter writes the text into the cell; Escape puts back the entry as it stands.</div>
                    <Show when={!isZero(entry())} fallback={<div class="hint map-pane-zero">This entry is zero. Type a term above to give it a diagram, which you can then edit here.</div>}>
                      <Show when={`${c().row},${c().col}`} keyed>
                        {(k) => (
                          <div class="map-pane-canvas" data-testid="map-pane-canvas">
                            {props.canvas({
                              cellId: `${props.cellId}~${refKey(props.mapRef)}~${k.replace(',', '~')}`,
                              get term() { return entry() ?? '0'; },
                              title: `row ${c().row + 1}, column ${c().col + 1}`,
                              onWrite: (t) => write(t),
                              onClose: () => props.onClose(),
                            })}
                          </div>
                        )}
                      </Show>
                    </Show>
                  </div>
                )}
              </Show>
            </>
          )}
        </Show>
      </div>
    </aside>
  );
}
