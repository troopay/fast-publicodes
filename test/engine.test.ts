import rules from "modele-social";
import PublicodesEngine from "publicodes";
import { describe, expect, test } from "vitest";
import Engine, {
  type EngineOptions,
  type RawPublicodes,
} from "../src/index.js";

type AnyEngine = Engine | PublicodesEngine;

/** Runs `use` on publicodes and on fast-publicodes, recording results, errors and warnings. */
function compare(
  ruleSet: RawPublicodes,
  options: EngineOptions,
  use: (engine: AnyEngine) => unknown,
) {
  const run = (create: (logger: EngineOptions["logger"]) => AnyEngine) => {
    const warnings: string[] = [];
    const logger = {
      log() {},
      error() {},
      warn: (m: string) => warnings.push(m),
    };
    try {
      const engine = create(logger);
      return { result: use(engine), warnings };
    } catch (error) {
      const { name, message } = error as Error;
      return { error: { name, message }, warnings };
    }
  };
  const expected = run(
    (logger) =>
      new PublicodesEngine(ruleSet as never, { ...options, logger } as never),
  );
  const received = run((logger) => new Engine(ruleSet, { ...options, logger }));
  expect(received).toEqual(expected);
}

const value = (engine: AnyEngine, expression: string) => {
  const { nodeValue, unit, missingVariables } = engine.evaluate(expression);
  return { nodeValue, unit, missingVariables };
};

const contrat = {
  salaire: {
    question: "Salaire ?",
    unité: "€/mois",
    "par défaut": "2000 €/mois",
  },
  contrat: { "une possibilité": ["CDI", "CDD"], "par défaut": "'CDI'" },
  "contrat . CDI": null,
  "contrat . CDD": null,
  durée: { "applicable si": "contrat = 'CDD'", valeur: "6 mois" },
  net: "salaire * 80%",
};

describe("errors", () => {
  test.each([
    ["unknown situation rule", { inconnue: 1 }],
    ["private situation rule", { "salaire . $SITUATION": 1 }],
    ["invalid situation expression", { salaire: "3 +" }],
  ])("%s", (_, situation) => {
    compare(contrat, {}, (engine) =>
      engine.setSituation(situation as never).getSituation(),
    );
  });

  test.each([
    ["syntax error", { a: "1 +" }],
    ["unknown reference", { a: "b" }],
    ["orphan rule", { "a . b": 1 }],
    ["space after / in a unit", { a: { valeur: 1, unité: "€/ mois" } }],
  ])("%s at construction", (_, ruleSet) => {
    compare(ruleSet, {}, () => null);
  });

  test("unknown and private rules", () => {
    compare(contrat, {}, (engine) => engine.getRule("inconnue"));
    compare(contrat, {}, (engine) =>
      engine.getRule("salaire . $SITUATION" as never),
    );
  });
});

describe("situation", () => {
  test("non-strict situations warn and drop invalid entries", () => {
    compare(contrat, { strict: false }, (engine) => [
      engine
        .setSituation({ inconnue: 1, salaire: "3 +", net: 5 } as never)
        .getSituation(),
      value(engine, "net"),
    ]);
  });

  test("keeps the previous situation on request", () => {
    compare(contrat, {}, (engine) => {
      engine.setSituation({ salaire: 3000 });
      engine.setSituation(
        { contrat: "'CDD'" },
        { keepPreviousSituation: true },
      );
      return [
        engine.getSituation(),
        value(engine, "net"),
        value(engine, "durée"),
      ];
    });
  });

  test("values outside the possibilities", () => {
    compare(contrat, {}, (engine) => [
      engine.setSituation({ contrat: "'stage'" }).getSituation(),
      value(engine, "contrat"),
    ]);
    compare(contrat, { strict: { checkPossibleValues: true } }, (engine) =>
      engine.setSituation({ contrat: "'stage'" }).getSituation(),
    );
  });
});

describe("evaluate", () => {
  test("numbers and objects", () => {
    compare(contrat, {}, (engine) => [
      value(engine, 3 as never),
      value(engine, { valeur: "salaire * 2", unité: "€/an" } as never),
    ]);
  });
});

