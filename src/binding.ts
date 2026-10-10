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
  evaluate(expression: string, json: boolean): object;
  missingVariables(missing: unknown): object;
  takeWarnings(): object;
  inversionFail(): boolean;
  resetCache(): void;
  shallowCopy(copyCache: boolean): NativeEngine;
  ruleNames(): string[];
  rule(name: string): object;
  possibilities(name: string, filterNotApplicable: boolean): object;
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
