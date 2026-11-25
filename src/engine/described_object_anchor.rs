//! Relation anchors a definite description names (`the sacrificed
//! creature`, `the discarded card`, `the exiled card`), read through the
//! records their producers already keep:
//!
//! - a cost-paid object (`decisionResult` of `sacrificedPermanentObject` or
//!   `discardedCardObject`): the record its paying cost wrote on the stack
//!   object when it was paid (`object_condition.rs`, `bindObjectAs`), read
//!   as last known information (CR 601.2h, 602.2b, 608.2h). A paid cost
//!   always records it; a missing record is an error, never an empty set;
//! - a linked exile (`cardsExiledWithSource`): the cards still in exile that
//!   the source's linked ability exiled (CR 607.2a), through the one
//!   linked-exile accessor (`cards_exiled_with`). An empty set is a real
//!   state (nothing was exiled): nothing then shares a characteristic with
//!   it.
//!
//! A static source (a cast prohibition) binds only its linked exile; a
//! cost record exists only for a resolving ability.
use super::object_predicate::DefinitionView;
use super::object_relation_eval::{filter_has_relation, is_relation_kind, relation_atom_supported};
use super::runtime_object_filter::ObjectFilterView;
use super::*;

/// The cost-paid object record an anchor names.
fn cost_paid_anchor_decision(anchor: &Value) -> Option<&str> {
    (value_kind(anchor) == Some("decisionResult"))
        .then(|| anchor["decisionId"].as_str())
        .flatten()
        .filter(|id| super::object_condition::COST_PAID_OBJECT_DECISIONS.contains(id))
}

/// The cards a linked exile of the source recorded (CR 607.2a): the one
/// exile card composition paired with the reader, or every exile of the
/// object.
fn linked_exile_anchor(anchor: &Value) -> bool {
    value_kind(anchor) == Some("cardsExiledWithSource")
        && matches!(anchor["link"].as_str(), Some("pair" | "face"))
}

/// Whether a relation anchor is one this module resolves.
pub(super) fn described_anchor_supported(anchor: &Value) -> bool {
    cost_paid_anchor_decision(anchor).is_some() || linked_exile_anchor(anchor)
}

/// The card a cost record holds (`costPaidObject`).
fn recorded_cost_card(record: &Value) -> Result<CardInstance, EngineError> {
    let malformed = || EngineError::new("a malformed cost-paid object record");
    if value_kind(record) != Some("costPaidObject") {
        return Err(malformed());
    }
    serde_json::from_value(record["card"].clone()).map_err(|_| malformed())
}

/// An anchor a static source binds alone: the source itself, the
/// permanent it is attached to, or its linked exile.
fn static_anchor_supported(anchor: &Value) -> bool {
    matches!(value_kind(anchor), Some("self" | "abilitySource"))
        || (value_kind(anchor) == Some("attachedPermanent")
            && value_kind(&anchor["attachment"]) == Some("self"))
        || linked_exile_anchor(anchor)
}

/// A spell filter of a static modifier (`spells with the same name as the
/// exiled card`): the printed card filter, whose relation leaves are shares
/// of a characteristic with an anchor the static source binds alone. A
/// candidate spell is a card definition, which has no battlefield relation
/// (attachment, block) to test.
pub(super) fn static_spell_filter_supported(filter: &Value) -> bool {
    if !filter_has_relation(filter) {
        return card_filter_supported(filter);
    }
    match value_kind(filter) {
        Some(kind) if is_relation_kind(kind) => {
            kind == "sharesCharacteristic"
                && relation_atom_supported(filter)
                && static_anchor_supported(&filter["object"])
        }
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|operands| {
            !operands.is_empty() && operands.iter().all(static_spell_filter_supported)
        }),
        Some("not") => static_spell_filter_supported(&filter["operand"]),
        _ => card_filter_supported(filter),
    }
}

impl GameEngine {
    /// The cards a described anchor names for the resolving ability
    /// `source`. `None` when the anchor is not a described one (the caller
    /// keeps its own anchor reader).
    pub(super) fn described_anchor_cards_for_stack(
        &self,
        anchor: &Value,
        source: &StackObject,
    ) -> Option<Result<Vec<CardInstance>, EngineError>> {
        if let Some(decision) = cost_paid_anchor_decision(anchor) {
            return Some(
                source
                    .decisions
                    .get(decision)
                    .ok_or_else(|| {
                        EngineError::new("the cost-paid object its paying cost did not record")
                    })
                    .and_then(recorded_cost_card)
                    .map(|card| vec![card]),
            );
        }
        linked_exile_anchor(anchor).then(|| {
            Ok(self
                .cards_exiled_with(&source.card)
                .into_iter()
                .cloned()
                .collect())
        })
    }

    /// The cards a described anchor names for a static source: only its
    /// linked exile. `None` for any other anchor.
    pub(super) fn described_anchor_cards_for_source(
        &self,
        anchor: &Value,
        source: &CardInstance,
    ) -> Option<Vec<CardInstance>> {
        linked_exile_anchor(anchor).then(|| {
            self.cards_exiled_with(source)
                .into_iter()
                .cloned()
                .collect()
        })
    }

    /// Whether a spell `card` matches the spell filter of the static
    /// modifier of `source` (`static_spell_filter_supported`): its relation
    /// leaves are prepared against the source, every other leaf is the
    /// printed card filter. An anchor the source cannot bind matches no
    /// spell.
    pub(super) fn static_spell_filter_matches(
        &self,
        source: &CardInstance,
        card: &CardDefinition,
        filter: &Value,
    ) -> bool {
        if !filter_has_relation(filter) {
            return matches_card_filter(card, filter);
        }
        self.prepare_relations_for_source(filter, Some(source))
            .is_some_and(|prepared| self.definition_matches_prepared_filter(card, &prepared))
    }

    fn definition_matches_prepared_filter(&self, card: &CardDefinition, filter: &Value) -> bool {
        match value_kind(filter) {
            Some(kind) if is_relation_kind(kind) => self.eval_object_relation(
                filter,
                ObjectFilterView::Definition(DefinitionView::new(card)),
            ),
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .all(|operand| self.definition_matches_prepared_filter(card, operand))
            }),
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .any(|operand| self.definition_matches_prepared_filter(card, operand))
            }),
            Some("not") => !self.definition_matches_prepared_filter(card, &filter["operand"]),
            _ => matches_card_filter(card, filter),
        }
    }
}
