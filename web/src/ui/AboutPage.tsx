// About the calculator: what it is, the build, the license, the privacy policy and a tip link.

import { Show } from 'solid-js';

/** Where a tip to the author goes; no tip link until it is set. */
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
      <dl class="about-facts">
        <dt>Version</dt>
        <dd><code data-testid="about-commit">{__COMMIT__}</code></dd>
        <dt>License</dt>
        <dd>Apache License 2.0</dd>
      </dl>
      <p>
        <Show when={TIP_URL}>
          <a href={TIP_URL} target="_blank" rel="noopener" data-testid="about-tip">Tip the author</a>
          {' · '}
        </Show>
        <a href="/privacy" target="_blank" rel="noopener" data-testid="about-privacy">Privacy policy</a>
      </p>
    </div>
  );
}
