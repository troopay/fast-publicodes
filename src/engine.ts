import { Engine as NativeEngine } from "../native/index.js";
import { fromNative, type NativeError, PublicodesError } from "./error.js";
import type {
  EngineOptions,
  EvaluatedNode,
  Logger,
  ParsedRules,
  Possibility,
  PublicodesExpression,
  RawPublicodes,
  RuleNode,
  Situation,
} from "./types.js";
import type { GetUnitKey, Unit } from "./units.js";

/** Warnings as the binding reports them: new ones as text, known ones by index. */
type NativeWarnings = {
  warnings?: (string | number)[];
  warningsReset?: boolean;
};

type NativeResult<T> = ({ error: string } | T) & NativeWarnings;

type NativeEvaluation = NativeResult<{
  nodeValue: EvaluatedNode["nodeValue"];
  unit?: Unit;
  missing: unknown;
}>;

function unwrap<T extends object>(result: NativeResult<T>): T {
  if ("error" in result) {
    throw fromNative(JSON.parse(result.error) as NativeError);
  }
  return result;
}

function today(): string {
  const date = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(date.getDate())}/${pad(date.getMonth() + 1)}/${date.getFullYear()}`;
}

/**
 * `getUnitKey` as the binding calls it: it returns the message of an `Error` it throws, and
 * keeps any other value it throws for the engine to throw once the call that ran it returns.
 */
class UnitKey {
  private thrown: { value: unknown } | undefined;

  constructor(private getUnitKey: GetUnitKey) {}

  call = (unit: string): string | { message?: string } => {
    try {
      return this.getUnitKey(unit);
    } catch (error) {
      if (error instanceof Error) return { message: error.message };
      this.thrown = { value: error };
      return {};
    }
  };

  rethrow() {
    const thrown = this.thrown;
    if (thrown) {
      this.thrown = undefined;
      throw thrown.value;
    }
  }
}

function createNative(
  rules: RawPublicodes,
  options: EngineOptions,
  unitKey: UnitKey | undefined,
): NativeEngine {
  if (typeof rules === "string") {
    throw new PublicodesError(
      "EngineError",
      "Publicodes does not parse yaml rule sets itself anymore. Please provide a parsed js object. E.g. the `eemeli/yaml` package.",
      {},
    );
  }
  const { logger: _, getUnitKey: __, ...serializable } = options;
  try {
    return new NativeEngine(
      JSON.stringify(rules),
      JSON.stringify(serializable),
      today,
      unitKey?.call,
    );
  } catch (error) {
    unitKey?.rethrow();
    let details: NativeError | undefined;
    try {
      details = JSON.parse((error as Error).message);
    } catch {
      throw error;
    }
    throw fromNative(details as NativeError);
  }
}

export class Engine<RuleNames extends string = string> {
  private native: NativeEngine;
  private logger: Logger;
  private publicSituation: Situation<RuleNames> = {};
  private parsedRules: ParsedRules<RuleNames> | undefined;
  private rules = new Map<string, RuleNode<RuleNames>>();
  private warningTexts: string[] = [];
  private unitKey: UnitKey | undefined;
  /** Results by evaluated value, until the cache is reset. */
  private evaluations = new Map<PublicodesExpression, EvaluatedNode>();

  constructor(
    rules: RawPublicodes<RuleNames> = {},
    options: EngineOptions = {},
  ) {
    this.logger = options.logger ?? console;
    this.unitKey = options.getUnitKey && new UnitKey(options.getUnitKey);
    this.native = createNative(rules, options, this.unitKey);
    this.logWarnings(this.native.takeWarnings() as NativeWarnings);
  }

  private logWarnings({ warnings, warningsReset }: NativeWarnings) {
    if (!warnings) return;
    if (warningsReset) this.warningTexts = [];
    for (const warning of warnings) {
      if (typeof warning === "string") {
        this.warningTexts.push(warning);
        this.logger.warn(warning);
      } else {
        this.logger.warn(this.warningTexts[warning] as string);
      }
    }
  }

  setSituation(
    situation: Situation<RuleNames> = {},
    {
      keepPreviousSituation = false,
      strict = false,
    }: { keepPreviousSituation?: boolean; strict?: boolean } = {},
  ): this {
    this.evaluations = new Map();
    const result = this.native.setSituation(
      JSON.stringify(situation),
      keepPreviousSituation,
      strict,
    ) as { accepted: RuleNames[] } & NativeWarnings &
      ({ error: string; situationReset: boolean } | { error?: undefined });
    this.logWarnings(result);
    const { accepted, error } = result;
    if (error !== undefined) {
      if (result.situationReset && !keepPreviousSituation) {
        this.publicSituation = {};
      }
      if (accepted.length) {
        this.accept(situation, accepted, keepPreviousSituation);
      }
      this.unitKey?.rethrow();
      throw fromNative(JSON.parse(error) as NativeError);
    }
    this.accept(situation, accepted, keepPreviousSituation);
    return this;
  }

  private accept(
    situation: Situation<RuleNames>,
    accepted: RuleNames[],
    keepPreviousSituation: boolean,
  ) {
    if (!keepPreviousSituation) {
      this.publicSituation = {};
    }
    for (const name of accepted) {
      this.publicSituation[name] = situation[name];
    }
  }

  getSituation(): Situation<RuleNames> {
    return this.publicSituation;
  }

  evaluate(value: PublicodesExpression): EvaluatedNode {
    const evaluated = this.evaluations.get(value);
    if (evaluated) return evaluated;
    if (typeof value === "object" && value !== null && "nodeKind" in value) {
      throw new PublicodesError(
        "EngineError",
        "fast-publicodes n'évalue pas les nœuds d'AST",
        {},
      );
    }
    const result = (
      typeof value === "string"
        ? this.native.evaluate(value, false)
        : this.native.evaluate(JSON.stringify(value ?? null), true)
    ) as NativeEvaluation;
    this.logWarnings(result);
    this.unitKey?.rethrow();
    const { nodeValue, unit, missing } = unwrap(result);
    const native = this.native;
    let missingVariables: Record<string, number> | undefined;
    const evaluation = {
      nodeValue,
      ...(unit && { unit }),
      // Built on first read: most callers only need the value.
      get missingVariables() {
        missingVariables ??= native.missingVariables(missing) as Record<
          string,
          number
        >;
        return missingVariables;
      },
    };
    this.evaluations.set(value, evaluation);
    return evaluation;
  }

  getRule(dottedName: RuleNames): RuleNode<RuleNames> {
    let rule = this.rules.get(dottedName);
    if (!rule) {
      const result = this.native.rule(dottedName) as NativeResult<{
        rule: string;
      }>;
      rule = JSON.parse(unwrap(result).rule) as RuleNode<RuleNames>;
      this.rules.set(dottedName, rule);
    }
    return rule;
  }

  getParsedRules(): ParsedRules<RuleNames> {
    if (!this.parsedRules) {
      const rules = {} as ParsedRules<RuleNames>;
      for (const name of this.native.ruleNames() as RuleNames[]) {
        rules[name] = this.getRule(name);
      }
      this.parsedRules = rules;
    }
    return this.parsedRules;
  }

  getPossibilitiesFor(
    dottedName: RuleNames,
    { filterNotApplicable = false }: { filterNotApplicable?: boolean } = {},
  ): Possibility[] | null {
    const result = this.native.possibilities(
      dottedName,
      filterNotApplicable,
    ) as NativeResult<{ possibilities: string }>;
    this.logWarnings(result);
    return JSON.parse(unwrap(result).possibilities);
  }

  inversionFail(): boolean {
    return this.native.inversionFail();
  }

  resetCache(): void {
    this.evaluations = new Map();
    this.native.resetCache();
  }

  shallowCopy({
    copyCache = true,
  }: {
    copyCache?: boolean;
    inherit?: boolean;
  } = {}): Engine<RuleNames> {
    const copy = Object.create(Engine.prototype) as Engine<RuleNames>;
    copy.native = this.native.shallowCopy(copyCache);
    copy.logger = this.logger;
    copy.publicSituation = { ...this.publicSituation };
    copy.parsedRules = this.parsedRules;
    copy.rules = this.rules;
    copy.warningTexts = [...this.warningTexts];
    copy.unitKey = this.unitKey;
    copy.evaluations = copyCache ? new Map(this.evaluations) : new Map();
    return copy;
  }
}
