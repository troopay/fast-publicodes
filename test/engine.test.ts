import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import veloRules from "@betagouv/aides-velo/rules";
import socialRules from "modele-social";
import PublicodesEngine from "publicodes";
import { describe, expect, test, vi } from "vitest";
import Engine, {
  type EngineOptions,
  type RawPublicodes,
  type Situation,
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

  test.each<[string, object]>([
    ["null value", { a: { valeur: null } }],
    ["boolean value", { a: true }],
    ["list without mechanism", { a: { valeur: [1, 2] } }],
    ["two mechanisms", { a: { somme: [1], produit: [2] } }],
    ["unknown mechanism", { a: { inconnu: 1 } }],
    ["somme of a string", { a: { somme: "b" }, b: 1 }],
    ["operation with one operand", { a: { "+": [1] } }],
    ["operation of a number", { a: { "+": 1 } }],
    ["operation of null", { a: { "+": null } }],
    [
      "barème tranches not a list",
      { a: { barème: { assiette: 1, tranches: 1 } } },
    ],
    ["variations not a list", { a: { variations: 1 } }],
    ["unité not a string", { a: { valeur: 1, unité: 5 } }],
    ["texte not a string", { a: { texte: 5 } }],
    ["possibilities not a list", { a: { "une possibilité": "x" } }],
    [
      "possibilities in two units",
      { a: { "une possibilité": ["1 €", "2 kg"] } },
    ],
    ["possibility not a constant", { a: { "une possibilité": ["1 + 2"] } }],
    [
      "deprecated possibilités",
      {
        a: {
          "une possibilité": {
            "choix obligatoire": "non",
            possibilités: ["'x'", 1],
          },
        },
      },
    ],
    [
      "private rule",
      { a: "x . p", x: 1, "x . p": { privé: "oui", valeur: 2 } },
    ],
    ["titre number", { a: { titre: 5, valeur: 1 } }],
    ["titre boolean", { a: { titre: true, valeur: 1 } }],
    [
      "durée in an unknown unit",
      { a: { durée: { depuis: "01/01/2020", unité: "kg" } } },
    ],
    [
      "durée from a number",
      { a: { durée: { depuis: 5, "jusqu'à": "01/01/2020" } } },
    ],
  ])("malformed rules: %s", (_, ruleSet) => {
    compare(ruleSet as RawPublicodes, {}, (engine) => value(engine, "a"));
  });

  test.each<[string, object, string]>([
    ["operation with three operands", { a: { "+": [1, 2, 3] } }, "a"],
    ["titre list", { a: { titre: ["ab"], valeur: 1 } }, "a"],
    [
      "texte with a carriage return",
      { a: 1, b: { texte: "x {{ a\r }} y" } },
      "b",
    ],
    [
      "arrondi to -0",
      { a: { valeur: -0.2, arrondi: "oui" }, b: { texte: "{{ a }}" } },
      "b",
    ],
    [
      "date with a hexadecimal day",
      { a: { durée: { depuis: "'0x1/01/2020'", "jusqu'à": "'01/03/2020'" } } },
      "a",
    ],
    [
      "empty units",
      { a: { valeur: 1, unité: "€.2/mois.3" }, b: "a * 2 mois" },
      "b",
    ],
    [
      "dates with a fractional month",
      { a: { durée: { depuis: "'0.5/2012'", "jusqu'à": "'29/28/2013'" } } },
      "a",
    ],
    [
      "dates with a negative month",
      { a: { durée: { depuis: "'01/01/2030'", "jusqu'à": "'01/-1/2031'" } } },
      "a",
    ],
  ])("odd rules: %s", (_, ruleSet, name) => {
    compare(ruleSet as RawPublicodes, {}, (engine) => [
      value(engine, name),
      engine.getRule(name).title,
    ]);
  });

  test.each<[string, object, (engine: AnyEngine) => unknown]>([
    [
      "undefined in the situation",
      { a: { "par défaut": 1 } },
      (engine) => value(engine.setSituation({ a: undefined } as never), "a"),
    ],
    [
      "NaN in the situation",
      { a: { "par défaut": 1 } },
      (engine) => value(engine.setSituation({ a: Number.NaN }), "a"),
    ],
    ["undefined evaluated", {}, (engine) => value(engine, undefined as never)],
    ["undefined rule", { a: undefined }, () => null],
    [
      "infinite values",
      { a: { somme: [Number.POSITIVE_INFINITY, 1] } },
      (engine) => value(engine, "a"),
    ],
  ])("values JSON cannot write: %s", (_, ruleSet, use) => {
    compare(ruleSet as RawPublicodes, {}, use);
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

  describe("replacements in added rules", () => {
    const ruleSet = {
      a: 1,
      b: "a * 2",
      c: { valeur: 10, contexte: { a: 100 } },
      d: { question: "?" },
      e: { "par défaut": 3 },
      g: { valeur: 4, remplace: { "références à": "a", priorité: 2 } },
    };
    const values = (engine: AnyEngine) =>
      ["b", "c", "d + a", "e"].map((name) => value(engine, name));
    test.each<[string, (engine: AnyEngine) => unknown]>([
      [
        "situation",
        (engine) => {
          engine.setSituation({
            e: { valeur: "d", remplace: { "références à": "a", priorité: 3 } },
          } as never);
          return values(engine);
        },
      ],
      [
        "situation kept, then dropped",
        (engine) => {
          engine.setSituation({
            e: { valeur: 7, "rend non applicable": "g" },
          } as never);
          engine.setSituation({ d: 1 }, { keepPreviousSituation: true });
          const kept = values(engine);
          engine.setSituation({ d: 1 });
          return [kept, values(engine)];
        },
      ],
      [
        "evaluate",
        (engine) => [
          value(engine, { valeur: 5, remplace: "g" } as never),
          values(engine),
        ],
      ],
    ])("%s", (_, use) => {
      compare(ruleSet, {}, use);
    });
  });
});

describe("evaluate", () => {
  test("durée ends by default on the day it is parsed", () => {
    const parsedOn = new Date(2026, 0, 1, 12);
    vi.useFakeTimers({ toFake: ["Date"], now: parsedOn });
    try {
      compare(
        { ancienneté: { durée: { depuis: "01/01/2020" } } },
        {},
        (engine) => {
          vi.setSystemTime(new Date(2026, 0, 3, 12));
          const values = [
            value(engine, "ancienneté"),
            value(engine, { durée: { depuis: "01/01/2020" } } as never),
          ];
          vi.setSystemTime(parsedOn);
          return values;
        },
      );
    } finally {
      vi.useRealTimers();
    }
  });

  test("AST nodes are refused, not evaluated as objects", () => {
    const node = new PublicodesEngine({ a: 1 }).getRule("a");
    const engine = new Engine({ a: 1 });
    for (const use of [
      () => engine.evaluate(node as never),
      () => engine.evaluate({ valeur: node } as never),
      () => engine.setSituation({ a: node } as never),
    ]) {
      expect(use).toThrow("fast-publicodes n'évalue pas les nœuds d'AST");
    }
  });

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
      if (unit === "y") throw { name: "Refus", message: "Unité y refusée" };
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

    test("is called for each unit written", () => {
      const calls: string[] = [];
      const getUnitKey = (unit: string) => {
        calls.push(unit);
        return keys[unit] ?? unit;
      };
      compare(translated, { getUnitKey }, (engine) => {
        engine.setSituation({ prime: "1200 €/year" });
        engine.evaluate("annuel / 12 month/year");
        return calls.splice(0);
      });
    });

    test.each(["x", "y"])("what it throws for %s", (unit) => {
      compare({ a: `1 ${unit}` }, { getUnitKey: failing }, () => null);
      compare(contrat, { getUnitKey: failing }, (engine) =>
        value(engine, `salaire + 2 ${unit}`),
      );
      for (const keepPreviousSituation of [true, false]) {
        for (const strict of [true, false]) {
          compare(contrat, { getUnitKey: failing }, (engine) => {
            engine.setSituation({ salaire: 2500 });
            try {
              engine.setSituation(
                { contrat: "'CDD'", salaire: `2 ${unit}` },
                { strict, keepPreviousSituation },
              );
            } catch (error) {
              return [(error as Error).message, engine.getSituation()];
            }
            return engine.getSituation();
          });
        }
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

  test("possibilities are those of the rule", () => {
    compare({ a: { "une possibilité": ["'x'"] } }, {}, (engine) => [
      engine.getPossibilitiesFor("a") ===
        engine.getRule("a").possibilities?.explanation,
      engine.getPossibilitiesFor("a")?.map((p) => p.publicodesValue),
    ]);
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

const models: [string, RawPublicodes, Situation][] = [
  [
    "modele-social",
    socialRules as RawPublicodes,
    { "salarié . contrat": "'CDI'", "salarié . contrat . salaire brut": 3000 },
  ],
  [
    "nosgestesclimat",
    JSON.parse(
      readFileSync(
        createRequire(import.meta.url).resolve(
          "@incubateur-ademe/nosgestesclimat/public/co2-model.FR-lang.fr.json",
        ),
        "utf8",
      ),
    ),
    { "alimentation . plats . végétalien . nombre": 3 },
  ],
  [
    "aides-velo",
    veloRules as RawPublicodes,
    { "localisation . code insee": "'75056'", "demandeur . âge": 22 },
  ],
];

test.each(models)(
  "%s: rules, possibilities and warnings",
  (_, ruleSet, situation) => {
    compare(ruleSet, {}, (engine) => {
      const names = Object.keys(engine.getParsedRules());
      const rules = names.map((name) => {
        const rule = engine.getRule(name);
        return [name, rule.title, rule.private, rule.rawNode];
      });
      const possibilities = names.map((name) =>
        engine
          .getPossibilitiesFor(name)
          ?.map(({ type, nodeValue, publicodesValue, dottedName, title }) => ({
            type,
            nodeValue,
            publicodesValue,
            dottedName,
            title,
          })),
      );
      engine.setSituation(situation);
      for (const name of names) {
        try {
          engine.evaluate(name);
        } catch {}
      }
      return [rules, possibilities];
    });
  },
);

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

  test("copies forget inversion failures", () => {
    const ruleSet = {
      brut: { unité: "€", "inversion numérique": { avec: ["net"] } },
      net: { valeur: "brut * 0", unité: "€" },
    };
    compare(ruleSet, {}, (engine) => {
      engine.setSituation({ net: 100 });
      engine.evaluate("brut");
      return [engine.inversionFail(), engine.shallowCopy().inversionFail()];
    });
  });

  test("evaluations stay the same objects until the cache is reset", () => {
    const sameTwice = (engine: AnyEngine, expression: object) => {
      const first = engine.evaluate(expression as never);
      return engine.evaluate(expression as never) === first;
    };
    compare(contrat, {}, (engine) => {
      const expression = { valeur: "net" };
      const first = engine.evaluate("net");
      const same = [
        engine.evaluate("net") === first,
        sameTwice(engine, expression),
        sameTwice(engine, { valeur: "net" }),
        engine.shallowCopy().evaluate("net") === first,
        engine.shallowCopy({ copyCache: false }).evaluate("net") === first,
      ];
      engine.resetCache();
      const afterReset = engine.evaluate("net");
      engine.setSituation({});
      return [
        ...same,
        afterReset === first,
        engine.evaluate("net") === afterReset,
      ];
    });
  });
});