describe("options", () => {
  const nested = {
    a: { question: "A ?", "par défaut": "non" },
    "a . b": "10",
    "c . d": "1",
    e: "e + 1",
  };

  test.each<[string, EngineOptions]>([
    [
      "no namespace disabling",
      { flag: { automaticNamespaceDisabling: false } },
    ],
    ["orphan rules allowed", { strict: false, allowOrphanRules: true }],
    ["strict", { strict: true }],
  ])("%s", (_, options) => {
    compare(nested, options, (engine) => [
      value(engine, "a . b"),
      value(engine, "c . d"),
    ]);
  });

  test("cycles warn, or fail with noCycleRuntime", () => {
    compare(nested, { allowOrphanRules: true, strict: false }, (engine) =>
      value(engine, "e"),
    );
    compare({ e: "e + 1" }, { strict: { noCycleRuntime: true } }, (engine) =>
      value(engine, "e"),
    );
  });

  describe("getUnitKey", () => {
    const keys: Record<string, string> = { month: "mois", year: "an" };
    const translated = {
      salaire: { valeur: "3000 €/month" },
      annuel: { valeur: "salaire", unité: "€/year" },
      prime: { question: "Prime ?", "par défaut": "100 €/month" },
    };
    const failing = (unit: string) => {
      if (unit === "x") throw new Error("Unité refusée");
      return unit;
    };

    test("normalizes the units of rules, situations and expressions", () => {
      compare(
        translated,
        { getUnitKey: (unit) => keys[unit] ?? unit },
        (engine) => [
          value(engine, "annuel"),
          engine.setSituation({ prime: "1200 €/year" }).getSituation(),
          value(engine, "prime + salaire"),
          value(engine, "annuel / 12 month/year"),
        ],
      );
    });

    test("errors it throws", () => {
      compare({ a: "1 x" }, { getUnitKey: failing }, () => null);
      compare(contrat, { getUnitKey: failing }, (engine) =>
        value(engine, "salaire + 2 x"),
      );
      for (const strict of [true, false]) {
        compare(contrat, { getUnitKey: failing }, (engine) =>
          engine.setSituation({ salaire: "2 x" }, { strict }).getSituation(),
        );
      }
    });
  });

  test("filters possibilities that are not applicable", () => {
    const ruleSet = {
      ...contrat,
      contrat: { "une possibilité": ["CDI", "CDD"] },
      "contrat . CDD": { "applicable si": "salaire > 1000 €/mois" },
      "contrat . CDI": { "applicable si": "salaire > 5000 €/mois" },
    };
    compare(
      ruleSet,
      { flag: { filterNotApplicablePossibilities: true } },
      (engine) => [
        value(engine, "contrat"),
        engine
          .getPossibilitiesFor("contrat", { filterNotApplicable: true })
          ?.map((p) => p.publicodesValue),
      ],
    );
  });
});

describe("warnings", () => {
  test("unit conversions", () => {
    compare({ a: "1 €", b: "2 kg", c: "a + b" }, {}, (engine) =>
      value(engine, "c"),
    );
  });

  test("experimental rules", () => {
    compare(
      { a: { experimental: "oui", valeur: 1 }, b: "a * 2" },
      {},
      (engine) => value(engine, "a + b"),
    );
  });

  test("experimental rules behind replacements and inline mechanisms", () => {
    const ruleSet = {
      a: { experimental: "oui", valeur: 1 },
      "a . b": { question: "B ?", "par défaut": 2 },
      c: 3,
      "a . d": { remplace: "c", valeur: "a . b" },
    };
    compare(ruleSet, {}, (engine) => [
      value(engine, "c"),
      value(engine, { valeur: "a . b", plafond: "c" } as never),
      value(engine, { moyenne: ["a", { somme: ["c", "a . b"] }] } as never),
      engine.setSituation({ "a . b": "c + 1" }).getSituation(),
    ]);
  });

  test("experimental check of an orphan situation rule", () => {
    compare(
      { "a . b": { question: "B ?", "par défaut": 1 } },
      { strict: false, allowOrphanRules: true },
      (engine) => {
        try {
          engine.setSituation({ "a . b": 2 });
        } catch (error) {
          return [(error as Error).message, engine.getSituation()];
        }
      },
    );
  });
});

describe("rules", () => {
  const parsed = (engine: AnyEngine) =>
    Object.entries(engine.getParsedRules()).map(([name, rule]) => [
      name,
      rule.title,
      rule.private,
      rule.rawNode,
    ]);

  test("modele-social rules", () => {
    compare(rules as RawPublicodes, {}, parsed);
  });

  test("modele-social warnings", () => {
    compare(rules as RawPublicodes, {}, (engine) => {
      engine.setSituation({
        "salarié . contrat": "'CDI'",
        "salarié . contrat . salaire brut": 3000,
      });
      for (const name of Object.keys(engine.getParsedRules())) {
        try {
          engine.evaluate(name);
        } catch {}
      }
    });
  });

  test("modele-social possibilities", () => {
    compare(rules as RawPublicodes, {}, (engine) =>
      Object.keys(engine.getParsedRules()).map((name) =>
        engine
          .getPossibilitiesFor(name)
          ?.map(({ type, nodeValue, publicodesValue, dottedName, title }) => ({
            type,
            nodeValue,
            publicodesValue,
            dottedName,
            title,
          })),
      ),
    );
  });
});

describe("copies and caches", () => {
  test("shallowCopy and resetCache", () => {
    compare(contrat, {}, (engine) => {
      engine.setSituation({ salaire: 3000 });
      const copy = engine.shallowCopy();
      copy.setSituation({ salaire: 1000 });
      engine.resetCache();
      return [value(engine, "net"), value(copy, "net"), engine.inversionFail()];
    });
  });
});
