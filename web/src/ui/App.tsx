import { createSignal, Match, onCleanup, Switch } from 'solid-js';
import { DocList } from './DocList';
import { SettingsPage } from './SettingsPage';
import { WorkspacePage } from './WorkspacePage';

/** Routes: `#/` the document list, `#/doc/<id>` a document, `#/settings` the settings. */
export function App() {
  const [hash, setHash] = createSignal(location.hash);
  // the route before the settings were opened, for their Back button
  let from = '';
  const on = () => {
    if (location.hash === '#/settings' && hash() !== '#/settings') from = hash();
    setHash(location.hash);
  };
  window.addEventListener('hashchange', on);
  onCleanup(() => window.removeEventListener('hashchange', on));
  const docId = () => hash().match(/^#\/doc\/([\w-]+)/)?.[1];
  return (
    <Switch fallback={<DocList />}>
      <Match when={hash() === '#/settings'}>
        <SettingsPage from={from} />
      </Match>
      <Match when={docId()} keyed>
        {(id) => <WorkspacePage id={id} />}
      </Match>
    </Switch>
  );
}
