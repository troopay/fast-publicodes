import PublicodesEngine from "publicodes";
import { describe, expect, test } from "vitest";
import Engine from "../../src/index.js";
import { diff, evaluateAll, evaluateEach, report, rulesOf } from "./outcome.js";
import { expectedOutcomes, loadSuites } from "./suites.js";

for (const suite of await loadSuites()) {
  describe(suite.name, () => {
    test("rule types", () => {
      const oracle = suite.oracle();
      const candidate = suite.candidate();
      const types = (engine: typeof oracle | typeof candidate) =>
        Object.keys(oracle.getParsedRules()).map((name) => [
          name,
          engine.context.nodesTypes.get(engine.getRule(name) as never),
        ]);
      expect(types(candidate)).toEqual(types(oracle));
    });

    test("suggestions", () => {
      const oracle = new PublicodesEngine(suite.rules as never);
      const candidate = new Engine(suite.rules);
      const names = Object.keys(oracle.getParsedRules());
      const suggestions = (engine: typeof oracle | typeof candidate) =>
        names.flatMap((name) =>
          Object.entries(engine.getRule(name).suggestions).map(
            ([suggestion, node]): [string, unknown] => [
              `${name} : ${suggestion}`,
              node,
            ],
          ),
        );
      for (const situation of [{}, ...Object.values(suite.situations)]) {
        const expected = evaluateEach(oracle, situation, suggestions(oracle));
        const mismatches = diff(
          expected,
          evaluateEach(candidate, situation, suggestions(candidate)),
        );
        if (mismatches.length > 0) {
          throw new Error(report(mismatches, rulesOf(expected).length));
        }
      }
    });

    for (const [name, situation] of Object.entries(suite.situations)) {
      test(name, async () => {
        const expected = await expectedOutcomes(suite, situation);
        const names = rulesOf(expected);
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
