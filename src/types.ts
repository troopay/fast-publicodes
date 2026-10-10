import type { Unit } from "./units.js";

export type PublicodesExpression = string | number | Record<string, unknown>;

export type Rule = Record<string, unknown> & {
  titre?: string;
  question?: string;
  experimental?: "oui";
  privé?: "oui";
};

export type RawRule = Omit<Rule, "nom"> | PublicodesExpression | null;

export type RawPublicodes<RuleNames extends string = string> = Partial<
  Record<RuleNames, RawRule>
>;

export type Situation<RuleNames extends string = string> = Partial<
  Record<RuleNames, PublicodesExpression>
>;

export type Evaluation = number | string | boolean | null | undefined;

export interface EvaluatedNode {
  nodeValue: Evaluation;
  unit?: Unit;
  missingVariables: Record<string, number>;
}

/** One of the values a rule with `une possibilité` takes. */
export interface Possibility {
  nodeKind: "reference" | "constant";
  type: "number" | "string" | "reference";
  nodeValue: string | number;
  publicodesValue: string;
  notApplicable: { nodeKind: string; nodeValue: Evaluation };
  unit?: Unit;
  name?: string;
  dottedName?: string;
  title?: string;
}

/** A parsed rule, without its explanation tree. */
export interface RuleNode<Name extends string = string> {
  dottedName: Name;
  title: string;
  nodeKind: "rule";
  private: boolean;
  virtualRule: boolean;
  rawNode: Rule;
  possibilities: {
    nodeKind: "une possibilité";
    type: "number" | "string" | "reference";
    unit?: Unit;
    explanation: Possibility[];
  } | null;
}

export type ParsedRules<RuleNames extends string = string> = Record<
  RuleNames,
  RuleNode<RuleNames>
>;

export interface Logger {
  log(message: string): void;
  warn(message: string): void;
  error(message: string): void;
}

export interface StrictOptions {
  situation?: boolean;
  noOrphanRule?: boolean;
  noCycleRuntime?: boolean;
  checkPossibleValues?: boolean;
}

export interface FlagOptions {
  filterNotApplicablePossibilities?: boolean;
  automaticNamespaceDisabling?: boolean;
}

export interface WarnOptions {
  experimentalRules?: boolean;
  unitConversion?: boolean;
  cyclicReferences?: boolean;
  deprecatedSyntax?: boolean;
  situationIssues?: boolean;
}

export interface EngineOptions {
  allowOrphanRules?: boolean;
  strict?: boolean | StrictOptions;
  logger?: Logger;
  flag?: FlagOptions;
  warn?: boolean | WarnOptions;
}
