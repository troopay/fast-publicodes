import { PublicodesError } from "./error.js";
import type { RuleNode } from "./types.js";

type Rules = Record<
  string,
  Pick<RuleNode, "private" | "rawNode" | "title"> | undefined
>;

const splitName = (name: string) => name.split(" . ");
const joinName = (parts: string[]) => parts.join(" . ");

export const nameLeaf = (dottedName: string) => splitName(dottedName).at(-1);

export const encodeRuleName = (dottedName: string): string =>
  dottedName
    ?.replace(/\s\.\s/g, "/")
    .replace(/-/g, "‑")
    .replace(/\s/g, "-");

export const decodeRuleName = (dottedName: string): string =>
  dottedName.replace(/\//g, " . ").replace(/-/g, " ").replace(/‑/g, "-");

export const ruleParent = (dottedName: string): string =>
  joinName(splitName(dottedName).slice(0, -1));

export const contextNameToDottedName = (contextName: string) =>
  contextName.endsWith("$SITUATION") ? ruleParent(contextName) : contextName;

/** The ancestors of a rule, nearest first. */
export function ruleParents(dottedName: string): string[] {
  const parts = splitName(dottedName).slice(0, -1);
  return parts.map((_, i) => joinName(parts.slice(0, i + 1))).reverse();
}

export const getChildrenRules = (rules: Rules, dottedName: string) =>
  Object.keys(rules).filter(
    (name) =>
      name.startsWith(dottedName) &&
      splitName(name).length === splitName(dottedName).length + 1,
  );

export function findCommonAncestor(dottedName1: string, dottedName2: string) {
  const parts1 = splitName(dottedName1);
  const parts2 = splitName(dottedName2);
  const index = parts1.findIndex((part, i) => parts2[i] !== part);
  return index === -1 ? dottedName1 : joinName(parts1.slice(0, index));
}

function exists(rules: Rules, name: string) {
  if (!(name in rules)) {
    throw new PublicodesError(
      "InternalError",
      `La règle "${name}" n'existe pas`,
      {
        dottedName: name,
      },
    );
  }
}

export function isAccessible(rules: Rules, contextName: string, name: string) {
  exists(rules, name);
  const commonAncestor = findCommonAncestor(contextName, name);
  const parents = [name, ...ruleParents(name), ""];
  const checked = parents.slice(
    0,
    Math.max(parents.indexOf(commonAncestor) - 1, 0),
  );
  return checked.every(
    (parent) => !(parent in rules) || rules[parent]?.private === false,
  );
}

export function isExperimental(rules: Rules, name: string) {
  exists(rules, name);
  return [name, ...ruleParents(name)].some(
    (parent) =>
      parent in rules && rules[parent]?.rawNode?.experimental === "oui",
  );
}

const withContext = (context: string, name: string) =>
  context ? `${context} . ${name}` : name;

export function disambiguateReference(
  rules: Rules,
  referencedFrom = "",
  partialName: string,
): string {
  const contexts = ruleParents(referencedFrom);
  contexts.push(referencedFrom);
  let name = partialName;
  if (name.startsWith("^ . ")) {
    const prefix = name.match(/^(\^ \. )+/)?.[0] ?? "";
    name = name.slice(prefix.length);
    contexts.splice(-(prefix.length / 4));
  }
  const root = contexts.pop();
  contexts.unshift(root as string);
  contexts.push("");
  const context = contexts.find((context) => {
    const dottedName = withContext(context, name);
    return (
      dottedName in rules &&
      dottedName !== referencedFrom &&
      isAccessible(rules, referencedFrom, dottedName)
    );
  });
  if (context !== undefined) return withContext(context, name);
  if (referencedFrom.endsWith(name)) return referencedFrom;
  const candidates = contexts.map((context) => withContext(context, name));
  const existing = candidates.find((candidate) => candidate in rules);
  throw new PublicodesError(
    "SyntaxError",
    existing === undefined
      ? `La référence "${name}" est introuvable.\nVérifiez que l'orthographe et l'espace de nom sont corrects`
      : `La règle "${existing}" n'est pas accessible depuis "${referencedFrom}".\n\tCela vient du fait qu'elle est privée ou qu'un de ses parent est privé`,
    { dottedName: contextNameToDottedName(referencedFrom) },
  );
}

export function ruleWithDedicatedDocumentationPage(rule: {
  virtualRule?: boolean;
  type?: string;
}) {
  return (
    rule.virtualRule !== true &&
    rule.type !== "groupe" &&
    rule.type !== "texte" &&
    rule.type !== "paragraphe" &&
    rule.type !== "notification"
  );
}
