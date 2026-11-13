use std::collections::BTreeSet;

use serde_json::{Value, json};

use super::GameEvent;

pub(super) const RANDOM_DISCARD_EVENT_KIND: &str = "cardsDiscardedAtRandom";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct RandomDiscardProvenance {
    pub decision_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CorrelatedRandomDiscard {
    pub decision_id: String,
    pub card_ids: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RandomDiscardContractError {
    ProvenanceOnNonDiscardEffect,
    UnsupportedProvenanceKind,
    MissingProvenanceDecisionId,
    CardsAreNotDecisionResult,
    MissingCardsDecisionId,
    DecisionIdMismatch,
}

/// Validates only the consumer-side contract. Ordinary `discardCards`
/// effects have no provenance and remain valid through their existing path.
pub(super) fn validate_random_discard_contract(
    effect: &Value,
) -> Result<Option<RandomDiscardProvenance>, RandomDiscardContractError> {
    let Some(provenance) = effect.get("selectionProvenance") else {
        return Ok(None);
    };
    if effect["kind"].as_str() != Some("discardCards") {
        return Err(RandomDiscardContractError::ProvenanceOnNonDiscardEffect);
    }
    if provenance["kind"].as_str() != Some("random") {
        return Err(RandomDiscardContractError::UnsupportedProvenanceKind);
    }
    let provenance_id = provenance["decisionId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or(RandomDiscardContractError::MissingProvenanceDecisionId)?;
    if effect["cards"]["kind"].as_str() != Some("decisionResult") {
        return Err(RandomDiscardContractError::CardsAreNotDecisionResult);
    }
    let cards_id = effect["cards"]["decisionId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or(RandomDiscardContractError::MissingCardsDecisionId)?;
    if provenance_id != cards_id {
        return Err(RandomDiscardContractError::DecisionIdMismatch);
    }
    Ok(Some(RandomDiscardProvenance {
        decision_id: provenance_id.to_string(),
    }))
}

/// Extends the existing `cardsRandomlySelected` detail with the stable keys
/// needed by a later consuming action. The event remains generic: it does not
/// claim that the selected cards were discarded.
pub(super) fn random_selection_event_detail(
    decision_id: &str,
    stack_id: &str,
    card_ids: &[String],
    minimum: usize,
    maximum: usize,
) -> Value {
    json!({
        "decisionId": decision_id,
        "stackId": stack_id,
        "cardIds": card_ids,
        "minimum": minimum,
        "maximum": maximum,
    })
}

/// Correlates actual hand mutations with their producer event. Merely
/// selecting cards at random is insufficient: at least one selected card must
/// have been discarded by the linked consumer.
pub(super) fn correlate_random_discard(
    events: &[GameEvent],
    stack_id: &str,
    player_id: &str,
    provenance: &RandomDiscardProvenance,
    discarded_card_ids: &[String],
) -> Option<CorrelatedRandomDiscard> {
    if discarded_card_ids.is_empty() {
        return None;
    }
    let unique_discarded = discarded_card_ids
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    if unique_discarded.len() != discarded_card_ids.len() {
        return None;
    }

    let producer = events.iter().rev().find(|event| {
        event.kind == "cardsRandomlySelected"
            && event.player_id.as_deref() == Some(player_id)
            && event.detail["decisionId"].as_str() == Some(provenance.decision_id.as_str())
            && event.detail["stackId"].as_str() == Some(stack_id)
    })?;
    let selected = producer.detail["cardIds"]
        .as_array()?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if !discarded_card_ids
        .iter()
        .all(|card_id| selected.contains(card_id.as_str()))
    {
        return None;
    }

    Some(CorrelatedRandomDiscard {
        decision_id: provenance.decision_id.clone(),
        card_ids: discarded_card_ids.to_vec(),
    })
}

/// Builds the aggregate event detail emitted after the linked discard action
/// succeeds. The caller records it once and then enqueues random-choice
/// triggers once for the discarding player.
pub(super) fn random_discard_event_detail(correlation: &CorrelatedRandomDiscard) -> Value {
    json!({
        "decisionId": &correlation.decision_id,
        "cardIds": &correlation.card_ids,
        "count": correlation.card_ids.len(),
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-1")
))]
#[path = "random_discard_tests.rs"]
mod tests;
