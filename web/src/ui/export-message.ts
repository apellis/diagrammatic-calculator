/** What the copy kept with a cloud document became after an export: not attempted (no cloud
 * document, view mode, or a branch), saved, or failed with an error message. */
export type KeptCopy = { kind: 'none' } | { kind: 'saved' } | { kind: 'failed'; error: string };

/** The message shown after Export: whether every cell parsed, and what happened to the copy kept
 * with the document in the account. A failed copy is said, never hidden: the download itself is
 * complete either way. */
export function exportMessage(canonical: boolean, copy: KeptCopy): string {
  const done = canonical ? 'Exported' : 'Exported as entered';
  const parse = canonical ? '' : ': some cells do not parse';
  switch (copy.kind) {
    case 'none':
      return `${done}${parse}.`;
    case 'saved':
      return `${done}${parse}; a copy is kept with the document in your account.`;
    case 'failed':
      return `${done}${parse}, but the copy kept with the document in your account was not saved (${copy.error}). The downloaded file is complete.`;
  }
}
