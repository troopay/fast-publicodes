import type { EvaluatedNode } from "./types.js";
import { convertUnit, serializeUnit, simplifyUnit } from "./units.js";

/** The node with its unit simplified, if it has one, and its value converted to it. */
export function simplifyNodeUnit<Node extends EvaluatedNode = EvaluatedNode>(
  node: Node,
): Node {
  if (!node.unit) {
    return node;
  }
  const unit = simplifyUnit(node.unit);
  return {
    ...node,
    nodeValue:
      typeof node.nodeValue === "number"
        ? convertUnit(node.unit, unit, node.nodeValue)
        : node.nodeValue,
    unit,
  };
}

/** The node's value as a publicodes expression. */
export function serializeEvaluation(node: EvaluatedNode): string | undefined {
  if (typeof node.nodeValue === "number") {
    const unit = serializeUnit(node.unit);
    return `${node.nodeValue}${unit ? unit.replace(/\s/g, "") : ""}`;
  }
  if (typeof node.nodeValue === "boolean") {
    return node.nodeValue ? "oui" : "non";
  }
  if (typeof node.nodeValue === "string") {
    return `'${node.nodeValue}'`;
  }
}
