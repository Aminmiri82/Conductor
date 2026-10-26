import { defineScenario, type ScenarioContext } from "../scenario.ts";

// Short: the edits are what matter here, not keystroke statistics.
const EDIT = ',"note":"edited"';

type Ctx = ScenarioContext<"workingSession">;

async function sendEach(ctx: Ctx, names: string[]) {
  for (const name of names) {
    await ctx.workspace().selectRequest(ctx.requestId(name));
    await ctx.workspace().sendActiveRequest();
  }
}

async function editBody(ctx: Ctx, name: string) {
  const requestId = ctx.requestId(name);
  await ctx.workspace().selectRequest(requestId);
  ctx.workspaceUi().setRequestEditorTab(requestId, "body");
  return ctx.type(await ctx.find("[data-body-editor]"), EDIT);
}

export default defineScenario({
  description:
    "A working session: 10 tabs with responses up to 5 MB, edits to a 150 KB body and sends, then leaves the app in the background with the largest response and then the body on screen.",
  fixtures: ["workingSession"],
  async run(ctx) {
    const { path, responses, bodies, small } = ctx.fixture("workingSession");
    await ctx.step("import", () => ctx.workspace().importCollection(path));

    await ctx.step("open-tabs", async () => {
      for (const name of [...responses, ...bodies, ...small]) {
        await ctx.workspace().selectRequest(ctx.requestId(name));
      }
    });
    await ctx.step("send-responses", () => sendEach(ctx, responses));
    await ctx.step("send-bodies", () => sendEach(ctx, bodies));

    await ctx.step("edit-body", () => editBody(ctx, bodies[0]));
    await ctx.step("send-edited", () => ctx.workspace().sendActiveRequest());
    await ctx.idle("idle-open");

    // Away with the largest response on screen.
    await ctx.step("view-largest-response", () =>
      ctx.workspace().selectRequest(ctx.requestId(responses[0])),
    );
    await ctx.background();
    await ctx.idle("idle-background", { long: true });
    await ctx.step("return", () => ctx.foreground());

    // Away again with the body editor on screen.
    await ctx.step("edit-body-again", () => editBody(ctx, bodies[1]));
    await ctx.step("send-again", () => ctx.workspace().sendActiveRequest());
    await ctx.background();
    await ctx.idle("idle-background-2", { long: true });
    await ctx.step("return-2", () => ctx.foreground());
  },
});
