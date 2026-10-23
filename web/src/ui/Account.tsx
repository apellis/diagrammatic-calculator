// The account in the page: signing in and out, documents shared with you, opening a shared
// document, the save status of an open document, and the share dialog. All of it is hidden when
// the site has no accounts API, so the anonymous app is unchanged.
import { createResource, createSignal, For, onMount, Show } from 'solid-js';
import { account, api, loadAccount, openFromAccount, signIn, signOut, signOutEverywhere, takeLinkToken, user } from '../account/account';
import { ApiError, type CloudDoc, type MailOutcome, type Role } from '../account/api';
import { statusText, type DocSync } from '../account/docsync';
import { findByCloud, updateInfo } from '../doc/store';
import { ago } from '../app';

const message = (e: unknown) => (e instanceof ApiError ? e.code : e instanceof Error ? e.message : String(e));

/** Sign in, or who is signed in and Sign out; nothing at all without accounts. */
export function AccountBar() {
  const [busy, setBusy] = createSignal(false);
  return (
    <Show when={account().status === 'anonymous' || account().status === 'signed-in'}>
      <span class="account row" data-testid="account">
        <Show
          when={user()}
          fallback={
            <button onClick={() => signIn()} title="Save documents to your Google account and share them" data-testid="sign-in">
              Sign in
            </button>
          }
        >
          {(u) => (
            <>
              <span class="muted" title={u().email} data-testid="account-name">{u().name || u().email}</span>
              <button
                class="ghost"
                disabled={busy()}
                onClick={async () => {
                  setBusy(true);
                  try {
                    await signOut();
                  } finally {
                    setBusy(false);
                  }
                }}
              >
                Sign out
              </button>
            </>
          )}
        </Show>
      </span>
    </Show>
  );
}

const SIGNIN: Record<string, string> = {
  failed: 'Signing in did not work. Please try again.',
  expired: 'Signing in took too long. Please try again.',
  cancelled: 'Signing in was cancelled.',
  unavailable: 'Accounts are not available here yet.',
};

export const signInMessage = (outcome: string | null) => (outcome ? SIGNIN[outcome] ?? SIGNIN.failed : '');

/** Signing out in every browser, for the settings page; nothing without an account. */
export function SignOutEverywhere() {
  return (
    <Show when={user()}>
      {(u) => (
        <section>
          <h3>Account</h3>
          <p class="muted">Signed in as {u().email}. Sessions end after 30 days, or after 7 days without use.</p>
          <button onClick={() => void signOutEverywhere()} data-testid="sign-out-everywhere">Sign out everywhere</button>
        </section>
      )}
    </Show>
  );
}

const MAIL: Record<MailOutcome, string> = {
  unavailable: 'e-mail is not set up on this site',
  'new-account': 'accounts can send share e-mails one day after they are made',
  'opted-out': 'that address asked for no share e-mails',
  limit: 'the daily limit of share e-mails was reached',
  failed: 'sending failed',
};

/** Documents others have shared with the signed-in account. */
export function SharedWithMe(props: { docs: CloudDoc[] }) {
  return (
    <>
      <h3 style={{ 'margin-top': '28px' }}>Shared with me</h3>
      <table data-testid="shared-with-me">
        <thead><tr><th>Name</th><th>Owner</th><th>Access</th><th>Edited</th></tr></thead>
        <tbody>
          <For each={props.docs} fallback={<tr><td colspan="4" class="muted">Nothing shared with you yet.</td></tr>}>
            {(d) => (
              <tr data-shared={d.id}>
                <td class="open" onClick={(e) => (e.target as HTMLElement).closest('a') || (location.hash = `#/s/${d.id}`)}><a href={`#/s/${d.id}`}>{d.name}</a></td>
                <td class="muted">{d.owner?.name || d.owner?.email}</td>
                <td class="muted">{d.role === 'editor' ? 'can edit' : 'can view'}</td>
                <td class="muted">{ago(d.edited)}</td>
              </tr>
            )}
          </For>
        </tbody>
      </table>
    </>
  );
}

/**
 * `#/s/<id>` and `#/s/<id>/<link token>`: opens a document from an account, making a local copy
 * the first time, and goes on to it.
 */
