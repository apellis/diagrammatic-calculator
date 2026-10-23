-- Storage quotas, shares bound to accounts, session idle timeout, mail limits and resumable
-- deletion (security review of 2026-10-23).

-- Bytes of exported files kept with a document (the snapshot's bytes are `size`); together they
-- count against the owner's storage, whoever wrote them.
ALTER TABLE documents ADD COLUMN files_size INTEGER NOT NULL DEFAULT 0;

-- The account that first opened a share sent to an address; from then on only that account has
-- it, even if another Google account later presents the same address.
ALTER TABLE shares ADD COLUMN user_id TEXT REFERENCES users (id) ON DELETE CASCADE;

-- The last request of a session, for the idle timeout.
ALTER TABLE sessions ADD COLUMN last_seen INTEGER NOT NULL DEFAULT 0;
UPDATE sessions SET last_seen = created;

-- A keyed hash of the recipient of each notification (never the address itself), for the
-- per-recipient limit; the global limit counts all rows.
ALTER TABLE sent_mail ADD COLUMN recipient TEXT;
CREATE INDEX sent_mail_recipient ON sent_mail (recipient, created);
CREATE INDEX sent_mail_created ON sent_mail (created);

-- Recipients (by the same keyed hash) who asked for no more share notifications.
CREATE TABLE mail_optout (
  recipient TEXT PRIMARY KEY,
  created INTEGER NOT NULL
);

-- Stored-object prefixes of deleted documents still to be emptied; worked off in bounded batches
-- so a deletion never fails half-way and is finished by later requests if it runs out of time.
CREATE TABLE pending_deletes (
  prefix TEXT PRIMARY KEY,
  created INTEGER NOT NULL
);
