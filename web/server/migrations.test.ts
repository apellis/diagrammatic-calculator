import { afterEach, describe, expect, it } from 'vitest';
import { applyMigrations, migrations, statements, storage } from './testing';
import type { D1Database } from './env';

describe('D1 migrations', () => {
  let dispose: (() => Promise<void>) | null = null;
  afterEach(async () => {
    await dispose?.();
    dispose = null;
  });
  async function fresh(): Promise<D1Database> {
    const s = await storage();
    dispose = () => s.mf.dispose();
    return s.DB;
  }

  it('are numbered files applied in order', () => {
    const names = migrations().map((m) => m.name);
    expect(names[0]).toBe('0001_accounts.sql');
    names.forEach((n, i) => expect(n.startsWith(String(i + 1).padStart(4, '0'))).toBe(true));
  });

  it('create the tables and indexes the API uses on an empty database', async () => {
    const db = await fresh();
    await applyMigrations(db);
    const rows = await db.prepare("SELECT type, name FROM sqlite_master WHERE type IN ('table', 'index') AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_cf_%' ORDER BY name").all<{ type: string; name: string }>();
    expect(new Set(rows.results.map((r) => `${r.type} ${r.name}`))).toEqual(
      new Set([
        'table users',
        'index users_email',
        'table sessions',
        'index sessions_user',
        'index sessions_expires',
        'table documents',
        'index documents_owner',
        'table shares',
        'index shares_email',
        'table sent_mail',
        'index sent_mail_sender',
        'index sent_mail_recipient',
        'index sent_mail_created',
        'table mail_optout',
        'table pending_deletes',
        'table doc_versions', 'index doc_versions_time',
        'table doc_branches', 'index doc_branches_user',
        'table live_sockets', 'index live_sockets_user',
      ]),
    );
    const cols = async (t: string) => (await db.prepare(`SELECT name FROM pragma_table_info('${t}')`).all<{ name: string }>()).results.map((r) => r.name);
    expect(await cols('documents')).toEqual(['id', 'owner_id', 'name', 'created', 'edited', 'version', 'size', 'snapshot_key', 'link_role', 'link_token', 'files_size', 'generation']);
    expect(await cols('shares')).toEqual(['doc_id', 'email', 'role', 'created', 'invited_by', 'user_id']);
    expect(await cols('sessions')).toEqual(['id_hash', 'user_id', 'created', 'expires', 'last_seen']);
  });

  it('fail on a second application, so wrangler must track them (it does)', async () => {
    const db = await fresh();
    await applyMigrations(db);
    await expect(applyMigrations(db)).rejects.toThrow();
  });

  it('enforce roles, link consistency, and cascading deletes', async () => {
    const db = await fresh();
    await applyMigrations(db);
    const run = (sql: string, ...v: unknown[]) => db.prepare(sql).bind(...v).run();
    await run('INSERT INTO users VALUES (?, ?, ?, ?, ?, ?)', 'u1', 'sub1', 'a@example.org', 'A', 1, 1);
    await run('INSERT INTO users VALUES (?, ?, ?, ?, ?, ?)', 'u2', 'sub2', 'b@example.org', 'B', 1, 1);
    await expect(run('INSERT INTO users VALUES (?, ?, ?, ?, ?, ?)', 'u3', 'sub1', 'c@example.org', 'C', 1, 1)).rejects.toThrow(/UNIQUE/);
    await run('INSERT INTO documents (id, owner_id, name, created, edited) VALUES (?, ?, ?, ?, ?)', 'd1', 'u1', 'Doc', 1, 1);
    await expect(run('INSERT INTO documents (id, owner_id, name, created, edited) VALUES (?, ?, ?, ?, ?)', 'd2', 'nobody', 'Doc', 1, 1)).rejects.toThrow(/FOREIGN KEY/);
    await expect(run("UPDATE documents SET link_role = 'owner', link_token = 't' WHERE id = 'd1'")).rejects.toThrow(/CHECK/);
    await expect(run("UPDATE documents SET link_role = 'viewer' WHERE id = 'd1'")).rejects.toThrow(/CHECK/);
    await run("UPDATE documents SET link_role = 'viewer', link_token = 't' WHERE id = 'd1'");
    await expect(run('INSERT INTO shares (doc_id, email, role, created, invited_by) VALUES (?, ?, ?, ?, ?)', 'd1', 'b@example.org', 'owner', 1, 'u1')).rejects.toThrow(/CHECK/);
    await run('INSERT INTO shares (doc_id, email, role, created, invited_by) VALUES (?, ?, ?, ?, ?)', 'd1', 'b@example.org', 'editor', 1, 'u1');
    await run('INSERT INTO sessions (id_hash, user_id, created, expires) VALUES (?, ?, ?, ?)', 'h1', 'u1', 1, 2);
    await run('INSERT INTO sent_mail (sender_id, created) VALUES (?, ?)', 'u1', 1);
    // a share claimed by an account goes with that account
    await run('INSERT INTO shares (doc_id, email, role, created, invited_by) VALUES (?, ?, ?, ?, ?)', 'd1', 'c@example.org', 'viewer', 1, 'u1');
    await run("UPDATE shares SET user_id = 'u2' WHERE email = 'c@example.org'");
    await run("DELETE FROM users WHERE id = 'u1'");
    for (const t of ['documents', 'shares', 'sessions', 'sent_mail']) {
      expect((await db.prepare(`SELECT COUNT(*) AS n FROM ${t}`).first<{ n: number }>())!.n).toBe(0);
    }
  });

  it('keep existing sessions alive across the idle-timeout migration', async () => {
    const db = await fresh();
    const [first, ...rest] = migrations();
    await db.batch(statements(first.sql).map((s) => db.prepare(s)));
    await db.prepare('INSERT INTO users VALUES (?, ?, ?, ?, ?, ?)').bind('u1', 'sub1', 'a@example.org', 'A', 1, 1).run();
    await db.prepare('INSERT INTO sessions VALUES (?, ?, ?, ?)').bind('h1', 'u1', 1234, 9999).run();
    for (const m of rest) await db.batch(statements(m.sql).map((s) => db.prepare(s)));
    expect(await db.prepare('SELECT last_seen FROM sessions').first()).toEqual({ last_seen: 1234 });
  });

  it('split into statements without comments', () => {
    const s = statements(migrations()[0].sql);
    expect(s.every((x) => !x.includes('--'))).toBe(true);
    expect(s.filter((x) => x.startsWith('CREATE TABLE'))).toHaveLength(5);
  });
});