export function OpenShared(props: { id: string; token?: string }) {
  const [problem, setProblem] = createSignal<'sign-in' | 'unavailable' | 'not-found' | 'error' | null>(null);
  const [detail, setDetail] = createSignal('');
  // a link token put aside while signing in (it does not travel through the sign-in address)
  const token = props.token ?? takeLinkToken(props.id);
  onMount(async () => {
    const s = await loadAccount();
    const local = await findByCloud(props.id);
    if (local) {
      if (token && !local.cloud?.token) await updateInfo(local.id, { cloud: { ...local.cloud!, token } });
      location.replace(`#/doc/${local.id}`);
      return;
    }
    if (s.status === 'unavailable' || s.status === 'loading') return setProblem('unavailable');
    const u = s.status === 'signed-in' ? s.user : null;
    if (!u && !token) return setProblem('sign-in');
    try {
      const meta = await api.get(props.id, token);
      location.replace(`#/doc/${await openFromAccount(u, meta, token)}`);
    } catch (e) {
      setDetail(message(e));
      setProblem(e instanceof ApiError && e.status === 404 ? 'not-found' : 'error');
    }
  });
  return (
    <div class="doclist" data-testid="open-shared">
      <Show when={problem()} fallback={<p class="muted">Opening…</p>}>
        {(p) => (
          <>
            <p>
              {p() === 'sign-in'
                ? 'Sign in to open this document.'
                : p() === 'unavailable'
                  ? 'This link needs accounts, which are not available here.'
                  : p() === 'not-found'
                    ? user()
                      ? `This document does not exist or is not shared with ${user()!.email}.`
                      : 'This document does not exist or is not shared with you. Signing in may help.'
                    : `The document could not be opened: ${detail()}.`}
            </p>
            <div class="row">
              <Show when={!user() && p() !== 'unavailable'}>
                <button class="primary" onClick={() => signIn(token ? `${location.pathname}#/s/${props.id}/${token}` : undefined)}>Sign in</button>
              </Show>
              <a class="button" href="#/">All documents</a>
            </div>
          </>
        )}
      </Show>
    </div>
  );
}

/** The open document's save status in the top bar. */
export function SyncBadge(props: { sync: DocSync }) {
  return (
    <span class="hint sync" data-testid="sync" data-status={props.sync.status()} classList={{ warn: !['saving', 'saved', 'view-only'].includes(props.sync.status()) }}>
      {statusText[props.sync.status()]}
    </span>
  );
}

