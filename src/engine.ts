import { binding, type NativeEngine, toJson } from "./binding.js";
import {
  fromNative,
  type NativeError,
  PublicodesError,
  restoreSurrogates,
} from "./error.js";
import { resolveOptions } from "./options.js";
import type {
  EngineOptions,
  EvaluatedNode,
  InferedType,
  Logger,
  ParsedRules,
  Possibility,
  PublicodesExpression,
  RawPublicodes,
  RuleNode,
  Situation,
  Suggestion,
} from "./types.js";
import type { GetUnitKey, Unit } from "./units.js";

/** Warnings as the binding reports them: new ones as text, known ones by index. */
type NativeWarnings = {
  warnings?: (string | number)[];
  warningsReset?: boolean;
};

type NativeResult<T> = ({ error: string } | T) & NativeWarnings;

/** A unit as the binding reports it: new ones whole, known ones by index. */
type NativeUnit = { unit?: number; newUnit?: Unit; unitsReset?: boolean };

type NativeEvaluation = NativeResult<
  { nodeValue: EvaluatedNode["nodeValue"]; index: number } & NativeUnit
>;

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
  logger: Logger,
  unitKey: UnitKey | undefined,
): NativeEngine {
  if (typeof rules === "string") {
    throw new PublicodesError(
      "EngineError",
      "Publicodes does not parse yaml rule sets itself anymore. Please provide a parsed js object. E.g. the `eemeli/yaml` package.",
      {},
    );
  }

  try {
    return new (binding().Engine)(
      toJson(rules),
      JSON.stringify(resolveOptions(options)),
      today,
      unitKey?.call,
    );
  } catch (error) {
    unitKey?.rethrow();
    let details: NativeError & { warnings: string[] };
    try {
      details = JSON.parse((error as Error).message);
    } catch {
      throw error;
    }
    for (const warning of details.warnings) {
      logger.warn(restoreSurrogates(warning));
    }
    throw fromNative(details);
  }
}

/** The rule and name of each suggestion `getRule()` returns, which any engine evaluates. */
const suggestionSources = new WeakMap<object, { rule: string; name: string }>();

export class Engine<RuleNames extends string = string> {
  private native: NativeEngine;
  private logger: Logger;
  private publicSituation: Situation<RuleNames> = {};
  private parsedRules: ParsedRules<RuleNames> | undefined;
  private rules = new Map<string, RuleNode<RuleNames>>();
  /** `nodesTypes` alone, for the rule nodes `getRule()` returns. */
  context: { nodesTypes: WeakMap<RuleNode, InferedType> } = {
    nodesTypes: new WeakMap(),
  };
  private warningTexts: string[] = [];
  private unitKey: UnitKey | undefined;
  /** Results by evaluated value, until the cache is reset. */
  private evaluations = new Map<PublicodesExpression, EvaluatedNode>();
  /** The engine's handle on the evaluations of the current cache. */
  private evaluationsHandle: unknown;
  private units: Unit[] = [];

  constructor(
    rules: RawPublicodes<RuleNames> = {},
    options: EngineOptions = {},
  ) {
    this.logger = options.logger ?? console;
    this.unitKey = options.getUnitKey && new UnitKey(options.getUnitKey);
    this.native = createNative(rules, options, this.logger, this.unitKey);
    this.logWarnings(this.native.takeWarnings() as NativeWarnings);
  }

