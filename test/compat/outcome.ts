import { inspect, isDeepStrictEqual } from "node:util";
import { serializeUnit } from "publicodes";
import type { EvaluatedNode, Situation } from "../../src/index.js";

export type Outcome =
  | { error: string }
  | {
      nodeValue?: EvaluatedNode["nodeValue"];
      unit?: string;
      missingVariables: Record<string, number>;
    };

export type Outcomes = Record<string, Outcome>;

export interface Mismatch {
  name: string;
  expected: Outcome;
  received: Outcome;
}

interface EvaluatingEngine {
  setSituation(situation: Situation): unknown;
  evaluate(expression: string): EvaluatedNode;
}

const RELATIVE_TOLERANCE = 1e-9;

export function evaluateAll(
  engine: EvaluatingEngine,
  situation: Situation,
  names: string[],
): Outcomes {
  try {
    engine.setSituation(situation);
  } catch (error) {
    const outcome = { error: messageOf(error) };
    return Object.fromEntries(names.map((name) => [name, outcome]));
  }
  return Object.fromEntries(
    names.map((name) => [name, evaluateOne(engine, name)]),
  );
}

function evaluateOne(engine: EvaluatingEngine, name: string): Outcome {
  try {
    const { nodeValue, unit, missingVariables } = engine.evaluate(name);
    return {
      ...(nodeValue !== undefined && { nodeValue }),
      ...(unit && { unit: serializeUnit(unit) }),
      missingVariables,
    };
  } catch (error) {
    return { error: messageOf(error) };
  }
}

function messageOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function diff(expected: Outcomes, received: Outcomes): Mismatch[] {
  return Object.entries(expected).flatMap(([name, outcome]) => {
    const actual = received[name] ?? { error: "not evaluated" };
    return sameOutcome(outcome, actual)
      ? []
      : [{ name, expected: outcome, received: actual }];
  });
}

function sameOutcome(a: Outcome, b: Outcome): boolean {
  if ("error" in a || "error" in b) return "error" in a && "error" in b;
  return (
    sameValue(a.nodeValue, b.nodeValue) &&
    a.unit === b.unit &&
    isDeepStrictEqual(a.missingVariables, b.missingVariables)
  );
}

function sameValue(a: unknown, b: unknown): boolean {
  if (typeof a === "number" && typeof b === "number") {
    return (
      a === b ||
      (Number.isNaN(a) && Number.isNaN(b)) ||
      Math.abs(a - b) <= RELATIVE_TOLERANCE * Math.max(Math.abs(a), Math.abs(b))
    );
  }
  return a === b;
}

export function report(mismatches: Mismatch[], total: number, shown = 10) {
  const lines = mismatches
    .slice(0, shown)
    .map(
      ({ name, expected, received }) =>
        `  ${name}\n    expected: ${inspect(expected, { breakLength: Infinity })}\n    received: ${inspect(received, { breakLength: Infinity })}`,
    );
  const more =
    mismatches.length > shown ? [`  … ${mismatches.length - shown} more`] : [];
  return [
    `${mismatches.length}/${total} rules differ`,
    ...errorSummary(mismatches),
    ...lines,
    ...more,
  ].join("\n");
}

/** The errors fast-publicodes raised where publicodes did not, most frequent first. */
function errorSummary(mismatches: Mismatch[], shown = 5): string[] {
  const counts = new Map<string, number>();
  for (const { expected, received } of mismatches) {
    if (!("error" in received) || "error" in expected) continue;
    const message = received.error.trim().split("\n").at(-1) ?? "";
    counts.set(message, (counts.get(message) ?? 0) + 1);
  }
  if (counts.size === 0) return [];
  const top = [...counts].sort(([, a], [, b]) => b - a).slice(0, shown);
  return [
    "Errors raised only by fast-publicodes:",
    ...top.map(([message, count]) => `  ${count} × ${message}`),
  ];
}
