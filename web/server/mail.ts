// Share notifications through Resend: the only e-mail the app sends. Every message names the
// inviter by their verified Google address, keeps the document's title (which anyone can choose)
// out of the subject and defuses anything in it that looks like a web address, and carries a
// one-click unsubscribe link (RFC 8058).
import type { Deps, Env, ReadyEnv } from './env';
import { sign } from './crypto';
import { readBody } from './http';

export const RESEND_API = 'https://api.resend.com/emails';

const oneLine = (s: string, max: number) => s.replace(/[\r\n\t\u0000-\u001f\u007f]+/g, ' ').trim().slice(0, max);
const escape = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

/**
 * Text that mail clients would turn into a link (a scheme, `www.`, or a dotted host name such as
 * `evil.example.com`) is replaced, so a title or display name cannot carry a link in our name.
 */
export function defang(s: string): string {
  return s
    .replace(/\b[a-z][a-z0-9+.-]*:\/\/\S*/gi, '[link removed]')
    .replace(/\bwww\.\S*/gi, '[link removed]')
    .replace(/\b[a-z0-9-]+(?:\.[a-z0-9-]+)*\.[a-z]{2,}\b(?:\/\S*)?/gi, '[link removed]');
}

export interface ShareMail {
  to: string;
  /** The inviter's display name (chosen by them) and verified address. */
  inviterName: string;
  inviterEmail: string;
  docName: string;
  role: 'viewer' | 'editor';
  url: string;
  /** Where the recipient stops further notifications. */
  unsubscribeUrl: string;
}

/** The message, as subject, plain text, and HTML (every inserted value escaped). */
export function shareMessage(m: ShareMail): { subject: string; text: string; html: string } {
  const name = defang(oneLine(m.inviterName, 60));
  const addr = oneLine(m.inviterEmail, 254);
  const who = name && name !== addr ? `${name} (${addr})` : addr;
  const doc = defang(oneLine(m.docName, 120)) || 'Untitled';
  const can = m.role === 'editor' ? 'edit' : 'view';
  const subject = `A document was shared with you on diagrammatic-calculator`;
  const text =
    `${who} shared a document with you on diagrammatic-calculator. You can ${can} it.\n` +
    `Title (as given by the sender): ${doc}\n\nOpen it: ${m.url}\n\n` +
    `Sign in with this e-mail address's Google account to see it.\n\n` +
    `To receive no more of these messages: ${m.unsubscribeUrl}\n`;
  const html =
    `<p>${escape(who)} shared a document with you on diagrammatic-calculator. You can ${can} it.</p>` +
    `<p>Title (as given by the sender): <b>${escape(doc)}</b></p>` +
    `<p><a href="${escape(m.url)}">Open the document</a></p>` +
    `<p style="color:#666">Sign in with this e-mail address's Google account to see it.</p>` +
    `<p style="color:#666;font-size:small"><a href="${escape(m.unsubscribeUrl)}">Receive no more of these messages</a></p>`;
  return { subject, text, html };
}

/** The keyed hash under which a recipient's address is counted and opted out (never stored in clear). */
export const recipientKey = (env: ReadyEnv, email: string) => sign(env.SESSION_SECRET, 'mail-recipient', email.toLowerCase());

/** Sends a share notification; false when e-mail is not configured or Resend refuses. */
export async function sendShareMail(env: Env, deps: Deps, m: ShareMail): Promise<boolean> {
  if (!env.RESEND_API_KEY || !env.EMAIL_FROM) return false;
  const msg = shareMessage(m);
  try {
    const r = await deps.fetch(RESEND_API, {
      method: 'POST',
      headers: { authorization: `Bearer ${env.RESEND_API_KEY}`, 'content-type': 'application/json' },
      body: JSON.stringify({
        from: env.EMAIL_FROM,
        to: [m.to],
        subject: msg.subject,
        text: msg.text,
        html: msg.html,
        headers: { 'List-Unsubscribe': `<${m.unsubscribeUrl}>`, 'List-Unsubscribe-Post': 'List-Unsubscribe=One-Click' },
      }),
    });
    return r.ok;
  } catch {
    return false;
  }
}

const page = (body: string, status = 200) =>
  new Response(
    `<!doctype html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>diagrammatic-calculator</title>` +
      `<body style="font:16px system-ui,sans-serif;max-width:36em;margin:3em auto;padding:0 16px">${body}</body>`,
    {
      status,
      headers: {
        'content-type': 'text/html; charset=utf-8',
        'cache-control': 'no-store',
        'x-content-type-options': 'nosniff',
        'referrer-policy': 'no-referrer',
        'content-security-policy': "default-src 'none'; style-src 'unsafe-inline'; form-action 'self'; frame-ancestors 'none'; base-uri 'none'",
      },
    },
  );

/**
 * GET /api/mail/unsubscribe?r=<key>: a page with a button (a GET never changes anything, since
 * mail scanners follow links); POST, from that button or a mail client's one-click unsubscribe,
 * records the opt-out. The key is the recipient's keyed hash, which only their messages carry.
 */
export async function unsubscribe(req: Request, env: ReadyEnv, deps: Deps): Promise<Response> {
  const r = new URL(req.url).searchParams.get('r') ?? '';
  if (!/^[A-Za-z0-9_-]{43}$/.test(r)) return page('<p>This unsubscribe link is not valid.</p>', 400);
  if (req.method === 'GET') {
    return page(
      `<p>Stop receiving e-mails when someone shares a diagrammatic-calculator document with this address?</p>` +
        `<form method="post"><button type="submit">Unsubscribe</button></form>`,
    );
  }
  await readBody(req, 1024);
  await env.DB.prepare('INSERT INTO mail_optout (recipient, created) VALUES (?, ?) ON CONFLICT (recipient) DO NOTHING').bind(r, deps.now()).run();
  return page('<p>Done: this address will receive no more share notifications from diagrammatic-calculator.</p>');
}