  private logWarnings({ warnings, warningsReset }: NativeWarnings) {
    if (!warnings) return;
    if (warningsReset) this.warningTexts = [];
    for (const warning of warnings) {
      if (typeof warning === "string") {
        const text = restoreSurrogates(warning);
        this.warningTexts.push(text);
        this.logger.warn(text);
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
    this.resetEvaluations();
    const result = this.native.setSituation(
      toJson(situation),
      Boolean(keepPreviousSituation),
      Boolean(strict),
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
    const result = this.evaluateNative(value);
    this.logWarnings(result);
    this.unitKey?.rethrow();
    const { nodeValue, index, ...nativeUnit } = unwrap(result);
    const unit = this.unitOf(nativeUnit);
    const native = this.native;
    this.evaluationsHandle ??= native.evaluations();
    const handle = this.evaluationsHandle;
    let missingVariables: Record<string, number> | undefined;
    const evaluation = {
      nodeValue,
      ...(unit && { unit }),
      // Built on first read: most callers only need the value.
      get missingVariables() {
        missingVariables ??= native.missingVariables(handle, index) as Record<
          string,
          number
        >;
        return missingVariables;
      },
    };
    this.evaluations.set(value, evaluation);
    return evaluation;
  }

  private evaluateNative(value: PublicodesExpression): NativeEvaluation {
    const suggestion =
      typeof value === "object" ? suggestionSources.get(value) : undefined;
    if (suggestion) {
      return this.native.evaluateSuggestion(
        suggestion.rule,
        suggestion.name,
      ) as NativeEvaluation;
    }
    const json = typeof value !== "string";
    const text = json ? toJson(value) : value;
    return this.native.evaluate(text, json) as NativeEvaluation;
  }

  getRule(dottedName: RuleNames): RuleNode<RuleNames> {
    let rule = this.rules.get(dottedName);
    if (!rule) {
      const result = this.native.rule(dottedName) as NativeResult<{
        rule: string;
      }>;
      const { node, type, suggestions } = JSON.parse(unwrap(result).rule) as {
        node: Omit<RuleNode<RuleNames>, "suggestions">;
        type: { [K in keyof InferedType]: InferedType[K] | null };
        suggestions: string[];
      };
      rule = { ...node, suggestions: {} };
      for (const name of suggestions) {
        const suggestion: Suggestion = Object.freeze({});
        suggestionSources.set(suggestion, { rule: dottedName, name });
        rule.suggestions[name] = suggestion;
      }
      this.rules.set(dottedName, rule);
      this.context.nodesTypes.set(rule, {
        isNullable: type.isNullable ?? undefined,
        type: type.type ?? undefined,
      });
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
    if (!filterNotApplicable) {
      const { possibilities } = this.getRule(dottedName);
      return possibilities ? possibilities.explanation : null;
    }
    const result = this.native.applicablePossibilities(
      dottedName,
    ) as NativeResult<{ possibilities: string }>;
    this.logWarnings(result);
    return JSON.parse(unwrap(result).possibilities);
  }

  inversionFail(): boolean {
    return this.native.inversionFail();
  }

  resetCache(): void {
    this.resetEvaluations();
    this.native.resetCache();
  }

  private resetEvaluations() {
    this.evaluations = new Map();
    this.evaluationsHandle = undefined;
  }

  private unitOf({ unit, newUnit, unitsReset }: NativeUnit): Unit | undefined {
    if (unitsReset) this.units = [];
    if (newUnit) {
      this.units.push(newUnit);
      return newUnit;
    }
    return unit === undefined ? undefined : this.units[unit];
  }

  shallowCopy({
    copyCache = true,
  }: {
    copyCache?: boolean;
    inherit?: boolean;
  } = {}): Engine<RuleNames> {
    const copy = Object.create(Engine.prototype) as Engine<RuleNames>;
    copy.native = this.native.shallowCopy(Boolean(copyCache));
    copy.logger = this.logger;
    copy.publicSituation = { ...this.publicSituation };
    copy.parsedRules = this.parsedRules;
    copy.rules = this.rules;
    copy.context = this.context;
    copy.warningTexts = [...this.warningTexts];
    copy.unitKey = this.unitKey;
    copy.evaluations = copyCache ? new Map(this.evaluations) : new Map();
    copy.evaluationsHandle = undefined;
    copy.units = [...this.units];
    return copy;
  }
}
