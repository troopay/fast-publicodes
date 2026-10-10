import { describe, test } from "vitest";
import { diff, evaluateAll, report } from "./outcome.js";
import { expectedOutcomes, loadSuites } from "./suites.js";

for (const suite of await loadSuites()) {
  describe(suite.name, () => {
    for (const [name, situation] of Object.entries(suite.situations)) {
      test(name, { timeout: 60_000 }, async () => {
        const expected = await expectedOutcomes(suite, situation);
        const names = Object.keys(expected);
        const mismatches = diff(
          expected,
          evaluateAll(suite.candidate(), situation, names),
        );
        if (mismatches.length > 0) {
          throw new Error(report(mismatches, names.length));
        }
      });
    }
  });
}
