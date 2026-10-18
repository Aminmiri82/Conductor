import type { CollectionNode } from "@/features/types";

export type RequestOption = {
  id: string;
  name: string;
  method: string;
  path: string;
  // Lowercased once when the tree changes, not on every keystroke.
  searchName: string;
  // Method and folder path, which ends with the name.
  searchText: string;
};

export function flattenRequests(
  nodes: CollectionNode[],
  parentPath = "",
): RequestOption[] {
  return nodes.flatMap((node) => {
    const path = parentPath ? `${parentPath} / ${node.name}` : node.name;
    if (node.kind === "request") {
      return [
        {
          id: node.requestId,
          name: node.name,
          method: node.method,
          path,
          searchName: node.name.toLowerCase(),
          searchText: `${node.method} ${path}`.toLowerCase(),
        },
      ];
    }
    return flattenRequests(node.children, path);
  });
}

// Every word of the query must appear in the method, folder path, or name.
// Matches in the name rank first, starting at a word before inside one; ties
// keep tree order. A query that matches no other way may still match as
// letters in order ("gusr" finds "Get users"), ranked below everything else.
export function searchRequests(
  options: RequestOption[],
  query: string,
  limit: number,
): { results: RequestOption[]; total: number } {
  const terms = query.toLowerCase().split(/\s+/).filter(Boolean);
  if (!terms.length) {
    return { results: options.slice(0, limit), total: options.length };
  }
  const letters = terms.join("");

  const scored: { option: RequestOption; score: number; index: number }[] = [];
  options.forEach((option, index) => {
    let score = 0;
    for (const term of terms) {
      const termScore = matchScore(option, term);
      if (!termScore) {
        score = 0;
        break;
      }
      score += termScore;
    }
    if (!score && inOrder(letters, option.searchText)) {
      score = 0.1;
    }
    if (score) scored.push({ option, score, index });
  });

  const results = scored
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .slice(0, limit)
    .map(({ option }) => option);
  return { results, total: scored.length };
}

function matchScore(option: RequestOption, term: string): number {
  const inName = wordMatch(option.searchName, term);
  if (inName === "start") return option.searchName.startsWith(term) ? 4 : 3;
  if (inName === "inside") return 2;
  const inText = wordMatch(option.searchText, term);
  if (inText === "start") return 1;
  return inText === "inside" ? 0.5 : 0;
}

function wordMatch(text: string, term: string): "start" | "inside" | null {
  let index = text.indexOf(term);
  if (index === -1) return null;
  for (; index !== -1; index = text.indexOf(term, index + 1)) {
    if (index === 0 || !/[a-z0-9]/.test(text[index - 1])) return "start";
  }
  return "inside";
}

function inOrder(letters: string, text: string): boolean {
  let at = 0;
  for (const char of text) {
    if (char === letters[at]) at += 1;
    if (at === letters.length) return true;
  }
  return false;
}
