/** The engine's list is authoritative; never enumerate permutations or guess objects. */
export function identityTerm(object: string | undefined, objects: readonly string[]): string | undefined {
  return object !== undefined && objects.includes(object) ? `e(${object})` : undefined;
}

/** Tokens come from the engine layout, not a character split of an object name. */
export function swapBoundary(tokens: readonly string[], left: number, objects?: readonly string[]): string | undefined {
  if (!Number.isInteger(left) || left < 0 || left + 1 >= tokens.length || tokens[left] === tokens[left + 1]) return undefined;
  const next = [...tokens];
  [next[left], next[left + 1]] = [next[left + 1], next[left]];
  const object = next.join(objects ? '' : ' ');
  return objects === undefined || objects.includes(object) ? object : undefined;
}

/** Explicit boundaries use declared colours and orientations; the engine still validates them. */
export function orientedIdentity(object: string | undefined, colours: readonly { name: string; oriented: boolean }[]): string | undefined {
  if (object === undefined) return undefined;
  const tokens = object.split(/\s+/).filter(Boolean);
  const allowed = new Set(colours.flatMap((c) => c.oriented ? [c.name, `${c.name}*`] : [c.name]));
  return tokens.every((token) => allowed.has(token)) ? `e(${tokens.join(' ')})` : undefined;
}
