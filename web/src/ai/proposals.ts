// A proposal contains data that any editor can evaluate, not a callback in its author's tab.
// Preparation never changes the working document. The room arbitrates the commit separately.
import * as Y from 'yjs';
import { Doc } from '../doc/model';
import { applyProposedWrite, type Proposal, type ToolHost } from './tools';

export function sharedProposals(doc: Doc): Proposal[] {
  return doc.proposalList().flatMap((record) => {
    const p = record.payload as unknown as Partial<Proposal> | null;
    if (!p || typeof p !== 'object' || !Number.isSafeInteger(p.id) || !['write', 'remove', 'accept-branch'].includes(p.kind ?? '') || typeof p.target !== 'string' || typeof p.note !== 'string') return [];
    return [{ ...p, key: record.id, author: record.author, state: record.status === 'pending' ? 'open' : record.status } as Proposal];
  });
}

export async function stateHash(doc: Y.Doc): Promise<string> {
  const bytes = Y.encodeStateAsUpdate(doc);
  return [...new Uint8Array(await crypto.subtle.digest('SHA-256', bytes.slice()))].map((b) => b.toString(16).padStart(2, '0')).join('');
}

export async function prepareDecision(main: Doc, proposal: Proposal, host: ToolHost, accept: boolean, userId: string): Promise<{ update: Uint8Array; base: string }> {
  const record = proposal.key && main.proposals.get(proposal.key);
  if (!record || record.status !== 'pending') throw new Error('This proposal was already decided.');
  if (accept && (!proposal.base || JSON.stringify(proposal.base) !== JSON.stringify(main.inputs())))
    throw new Error('The document changed since this proposal. Ask for an updated proposal.');
  const source = Y.encodeStateAsUpdate(main.ydoc);
  const digest = [...new Uint8Array(await crypto.subtle.digest('SHA-256', source.slice()))].map((b) => b.toString(16).padStart(2, '0')).join('');
  const ydoc = new Y.Doc();
  Y.applyUpdate(ydoc, source);
  const clone = new Doc(ydoc); clone.setIdentity(userId);
  try {
    const cloneHost: ToolHost = { ...host, main: clone, doc: () => clone, branch: () => null, termContext: () => clone.context, foreign: () => false, readOnly: () => false,
      createBranch: () => { throw new Error('Personal branches are not shared proposals'); },
      discardBranch: () => { throw new Error('Personal branches are not shared proposals'); },
    };
    if (accept) {
      if (proposal.kind === 'remove') clone.removeCell(proposal.target, proposal.note);
      else if (proposal.kind === 'accept-branch') {
        if (!proposal.after) throw new Error('This branch proposal has no published changes.');
        clone.setInputsStep(proposal.after, proposal.note);
      } else {
        if (!proposal.operation) throw new Error('This old proposal must be made again by its author.');
        await applyProposedWrite(cloneHost, proposal.operation);
      }
    }
    clone.proposals.set(record.id, { ...record, status: accept ? 'accepted' : 'rejected', decidedBy: userId });
    clone.log({ kind: 'proposal', author: userId, text: `${accept ? 'accepted' : 'rejected'}: ${proposal.note}` });
    return { update: Y.encodeStateAsUpdate(ydoc, Y.encodeStateVectorFromUpdate(source)), base: digest };
  } finally { clone.ydoc.destroy(); clone.privateDoc.destroy(); }
}
