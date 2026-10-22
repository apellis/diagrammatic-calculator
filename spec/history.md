# History file (`.dch`)

An optional companion of a `.dcw` file: the version history and pinned snapshots of a document, as
JSON. The web app exports it (Export history); reading it back is not built yet.

```json
{
  "format": "diagcalc-history/1",
  "name": "document name",
  "versions": [
    { "n": 1, "author": "you", "note": "add watch x_1 e(ij)", "time": "2026-10-22T07:00:00.000Z",
      "workspace": "diagcalc-workspace/1\ncontext …\n…" }
  ],
  "snapshots": [
    { "name": "first", "cellId": "w1", "cellText": "watch x_1 e(ij)", "context": "…", "version": 1,
      "key": "…", "value": { "kind": "watch", "normal_form": "…", "provenance": { … } },
      "time": "2026-10-22T07:00:00.000Z" }
  ]
}
```

Each version is a step: one change by one author with a one-line note, and the workspace as it was
after the step, as a `.dcw` file with cells as entered (not canonicalized). A snapshot freezes a
result: the cell and context it came from, the version, the hash of its inputs (`key`) and the
value as `workspace.run` returned it, provenance included.
