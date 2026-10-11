//! Engine options, with the defaults `publicodes` applies.

use serde_json::Value as Json;

#[derive(Debug, Clone, Copy)]
pub struct Strict {
    pub situation: bool,
    pub no_orphan_rule: bool,
    pub no_cycle_runtime: bool,
    pub check_possible_values: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Warn {
    pub experimental_rules: bool,
    pub unit_conversion: bool,
    pub cyclic_references: bool,
    pub deprecated_syntax: bool,
    pub situation_issues: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub strict: Strict,
    pub warn: Warn,
    pub filter_not_applicable_possibilities: bool,
    pub automatic_namespace_disabling: bool,
    /// The iterations of the root search of inversions and circular references, when set.
    pub inversion_max_iterations: Option<f64>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            strict: Strict {
                situation: true,
                no_orphan_rule: true,
                no_cycle_runtime: false,
                check_possible_values: false,
            },
            warn: Warn {
                experimental_rules: true,
                unit_conversion: true,
                cyclic_references: true,
                deprecated_syntax: true,
                situation_issues: true,
            },
            filter_not_applicable_possibilities: false,
            automatic_namespace_disabling: true,
            inversion_max_iterations: None,
        }
    }
}

impl Options {
    /// Reads options resolved as `publicodes` reads them: `strict`, `warn` and `flag` as
    /// booleans, and `inversionMaxIterations`.
    pub fn from_json(options: &Json) -> Self {
        let defaults = Options::default();
        let read = |section: &str, key: &str, default: bool| {
            options
                .get(section)
                .and_then(|section| section.get(key))
                .and_then(Json::as_bool)
                .unwrap_or(default)
        };
        let (strict, warn) = (defaults.strict, defaults.warn);
        Options {
            strict: Strict {
                situation: read("strict", "situation", strict.situation),
                no_orphan_rule: read("strict", "noOrphanRule", strict.no_orphan_rule),
                no_cycle_runtime: read("strict", "noCycleRuntime", strict.no_cycle_runtime),
                check_possible_values: read(
                    "strict",
                    "checkPossibleValues",
                    strict.check_possible_values,
                ),
            },
            warn: Warn {
                experimental_rules: read("warn", "experimentalRules", warn.experimental_rules),
                unit_conversion: read("warn", "unitConversion", warn.unit_conversion),
                cyclic_references: read("warn", "cyclicReferences", warn.cyclic_references),
                deprecated_syntax: read("warn", "deprecatedSyntax", warn.deprecated_syntax),
                situation_issues: read("warn", "situationIssues", warn.situation_issues),
            },
            filter_not_applicable_possibilities: read(
                "flag",
                "filterNotApplicablePossibilities",
                defaults.filter_not_applicable_possibilities,
            ),
            automatic_namespace_disabling: read(
                "flag",
                "automaticNamespaceDisabling",
                defaults.automatic_namespace_disabling,
            ),
            inversion_max_iterations: options.get("inversionMaxIterations").and_then(Json::as_f64),
        }
    }
}
