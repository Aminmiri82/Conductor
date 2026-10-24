import { defineScenario } from "../scenario.ts";

export default defineScenario({
  description:
    "Starts with 2,000 requests already imported, so startup includes loading and showing them, then stays idle.",
  fixtures: ["largeCollection"],
  async prepare(ctx) {
    await ctx.workspace().importCollection(ctx.fixture("largeCollection").path);
  },
  async run(ctx) {
    await ctx.idle("idle");
  },
});
