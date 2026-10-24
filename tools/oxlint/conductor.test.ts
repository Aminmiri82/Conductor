import { execFileSync } from "node:child_process";
import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { afterAll, describe, expect, it } from "vitest";

const oxlint = fileURLToPath(
  new URL("../../node_modules/oxlint/bin/oxlint", import.meta.url),
);
const plugin = fileURLToPath(new URL("./conductor.ts", import.meta.url));
const directory = mkdtempSync(join(tmpdir(), "conductor-lint-"));
afterAll(() => rmSync(directory, { recursive: true, force: true }));

// Lints `source` with only `rule` on, through the real oxlint, and returns the
// lines it flagged. Fixtures mark the lines they expect with `// flagged`.
function flaggedLines(rule: string, source: string) {
  const config = join(directory, `${rule}.json`);
  const file = join(directory, `${rule}.ts`);
  writeFileSync(
    config,
    JSON.stringify({
      categories: { correctness: "off" },
      jsPlugins: [plugin],
      rules: { [`conductor/${rule}`]: "error" },
    }),
  );
  writeFileSync(file, source);
  let output: string;
  try {
    output = execFileSync(
      process.execPath,
      [oxlint, "-c", config, "-f", "unix", file],
      {
        encoding: "utf8",
      },
    );
  } catch (error) {
    // oxlint exits non-zero when it reports anything.
    output = (error as { stdout: string }).stdout;
  }
  return [...output.matchAll(/:(\d+):\d+: /g)]
    .map((match) => Number(match[1]))
    .sort((a, b) => a - b);
}

function markedLines(source: string) {
  return source
    .split("\n")
    .flatMap((line, index) => (line.includes("// flagged") ? [index + 1] : []));
}

describe("narrow-store-selector", () => {
  it("flags selectors that re-render on any store change and allows narrow ones", () => {
    const source = `
const all = useWorkspaceStore(); // flagged
const same = useWorkspaceStore((state) => state); // flagged
const picked = useWorkspaceStore((state) => ({ tabs: state.tabs })); // flagged
const open = useWorkspaceStore((state) => state.tabs.filter((tab) => tab.dirty)); // flagged
const fallback = useWorkspaceStore((state) => state.error ?? []); // flagged
const block = useDraftStore((state) => { // flagged
  return [state.revision];
});

const tabs = useWorkspaceStore((state) => state.tabs);
const dirty = useWorkspaceStore(
  (state) => state.tabs.find((tab) => tab.requestId === id)?.dirty ?? false,
);
const draft = useDraftStore((state) => {
  const found = state.drafts.get(id);
  return found && select(found);
});
const shallow = useWorkspaceStore(useShallow((state) => ({ tabs: state.tabs })));
const outside = useWorkspaceStore.getState();
`;
    expect(flaggedLines("narrow-store-selector", source)).toEqual(
      markedLines(source),
    );
  });
});

describe("no-ipc-in-loop", () => {
  it("flags commands called once per item and allows calls that run once", () => {
    const source = `
for (const id of ids) {
  await api.getRequest(id); // flagged
}
while (pending()) await commands.listCollections(); // flagged
await Promise.all(ids.map((id) => api.getRequest(id))); // flagged
ids.forEach(async (id) => {
  await commands.deleteNode(id); // flagged
});

for (const collection of await api.listCollections()) use(collection);
await Promise.all([api.listCollections(), api.listEnvironments()]);
for (const id of ids) {
  buttons.push(() => api.getRequest(id));
}
load().then((rows) => api.saveRequest(rows[0]));
`;
    expect(flaggedLines("no-ipc-in-loop", source)).toEqual(markedLines(source));
  });
});