/** Who has access to a document, and (for its owner) adding people and link sharing. */
export function ShareDialog(props: { cloudId: string; name: string; role: Role; onClose: () => void; onCleanCopy?: () => void }) {
  const owner = () => props.role === 'owner';
  const [people, { refetch }] = createResource(() => (props.role === 'viewer' ? false : props.cloudId), (id) => api.shares(id));
  const [meta, { mutate }] = createResource(() => props.cloudId, (id) => api.get(id));
  const [email, setEmail] = createSignal('');
  const [role, setRole] = createSignal<'viewer' | 'editor'>('viewer');
  const [notify, setNotify] = createSignal(true);
  const [note, setNote] = createSignal('');
  const [error, setError] = createSignal('');
  const act = async (fn: () => Promise<string | void>) => {
    setError('');
    setNote('');
    try {
      const n = await fn();
      if (n) setNote(n);
    } catch (e) {
      setError(message(e));
    }
  };
  const add = (e: Event) => {
    e.preventDefault();
    void act(async () => {
      const r = await api.share(props.cloudId, email().trim(), role(), notify());
      setEmail('');
      refetch();
      if (r.notified) return `Shared with ${r.email}, who was sent an e-mail.`;
      return r.mail ? `Shared with ${r.email}. No e-mail was sent: ${MAIL[r.mail] ?? 'sending failed'}; send them the link yourself.` : `Shared with ${r.email}.`;
    });
  };
  const url = (token?: string) => `${location.origin}/#/s/${props.cloudId}${token ? `/${token}` : ''}`;
  const copy = (text: string) =>
    act(async () => {
      await navigator.clipboard.writeText(text);
      return 'Link copied.';
    });
  const link = () => meta()?.link ?? null;

  return (
    <div class="modal-bg" onClick={props.onClose}>
      <div class="modal share" onClick={(e) => e.stopPropagation()} role="dialog" aria-label="Share">
        <h3 style={{ margin: '0 0 10px' }}>Share “{props.name}”</h3>
        <Show when={owner()} fallback={<p class="hint">Only the owner can change who has access.</p>}>
          <form class="row" onSubmit={add}>
            <input class="grow" type="email" required placeholder="E-mail address" aria-label="E-mail address" value={email()} onInput={(e) => setEmail(e.currentTarget.value)} />
            <select aria-label="Access" value={role()} onChange={(e) => setRole(e.currentTarget.value as 'viewer' | 'editor')}>
              <option value="viewer">Can view</option>
              <option value="editor">Can edit</option>
            </select>
            <button class="primary" type="submit">Share</button>
          </form>
          <label class="hint row" style={{ 'margin-top': '6px' }}>
            <input type="checkbox" checked={notify()} onChange={(e) => setNotify(e.currentTarget.checked)} /> Tell them by e-mail
          </label>
        </Show>
        <Show when={owner()}>
          <p class="hint" data-testid="share-history-note">
            Everyone who can open the document receives its whole history, including text that was later deleted.
            <Show when={props.onCleanCopy}>{' '}<a href="#" onClick={(e) => { e.preventDefault(); props.onCleanCopy!(); }}>Share a clean copy instead</a> (a new document with only the current content).</Show>
          </p>
        </Show>
        <Show when={note()}><p class="hint" data-testid="share-note">{note()}</p></Show>
        <Show when={error()}><div class="error" style={{ 'margin-top': '8px' }}>{error()}</div></Show>

        <Show when={people()}>
          {(p) => (
            <table class="kv people" style={{ 'margin-top': '12px' }} data-testid="people">
              <tbody>
                <tr><td>{p().owner.name && p().owner.email ? `${p().owner.name} (${p().owner.email})` : p().owner.name || p().owner.email}</td><td class="muted">owner</td><td /></tr>
                <For each={p().shares}>
                  {(s) => (
                    <tr data-person={s.email}>
                      <td title={s.account ? `Opened by the Google account ${s.account.name ? `${s.account.name}, ` : ''}${s.account.email}; only that account has this access` : 'Not opened yet; the first Google account with this address to open it keeps the access'}>
                        {s.email}
                        <Show when={s.account?.name}><span class="muted"> ({s.account!.name})</span></Show>
                      </td>
                      <td>
                        <Show when={owner()} fallback={<span class="muted">{s.role === 'editor' ? 'can edit' : 'can view'}</span>}>
                          <select aria-label={`Access for ${s.email}`} value={s.role}
                            onChange={(e) => act(async () => { await api.share(props.cloudId, s.email, e.currentTarget.value as 'viewer' | 'editor', false); refetch(); })}>
                            <option value="viewer">Can view</option>
                            <option value="editor">Can edit</option>
                          </select>
                        </Show>
                      </td>
                      <td style={{ 'text-align': 'right' }}>
                        <Show when={owner() || s.email === user()?.email}>
                          <button class="ghost" onClick={() => act(async () => { await api.unshare(props.cloudId, s.email); refetch(); })}>
                            {owner() ? 'Remove' : 'Leave'}
                          </button>
                        </Show>
                      </td>
                    </tr>
                  )}
                </For>
              </tbody>
            </table>
          )}
        </Show>

        <h4 style={{ margin: '16px 0 6px' }}>Link</h4>
        <Show when={owner()}>
          <select aria-label="Link sharing" data-testid="link-role" value={link()?.role ?? 'off'}
            onChange={(e) => act(async () => { mutate(await api.setLink(props.cloudId, e.currentTarget.value as 'off' | 'viewer' | 'editor')); })}>
            <option value="off">Only people added above</option>
            <option value="viewer">Anyone with the link can view</option>
            <option value="editor">Anyone with the link can edit (signed in)</option>
          </select>
        </Show>
        <div class="row" style={{ 'margin-top': '6px' }}>
          <input class="grow" readOnly aria-label="Link" data-testid="share-link" value={link() ? url(link()!.token) : url()} onFocus={(e) => e.currentTarget.select()} />
          <button onClick={() => copy(link() ? url(link()!.token) : url())}>Copy link</button>
        </div>
        <p class="hint">{link() ? 'Anyone with this link can open the document.' : 'This link works for the people the document is shared with.'}</p>
        <div class="row" style={{ 'margin-top': '10px' }}>
          <div class="grow" />
          <button onClick={props.onClose}>Done</button>
        </div>
      </div>
    </div>
  );
}
