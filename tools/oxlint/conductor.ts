// Conductor's own lint rules, loaded by .oxlintrc.json as a JS plugin.
import type { RuleTester } from "oxlint/plugins-dev";

// oxlint exports its rule and AST types only through RuleTester's signature.
type Rule = Parameters<RuleTester["run"]>[1];
type Visitor = ReturnType<NonNullable<Rule["create"]>>;
type NodeOf<Name extends keyof Visitor> = Parameters<
  NonNullable<Visitor[Name]>
>[0];
type CallExpression = NodeOf<"CallExpression">;
type Node = CallExpression["parent"];
type FunctionNode = Extract<
  Node,
  {
    type:
      "ArrowFunctionExpression" | "FunctionExpression" | "FunctionDeclaration";
  }
>;
type ReturnArgument = Extract<Node, { type: "ReturnStatement" }>["argument"];

const MOCK_METHODS = new Set(["mock", "doMock", "unstable_mockModule"]);

// Our modules, and the IPC layer that stands in for our Rust code.
function isOurCode(specifier: string) {
  return (
    specifier.startsWith(".") ||
    specifier.startsWith("@/") ||
    specifier.startsWith("@tauri-apps/api")
  );
}

function mockedSpecifier(node: CallExpression) {
  const { callee } = node;
  if (callee.type !== "MemberExpression" || callee.computed) return null;
  if (callee.object.type !== "Identifier" || callee.object.name !== "vi")
    return null;
  if (!MOCK_METHODS.has(callee.property.name)) return null;
  const [argument] = node.arguments;
  if (argument?.type === "Literal" && typeof argument.value === "string")
    return argument.value;
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
        context.report({
          node,
          messageId: "internalMock",
          data: { module: specifier },
        });
      },
    };
  },
} satisfies Rule;

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
        if (node.object.name !== "window" && node.object.name !== "globalThis")
          return;
        if (!NATIVE_DIALOGS.has(node.property.name)) return;
        context.report({
          node,
          messageId: "windowDialog",
          data: { name: node.property.name },
        });
      },
    };
  },
} satisfies Rule;

// Zustand compares a selector's result by identity, so a selector that returns
// the whole store re-renders its component on every change to any field. One
// that builds a new object or array is worse: in Zustand 5 each render sees a
// "changed" value, and React re-renders in a loop. See "Frontend State And
// Rendering" in docs/01-architecture/01-performance.md.
const STORE_HOOK = /^use[A-Z]\w*Store$/;
const FRESH_METHODS = new Set([
  "map",
  "filter",
  "flatMap",
  "slice",
  "concat",
  "toSorted",
  "toReversed",
  "toSpliced",
  "keys",
  "values",
  "entries",
  "fromEntries",
  "from",
]);

// Walks reach plain objects too (spans, `loc`), so this checks the shape.
function isNode(value: unknown): value is Node {
  return (
    typeof value === "object" &&
    value !== null &&
    "type" in value &&
    typeof value.type === "string"
  );
}

function isFunction(node: unknown): node is FunctionNode {
  return (
    isNode(node) &&
    (node.type === "ArrowFunctionExpression" ||
      node.type === "FunctionExpression" ||
      node.type === "FunctionDeclaration")
  );
}

// Whether evaluating `node` can produce a value no earlier call produced.
function buildsNewValue(node: Node | null | undefined): boolean {
  switch (node?.type) {
    case "ObjectExpression":
    case "ArrayExpression":
    case "NewExpression":
      return true;
    case "ConditionalExpression":
      return buildsNewValue(node.consequent) || buildsNewValue(node.alternate);
    case "LogicalExpression":
      return buildsNewValue(node.left) || buildsNewValue(node.right);
    case "CallExpression":
      return (
        node.callee.type === "MemberExpression" &&
        !node.callee.computed &&
        FRESH_METHODS.has(node.callee.property.name)
      );
    default:
      return false;
  }
}

