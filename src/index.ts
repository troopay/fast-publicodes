import { evaluate } from "../native/index.js";

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

export default class Engine {
  // biome-ignore lint/complexity/noUselessConstructor: publicodes takes rules here
  constructor(_rules: Rules = {}) {}

  setSituation(_situation: Situation = {}): this {
    return this;
  }

  evaluate(expression: string): Evaluation {
    return JSON.parse(evaluate(expression));
  }
}
