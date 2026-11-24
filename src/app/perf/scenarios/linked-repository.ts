import { defineScenario } from "../scenario.ts";

export default defineScenario({
  description:
    "Starts with a linked repository of 15 folders, 300 requests and 3 environments, rescans unchanged files, and saves one request.",
  fixtures: ["linkedRepository"],
  async prepare(ctx) {
    await ctx
      .workspace()
      .openRepository(ctx.fixture("linkedRepository").path, false, null);
    await ctx.workspace().refreshRepositories();
  },
  async run(ctx) {
    if (
      ctx.requestIds().length !== 300 ||
      ctx.workspace().environments.length !== 3
    ) {
      throw new Error("Incomplete linked repository fixture");
    }
    await ctx.idle("after-startup");
    // Await the same action the focus listener invokes, excluding macOS activation.
    await ctx.step("focus-rescan", () => ctx.workspace().refreshRepositories());
    await ctx.step("open-request", () =>
      ctx.workspace().selectRequest(ctx.requestId("Request 0.0")),
    );
    await ctx.step("save-request", async () => {
      ctx.workspace().updateRequest({ name: "Updated request" });
      await ctx.workspace().saveActiveRequest();
      ctx.requestId("Updated request");
    });
  },
});
