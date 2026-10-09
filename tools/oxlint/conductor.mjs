// Conductor's own lint rules, loaded by .oxlintrc.json as a JS plugin.

const MOCK_METHODS = new Set(["mock", "doMock", "unstable_mockModule"]);

// Our modules, and the IPC layer that stands in for our Rust code.
function isOurCode(specifier) {
  return (
    specifier.startsWith(".") ||
    specifier.startsWith("@/") ||
    specifier.startsWith("@tauri-apps/api")
  );
}

function mockedSpecifier(node) {
  const { callee } = node;
  if (callee.type !== "MemberExpression" || callee.computed) return null;
  if (callee.object.type !== "Identifier" || callee.object.name !== "vi") return null;
  if (!MOCK_METHODS.has(callee.property.name)) return null;
  const [argument] = node.arguments;
  if (argument?.type === "Literal" && typeof argument.value === "string") return argument.value;
  return null;
}

const noInternalMocking = {
  meta: {
    type: "problem",
    docs: {
      description:
        "Tests exercise our real code; only true external boundaries may be faked.",
    },
    messages: {
      internalMock:
        "Do not mock `{{module}}`. Extract a pure function and test that instead (see the writing-tests skill).",
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        const specifier = mockedSpecifier(node);
        if (specifier === null || !isOurCode(specifier)) return;
        context.report({ node, messageId: "internalMock", data: { module: specifier } });
      },
    };
  },
};

export default {
  meta: { name: "conductor" },
  rules: { "no-internal-mocking": noInternalMocking },
};
