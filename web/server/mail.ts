// Share notifications through Resend: the only e-mail the app sends.
import type { Deps, Env } from './env';

export const RESEND_API = 'https://api.resend.com/emails';

const oneLine = (s: string, max: number) => s.replace(/[\r\n\t]+/g, ' ').trim().slice(0, max);
const escape = (s: string) => s.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);

export interface ShareMail {
  to: string;
  inviter: string;
  docName: string;
  role: 'viewer' | 'editor';
  url: string;
}

/** The message, as subject, plain text and HTML (every inserted value escaped). */
export function shareMessage(m: ShareMail): { subject: string; text: string; html: string } {
  const who = oneLine(m.inviter, 120) || 'Someone';
  const doc = oneLine(m.docName, 120) || 'Untitled';
  const can = m.role === 'editor' ? 'edit' : 'view';
  const subject = `${who} shared “${doc}” with you`;
  const text = `${who} shared the document “${doc}” with you on diagrammatic-calculator. You can ${can} it.\n\nOpen it: ${m.url}\n\nSign in with this e-mail address's Google account to see it.\n`;
  const html =
    `<p>${escape(who)} shared the document <b>${escape(doc)}</b> with you on diagrammatic-calculator. You can ${can} it.</p>` +
    `<p><a href="${escape(m.url)}">Open “${escape(doc)}”</a></p>` +
    `<p style="color:#666">Sign in with this e-mail address's Google account to see it.</p>`;
  return { subject, text, html };
}

/** Sends a share notification; false when e-mail is not configured or Resend refuses. */
export async function sendShareMail(env: Env, deps: Deps, m: ShareMail): Promise<boolean> {
  if (!env.RESEND_API_KEY || !env.EMAIL_FROM) return false;
  const msg = shareMessage(m);
  try {
    const r = await deps.fetch(RESEND_API, {
      method: 'POST',
      headers: { authorization: `Bearer ${env.RESEND_API_KEY}`, 'content-type': 'application/json' },
      body: JSON.stringify({ from: env.EMAIL_FROM, to: [m.to], subject: msg.subject, text: msg.text, html: msg.html }),
    });
    return r.ok;
  } catch {
    return false;
  }
}
