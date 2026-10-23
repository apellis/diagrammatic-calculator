// The bindings and secrets the accounts API runs with, and the small parts of the D1 and R2
// interfaces it uses (declared here so the server code type-checks with the app's own settings,
// without the Workers type library).

export interface D1Result<T = Record<string, unknown>> {
  results: T[];
  meta: { changes?: number };
}

export interface D1Statement {
  bind(...values: unknown[]): D1Statement;
  first<T = Record<string, unknown>>(): Promise<T | null>;
  all<T = Record<string, unknown>>(): Promise<D1Result<T>>;
  run(): Promise<D1Result>;
}

export interface D1Database {
  prepare(sql: string): D1Statement;
  batch(statements: D1Statement[]): Promise<D1Result[]>;
}

export interface R2Object {
  key: string;
  size: number;
  uploaded: Date;
  httpMetadata?: { contentType?: string };
}

export interface R2ObjectBody extends R2Object {
  arrayBuffer(): Promise<ArrayBuffer>;
}

export interface R2Bucket {
  get(key: string): Promise<R2ObjectBody | null>;
  put(key: string, value: ArrayBuffer | Uint8Array | string, options?: { httpMetadata?: { contentType?: string } }): Promise<unknown>;
  delete(keys: string | string[]): Promise<void>;
  list(options: { prefix: string; cursor?: string }): Promise<{ objects: R2Object[]; truncated: boolean; cursor?: string }>;
}

/**
 * What the Pages Function receives. Bindings come from wrangler.toml (`DB`, `FILES`, and the
 * `EMAIL_FROM` variable); the rest are secrets set on the Pages project. Any of them may be
 * missing: the API then reports that accounts are not available and the app stays anonymous.
 */
export interface Env {
  DB?: D1Database;
  FILES?: R2Bucket;
  GOOGLE_CLIENT_ID?: string;
  GOOGLE_CLIENT_SECRET?: string;
  /** At least 32 random bytes; signs session cookies, CSRF tokens and the sign-in state. */
  SESSION_SECRET?: string;
  /** Share notifications are sent only when this and EMAIL_FROM are set. */
  RESEND_API_KEY?: string;
  /** The sender of share notifications, for example `Name <share@send.example.org>`. */
  EMAIL_FROM?: string;
}

/** An Env with everything sign-in needs. */
export interface ReadyEnv extends Env {
  DB: D1Database;
  FILES: R2Bucket;
  GOOGLE_CLIENT_ID: string;
  GOOGLE_CLIENT_SECRET: string;
  SESSION_SECRET: string;
}

export function ready(env: Env): env is ReadyEnv {
  return !!(env.DB && env.FILES && env.GOOGLE_CLIENT_ID && env.GOOGLE_CLIENT_SECRET && env.SESSION_SECRET && env.SESSION_SECRET.length >= 32);
}

/** Outside effects, replaceable in tests. */
export interface Deps {
  fetch: typeof fetch;
  now: () => number;
}

export const defaultDeps: Deps = { fetch: (input, init) => fetch(input, init), now: () => Date.now() };
