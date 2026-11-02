// Files the perf harness writes before it launches the app, by name. A
// scenario lists the fixtures it needs and reads each one's `info` (plus the
// `path` it was written to) with `ctx.fixture(name)`.
//
// No runtime imports: scripts/perf/harness.ts imports this file directly
// (Node strips the types), and only scenarios' types reach the webview.

export type FixtureEnv = {
  /** The local echo server, e.g. `http://127.0.0.1:53211`. */
  echoUrl: string;
};

const LARGE_RESPONSE_BYTES = 10_000_000;
const LARGE_BODY_BYTES = 1_000_000;
// What Yara's work actually sees: no text response past 5 MB, most under
// 150 KB, and request bodies under 100 KB.
const SESSION_RESPONSE_BYTES = [
  5_000_000, 2_000_000, 150_000, 150_000, 150_000, 150_000,
];
const SESSION_BODY_BYTES = 150_000;

/** Pretty JSON of about `bytes`, shaped like a list a user would paste. */
function prettyRows(bytes: number) {
  const rows = [];
  for (let id = 0, length = 0; length < bytes; id++) {
    const row = { id, name: `Item ${id}`, tags: ["alpha", "beta"] };
    rows.push(row);
    length += JSON.stringify(row, null, 2).length + 4;
  }
  return JSON.stringify(rows, null, 2);
}

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

  /**
   * A working session's tabs: GETs whose JSON responses are 5 MB, 2 MB, and
   * four of 150 KB, two POSTs with 150 KB pretty bodies (the echo server
   * sends each body back), and two small GETs.
   */
  workingSession: ({ echoUrl }: FixtureEnv) => {
    const responses = SESSION_RESPONSE_BYTES.map((bytes, index) => ({
      name: `Response ${index + 1}`,
      request: {
        method: "GET",
        url: `${echoUrl}/reports/${index + 1}?_json=${bytes}`,
      },
    }));
    const bodies = [1, 2].map((index) => ({
      name: `Body ${index}`,
      request: {
        method: "POST",
        url: `${echoUrl}/items/import`,
        body: {
          mode: "raw",
          raw: prettyRows(SESSION_BODY_BYTES),
          options: { raw: { language: "json" } },
        },
      },
    }));
    const small = [1, 2].map((index) => ({
      name: `Small ${index}`,
      request: { method: "GET", url: `${echoUrl}/users/${index}` },
    }));
    return {
      file: postmanCollection("Perf session", [
        ...responses,
        ...bodies,
        ...small,
      ]),
      info: {
        responses: responses.map(({ name }) => name),
        bodies: bodies.map(({ name }) => name),
        small: small.map(({ name }) => name),
      },
    };
  },

  /** Two POSTs to type into: a small JSON body and a 1 MB pretty one. */
  typingBodies: ({ echoUrl }: FixtureEnv) => {
    const smallBody = "Small body";
    const largeBody = "Large body";
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
        post(largeBody, prettyRows(LARGE_BODY_BYTES)),
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
