// Help: practical first steps with the calculator.

/** `from`: the route the page was opened from, returned to by Back (the document list if none). */
export function HelpPage(props: { from?: string }) {
  return (
    <div class="doclist about">
      <div class="row">
        <button class="ghost" onClick={() => (location.hash = props.from || '#/')} data-testid="help-back">← Back</button>
      </div>
      <section class="getting-started">
        <h1>Getting started</h1>
        <ol>
          <li>Open an example from the document list to make your own editable copy, or choose New document and set its context.</li>
          <li>Add an Element cell to name an expression, a Watch cell to evaluate one, or a Check cell to test an equality or property.</li>
          <li>Use Type to enter an expression or Draw to edit its string diagram. Open Commands (⋯) for document actions and keyboard shortcuts.</li>
          <li>Document edits save automatically in this browser. Use Export beside a document for a <code>.dcw</code> workspace backup, then Import to open it as a new document. For versions and snapshots, use Export history (<code>.dch</code>) in the document's Commands.</li>
        </ol>
      </section>
    </div>
  );
}
