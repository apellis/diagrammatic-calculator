// Responses, errors and cookies.

export class HttpError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    readonly extra: Record<string, unknown> = {},
  ) {
    super(code);
    this.name = 'HttpError';
  }
}

const base = { 'cache-control': 'no-store', 'x-content-type-options': 'nosniff' };

export function json(body: unknown, status = 200, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json; charset=utf-8', ...base, ...headers } });
}

export function empty(status = 204, cookies: string[] = []): Response {
  const h = new Headers(base);
  for (const c of cookies) h.append('set-cookie', c);
  return new Response(null, { status, headers: h });
}

export function binary(body: ArrayBuffer, headers: Record<string, string>): Response {
  return new Response(body, { status: 200, headers: { ...base, ...headers } });
}

/** A redirect to a same-origin path or an absolute URL, setting cookies on the way. */
export function redirect(location: string, cookies: string[] = []): Response {
  const h = new Headers({ ...base, location });
  for (const c of cookies) h.append('set-cookie', c);
  return new Response(null, { status: 302, headers: h });
}

export function cookies(req: Request): Record<string, string> {
  const out: Record<string, string> = {};
  for (const part of (req.headers.get('cookie') ?? '').split(';')) {
    const i = part.indexOf('=');
    if (i < 0) continue;
    const k = part.slice(0, i).trim();
    if (k && !(k in out)) out[k] = part.slice(i + 1).trim();
  }
  return out;
}

/**
 * A `__Host-` cookie: sent only over HTTPS, only to this origin, never readable by scripts, and
 * sent on top-level navigations from other sites (SameSite=Lax) so Google's redirect back carries
 * the sign-in state.
 */
export function setCookie(name: string, value: string, maxAgeSeconds: number): string {
  return `${name}=${value}; Path=/; Max-Age=${maxAgeSeconds}; HttpOnly; Secure; SameSite=Lax`;
}

export const clearCookie = (name: string) => setCookie(name, '', 0);

export async function readJson<T>(req: Request, maxBytes = 16 * 1024): Promise<T> {
  if (!(req.headers.get('content-type') ?? '').startsWith('application/json')) throw new HttpError(415, 'expected application/json');
  const text = await req.text();
  if (text.length > maxBytes) throw new HttpError(413, 'request too large');
  try {
    const v = JSON.parse(text) as unknown;
    if (typeof v !== 'object' || v === null || Array.isArray(v)) throw new Error();
    return v as T;
  } catch {
    throw new HttpError(400, 'invalid JSON');
  }
}