// The `return` arguments of a block-bodied function, not of functions inside it.
function returnedValues(node: unknown, values: ReturnArgument[] = []) {
  if (!isNode(node)) return values;
  if (node.type === "ReturnStatement") values.push(node.argument);
  for (const [key, child] of Object.entries(node)) {
    if (key === "parent" || isFunction(child)) continue;
    if (Array.isArray(child))
      child.forEach((item) => returnedValues(item, values));
    else if (child && typeof child === "object") returnedValues(child, values);
  }
  return values;
}

function selectorProblem(
  selector: CallExpression["arguments"][number] | undefined,
) {
  if (!selector) return "wholeStore";
  if (!isFunction(selector)) return null;
  const [param] = selector.params;
  const results =
    selector.body?.type === "BlockStatement"
      ? returnedValues(selector.body)
      : [selector.body];
  if (
    param?.type === "Identifier" &&
    results.some(
      (result) => result?.type === "Identifier" && result.name === param.name,
    )
  ) {
    return "wholeStore";
  }
  return results.some(buildsNewValue) ? "newValue" : null;
}

const narrowStoreSelector = {
  meta: {
    type: "problem",
    docs: {
      description:
        "Store hooks select one stable value, so a change elsewhere does not re-render.",
    },
    messages: {
      wholeStore:
        "`{{hook}}` re-renders on every store change without a selector. Select only the field this component reads.",
      newValue:
        "This `{{hook}}` selector builds a new value on every call, so it re-renders on every store change. Select the fields separately, or wrap the selector in `useShallow`.",
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        if (
          node.callee.type !== "Identifier" ||
          !STORE_HOOK.test(node.callee.name)
        )
          return;
        const messageId = selectorProblem(node.arguments[0]);
        if (messageId)
          context.report({ node, messageId, data: { hook: node.callee.name } });
      },
    };
  },
} satisfies Rule;

// Each command call crosses IPC, so one per item turns a list into a train of
// round trips. See "Keep IPC Small And Few" in the performance doc.
const IPC_OBJECTS = new Set(["api", "commands"]);
const ITERATION_METHODS = new Set([
  "forEach",
  "map",
  "flatMap",
  "filter",
  "reduce",
  "some",
  "every",
  "find",
  "findIndex",
]);
const LOOPS = new Set([
  "ForStatement",
  "ForInStatement",
  "ForOfStatement",
  "WhileStatement",
  "DoWhileStatement",
]);
// The parts of a loop that run once, unlike its body, test and update.
const RUNS_ONCE = new Set(["init", "right"]);

function isIterationCallback(fn: Node) {
  const call = fn.parent;
  return (
    call?.type === "CallExpression" &&
    call.arguments.some((argument) => argument === fn) &&
    call.callee.type === "MemberExpression" &&
    !call.callee.computed &&
    ITERATION_METHODS.has(call.callee.property.name)
  );
}

function repeatsPerItem(node: Node) {
  for (
    let child = node, parent = node.parent;
    parent;
    child = parent, parent = parent.parent
  ) {
    if (LOOPS.has(parent.type)) {
      const part = Object.entries(parent).find(
        ([, value]) => value === child,
      )?.[0];
      if (!part || !RUNS_ONCE.has(part)) return true;
    }
    if (isFunction(parent)) return isIterationCallback(parent);
  }
  return false;
}

const noIpcInLoop = {
  meta: {
    type: "problem",
    docs: {
      description: "Commands are called once per action, not once per item.",
    },
    messages: {
      ipcInLoop:
        "`{{call}}` runs once per item, a round trip across IPC each time. Add a Rust command that does the whole batch.",
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        const { callee } = node;
        if (
          callee.type !== "MemberExpression" ||
          callee.object.type !== "Identifier" ||
          !IPC_OBJECTS.has(callee.object.name) ||
          !repeatsPerItem(node)
        ) {
          return;
        }
        const call = `${callee.object.name}.${"name" in callee.property ? callee.property.name : "…"}`;
        context.report({ node, messageId: "ipcInLoop", data: { call } });
      },
    };
  },
} satisfies Rule;

export default {
  meta: { name: "conductor" },
  rules: {
    "no-internal-mocking": noInternalMocking,
    "no-window-dialogs": noWindowDialogs,
    "narrow-store-selector": narrowStoreSelector,
    "no-ipc-in-loop": noIpcInLoop,
  },
};
