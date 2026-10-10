import rules from "modele-social";
import PublicodesEngine from "publicodes";
import { test } from "vitest";
import Engine, { type Situation } from "../src/index.js";

const situation: Situation = {
  "salarié . contrat": "'CDI'",
  "salarié . contrat . salaire brut": 3000,
};
const target = "salarié . rémunération . net . à payer avant impôt";
const RUN_OPTIONS = {
  time: 1000,
  iterations: 5,
  warmupTime: 200,
  warmupIterations: 1,
};
// Module export getters add overhead the benchmark would measure.
const FastEngine = Engine;
const silent = { log() {}, warn() {}, error() {} };

const engines = {
  publicodes: () => new PublicodesEngine(rules, { logger: silent }),
  "fast-publicodes": () => new FastEngine(rules, { logger: silent }),
};
const names = Object.keys(engines.publicodes().getParsedRules());

function evaluateAll(engine: { evaluate(name: string): unknown }) {
  for (const name of names) {
    try {
      engine.evaluate(name);
    } catch {}
  }
}

test("parse rules", async ({ bench }) => {
  await bench.compare(
    bench("publicodes", engines.publicodes),
    bench("fast-publicodes", engines["fast-publicodes"]),
    RUN_OPTIONS,
  );
});

test("evaluate one rule in a new situation", async ({ bench }) => {
  await bench.compare(
    ...Object.entries(engines).map(([name, create]) => {
      const engine = create();
      return bench(name, () => {
        engine.setSituation(situation);
        try {
          engine.evaluate(target);
        } catch {}
      });
    }),
    RUN_OPTIONS,
  );
});

test("evaluate every rule in a new situation", async ({ bench }) => {
  await bench.compare(
    ...Object.entries(engines).map(([name, create]) => {
      const engine = create();
      return bench(name, () => {
        engine.setSituation(situation);
        evaluateAll(engine);
      });
    }),
    RUN_OPTIONS,
  );
});
