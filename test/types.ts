/**
 * Type-level comparison with `publicodes`, checked by `tsc`: each type is interchangeable with
 * its `publicodes` counterpart, AST fields apart. Methods are compared by their parameters, as
 * the two `Engine` classes are distinct types.
 */
import type * as P from "publicodes";
import type * as F from "../src/index.js";

type Interchangeable<A, B> = [A] extends [B]
  ? [B] extends [A]
    ? true
    : false
  : false;

type Assert<T extends true> = T;

type Engines = [F.Engine, P.default];
type Params<Method extends keyof F.Engine & keyof P.default> = Interchangeable<
  Parameters<Engines[0][Method]>,
  Parameters<Engines[1][Method]>
>;

export type Checks = [
  Assert<Interchangeable<F.Unit, P.Unit>>,
  Assert<Interchangeable<F.Evaluation, P.Evaluation>>,
  Assert<Interchangeable<F.Logger, P.Logger>>,
  Assert<Interchangeable<F.EngineOptions, P.EngineOptions>>,
  Assert<Interchangeable<F.StrictOptions, P.StrictOptions>>,
  Assert<Interchangeable<F.FlagOptions, P.FlagOptions>>,
  Assert<Interchangeable<F.WarnOptions, P.WarnOptions>>,
  Assert<Interchangeable<F.Rule, P.Rule>>,
  Assert<Interchangeable<F.RawPublicodes<"a">, P.RawPublicodes<"a">>>,
  Assert<Interchangeable<F.Situation<"a">, P.Situation<"a">>>,
  Assert<Interchangeable<F.PublicodesExpression, P.PublicodesExpression>>,
  Assert<Interchangeable<F.ExprAST, P.ExprAST>>,
  Assert<Interchangeable<F.BinaryOp, P.BinaryOp>>,
  Assert<Interchangeable<F.UnaryOp, P.UnaryOp>>,
  Assert<
    Interchangeable<
      F.EvaluatedNode,
      Pick<P.EvaluatedNode, "nodeValue" | "unit" | "missingVariables">
    >
  >,
  Assert<
    Interchangeable<
      Omit<F.Possibility, "notApplicable">,
      Omit<P.Possibility, "notApplicable">
    >
  >,
  Assert<
    Interchangeable<
      Omit<F.RuleNode, "possibilities">,
      Pick<
        P.RuleNode,
        | "dottedName"
        | "title"
        | "nodeKind"
        | "virtualRule"
        | "private"
        | "rawNode"
      >
    >
  >,
  Assert<
    P.EvaluatedNode extends Parameters<typeof F.serializeEvaluation>[0]
      ? true
      : false
  >,
  Assert<
    Interchangeable<
      ReturnType<typeof F.serializeEvaluation>,
      ReturnType<typeof P.serializeEvaluation>
    >
  >,
  Assert<
    P.EvaluatedNode extends Parameters<typeof F.simplifyNodeUnit>[0]
      ? true
      : false
  >,
  Assert<Interchangeable<typeof F.formatValue, typeof P.formatValue>>,
  Assert<Interchangeable<typeof F.capitalise0, typeof P.capitalise0>>,
  Assert<Interchangeable<typeof F.parseUnit, typeof P.parseUnit>>,
  Assert<Interchangeable<typeof F.serializeUnit, typeof P.serializeUnit>>,
  Assert<Interchangeable<typeof F.parseExpression, typeof P.parseExpression>>,
  Assert<
    Interchangeable<
      F.PublicodesError<"SyntaxError">,
      P.PublicodesError<"SyntaxError">
    >
  >,
  Assert<
    Interchangeable<
      ConstructorParameters<typeof F.Engine>,
      ConstructorParameters<typeof P.default>
    >
  >,
  Assert<
    Interchangeable<
      Parameters<F.Engine["evaluate"]>,
      [Exclude<Parameters<P.default["evaluate"]>[0], P.ASTNode>]
    >
  >,
  Assert<Params<"setSituation">>,
  Assert<Params<"getRule">>,
  Assert<Params<"getPossibilitiesFor">>,
  Assert<Params<"shallowCopy">>,
  Assert<
    Interchangeable<
      ReturnType<F.Engine["getSituation"]>,
      ReturnType<P.default["getSituation"]>
    >
  >,
  Assert<
    Interchangeable<
      ReturnType<F.Engine["inversionFail"]>,
      ReturnType<P.default["inversionFail"]>
    >
  >,
  Assert<
    Interchangeable<
      Exclude<keyof typeof P, keyof typeof F>,
      "parsePublicodes" | "reduceAST" | "transformAST" | "traverseASTNode"
    >
  >,
  Assert<
    Interchangeable<
      Exclude<keyof typeof P.utils, keyof typeof F.utils>,
      | "cyclicDependencies"
      | "disambiguateReferenceNode"
      | "updateReferencesMapsFromReferenceNode"
    >
  >,
];
