// About the calculator: what it is, the build, the license, the privacy policy, and a tip link.

import { Show } from 'solid-js';

/** Where a tip to the creator goes; no tip link until it is set. */
const TIP_URL: string = 'https://buy.stripe.com/bJe28reyVa2081W86gaEE01';

/** `from`: the route the page was opened from, returned to by Back (the document list if none). */
export function AboutPage(props: { from?: string }) {
  return (
    <div class="doclist about">
      <div class="row">
        <button class="ghost" onClick={() => (location.hash = props.from || '#/')} data-testid="about-back">← Back</button>
      </div>
      <h1>diagrammatic-calculator</h1>
      <p>A fast, exact calculator for diagrammatic algebra, whose terms are read and edited as string diagrams.</p>
      <section class="getting-started">
        <h2>Getting started</h2>
        <ol>
          <li>Open an example from the document list to make your own editable copy, or choose New document and set its context.</li>
          <li>Add an Element cell to name an expression, a Watch cell to evaluate one, or a Check cell to test an equality or property.</li>
          <li>Use Type to enter an expression or Draw to edit its string diagram. Open Commands (⋯) for document actions and keyboard shortcuts.</li>
          <li>Document edits save automatically in this browser. Use Export beside a document for a <code>.dcw</code> workspace backup, then Import to open it as a new document. For versions and snapshots, use Export history (<code>.dch</code>) in the document's Commands.</li>
        </ol>
        <a href="#/">Open the document list</a>
      </section>
      <dl class="about-facts">
        <dt>Version</dt>
        <dd><code data-testid="about-commit">{__COMMIT__}</code></dd>
        <dt>License</dt>
        <dd>Apache License 2.0</dd>
      </dl>
      <p>
        <Show when={TIP_URL}>
          <a href={TIP_URL} target="_blank" rel="noopener" data-testid="about-tip">Tip the creator</a>
          {' · '}
        </Show>
        <a href="/privacy" target="_blank" rel="noopener" data-testid="about-privacy">Privacy policy</a>
      </p>
    </div>
  );
}
