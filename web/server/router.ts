// The accounts API under /api/: sign-in, the session, documents, and sharing. Every route checks
// the session and the caller's role on the document; every state-changing request needs the
// session's CSRF token.
import { appOrigin, defaultDeps, ready, type Deps, type Env, type ReadyEnv } from './env';
import { HttpError, empty, json, redirect } from './http';
import { callback, csrfOk, csrfToken, currentSession, login, logout, logoutEverywhere, rotate, securityEvent, type Session } from './auth';
import * as docs from './docs';
import * as live from './live';
import * as versions from './versions';
import { unsubscribe } from './mail';

const UNSAFE = new Set(['POST', 'PUT', 'PATCH', 'DELETE']);

interface Call {
  req: Request;
  env: ReadyEnv;
  deps: Deps;
  s: Session | null;
  method: string;
  path: string;
  seg: string[];
}

export async function handle(req: Request, env: Env, deps: Deps = defaultDeps): Promise<Response> {
  const path = new URL(req.url).pathname.replace(/^\/api\/?/, '').replace(/\/$/, '');
  const seg = path.split('/').map((p) => {
    try {
      return decodeURIComponent(p);
    } catch {
      return '\u0000';
    }
  });
  const method = req.method.toUpperCase();
  // for the log: the route with IDs, names, and addresses left out
  const route = `${method} ${seg.map((p, i) => (seg[0] === 'docs' && i === 1 ? ':id' : i === 3 ? ':name' : p)).join('/')}`;
  try {
    if (!ready(env)) {
      if (path === 'me' && method === 'GET') return json({ accounts: false, user: null });
      if (path === 'auth/login') return redirect('/?signin=unavailable#/');
      throw new HttpError(503, 'accounts are not available');
    }
    // only the configured site's own host serves the API (not an old deployment's address)
    if (new URL(req.url).origin !== appOrigin(env, req)) throw new HttpError(421, 'this address does not serve the accounts API');
    // one-click unsubscribe from a share notification: no session, as mail clients send it
    if (path === 'mail/unsubscribe' && (method === 'GET' || method === 'POST')) return await unsubscribe(req, env, deps);
    // Google's security events (Cross-Account Protection): signed by Google, not sent by a browser
    if (path === 'google/risc' && method === 'POST') return await securityEvent(req, env, deps);
    const s = await currentSession(req, env, deps);
    const anonymousTicket = !s && method === 'POST' && seg.length === 3 && seg[0] === 'docs' && seg[2] === 'live-ticket' && !!req.headers.get('x-link-token');
    if (anonymousTicket) live.sameOrigin(req, env);
    if (UNSAFE.has(method) && !anonymousTicket) {
      if (!s) {
        // signing out without a session only clears the cookie
        if (path === 'auth/logout') return empty(204, await logout(env, null));
        throw new HttpError(401, 'sign in first');
      }
      if (!(await csrfOk(req, env, s))) throw new HttpError(403, 'missing or wrong CSRF token');
    }
    return await dispatch({ req, env, deps, s, method, path, seg });
  } catch (e) {
    if (e instanceof HttpError) return json({ error: e.code, ...e.extra }, e.status);
    // never log request data: only the route pattern and the kind of error
    console.error(`api: ${route}: ${e instanceof Error ? e.name : 'error'}`);
    return json({ error: 'internal error' }, 500);
  }
}

async function dispatch({ req, env, deps, s, method, path, seg }: Call): Promise<Response> {
  switch (`${method} ${path}`) {
    case 'GET me': {
      if (!s) return json({ accounts: true, user: null });
      const r = await rotate(env, s, deps);
      return json({ accounts: true, user: s.user, csrf: await csrfToken(env, r.s) }, 200, {}, r.cookies);
    }
    case 'DELETE me':
      await docs.deleteAccount(env, s!, deps);
      return empty(204, await logout(env, null));
    case 'GET auth/login':
      return login(req, env, deps);
    case 'GET auth/callback':
      return callback(req, env, deps);
    case 'POST auth/logout':
      return empty(204, await logout(env, s));
    case 'POST auth/logout-all':
      return empty(204, await logoutEverywhere(env, s!));
    case 'GET docs':
      return docs.listDocs(env, s);
    case 'POST docs':
      return docs.createDoc(req, env, s, deps);
  }
  if (seg[0] !== 'docs' || seg.length < 2 || seg.length > 5) throw new HttpError(404, 'no such route');

  if (method === 'GET' && seg.length === 3 && seg[2] === 'live') return live.openLive(req, env, seg[1], s, deps);
  const a = await docs.access(env, seg[1], s, req.headers.get('x-link-token'));
  if(seg.length===5&&seg[2]==='proposals'&&seg[4]==='decision'&&method==='POST')return live.decideProposal(req,env,a,s,seg[3]);
  const sub = seg.slice(2, 3).join('');
  const name = seg[3];
  switch (`${method} ${sub}${name === undefined ? '' : '/:name'}`) {
    case 'GET ':
      return docs.getDoc(a);
    case 'PATCH ':
      return docs.patchDoc(req, env, a, s);
    case 'DELETE ':
      return docs.deleteDoc(env, a, s, deps);
    case 'GET snapshot':
      return docs.getSnapshot(env, a);
    case 'PUT snapshot':
      return live.forwardWrite(req, env, a, s);
    case 'POST migrate':
      return live.forwardWrite(req, env, a, s, true);
    case 'POST live-ticket':
      return live.liveTicket(req, env, a, s, deps);
    case 'GET versions':
      return versions.listVersions(env, a);
    case 'GET versions/:name':
      return versions.getVersion(env, a, name);
    case 'GET branches':
      return versions.listBranches(env, a, s);
    case 'GET branches/:name':
      return versions.getBranch(env, a, s, name);
    case 'PUT branches/:name':
      return versions.putBranch(req, env, a, s, name, deps);
    case 'DELETE branches/:name':
      return versions.deleteBranch(env, a, s, name, deps);
    case 'GET shares':
      return docs.listShares(env, a, s);
    case 'PUT shares':
      return docs.putShare(req, env, a, s, deps);
    case 'DELETE shares/:name':
      return docs.deleteShare(env, a, s, name);
    case 'GET files':
      return docs.listFiles(env, a);
    case 'GET files/:name':
      return docs.getFile(env, a, name);
    case 'PUT files/:name':
      return docs.putFile(req, env, a, s, name);
    case 'DELETE files/:name':
      return docs.deleteFile(env, a, s, name);
  }
  throw new HttpError(405, 'method not allowed');
}
