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
        }
    }
}

fn flag(options: &Json, key: &str) -> Option<bool> {
    options.get(key).and_then(Json::as_bool)
}

impl Options {
    /// Reads the options of the `publicodes` `Engine` constructor.
    pub fn from_json(options: &Json) -> Self {
        let mut result = Options::default();
        let strict = &mut result.strict;
        match options.get("strict") {
            Some(Json::Bool(all)) => {
                strict.situation = *all;
                strict.no_orphan_rule = flag(options, "allowOrphanRules") != Some(true) && *all;
                strict.check_possible_values = *all;
                strict.no_cycle_runtime = *all;
            }
            Some(object @ Json::Object(_)) => {
                let read = |key, default| flag(object, key).unwrap_or(default);
                strict.situation = read("situation", strict.situation);
                strict.no_orphan_rule = read("noOrphanRule", strict.no_orphan_rule);
                strict.no_cycle_runtime = read("noCycleRuntime", strict.no_cycle_runtime);
                strict.check_possible_values =
                    read("checkPossibleValues", strict.check_possible_values);
            }
            _ => {}
        }
        let warn = &mut result.warn;
        match options.get("warn") {
            Some(Json::Bool(all)) => {
                warn.experimental_rules = *all;
                warn.unit_conversion = *all;
                warn.cyclic_references = *all;
            }
            Some(object @ Json::Object(_)) => {
                let read = |key, default| flag(object, key).unwrap_or(default);
                warn.experimental_rules = read("experimentalRules", warn.experimental_rules);
                warn.unit_conversion = read("unitConversion", warn.unit_conversion);
                warn.cyclic_references = read("cyclicReferences", warn.cyclic_references);
                warn.deprecated_syntax = read("deprecatedSyntax", warn.deprecated_syntax);
                warn.situation_issues = read("situationIssues", warn.situation_issues);
            }
            _ => {}
        }
        if let Some(flags) = options.get("flag") {
            result.filter_not_applicable_possibilities =
                flag(flags, "filterNotApplicablePossibilities")
                    .unwrap_or(result.filter_not_applicable_possibilities);
            result.automatic_namespace_disabling = flag(flags, "automaticNamespaceDisabling")
                .unwrap_or(result.automatic_namespace_disabling);
        }
        result
    }
}
