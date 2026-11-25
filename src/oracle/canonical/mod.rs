use serde_json::{Value, json};

mod abilities;
mod automata;
mod card_composition;

mod context;

mod dispatch;
mod effects;
mod engine_gap;
mod failure_owner;

mod ir;
mod machine;
#[cfg(test)]
mod machine_tests;
mod numeric;
mod primitive_helpers;

pub(crate) use abilities::keyword_template_expansion::{
    expand_keyword_template_line, keyword_expansion_rules,
};
use abilities::*;
use automata::*;
pub(crate) use card_composition::compose_card_rules_for_request;

pub(crate) use context::CanonicalRuleDraft;

#[cfg(test)]
pub(crate) use dispatch::parse_canonical_rule;
pub(crate) use dispatch::{CanonicalParseOutcome, parse_canonical_rule_native};
use effects::*;

use ir::*;
use machine::*;
use numeric::*;
use primitive_helpers::*;

/// The byte offset of the body of a labeled spell paragraph whose failure
/// is an antecedent the paragraph before it may bind (CR 207.2c, 608.2c).
pub(crate) fn spell_paragraph_continuation(
    tokens: &[crate::oracle::model::RecognizedEntity],
    failure_state: &str,
) -> Option<usize> {
    automata::spell_paragraph_continuation(tokens, failure_state)
}

pub(crate) fn compile_named_token_catalog_rules(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> Option<Vec<Value>> {
    automata::compile_named_token_catalog_rules(token_name, type_line, oracle)
}
