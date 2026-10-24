// Files the perf harness writes before it launches the app, by name. A
// scenario lists the fixtures it needs and reads each one's `info` (plus the
// `path` it was written to) with `ctx.fixture(name)`.
//
// No runtime imports: scripts/perf-harness.mjs imports this file directly
// (Node strips the types), and only scenarios' types reach the webview.

export type FixtureEnv = {
  /** The local echo server, e.g. `http://127.0.0.1:53211`. */
  echoUrl: string;
};

const LARGE_RESPONSE_BYTES = 10_000_000;
const LARGE_BODY_BYTES = 1_000_000;

function postmanCollection(name: string, item: unknown[]) {
  return JSON.stringify({
    info: {
      name,
      schema:
        "https://schema.getpostman.com/json/collection/v2.1.0/collection.json",
    },
    item,
  });
}

export const fixtures = {
  /**
   * A "Large response" request (10 MB of JSON from the echo server) at the
   * top, then 40 folders of 50 requests. `large_postman_collection` in
   * src-tauri/src/fixtures.rs builds the same folders for the Rust heap and
   * payload budgets; keep the two in step.
   */
  largeCollection: ({ echoUrl }: FixtureEnv) => {
    const largeResponseName = "Large response";
    const item: unknown[] = [
      {
        name: largeResponseName,
        request: {
          method: "GET",
          url: `${echoUrl}/large?_json=${LARGE_RESPONSE_BYTES}`,
        },
      },
    ];
    for (let folder = 0; folder < 40; folder++) {
      const requests = [];
      for (let index = 0; index < 50; index++) {
        requests.push({
          name: `Request ${folder}.${index}`,
          request: {
            method: index % 2 ? "POST" : "GET",
            header: [
              { key: "Accept", value: "application/json" },
              { key: "X-Trace", value: `${folder}-${index}` },
            ],
            url: `${echoUrl}/folders/${folder}/items/${index}?page=1&limit=50`,
            ...(index % 2
              ? {
                  body: {
                    mode: "raw",
                    raw: JSON.stringify({
                      folder,
                      index,
                      note: "x".repeat(200),
                    }),
                    options: { raw: { language: "json" } },
                  },
                }
              : {}),
          },
        });
      }
      item.push({ name: `Folder ${folder}`, item: requests });
    }
    return {
      file: postmanCollection("Perf harness", item),
      info: { largeResponseName },
    };
  },

  /** Two POSTs to type into: a small JSON body and a 1 MB pretty one. */
  typingBodies: ({ echoUrl }: FixtureEnv) => {
    const smallBody = "Small body";
    const largeBody = "Large body";
    const rows = [];
    for (let id = 0, length = 0; length < LARGE_BODY_BYTES; id++) {
      const row = { id, name: `Item ${id}`, tags: ["alpha", "beta"] };
      rows.push(row);
      length += JSON.stringify(row, null, 2).length + 4;
    }
    const post = (name: string, raw: string) => ({
      name,
      request: {
        method: "POST",
        url: `${echoUrl}/items`,
        body: { mode: "raw", raw, options: { raw: { language: "json" } } },
      },
    });
    return {
      file: postmanCollection("Perf typing", [
        post(
          smallBody,
          JSON.stringify({ name: "Ada", role: "admin" }, null, 2),
        ),
        post(largeBody, JSON.stringify(rows, null, 2)),
      ]),
      info: { smallBody, largeBody },
    };
  },
} satisfies Record<
  string,
  (env: FixtureEnv) => { file: string; info: Record<string, unknown> }
>;

export type FixtureName = keyof typeof fixtures;

export type FixtureInfo<Name extends FixtureName> = ReturnType<
  (typeof fixtures)[Name]
>["info"] & { path: string };
