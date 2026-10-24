import { defineScenario, type ScenarioContext } from "../scenario.ts";

// Long enough for a p95 that is not just the slowest keystroke.
const TEXT = '&sort=name&page=2&filter="active users"&q=x';

// The URL shows as variable tokens until it is clicked, unless it is flat.
async function urlField(ctx: ScenarioContext<never>) {
  const field = await ctx.find("[data-url-input]");
  if (field instanceof HTMLInputElement) return field;
  field.click();
  return ctx.find<HTMLInputElement>("input[data-url-input]");
}

async function openBody(ctx: ScenarioContext<never>, requestId: string) {
  await ctx.workspace().selectRequest(requestId);
  ctx.workspaceUi().setRequestEditorTab(requestId, "body");
}

export default defineScenario({
  description:
    "Types into the URL, a small body, and a 1 MB body, timing each keystroke until it paints.",
  fixtures: ["typingBodies"],
  async run(ctx) {
    const { path, smallBody, largeBody } = ctx.fixture("typingBodies");
    await ctx.step("import", () => ctx.workspace().importCollection(path));

    const small = ctx.requestId(smallBody);
    await ctx.step("open-request", () => ctx.workspace().selectRequest(small));
    await ctx.step("type-url", async () => ctx.type(await urlField(ctx), TEXT));

    await ctx.step("open-body", () => openBody(ctx, small));
    await ctx.step("type-body", async () =>
      ctx.type(await ctx.find("textarea[data-body-editor]"), TEXT),
    );

    await ctx.step("open-large-body", () =>
      openBody(ctx, ctx.requestId(largeBody)),
    );
    await ctx.step("type-large-body", async () =>
      ctx.type(await ctx.find("textarea[data-body-editor]"), TEXT),
    );
  },
});
