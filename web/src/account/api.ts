// The accounts API as the page calls it (the server is web/server/). Every state-changing call
// carries the session's CSRF token; a link token, when there is one, goes in X-Link-Token.

export type Role = 'owner' | 'editor' | 'viewer';

export interface User {
  id: string;
  email: string;
  name: string;
}

export interface Me {
  accounts: boolean;
  user: User | null;
  csrf?: string;
}

export interface Person {
  /** Left out for someone who has the document only through its link. */
  email?: string;
  name: string;
}

export interface CloudDoc {
  id: string;
  name: string;
  created: number;
  edited: number;
  version: number;
  size: number;
  role: Role;
  owner?: Person;
  /** The owner sees the link settings: null while link sharing is off. */
  link?: { role: 'viewer' | 'editor'; token: string } | null;
}

export interface Share {
  email: string;
  role: 'viewer' | 'editor';
  created: number;
  /** The account that has used this share (only that account has it from then on), if any. */
  account?: { name: string; email: string } | null;
}

/** A file kept with a document (an exported .dcw or .dch). */
export interface KeptFile {
  name: string;
  size: number;
  uploaded: number;
}

/** Why a share notification asked for was not sent. */
export type MailOutcome = 'unavailable' | 'new-account' | 'opted-out' | 'limit' | 'failed';

export class ApiError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    /** The server's current version, on a 409 from a save. */
    readonly version?: number,
  ) {
    super(code);
    this.name = 'ApiError';
  }
}

type Init = RequestInit & { json?: unknown; link?: string };

export class Api {
  csrf = '';
  constructor(
    private readonly base = '/api',
    private readonly f: typeof fetch = (input, init) => fetch(input, init),
  ) {}

  private async call(path: string, init: Init = {}, retried = false): Promise<Response> {
    const headers = new Headers(init.headers);
    const method = (init.method ?? 'GET').toUpperCase();
    if (method !== 'GET' && this.csrf) headers.set('x-csrf-token', this.csrf);
    if (init.link) headers.set('x-link-token', init.link);
    let body = init.body;
    if (init.json !== undefined) {
      body = JSON.stringify(init.json);
      headers.set('content-type', 'application/json');
    }
    const r = await this.f(this.base + path, { method, headers, body, credentials: 'same-origin' });
    if (!r.ok) {
      let e: { error?: string; version?: number } = {};
      try {
        e = (await r.json()) as typeof e;
      } catch {
        // not JSON: keep the status
      }
      // the session was renewed (another tab read /api/me): fetch its token and try once more
      if (r.status === 403 && e.error === 'missing or wrong CSRF token' && !retried && method !== 'GET' && (await this.me())) return this.call(path, init, true);
      throw new ApiError(r.status, e.error ?? r.statusText, e.version);
    }
    return r;
  }

  /**
   * Who is signed in. Null when there is no accounts API here (a static host, the development
   * server, no network): the app then stays anonymous.
   */
  async me(timeoutMs = 5000): Promise<Me | null> {
    try {
      const r = await this.f(`${this.base}/me`, { credentials: 'same-origin', signal: AbortSignal.timeout(timeoutMs) });
      if (!r.ok || !(r.headers.get('content-type') ?? '').includes('application/json')) return null;
      const me = (await r.json()) as Me;
      if (typeof me?.accounts !== 'boolean') return null;
      this.csrf = me.csrf ?? '';
      return me;
    } catch {
      return null;
    }
  }

  async logout(): Promise<void> {
    await this.call('/auth/logout', { method: 'POST' });
    this.csrf = '';
  }

  /** Ends every session of the account, in every browser. */
  async logoutEverywhere(): Promise<void> {
    await this.call('/auth/logout-all', { method: 'POST' });
    this.csrf = '';
  }

  async list(): Promise<{ owned: CloudDoc[]; shared: CloudDoc[] }> {
    return (await this.call('/docs')).json();
  }

  async create(name: string): Promise<CloudDoc> {
    return (await this.call('/docs', { method: 'POST', json: { name } })).json();
  }

  async get(id: string, link?: string): Promise<CloudDoc> {
    return (await this.call(`/docs/${id}`, { link })).json();
  }

  async snapshot(id: string, link?: string): Promise<{ data: Uint8Array; version: number; role: Role }> {
    const r = await this.call(`/docs/${id}/snapshot`, { link });
    return { data: new Uint8Array(await r.arrayBuffer()), version: Number(r.headers.get('x-doc-version')), role: r.headers.get('x-doc-role') as Role };
  }

  /** Saves on top of `version`; an ApiError with status 409 carries the current version. */
  async save(id: string, data: Uint8Array, version: number, name: string, link?: string): Promise<{ version: number }> {
    const headers = { 'if-match': `"${version}"`, 'x-doc-name': encodeURIComponent(name), 'content-type': 'application/octet-stream' };
    return (await this.call(`/docs/${id}/snapshot`, { method: 'PUT', headers, body: data.slice(), link })).json();
  }

  async setLink(id: string, link: 'off' | 'viewer' | 'editor'): Promise<CloudDoc> {
    return (await this.call(`/docs/${id}`, { method: 'PATCH', json: { link } })).json();
  }

  async remove(id: string): Promise<void> {
    await this.call(`/docs/${id}`, { method: 'DELETE' });
  }

  async shares(id: string): Promise<{ owner: Person; shares: Share[] }> {
    return (await this.call(`/docs/${id}/shares`)).json();
  }

  async share(id: string, email: string, role: 'viewer' | 'editor', notify: boolean): Promise<{ email: string; role: string; notified: boolean; mail?: MailOutcome }> {
    return (await this.call(`/docs/${id}/shares`, { method: 'PUT', json: { email, role, notify } })).json();
  }

  async unshare(id: string, email: string): Promise<void> {
    await this.call(`/docs/${id}/shares/${encodeURIComponent(email)}`, { method: 'DELETE' });
  }

  async putFile(id: string, name: string, text: string): Promise<void> {
    await this.call(`/docs/${id}/files/${encodeURIComponent(name)}`, { method: 'PUT', body: text, headers: { 'content-type': 'text/plain; charset=utf-8' } });
  }

  async files(id: string, link?: string): Promise<KeptFile[]> {
    return ((await (await this.call(`/docs/${id}/files`, { link })).json()) as { files: KeptFile[] }).files;
  }

  async file(id: string, name: string, link?: string): Promise<string> {
    return (await this.call(`/docs/${id}/files/${encodeURIComponent(name)}`, { link })).text();
  }

  async deleteFile(id: string, name: string): Promise<void> {
    await this.call(`/docs/${id}/files/${encodeURIComponent(name)}`, { method: 'DELETE' });
  }
}
