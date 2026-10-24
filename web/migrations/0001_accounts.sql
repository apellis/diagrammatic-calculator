-- Accounts, documents, and sharing (phase 6). Applied by `wrangler d1 migrations apply`, which
-- records applied files in its own table; never edit a migration once it is on master, add the
-- next numbered file instead.

-- One row per Google account that has signed in. `google_sub` is Google's stable subject
-- identifier; the e-mail address is refreshed at every sign-in and is what shares refer to.
CREATE TABLE users (
  id TEXT PRIMARY KEY,
  google_sub TEXT NOT NULL UNIQUE,
  email TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  created INTEGER NOT NULL,
  last_login INTEGER NOT NULL
);
CREATE INDEX users_email ON users (email);

-- Signed-in browsers. The cookie carries a random session ID and its HMAC; only the SHA-256 of
-- the ID is stored, so this table alone cannot be used to sign in.
CREATE TABLE sessions (
  id_hash TEXT PRIMARY KEY,
  user_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  created INTEGER NOT NULL,
  expires INTEGER NOT NULL
);
CREATE INDEX sessions_user ON sessions (user_id);
CREATE INDEX sessions_expires ON sessions (expires);

-- Documents saved to an account. The content is a Yjs update in R2 under `snapshot_key`; a new
-- snapshot is written under a fresh key and `version` moves on only if nobody saved in between.
-- Link sharing is off while `link_role` is NULL; turning it on issues a fresh `link_token`.
CREATE TABLE documents (
  id TEXT PRIMARY KEY,
  owner_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  created INTEGER NOT NULL,
  edited INTEGER NOT NULL,
  version INTEGER NOT NULL DEFAULT 0,
  size INTEGER NOT NULL DEFAULT 0,
  snapshot_key TEXT,
  link_role TEXT CHECK (link_role IN ('viewer', 'editor')),
  link_token TEXT UNIQUE,
  CHECK ((link_role IS NULL) = (link_token IS NULL))
);
CREATE INDEX documents_owner ON documents (owner_id, edited DESC);

-- People a document is shared with, by lower-case e-mail address, so a document can be shared
-- with someone who has not signed in yet.
CREATE TABLE shares (
  doc_id TEXT NOT NULL REFERENCES documents (id) ON DELETE CASCADE,
  email TEXT NOT NULL,
  role TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
  created INTEGER NOT NULL,
  invited_by TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  PRIMARY KEY (doc_id, email)
);
CREATE INDEX shares_email ON shares (email);

-- Share notifications sent, for the daily limit per sender. No recipient is stored.
CREATE TABLE sent_mail (
  id INTEGER PRIMARY KEY,
  sender_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
  created INTEGER NOT NULL
);
CREATE INDEX sent_mail_sender ON sent_mail (sender_id, created);
