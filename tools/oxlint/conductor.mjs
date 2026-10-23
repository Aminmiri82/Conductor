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

// tauri-plugin-dialog replaces these with async versions. `window.confirm`
// then returns a promise, which is always truthy, so a confirm guard never
// stops anything.
const NATIVE_DIALOGS = new Set(["alert", "confirm", "prompt"]);

const noWindowDialogs = {
  meta: {
    type: "problem",
    docs: {
      description: "Use the dialog plugin's awaited API, not window dialogs.",
    },
    messages: {
      windowDialog:
        "`window.{{name}}` is replaced by an async version under the dialog plugin. Await `confirm`, `ask`, or `message` from @tauri-apps/plugin-dialog instead.",
    },
  },
  create(context) {
    return {
      MemberExpression(node) {
        if (node.computed || node.object.type !== "Identifier") return;
        if (node.object.name !== "window" && node.object.name !== "globalThis") return;
        if (!NATIVE_DIALOGS.has(node.property.name)) return;
        context.report({ node, messageId: "windowDialog", data: { name: node.property.name } });
      },
    };
  },
};

export default {
  meta: { name: "conductor" },
  rules: {
    "no-internal-mocking": noInternalMocking,
    "no-window-dialogs": noWindowDialogs,
  },
};
