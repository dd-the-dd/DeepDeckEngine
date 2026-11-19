//! This phase separates hypothetical output from activation legality.
//! It does not replace live discovery or certify a complete runtime context.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum ManaType {
    White,
    Blue,
    Black,
    Red,
    Green,
    Colorless,
}

impl ManaType {
    pub(super) fn from_symbol(symbol: &str) -> Option<Self> {
        match symbol {
            "W" => Some(Self::White),
            "U" => Some(Self::Blue),
            "B" => Some(Self::Black),
            "R" => Some(Self::Red),
            "G" => Some(Self::Green),
            "C" => Some(Self::Colorless),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NoManaReason {
    UndefinedType,
    ZeroAmount,
    NoManaAbility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum ManaTypeProjection {
    NoMana(NoManaReason),
    Types(BTreeSet<ManaType>),
}

impl ManaTypeProjection {
    pub(super) fn from_types(types: BTreeSet<ManaType>, empty_reason: NoManaReason) -> Self {
        if types.is_empty() {
            Self::NoMana(empty_reason)
        } else {
            Self::Types(types)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ManaProjectionUnsupportedState {
    InvalidProducerRule,
    UnsupportedProducerEnvelope,
    StatefulPrefix,
    NestedOrRepeatedProduction,
    RuleResolutionCondition,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ManaProjectionUnsupported {
    pub source_instance_id: String,
    pub rule_index: usize,
    pub state: ManaProjectionUnsupportedState,
}

impl ManaProjectionUnsupported {
    pub(super) fn into_engine_error(self) -> EngineError {
        EngineError::new(format!(
            "unsupported counterfactual mana projection: source={} rule={} state={:?}",
            self.source_instance_id, self.rule_index, self.state,
        ))
    }
}

/// Retains the original IR node and source identity. Private construction
/// proves program shape only; amounts and replacement contexts remain separate.
#[derive(Debug)]
pub(super) struct PureManaProductionProgram<'a> {
    pub source_instance_id: &'a str,
    pub rule_index: usize,
    pub mana: &'a Value,
    pub followups: &'a [Value],
}

pub(super) fn certify_pure_mana_program<'a>(
    source_instance_id: &'a str,
    rule_index: usize,
    rule: &'a Value,
) -> Result<PureManaProductionProgram<'a>, ManaProjectionUnsupported> {
    let fail = |state| ManaProjectionUnsupported {
        source_instance_id: source_instance_id.to_string(),
        rule_index,
        state,
    };
    if value_kind(rule) != Some("manaAbility") {
        return Err(fail(
            ManaProjectionUnsupportedState::UnsupportedProducerEnvelope,
        ));
    }
    if !mana_rule_supported(rule) {
        return Err(fail(ManaProjectionUnsupportedState::InvalidProducerRule));
    }
    if rule.get("condition").is_some() {
        return Err(fail(
            ManaProjectionUnsupportedState::RuleResolutionCondition,
        ));
    }
    let Some(effects) = rule["effects"].as_array() else {
        return Err(fail(ManaProjectionUnsupportedState::InvalidProducerRule));
    };
    let Some(first) = effects.first() else {
        return Err(fail(ManaProjectionUnsupportedState::InvalidProducerRule));
    };
    if value_kind(first) != Some("addMana") {
        return Err(fail(ManaProjectionUnsupportedState::StatefulPrefix));
    }
    if effects[1..]
        .iter()
        .any(|effect| rule_contains_kind(effect, "addMana"))
    {
        return Err(fail(
            ManaProjectionUnsupportedState::NestedOrRepeatedProduction,
        ));
    }
    Ok(PureManaProductionProgram {
        source_instance_id,
        rule_index,
        mana: &first["mana"],
        followups: &effects[1..],
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "mana_projection_contract_tests.rs"]
mod tests;
