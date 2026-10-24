// A local server for agents to send to while verifying in the app. It answers
// every request with JSON describing what it received, so the response pane
// shows exactly what Conductor sent. Loopback only; never point the app at a
// real environment instead.
//
//   node scripts/echo/echo-server.ts [--port 18766] [--routes ./my-routes.ts]
//
// Query params starting with `_` shape the reply and are left out of the echo:
//   _status=404   reply with that status
//   _delay=3000   wait that many milliseconds first (loading and cancel states)
//   _bytes=4096   reply with that many bytes of binary download instead
//   _json=1000000 reply with a JSON array of about that many bytes instead
//
// --routes adds endpoints for one verification without editing this file.
// The module's default export is `async (request, response, body) => handled`,
// where `body` is a Buffer; return true when it wrote the response. Keep
// one-off route files in .agent-app/, which git ignores.

import {
  createServer,
  type IncomingMessage,
  type ServerResponse,
} from "node:http";
import type { AddressInfo } from "node:net";
import { resolve } from "node:path";
import { pathToFileURL } from "node:url";
import { parseArgs } from "node:util";
import { setTimeout as delay } from "node:timers/promises";

const { values: options } = parseArgs({
  options: {
    port: { type: "string", default: "18766" },
    routes: { type: "string" },
  },
});
type Routes = (
  request: IncomingMessage,
  response: ServerResponse,
  body: Buffer,
) => Promise<boolean>;

const routes: Routes | null = options.routes
  ? (await import(pathToFileURL(resolve(options.routes)).href)).default
  : null;

function echo(request: IncomingMessage, url: URL, body: Buffer) {
  const text = body.toString("utf8");
  let json: unknown = null;
  try {
    json = JSON.parse(text);
  } catch {
    // Not JSON; the text is still echoed.
  }
  return {
    method: request.method,
    path: url.pathname,
    query: [...url.searchParams].filter(([key]) => !key.startsWith("_")),
    headers: request.headers,
    body: text,
    json,
  };
}

const server = createServer(async (request, response) => {
  const chunks: Buffer[] = [];
  for await (const chunk of request) chunks.push(chunk);
  const body = Buffer.concat(chunks);
  const url = new URL(request.url!, "http://127.0.0.1");
  console.log(`${request.method} ${request.url} (${body.length} bytes)`);

  if (routes && (await routes(request, response, body))) return;

  const knobs = url.searchParams;
  if (knobs.has("_delay")) await delay(Number(knobs.get("_delay")));
  const status = Number(knobs.get("_status") ?? 200);

  if (knobs.has("_bytes")) {
    const bytes = Buffer.alloc(Number(knobs.get("_bytes")));
    for (let index = 0; index < bytes.length; index++)
      bytes[index] = index % 256;
    response.writeHead(status, {
      "content-type": "application/octet-stream",
      "content-disposition": 'attachment; filename="echo.bin"',
    });
    response.end(bytes);
    return;
  }

  if (knobs.has("_json")) {
    const size = Number(knobs.get("_json"));
    const items: string[] = [];
    for (let id = 0, length = 2; length < size; id++) {
      const item = JSON.stringify({
        id,
        name: `Item ${id}`,
        email: `user${id}@example.com`,
        active: id % 3 !== 0,
        tags: ["alpha", "beta", `tag-${id % 17}`],
        score: (id * 7919) % 1000,
      });
      items.push(item);
      length += item.length + 1;
    }
    response.writeHead(status, { "content-type": "application/json" });
    response.end(`[${items.join(",")}]`);
    return;
  }

  response.writeHead(status, { "content-type": "application/json" });
  response.end(JSON.stringify(echo(request, url, body), null, 2));
});

server.listen(Number(options.port), "127.0.0.1", () => {
  console.log(
    `Echo server on http://127.0.0.1:${(server.address() as AddressInfo).port} (pid ${process.pid})`,
  );
});
