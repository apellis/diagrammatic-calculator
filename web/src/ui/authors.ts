import { createEffect, createResource, createSignal } from 'solid-js';
import { api, user } from '../account/account';
import type { DocSync } from '../account/docsync';

/** Document content provides IDs, never display names. Resolve names only from authenticated APIs. */
export function useAuthors(sync: () => DocSync | null) {
  const [known, setKnown] = createSignal<Record<string, string>>({});
  const [people] = createResource(() => {
    const s = sync(), u = user();
    return s?.cloudId && s.canEdit && u ? JSON.stringify([u.id, s.cloudId]) : undefined;
  }, async (key) => {
    try { return await api.shares(JSON.parse(key)[1]); } catch { return undefined; }
  });
  createEffect(() => {
    const names = { ...known() };
    for (const p of sync()?.presence() ?? []) if (p.user) names[p.user.id] = p.user.name;
    const shares = people();
    if (shares?.owner.id) names[shares.owner.id] = shares.owner.name;
    for (const s of shares?.shares ?? []) if (s.accountId && s.account) names[s.accountId] = s.account.name;
    if (JSON.stringify(names) !== JSON.stringify(known())) setKnown(names);
  });
  return (id?: string, kind?: 'person' | 'ai') => {
    const own = id === 'you' || id === 'AI' || id === user()?.id;
    const name = own ? 'you' : id && known()[id] || 'another collaborator';
    return kind === 'ai' || id === 'AI' ? own ? 'your AI' : `${name}'s AI` : name;
  };
}
