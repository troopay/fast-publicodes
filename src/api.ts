import { binding } from "./binding.js";
import { Engine } from "./engine.js";
import { fromNative, type NativeError } from "./error.js";
import * as utils from "./ruleUtils.js";

export { isPublicodesError, PublicodesError } from "./error.js";
export { capitalise0, formatValue } from "./format.js";
export type * from "./types.js";
export { parseUnit, serializeUnit, type Unit } from "./units.js";
export { Engine, utils };
export default Engine;

/** The tree of a publicodes expression, as `publicodes` writes it. */
export type ExprAST = Record<string, unknown>;

export function parseExpression(
  expression: string,
  dottedName = "<expression>",
): ExprAST {
  const result = binding().parseExpression(expression, dottedName) as
    | { error: string }
    | { expression: string };
  if ("error" in result) {
    throw fromNative(JSON.parse(result.error) as NativeError);
  }
  return JSON.parse(result.expression);
}
