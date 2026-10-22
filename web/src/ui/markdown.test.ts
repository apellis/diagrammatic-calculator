import { describe, expect, it } from 'vitest';
import { markdown } from './markdown';

describe('markdown', () => {
  it('typesets display and inline mathematics and keeps code fixed-width', () => {
    const html = markdown([
      'The definitions differ by a sign:',
      '\\[',
      'a=\\psi_1\\psi_2\\psi_1e(iji),\\qquad',
      'b=-\\psi_2\\psi_1\\psi_2e(iji).',
      '\\]',
      '',
      'They are **complementary orthogonal idempotents**:',
      '- \\(a^2=a\\) and \\(b^2=b\\): confirmed by the new `check idempotent(b)`.',
      '- They are orthogonal.',
    ].join('\n'));
    expect(html).toContain('<div class="md-math"><span class="katex-display">');
    expect(html).toContain('<strong>complementary orthogonal idempotents</strong>');
    expect(html).toContain('<ul><li>');
    expect(html).toContain('<code>check idempotent(b)</code>');
    expect(html.match(/class="katex"/g)?.length).toBeGreaterThanOrEqual(3);
    expect(html).not.toContain('\\(');
  });

  it('escapes markup, reads fenced code literally and leaves dollar amounts alone', () => {
    const html = markdown('<b>hi</b> costs $5 and $6\n\n```\nwatch a * b <x>\n```');
    expect(html).toContain('&lt;b&gt;hi&lt;/b&gt;');
    expect(html).toContain('<pre><code>watch a * b &lt;x&gt;</code></pre>');
    expect(html).not.toContain('katex');
  });

  it('shows unreadable mathematics as its source and an unclosed display as text while streaming', () => {
    expect(markdown('\\(\\frac{1}\\)')).toContain('<code>\\frac{1}</code>');
    expect(markdown('so\n\\[\na = b')).toContain('<p>so\n\\[\na = b</p>');
  });

  it('reads emphasis around code and mathematics', () => {
    const html = markdown('In the workspace, **A is `a` and B is `b`**: and *\\(x\\) here*.');
    expect(html).toContain('<strong>A is <code>a</code> and B is <code>b</code></strong>');
    expect(html).toMatch(/<em><span class="katex">.*<\/span> here<\/em>/);
    expect(html).not.toContain('**');
  });
});
