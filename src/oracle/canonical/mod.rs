use serde_json::{Value, json};

mod abilities;
mod automata;
mod card_composition;

mod context;

mod dispatch;
mod effects;

mod ir;
mod machine;
#[cfg(test)]
mod machine_tests;
mod numeric;
mod primitive_helpers;

use abilities::*;
use automata::*;
pub(crate) use card_composition::{compose_card_rules, compose_card_rules_native};

pub(crate) use context::CanonicalRuleDraft;

pub(crate) use dispatch::{
    CanonicalParseOutcome, parse_canonical_rule, parse_canonical_rule_native,
};
use effects::*;

use ir::*;
use machine::*;
use numeric::*;
use primitive_helpers::*;

pub(crate) fn compile_named_token_native_rule(oracle: &str) -> Option<Value> {
    automata::compile_named_token_native_rule(oracle)
}

pub(crate) fn compile_named_token_catalog_rules(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> Option<Vec<Value>> {
    automata::compile_named_token_catalog_rules(token_name, type_line, oracle)
}
