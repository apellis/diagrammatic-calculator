-- Generation zero fences legacy clients until owner migration through the room.
ALTER TABLE documents ADD COLUMN generation INTEGER NOT NULL DEFAULT 0;
CREATE TABLE doc_versions (
  id TEXT NOT NULL,
  doc_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  time INTEGER NOT NULL,
  author TEXT NOT NULL,
  note TEXT NOT NULL DEFAULT '',
  size INTEGER NOT NULL,
  object_key TEXT NOT NULL,
  PRIMARY KEY (doc_id, id)
);
CREATE INDEX doc_versions_time ON doc_versions(doc_id, time DESC);
CREATE TABLE doc_branches (
  doc_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  id TEXT NOT NULL,
  name TEXT NOT NULL,
  updated INTEGER NOT NULL,
  size INTEGER NOT NULL,
  object_key TEXT NOT NULL,
  PRIMARY KEY (doc_id, user_id, id)
);
CREATE INDEX doc_branches_user ON doc_branches(user_id);
CREATE TABLE live_sockets (
  doc_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
  socket_id TEXT NOT NULL,
  user_id TEXT REFERENCES users(id) ON DELETE CASCADE,
  since INTEGER NOT NULL,
  PRIMARY KEY (doc_id, socket_id)
);
CREATE INDEX live_sockets_user ON live_sockets(user_id);
