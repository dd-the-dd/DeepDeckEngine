use std::collections::BTreeSet;

use regex::Regex;
use serde_json::{Value, json};

use crate::card_catalog::named_token_printing;
use crate::oracle::syntax::strip_short_oracle_label;

mod abilities;
mod conditions;
mod context;
mod costs;
mod criteria;
mod dispatch;
mod effects;
mod grammar;
mod ir;
mod numeric;
mod operations;
mod values;

use abilities::*;
use conditions::*;
pub(crate) use context::CanonicalRuleDraft;
use costs::*;
use criteria::*;
pub(crate) use dispatch::parse_canonical_rule;
use effects::*;
use grammar::*;
use ir::*;
use numeric::*;
use operations::*;
use values::*;
#[cfg(all(
    test,
    any(
        not(feature = "ci-test-partition"),
        feature = "ci-unit-tests-2",
        feature = "ci-unit-tests-3"
    )
))]
#[cfg_attr(feature = "ci-test-partition", allow(dead_code, unused_imports))]
#[path = "tests.rs"]
mod tests;
