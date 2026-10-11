/** What the Node addon and the WebAssembly module both export. */
export interface Binding {
  Engine: new (
    rules: string,
    options: string,
    getToday: () => string,
    getUnitKey?: (unit: string) => string | { message?: string },
  ) => NativeEngine;
  parseExpression(expression: string, dottedName: string): object;
}

export interface NativeEngine {
  setSituation(
    situation: string,
    keepPrevious: boolean,
    strict: boolean,
  ): object;
  evaluate(text: string, json: boolean): object;
  evaluations(): unknown;
  missingVariables(evaluations: unknown, index: number): object;
  takeWarnings(): object;
  inversionFail(): boolean;
  resetCache(): void;
  shallowCopy(copyCache: boolean): NativeEngine;
  ruleNames(): string[];
  rule(name: string): object;
  applicablePossibilities(name: string): object;
}

let current: Binding | undefined;

/** Sets the binding the package uses; its entry points call it before anything else runs. */
export function useBinding(binding: Binding) {
  current = binding;
}

export function binding(): Binding {
  if (!current)
    throw new Error("fast-publicodes was loaded without its engine");
  return current;
}

/**
 * `value` as JSON for the binding, writing what JSON cannot as `{ "\u0000": String(it) }`:
 * non-finite numbers, and `undefined` as `value` or as one of its properties. `encode` may
 * replace any object of `value`.
 */
export function toJson(
  value: unknown,
  encode?: (item: object) => object | undefined,
): string {
  if (value === undefined) return UNDEFINED;
  return JSON.stringify(value, function (this: unknown, _key, item) {
    if (typeof item === "number" && !Number.isFinite(item)) {
      return { "\u0000": String(item) };
    }
    if (encode && typeof item === "object" && item !== null) {
      const encoded = encode(item);
      if (encoded) return encoded;
    }
    return item === undefined && this === value
      ? { "\u0000": "undefined" }
      : item;
  });
}

const UNDEFINED = JSON.stringify({ "\u0000": "undefined" });
