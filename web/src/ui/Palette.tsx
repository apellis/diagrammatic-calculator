// The command palette (⌘K): every command by name.
import { createSignal, For } from 'solid-js';

export interface Command {
  name: string;
  keys?: string;
  run: () => void;
}

export function Palette(props: { commands: Command[]; onClose: () => void }) {
  const [q, setQ] = createSignal('');
  const [i, setI] = createSignal(0);
  const shown = () => props.commands.filter((c) => c.name.toLowerCase().includes(q().toLowerCase()));
  const run = (c?: Command) => {
    if (!c) return;
    props.onClose();
    c.run();
  };
  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class="modal palette" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Command palette">
        <input autofocus placeholder="Type a command" value={q()}
          onInput={(e) => { setQ(e.currentTarget.value); setI(0); }}
          onKeyDown={(e) => {
            if (e.key === 'ArrowDown') { setI(Math.min(i() + 1, shown().length - 1)); e.preventDefault(); }
            if (e.key === 'ArrowUp') { setI(Math.max(i() - 1, 0)); e.preventDefault(); }
            if (e.key === 'Enter') run(shown()[i()]);
            if (e.key === 'Escape') props.onClose();
          }} />
        <div style={{ 'margin-top': '8px' }}>
          <For each={shown()}>
            {(c, k) => (
              <div class="opt row" classList={{ on: k() === i() }} onMouseEnter={() => setI(k())} onClick={() => run(c)}>
                <span class="grow">{c.name}</span>
                {c.keys && <kbd>{c.keys}</kbd>}
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
