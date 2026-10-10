import { Engine as NativeEngine } from "../native/index.js";

export type Rules = Record<string, unknown>;

export type Situation = Record<string, string | number>;

export interface Unit {
  numerators: string[];
  denominators: string[];
}

export interface Evaluation {
  nodeValue: number | string | boolean | null | undefined;
  unit?: Unit;
  missingVariables: Record<string, number>;
}

type NativeEvaluation =
  | { error: string }
  | {
      nodeValue: Evaluation["nodeValue"];
      unit?: Unit;
      missing: unknown;
    };

export default class Engine {
  #native: NativeEngine;
  #situation: Situation = {};

  constructor(rules: Rules = {}) {
    this.#native = new NativeEngine(JSON.stringify(rules));
  }

  setSituation(situation: Situation = {}): this {
    const error = this.#native.setSituation(JSON.stringify(situation));
    if (error) throw new Error(error);
    this.#situation = situation;
    return this;
  }

  getSituation(): Situation {
    return this.#situation;
  }

  evaluate(expression: string): Evaluation {
    const result = this.#native.evaluate(expression) as NativeEvaluation;
    if ("error" in result) throw new Error(result.error);
    const native = this.#native;
    let missingVariables: Record<string, number> | undefined;
    return {
      nodeValue: result.nodeValue,
      ...(result.unit && { unit: result.unit }),
      // Built on first read: most callers only need the value.
      get missingVariables() {
        missingVariables ??= native.missingVariables(result.missing) as Record<
          string,
          number
        >;
        return missingVariables;
      },
    };
  }
}
