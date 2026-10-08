import { describe, expect, it } from "vitest";
import {
  applyPathParamRowChanges,
  loadQuery,
  buildQueryString,
  deriveQueryRows,
  parsePathParamKeys,
  replaceQueryInUrl,
} from "./urlParams";

describe("path params", () => {
  it("only treats `:name` segments as path params, not ports or variables", () => {
    expect(
      parsePathParamKeys(
        "http://localhost:8080/{{version}}/users/:userId/posts/:postId?sort=:asc",
      ),
    ).toEqual(["userId", "postId"]);
  });

  it("renaming a param row renames only that segment in the URL", () => {
    const url = "{{base}}/orgs/:id/members/:idx?id=:id";
    const renamed = applyPathParamRowChanges(
      url,
      [
        { key: "id", value: "", enabled: true },
        { key: "idx", value: "", enabled: true },
      ],
      [
        { key: "orgId", value: "", enabled: true },
        { key: "idx", value: "", enabled: true },
      ],
    );
    expect(renamed).toBe("{{base}}/orgs/:orgId/members/:idx?id=:id");
  });
});

describe("query params", () => {
  it("keeps a disabled row when the user edits the URL", () => {
    const rows = deriveQueryRows("https://api.test/items?page=2", [
      { key: "page", value: "1", enabled: true },
      { key: "debug", value: "true", enabled: false },
    ]);
    expect(rows).toEqual([
      { key: "page", value: "2", enabled: true },
      { key: "debug", value: "true", enabled: false },
    ]);
  });

  it("keeps every row of a repeated query key, in URL order", () => {
    const rows = deriveQueryRows("https://api.test/items?id=1&id=2&page=3", []);
    expect(rows).toEqual([
      { key: "id", value: "1", enabled: true },
      { key: "id", value: "2", enabled: true },
      { key: "page", value: "3", enabled: true },
    ]);
    expect(buildQueryString(rows)).toBe("id=1&id=2&page=3");
  });

  it("matches repeated keys to their previous rows in order when the URL is edited", () => {
    const rows = deriveQueryRows("https://api.test/items?id=1&id=20", [
      { key: "id", value: "1", enabled: true },
      { key: "id", value: "2", enabled: true },
      { key: "id", value: "3", enabled: false },
    ]);
    expect(rows).toEqual([
      { key: "id", value: "1", enabled: true },
      { key: "id", value: "20", enabled: true },
      { key: "id", value: "3", enabled: false },
    ]);
  });

  it("loading a request keeps stored disabled rows and cuts them from the URL", () => {
    const loaded = loadQuery(
      "https://api.test/items?debug=true&id=1&id=2#top",
      [
        { key: "debug", value: "true", enabled: false },
        { key: "id", value: "1", enabled: true },
        { key: "id", value: "2", enabled: false },
      ],
    );
    expect(loaded).toEqual({
      url: "https://api.test/items?id=1#top",
      query: [
        { key: "debug", value: "true", enabled: false },
        { key: "id", value: "1", enabled: true },
        { key: "id", value: "2", enabled: false },
      ],
    });
  });

  it("an imported disabled row stays disabled after the URL is edited", () => {
    const loaded = loadQuery("https://api.test/items?debug=true&id=1", [
      { key: "debug", value: "true", enabled: false },
      { key: "id", value: "1", enabled: true },
    ]);
    const edited = deriveQueryRows(
      loaded.url.replace("api.test", "api.tests"),
      loaded.query,
    );
    expect(edited).toEqual([
      { key: "id", value: "1", enabled: true },
      { key: "debug", value: "true", enabled: false },
    ]);
    expect(buildQueryString(edited)).toBe("id=1");
  });

  it("loading leaves a URL with no disabled rows exactly as written", () => {
    const url = "{{base}}/items?token={{token}}&q=a%20b";
    expect(loadQuery(url, []).url).toBe(url);
  });

  it("rewriting the query string preserves the fragment and leaves disabled rows out", () => {
    const query = buildQueryString([
      { key: "q", value: "a b", enabled: true },
      { key: "debug", value: "true", enabled: false },
    ]);
    expect(
      replaceQueryInUrl("https://api.test/search?old=1#results", query),
    ).toBe("https://api.test/search?q=a+b#results");
  });
});
