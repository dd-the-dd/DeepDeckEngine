use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::card_catalog::named_token_printing;
use crate::oracle::regex_cache::Regex;
use crate::oracle::syntax::strip_short_oracle_label;

mod abilities;
mod automata;
mod card_composition;
mod conditions;
mod context;
mod costs;
mod criteria;
mod dispatch;
mod effects;
mod grammar;
mod ir;
mod machine;
#[cfg(test)]
mod machine_tests;
mod numeric;
mod operations;
mod values;

use abilities::*;
use automata::*;
pub(crate) use card_composition::{compose_card_rules, compose_card_rules_native};
use conditions::*;
pub(crate) use context::CanonicalRuleDraft;
use costs::*;
use criteria::*;
pub(crate) use dispatch::{
    CanonicalParseOutcome, parse_canonical_rule, parse_canonical_rule_native,
};
use effects::*;
use grammar::*;
use ir::*;
use machine::*;
use numeric::*;
use operations::*;
use values::*;

pub(crate) fn compile_named_token_native_rule(oracle: &str) -> Option<Value> {
    automata::compile_named_token_native_rule(oracle)
}
#[cfg(all(
    test,
    not(feature = "parser-standalone"),
    any(
        not(feature = "ci-test-partition"),
        feature = "ci-unit-tests-2",
        feature = "ci-unit-tests-3"
    )
))]
#[cfg_attr(feature = "ci-test-partition", allow(dead_code, unused_imports))]
#[path = "tests.rs"]
mod tests;
