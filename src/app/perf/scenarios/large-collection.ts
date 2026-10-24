import { defineScenario } from "../scenario.ts";

const TABS = 20;

export default defineScenario({
  description:
    "Imports 2,000 requests, opens 20 tabs, receives a 10 MB JSON response, closes every tab, with idle windows before and after.",
  fixtures: ["largeCollection"],
  async run(ctx) {
    const { path, largeResponseName } = ctx.fixture("largeCollection");
    await ctx.idle("idle");

    await ctx.step("import", () => ctx.workspace().importCollection(path));

    const largeResponse = ctx.requestId(largeResponseName);
    const requestIds = ctx
      .requestIds()
      .filter((id) => id !== largeResponse)
      .slice(0, TABS);
    await ctx.step(`open-${TABS}-tabs`, async () => {
      // One at a time, as a user opens tabs.
      for (const requestId of requestIds) {
        await ctx.workspace().selectRequest(requestId);
      }
    });

    await ctx.workspace().selectRequest(largeResponse);
    await ctx.step("large-response", () => ctx.workspace().sendActiveRequest());

    await ctx.step("close-tabs", async () => {
      for (const { requestId } of ctx.workspace().tabs) {
        await ctx.workspace().closeRequestTab(requestId);
      }
    });

    await ctx.idle("idle-after");
  },
});
