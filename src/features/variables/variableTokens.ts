export type VariableToken =
  { kind: "text"; value: string } | { kind: "variable"; name: string };

// The same grammar as Rust's `resolve_text` (commands/requests/resolver.rs):
// a `{{` opens a variable at the next `}}`, and its name is what lies between,
// trimmed. A `{{` with no `}}` after it is plain text. Keep the two in step,
// or the UI will call a variable resolved that a send cannot resolve.
export function parseVariableTokens(text: string): VariableToken[] {
  const tokens: VariableToken[] = [];
  let rest = text;

  for (;;) {
    const start = rest.indexOf("{{");
    const end = start === -1 ? -1 : rest.indexOf("}}", start + 2);
    if (end === -1) break;
    if (start > 0) tokens.push({ kind: "text", value: rest.slice(0, start) });
    tokens.push({ kind: "variable", name: rest.slice(start + 2, end).trim() });
    rest = rest.slice(end + 2);
  }

  if (rest) tokens.push({ kind: "text", value: rest });
  return tokens;
}

export function variableNames(text: string): string[] {
  return parseVariableTokens(text).flatMap((token) =>
    token.kind === "variable" ? [token.name] : [],
  );
}
