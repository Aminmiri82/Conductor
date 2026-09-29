import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const json = (path) => JSON.parse(readFileSync(path, "utf8"));
const version = json("package.json").version;
assert.match(version, /^\d+\.\d+\.\d+$/);
assert.equal(
  json("src-tauri/tauri.conf.json").version,
  version,
  "Tauri version differs",
);
assert.equal(
  json(".release-please-manifest.json")["."],
  version,
  "Release manifest version differs",
);
const cargo = readFileSync("src-tauri/Cargo.toml", "utf8");
const packageSection = cargo.match(
  /^\[package\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m,
)?.[1];
assert.equal(
  packageSection?.match(/^version = "([^"]+)"/m)?.[1],
  version,
  "Cargo version differs",
);
const lock = readFileSync("src-tauri/Cargo.lock", "utf8");
const app = lock
  .split("[[package]]")
  .find((section) => /^name = "conductor"$/m.test(section));
assert.equal(
  app?.match(/^version = "([^"]+)"/m)?.[1],
  version,
  "Cargo.lock version differs",
);
if (process.env.RELEASE_TAG)
  assert.equal(
    process.env.RELEASE_TAG,
    `app-v${version}`,
    "Tag version differs",
  );
console.log(`All app versions agree: ${version}`);
