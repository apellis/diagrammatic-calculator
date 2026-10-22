// The AI's replies, shown as formatted text: Markdown for structure and code, LaTeX for
// mathematics (typeset by KaTeX, as everywhere else in the app). Only a small, safe subset is
// read: everything is escaped first, so a reply can never inject markup.
import { render } from './tex';

const escape = (s: string) => s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Mathematics typeset by KaTeX; when KaTeX cannot read it, the source in the fixed-width font. */
function math(tex: string, display: boolean): string {
  const html = render(tex.trim(), display);
  if (html) return display ? `<div class="md-math">${html}</div>` : html;
  return `<code>${escape(tex)}</code>`;
}

/** Bold, italic and code spans, with mathematics, in one line of prose. */
function inline(src: string): string {
  // code and mathematics first, so nothing inside them is read as emphasis
  const token = /`([^`]+)`|\\\[([\s\S]+?)\\\]|\$\$([\s\S]+?)\$\$|\\\(([\s\S]+?)\\\)|\$([^\s$](?:[^$]*[^\s$])?)\$(?!\d)/g;
  let out = '';
  let last = 0;
  for (let m; (m = token.exec(src)); ) {
    out += emphasis(escape(src.slice(last, m.index)));
    if (m[1] !== undefined) out += `<code>${escape(m[1])}</code>`;
    else if (m[2] !== undefined || m[3] !== undefined) out += math(m[2] ?? m[3], true);
    else out += math(m[4] ?? m[5], false);
    last = m.index + m[0].length;
  }
  return out + emphasis(escape(src.slice(last)));
}

function emphasis(s: string): string {
  return s
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/__(.+?)__/g, '<strong>$1</strong>')
    .replace(/(^|[^*\w])\*(?!\s)(.+?)(?<!\s)\*(?!\*)/g, '$1<em>$2</em>')
    .replace(/(^|[^\w])_(?!\s)(.+?)(?<!\s)_(?!\w)/g, '$1<em>$2</em>');
}

/** HTML for a reply: paragraphs, headings, lists, fenced code, display mathematics. */
export function markdown(src: string): string {
  const lines = src.replace(/\r\n?/g, '\n').split('\n');
  const out: string[] = [];
  let para: string[] = [];
  let list: { ordered: boolean; items: string[] } | null = null;
  const flushPara = () => {
    if (para.length) out.push(`<p>${inline(para.join('\n'))}</p>`);
    para = [];
  };
  const flushList = () => {
    if (list) out.push(`<${list.ordered ? 'ol' : 'ul'}>${list.items.map((x) => `<li>${inline(x)}</li>`).join('')}</${list.ordered ? 'ol' : 'ul'}>`);
    list = null;
  };
  const flush = () => {
    flushPara();
    flushList();
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const fence = /^\s*(```|~~~)\s*([\w-]*)\s*$/.exec(line);
    if (fence) {
      flush();
      const body: string[] = [];
      for (i++; i < lines.length && !lines[i].trim().startsWith(fence[1]); i++) body.push(lines[i]);
      const lang = fence[2].toLowerCase();
      if (lang === 'math' || lang === 'latex' || lang === 'tex') out.push(math(body.join('\n'), true));
      else out.push(`<pre><code>${escape(body.join('\n'))}</code></pre>`);
      continue;
    }
    // display mathematics on lines of its own: \[ … \] or $$ … $$
    const open = /^\s*(\\\[|\$\$)\s*(.*)$/.exec(line);
    if (open) {
      const close = open[1] === '$$' ? '$$' : '\\]';
      const body: string[] = [];
      let rest = open[2];
      let j = i;
      while (!rest.includes(close) && j + 1 < lines.length) {
        body.push(rest);
        rest = lines[++j];
      }
      if (rest.includes(close)) {
        flush();
        const at = rest.indexOf(close);
        body.push(rest.slice(0, at));
        out.push(math(body.join('\n'), true));
        const after = rest.slice(at + close.length).trim();
        if (after) para.push(after);
        i = j;
        continue;
      }
      // unclosed (still streaming): show it as it is
    }
    if (!line.trim()) {
      flush();
      continue;
    }
    const heading = /^\s*(#{1,6})\s+(.*)$/.exec(line);
    if (heading) {
      flush();
      const n = Math.min(6, heading[1].length + 3);
      out.push(`<h${n}>${inline(heading[2])}</h${n}>`);
      continue;
    }
    const item = /^\s*([-*+]|\d+[.)])\s+(.*)$/.exec(line);
    if (item) {
      flushPara();
      const ordered = /\d/.test(item[1]);
      if (list && list.ordered !== ordered) flushList();
      list ??= { ordered, items: [] };
      list.items.push(item[2]);
      continue;
    }
    if (list && /^\s{2,}\S/.test(line)) {
      // a continuation of the last item
      list.items[list.items.length - 1] += `\n${line.trim()}`;
      continue;
    }
    flushList();
    para.push(line);
  }
  flush();
  return out.join('');
}
