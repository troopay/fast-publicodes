import PublicodesEngine from "publicodes";
import { describe, expect, test } from "vitest";
import Engine from "../../src/index.js";
import {
  diff,
  evaluateAll,
  evaluateEach,
  type Outcomes,
  report,
  rulesOf,
} from "./outcome.js";
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

    // Each suggestion evaluated alone, inside an expression with the rule's unit, and given
    // as the rule's answer.
    test("suggestions", () => {
      const oracle = new PublicodesEngine(suite.rules as never);
      const candidate = new Engine(suite.rules);
      type AnyEngine = typeof oracle | typeof candidate;
      const suggestions = Object.keys(oracle.getParsedRules()).flatMap((rule) =>
        Object.keys(oracle.getRule(rule).suggestions).map((name) => ({
          rule,
          name,
          unit: oracle.getRule(rule).rawNode.unité,
        })),
      );
      const node = (engine: AnyEngine, rule: string, name: string) =>
        engine.getRule(rule).suggestions[name] as never;
      const expressions = (engine: AnyEngine) =>
        suggestions.flatMap(({ rule, name, unit }): [string, unknown][] => [
          [`${rule} : ${name}`, node(engine, rule, name)],
          [
            `${rule} : ${name} (${unit ?? "no unit"})`,
            {
              valeur: node(engine, rule, name),
              ...(unit ? { unité: unit } : {}),
            },
          ],
        ]);
      const check = (expected: Outcomes, received: Outcomes) => {
        const mismatches = diff(expected, received);
        if (mismatches.length > 0) {
          throw new Error(report(mismatches, rulesOf(expected).length));
        }
      };
      for (const situation of [{}, ...Object.values(suite.situations)]) {
        check(
          evaluateEach(oracle, situation, expressions(oracle)),
          evaluateEach(candidate, situation, expressions(candidate)),
        );
      }
      for (const { rule, name } of suggestions) {
        const answer = (engine: AnyEngine) =>
          evaluateEach(engine, { [rule]: node(engine, rule, name) }, [
            [rule, rule],
          ]);
        check(answer(oracle), answer(candidate));
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
