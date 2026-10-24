// Writes the updater's latest.json once every platform has uploaded to the
// draft. The platform builds run in parallel, so none of them may write it:
// two jobs rewriting the same file would drop a platform from the updater.
//
//   node scripts/release/updater-json.ts <release.json> <signature dir> > latest.json
//
// release.json is `gh api repos/{owner}/{repo}/releases/{id}`; the signature
// dir holds the draft's downloaded *.sig files.
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

// The keys and artifacts tauri-action wrote before the builds ran in parallel.
// The bare keys are what installed apps look up; the suffixed ones pin the
// installer type.
const PLATFORMS = [
  ["darwin-aarch64", ".app.tar.gz"],
  ["darwin-aarch64-app", ".app.tar.gz"],
  ["windows-x86_64", "-setup.exe"],
  ["windows-x86_64-nsis", "-setup.exe"],
  ["linux-x86_64", ".AppImage"],
  ["linux-x86_64-appimage", ".AppImage"],
  ["linux-x86_64-deb", ".deb"],
];

type Asset = { name: string; url: string };

export function buildUpdaterJson({
  version,
  notes,
  pubDate,
  assets,
  signatureFor,
}: {
  version: string;
  notes: string;
  pubDate: string;
  assets: Asset[];
  signatureFor: (assetName: string) => string;
}) {
  const platforms: Record<string, { signature: string; url: string }> = {};
  for (const [platform, suffix] of PLATFORMS) {
    const matches = assets.filter((asset) => asset.name.endsWith(suffix));
    if (matches.length !== 1) {
      throw new Error(
        `Expected one ${suffix} asset for ${platform}, found ${matches.length}`,
      );
    }
    const [asset] = matches;
    platforms[platform] = {
      signature: signatureFor(asset.name).trim(),
      url: asset.url,
    };
  }
  return { version, notes, pub_date: pubDate, platforms };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [releasePath, signatureDir] = process.argv.slice(2);
  const release: { tag_name: string; body: string | null; assets: Asset[] } =
    JSON.parse(readFileSync(releasePath, "utf8"));
  const manifest = buildUpdaterJson({
    version: release.tag_name.replace(/^app-v/, ""),
    notes: release.body ?? "",
    pubDate: new Date().toISOString(),
    assets: release.assets,
    signatureFor: (name) =>
      readFileSync(join(signatureDir, `${name}.sig`), "utf8"),
  });
  console.log(JSON.stringify(manifest, null, 2));
}
