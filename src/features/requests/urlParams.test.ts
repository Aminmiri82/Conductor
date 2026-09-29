import { describe, expect, it } from "vitest";
import {
  applyPathParamRowChanges,
  buildQueryString,
  deriveQueryRows,
  parsePathParamKeys,
  replaceQueryInUrl,
} from "./urlParams";

describe("path params", () => {
  it("only treats `:name` segments as path params, not ports or variables", () => {
    expect(
      parsePathParamKeys("http://localhost:8080/{{version}}/users/:userId/posts/:postId?sort=:asc"),
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

  it("rewriting the query string preserves the fragment and leaves disabled rows out", () => {
    const query = buildQueryString([
      { key: "q", value: "a b", enabled: true },
      { key: "debug", value: "true", enabled: false },
    ]);
    expect(replaceQueryInUrl("https://api.test/search?old=1#results", query)).toBe(
      "https://api.test/search?q=a+b#results",
    );
  });
});
