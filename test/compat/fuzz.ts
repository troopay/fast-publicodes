import type { Situation } from "../../src/index.js";
import { diff, evaluateAll, rulesOf } from "./outcome.js";
import { cached, hash, type Suite } from "./suites.js";

type Input =
  | { name: string; kind: "boolean" | "date" | "percent" | "number" }
  | { name: string; kind: "choice"; values: string[] };

const INCLUSION_PROBABILITY = 0.25;

/** The suite's questions, typed so that random values for them are valid. */
export function inputsOf(suite: Suite): Promise<Input[]> {
  return cached(hash(suite.cacheKey, "inputs"), () => {
    const oracle = suite.oracle();
    return Object.entries(oracle.getParsedRules()).flatMap(
      ([name, rule]): Input[] => {
        if (!rule.rawNode.question) return [];
        const possibilities = oracle.getPossibilitiesFor(name);
        if (possibilities) {
          const values = possibilities.map((p) => p.publicodesValue);
          return [{ name, kind: "choice", values }];
        }
        switch (oracle.context.nodesTypes.get(rule)?.type) {
          case "boolean":
            return [{ name, kind: "boolean" }];
          case "date":
            return [{ name, kind: "date" }];
          case "number":
            return [
              { name, kind: rule.rawNode.unité === "%" ? "percent" : "number" },
            ];
          default:
            return [];
        }
      },
    );
  });
}

export function generate(inputs: Input[], seed: number): Situation {
  const random = mulberry32(seed);
  const pick = <T>(values: T[]): T =>
    values[Math.floor(random() * values.length)] as T;
  const situation: Situation = {};
  for (const input of inputs) {
    if (random() >= INCLUSION_PROBABILITY) continue;
    switch (input.kind) {
      case "boolean":
        situation[input.name] = pick(["oui", "non"]);
        break;
      case "choice":
        situation[input.name] = pick(input.values);
        break;
      case "date": {
        const day = String(1 + Math.floor(random() * 28)).padStart(2, "0");
        const month = String(1 + Math.floor(random() * 12)).padStart(2, "0");
        situation[input.name] =
          `${day}/${month}/${2015 + Math.floor(random() * 16)}`;
        break;
      }
      case "percent":
        situation[input.name] = Math.floor(random() * 101);
        break;
      case "number":
        situation[input.name] =
          random() < 0.15 ? 0 : Math.round(10 ** (random() * 6));
        break;
    }
  }
  return situation;
}

/** Drops every key of `situation` that `rule` does not need to keep differing. */
export function shrink(
  suite: Suite,
  situation: Situation,
  rule: string,
): Situation {
  const differs = (candidate: Situation) =>
    diff(
      evaluateAll(suite.oracle(), candidate, rulesOf([rule])),
      evaluateAll(suite.candidate(), candidate, rulesOf([rule])),
    ).length > 0;
  const shrunk = { ...situation };
  for (const key of Object.keys(situation)) {
    const { [key]: _, ...without } = shrunk;
    if (differs(without)) delete shrunk[key];
  }
  return shrunk;
}

export function mulberry32(seed: number): () => number {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}
