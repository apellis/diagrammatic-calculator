import { createSignal, onCleanup, Show } from 'solid-js';
import { DocList } from './DocList';
import { WorkspacePage } from './WorkspacePage';

/** Routes: `#/` the document list, `#/doc/<id>` a document. */
export function App() {
  const [hash, setHash] = createSignal(location.hash);
  const on = () => setHash(location.hash);
  window.addEventListener('hashchange', on);
  onCleanup(() => window.removeEventListener('hashchange', on));
  const docId = () => hash().match(/^#\/doc\/([\w-]+)/)?.[1];
  return (
    <Show when={docId()} keyed fallback={<DocList />}>
      {(id) => <WorkspacePage id={id} />}
    </Show>
  );
}
