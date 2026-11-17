// Reading a context line into its presentation and options.

/** A context line's presentation and options, in order. */
export function parseLine(line: string): { name: string; opts: [string, string][] } {
  // a quoted value, such as red="i@0, j@1", is one option
  const [name = '', ...rest] = line.trim().match(/(?:[^\s"]+|"[^"]*"?)+/g) ?? [];
  return { name, opts: rest.map((w) => { const i = w.indexOf('='); return (i < 0 ? [w, ''] : [w.slice(0, i), w.slice(i + 1)]) as [string, string]; }) };
}
