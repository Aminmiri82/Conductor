import { describe, expect, it } from "vitest";
import { buildUpdaterJson } from "./updater-json.ts";

const asset = (name: string, id: number) => ({
  name,
  url: `https://api.github.com/repos/o/r/releases/assets/${id}`,
});

const draftAssets = [
  asset("Conductor_1.2.3_aarch64.app.tar.gz", 1),
  asset("Conductor_1.2.3_aarch64.app.tar.gz.sig", 2),
  asset("Conductor_1.2.3_aarch64.dmg", 3),
  asset("Conductor_1.2.3_x64-setup.exe", 4),
  asset("Conductor_1.2.3_x64-setup.exe.sig", 5),
  asset("Conductor_1.2.3_amd64.AppImage", 6),
  asset("Conductor_1.2.3_amd64.AppImage.sig", 7),
  asset("Conductor_1.2.3_amd64.deb", 8),
  asset("Conductor_1.2.3_amd64.deb.sig", 9),
  asset("perf-size.json", 10),
];

const build = (assets: ReturnType<typeof asset>[]) =>
  buildUpdaterJson({
    version: "1.2.3",
    notes: "notes",
    pubDate: "2026-01-01T00:00:00.000Z",
    assets,
    signatureFor: (name) => `sig of ${name}\n`,
  });

describe("updater latest.json", () => {
  it("points every platform at its own signed update artifact", () => {
    const { platforms } = build(draftAssets);

    expect(Object.keys(platforms).sort()).toEqual([
      "darwin-aarch64",
      "darwin-aarch64-app",
      "linux-x86_64",
      "linux-x86_64-appimage",
      "linux-x86_64-deb",
      "windows-x86_64",
      "windows-x86_64-nsis",
    ]);
    expect(platforms["darwin-aarch64"]).toEqual({
      signature: "sig of Conductor_1.2.3_aarch64.app.tar.gz",
      url: draftAssets[0].url,
    });
    expect(platforms["windows-x86_64"].url).toBe(draftAssets[3].url);
    expect(platforms["linux-x86_64"].url).toBe(draftAssets[5].url);
    expect(platforms["linux-x86_64-deb"].url).toBe(draftAssets[7].url);
  });

  it("refuses to write a manifest when a platform's build is missing", () => {
    const withoutWindows = draftAssets.filter(
      (asset) => !asset.name.includes("setup.exe"),
    );

    expect(() => build(withoutWindows)).toThrow(/windows-x86_64/);
  });
});
