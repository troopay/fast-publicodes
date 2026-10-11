import type { EngineOptions } from "./types.js";

/** The options as `publicodes` resolves them, then reads them as conditions. */
export function resolveOptions(options: EngineOptions) {
  const { strict, warn, flag } = options;
  const strictOptions = {
    situation: true,
    noOrphanRule: true,
    noCycleRuntime: false,
    checkPossibleValues: false,
    ...(typeof strict === "boolean"
      ? {
          situation: strict,
          noOrphanRule: options.allowOrphanRules === true ? false : strict,
          checkPossibleValues: strict,
          noCycleRuntime: strict,
        }
      : typeof strict === "object"
        ? strict
        : {}),
  };
  const warnOptions = {
    cyclicReferences: true,
    experimentalRules: true,
    unitConversion: true,
    deprecatedSyntax: true,
    situationIssues: true,
    ...(typeof warn === "boolean"
      ? {
          cyclicReferences: warn,
          experimentalRules: warn,
          unitConversion: warn,
        }
      : typeof warn === "object"
        ? warn
        : {}),
  };
  const iterations = (options as { inversionMaxIterations?: unknown })
    .inversionMaxIterations;
  return {
    strict: {
      situation: Boolean(strictOptions.situation),
      noOrphanRule: Boolean(strictOptions.noOrphanRule),
      noCycleRuntime: Boolean(strictOptions.noCycleRuntime),
      checkPossibleValues: Boolean(strictOptions.checkPossibleValues),
    },
    warn: {
      cyclicReferences: Boolean(warnOptions.cyclicReferences),
      experimentalRules: Boolean(warnOptions.experimentalRules),
      unitConversion: Boolean(warnOptions.unitConversion),
      deprecatedSyntax: Boolean(warnOptions.deprecatedSyntax),
      situationIssues: Boolean(warnOptions.situationIssues),
    },
    flag: {
      filterNotApplicablePossibilities: Boolean(
        flag?.filterNotApplicablePossibilities ?? false,
      ),
      automaticNamespaceDisabling: Boolean(
        flag?.automaticNamespaceDisabling ?? true,
      ),
    },
    inversionMaxIterations: iterations == null ? null : Number(iterations),
  };
}
