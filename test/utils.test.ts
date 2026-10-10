import PublicodesEngine, * as publicodes from "publicodes";
import { describe, expect, test } from "vitest";
import * as fast from "../src/index.js";

type Module = typeof fast;

/** Runs `use` on publicodes' exports and on ours, recording results and errors. */
function same(use: (module: Module) => unknown) {
  const run = (module: Module) => {
    try {
      return { result: use(module) };
    } catch (error) {
      const { name, message } = error as Error;
      return { error: { name, message } };
    }
  };
  expect(run(fast)).toEqual(run(publicodes as unknown as Module));
}

const rules = {
  a: { "par défaut": 1 },
  "a . b": { privé: "oui", valeur: 2 },
  "a . b . c": 3,
  "a . d": { experimental: "oui", valeur: 4 },
  e: 5,
};

const util = (module: Module, name: string) => {
  const utils = module.utils as unknown as Record<string, unknown>;
  return utils[name] as (...args: unknown[]) => unknown;
};

const parsed = (module: Module) =>
  (module === fast
    ? new fast.Engine(rules)
    : new PublicodesEngine(rules as never)
  ).getParsedRules() as never;

describe("utils", () => {
  test.each([
    ["nameLeaf", ["a . b . c"]],
    ["ruleParent", ["a . b . c"]],
    ["ruleParents", ["a . b . c"]],
    ["encodeRuleName", ["a . b c/d-e"]],
    ["decodeRuleName", ["a/b-c‑d"]],
    ["contextNameToDottedName", ["a . b . $SITUATION"]],
    ["findCommonAncestor", ["a . b . c", "a . d"]],
    ["findCommonAncestor", ["a . b", "e"]],
  ])("%s(%j)", (name, args) => {
    same((module) => util(module, name)(...args));
  });

  test.each([
    ["getChildrenRules", ["a"]],
    ["isAccessible", ["e", "a . b . c"]],
    ["isAccessible", ["a . d", "a . b . c"]],
    ["isAccessible", ["e", "inconnue"]],
    ["isExperimental", ["a . d"]],
    ["isExperimental", ["a"]],
    ["disambiguateReference", ["a . d", "b"]],
    ["disambiguateReference", ["e", "b . c"]],
    ["disambiguateReference", ["a . b . c", "^ . d"]],
    ["disambiguateReference", ["e", "inconnue"]],
  ])("%s(rules, %j)", (name, args) => {
    same((module) => util(module, name)(parsed(module), ...args));
  });

  test("ruleWithDedicatedDocumentationPage", () => {
    same((module) =>
      [{}, { virtualRule: true }, { type: "groupe" }].map((rule) =>
        module.utils.ruleWithDedicatedDocumentationPage(rule as never),
      ),
    );
  });
});

describe("units", () => {
  test.each(["€/mois", "kg.m/s2", "%", "€/an/personne", "", "jour ouvré"])(
    "%j",
    (unit) => {
      same((module) => {
        const parsedUnit = module.parseUnit(unit);
        return [parsedUnit, module.serializeUnit(parsedUnit)];
      });
    },
  );
});

describe("formatValue", () => {
  test.each<[unknown, object?]>([
    [1234.5678],
    [0.5, { displayedUnit: "%" }],
    [{ nodeValue: 2500, unit: { numerators: ["€"], denominators: ["mois"] } }],
    [{ nodeValue: 0.123, unit: { numerators: ["%"], denominators: [] } }],
    [{ nodeValue: 3, unit: { numerators: ["jour"], denominators: [] } }],
    [{ nodeValue: 12.3456, unit: { numerators: ["€"], denominators: [] } }],
    ["texte"],
    [true],
    [null],
    [{ nodeValue: 1.5, unit: undefined }, { precision: 0 }],
    [1234.5, { language: "en" }],
  ])("%j %j", (value, options) => {
    same((module) => module.formatValue(value as never, options as never));
  });
});

describe("parseExpression", () => {
  test.each([
    "a + b * 2",
    "salaire . brut >= 2000 €/mois",
    "'texte' = a",
    "-(3 - 2) / 4 %",
    "1 +",
    "a . (b)",
  ])("%j", (expression) => {
    same((module) => module.parseExpression(expression, "règle"));
  });

  test("capitalise0", () => {
    same((module) => ["élan", "", "Déjà"].map((s) => module.capitalise0(s)));
  });
});
