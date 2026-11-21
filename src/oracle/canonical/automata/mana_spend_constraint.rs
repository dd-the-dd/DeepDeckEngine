use super::{CharacteristicChoiceBinding, ObjectPredicate};

/// An object predicate, optionally refined by a characteristic chosen for the
/// mana source. Color restrictions reuse `ObjectPredicate::Color` instead of
/// introducing a second color vocabulary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendPredicateNode {
    pub predicate: ObjectPredicate,
    pub binding: Option<CharacteristicChoiceBinding>,
}

/// One atomic use to which a mana restriction may refer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaSpendUsageNode {
    CastSpell { spell: ManaSpendPredicateNode },
}

/// A nonempty, composable mana-spending policy. `alternatives` are joined by
/// Oracle `or` within the same policy; the mandatory first usage prevents an
/// empty restriction from reaching lowering or runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaSpendConstraintNode {
    Only {
        usage: ManaSpendUsageNode,
        alternatives: Vec<ManaSpendUsageNode>,
    },
}

/// A semantic consequence attached to spending the produced mana, kept
/// independent from the set of permitted or prohibited uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ManaSpendConsequenceNode {
    SpellCantBeCountered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ManaSpendClauseNode {
    pub constraint: ManaSpendConstraintNode,
    pub consequences: Vec<ManaSpendConsequenceNode>,
}

#[cfg(test)]
#[path = "mana_spend_constraint_tests.rs"]
mod tests;
