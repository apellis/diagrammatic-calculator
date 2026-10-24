import { createSignal, Match, onCleanup, Switch } from 'solid-js';
import { DocList } from './DocList';
import { SettingsPage } from './SettingsPage';
import { AboutPage } from './AboutPage';
import { HelpPage } from './HelpPage';
import { WorkspacePage } from './WorkspacePage';
import { OpenShared } from './Account';
import { loadAccount } from '../account/account';

/**
 * Routes: `#/` the document list, `#/doc/<id>` a document, `#/settings` the settings, `#/about`
 * about the calculator, `#/help` first steps, and
 * `#/s/<id>` or `#/s/<id>/<link token>` a document from an account.
 */
export function App() {
  const [hash, setHash] = createSignal(location.hash);
  // the route before the settings, About, or Help page were opened, for their Back button
  let from = '';
  const on = () => {
    const side = (h: string) => h === '#/settings' || h === '#/about' || h === '#/help';
    if (side(location.hash) && !side(hash())) from = hash();
    setHash(location.hash);
  };
  window.addEventListener('hashchange', on);
  onCleanup(() => window.removeEventListener('hashchange', on));
  const docId = () => hash().match(/^#\/doc\/([\w-]+)/)?.[1];
  const shared = () => {
    const m = hash().match(/^#\/s\/([\w-]{16})(?:\/([\w-]+))?$/);
    return m ? `${m[1]}/${m[2] ?? ''}` : undefined;
  };
  // whether this site has accounts, and who is signed in; the app works the same meanwhile
  void loadAccount();
  return (
    <Switch fallback={<DocList />}>
      <Match when={hash() === '#/settings'}>
        <SettingsPage from={from} />
      </Match>
      <Match when={hash() === '#/about'}>
        <AboutPage from={from} />
      </Match>
      <Match when={hash() === '#/help'}>
        <HelpPage from={from} />
      </Match>
      <Match when={docId()} keyed>
        {(id) => <WorkspacePage id={id} />}
      </Match>
      <Match when={shared()} keyed>
        {(s) => <OpenShared id={s.split('/')[0]} token={s.split('/')[1] || undefined} />}
      </Match>
    </Switch>
  );
}
