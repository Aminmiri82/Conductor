import { describe, expect, it } from "vitest";
import { parseVariableTokens, variableNames } from "./variableTokens";

describe("variable tokens", () => {
  it("reads spaces inside the braces as the same variable Rust resolves", () => {
    expect(parseVariableTokens("{{ host }}/{{port}}")).toEqual([
      { kind: "variable", name: "host" },
      { kind: "text", value: "/" },
      { kind: "variable", name: "port" },
    ]);
  });

  it("treats a {{ with no closing }} as plain text", () => {
    expect(parseVariableTokens("{{host}}/api?q={{unfinished")).toEqual([
      { kind: "variable", name: "host" },
      { kind: "text", value: "/api?q={{unfinished" },
    ]);
  });

  it("splits adjacent variables without text between them", () => {
    expect(variableNames("{{scheme}}{{host}}{{path}}")).toEqual([
      "scheme",
      "host",
      "path",
    ]);
  });

  it("closes a variable at the first }} like Rust, even with a stray brace", () => {
    expect(parseVariableTokens("{{{a}} {{}}")).toEqual([
      { kind: "variable", name: "{a" },
      { kind: "text", value: " " },
      { kind: "variable", name: "" },
    ]);
  });

  it("has no tokens for empty text", () => {
    expect(parseVariableTokens("")).toEqual([]);
  });
});
