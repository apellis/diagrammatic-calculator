# History file (`.dch`)

An optional companion of a `.dcw` file: the version history, pinned snapshots, and AI activity
timeline of a document, as JSON. The web app exports it (Export history) and imports it as a new document, rebuilding each
version with its author, note, and time, the snapshots, and the timeline; entries it cannot read are
left out, and the import says how many.

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
  ],
  "timeline": [
    { "time": "2026-10-22T07:01:00.000Z", "author": "AI", "kind": "tool",
      "text": "cell_add: watch x_1 psi_1 e(ii)", "detail": "{…}", "version": 2, "branch": "slide" }
  ]
}
```

Each version is a step: one change by one author with a one-line note, and the workspace as it was
after the step, as a `.dcw` file with cells as entered (not canonicalized). A snapshot freezes a
result: the cell and context it came from, the version, the hash of its inputs (`key`), and the
value as `workspace.run` returned it, provenance included.

The timeline records the AI's activity: each request (`kind` `request`), tool call (`tool`), note
(`note`), milestone summary (`milestone`), proposal for the person (`proposal`), and failure
(`error`). `detail` holds more (a tool's input, a milestone's text), `version` the step a workspace
change made, and `branch` the what-if branch it happened on, when not the document itself. A
milestone's `snapshots` lists the `key`s of the snapshots that froze its key diagrams. Older
files without `timeline` read as an empty timeline.
