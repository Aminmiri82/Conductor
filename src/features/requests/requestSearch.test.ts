import { describe, expect, it } from "vitest";
import type { CollectionNode } from "@/features/types";
import { flattenRequests, searchRequests } from "./requestSearch";

function request(name: string, method = "GET"): CollectionNode {
  return {
    id: `node-${name}`,
    parentId: null,
    position: 0,
    name,
    kind: "request",
    requestId: name,
    method,
  };
}

function folder(name: string, children: CollectionNode[]): CollectionNode {
  return {
    id: `folder-${name}`,
    parentId: null,
    position: 0,
    name,
    kind: "folder",
    children,
  };
}

const options = flattenRequests([
  folder("Users", [request("List members"), request("Delete user", "DELETE")]),
  request("Get users"),
  request("Create order", "POST"),
]);

function names(query: string, limit = 50) {
  return searchRequests(options, query, limit).results.map(
    (option) => option.name,
  );
}

describe("open request search", () => {
  it("ranks a match in the name above one only in the folder path", () => {
    expect(names("user")).toEqual(["Delete user", "Get users", "List members"]);
  });

  it("needs every word of the query, in any field", () => {
    expect(names("delete user")).toEqual(["Delete user"]);
    expect(names("post order")).toEqual(["Create order"]);
    expect(names("post user")).toEqual([]);
  });

  it("still finds letters typed in order, below every word match", () => {
    expect(names("crord")).toEqual(["Create order"]);
  });

  it("shows only the top results but counts every match", () => {
    expect(searchRequests(options, "", 2)).toMatchObject({
      results: [{ name: "List members" }, { name: "Delete user" }],
      total: 4,
    });
  });
});
