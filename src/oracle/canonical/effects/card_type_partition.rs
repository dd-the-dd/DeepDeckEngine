use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

const REVEALED_TOP_CARDS_BINDING: &str = "revealedTopCards";

/// Claims the source-entry top-card partition envelope. The first two anchors
/// distinguish it from ordinary reveal-and-partition sequences while the
/// committed machine owns the category, zones, ordering, and reminder list.
pub(in crate::oracle::canonical) fn claims_promoted_card_type_partition_family(
    input: &AbilityInput<'_>,
) -> bool {
    claims_card_type_partition_tokens(input.tokens)
}

fn claims_card_type_partition_tokens(tokens: &[crate::oracle::model::RecognizedEntity]) -> bool {
    let source_entry = tokens.first().is_some_and(|token| {
        token.token_type == "triggerWord" && token.raw.eq_ignore_ascii_case("when")
    }) && tokens
        .iter()
        .any(|token| token.token_type == "eventWord" && token.raw.eq_ignore_ascii_case("enters"));
    let reveal_top = tokens.windows(3).any(|window| {
        is_verb_form(&window[0].raw, VerbLemma::Reveal, VerbForm::Base)
            && window[1].raw.eq_ignore_ascii_case("the")
            && window[2].raw.eq_ignore_ascii_case("top")
    });
    let per_category = tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("for") && window[1].raw.eq_ignore_ascii_case("each")
    });
    source_entry && reveal_top && per_category
}

fn set_difference(left: Value, right: Value) -> Value {
    json!({ "kind": "setDifference", "left": left, "right": right })
}

fn lower_card_type_choices(card_types: &[ObjectCardType]) -> (Vec<Value>, Value) {
    let controller = controller();
    let mut effects = Vec::new();
    let mut remaining = bound_objects(REVEALED_TOP_CARDS_BINDING);

    for card_type in card_types {
        let decision_id = format!("cardType{}", card_type.ir_name());
        let selected = decision_result(&decision_id);
        effects.push(json!({
            "kind": "chooseCards",
            "id": decision_id,
            "player": controller.clone(),
            "from": remaining.clone(),
            "where": {
                "kind": "cardTypeContains",
                "value": card_type.ir_name(),
            },
            "minimum": integer(0),
            "maximum": integer(1),
        }));
        effects.push(json!({
            "kind": "moveCards",
            "cards": selected.clone(),
            "to": {
                "kind": "hand",
                "player": controller.clone(),
            },
        }));
        remaining = set_difference(remaining, selected);
    }

    (effects, remaining)
}

pub(in crate::oracle::canonical) fn parse_promoted_card_type_partition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("card-type-partition", input.tokens);
    let sequence = recognize_card_type_partition_sequence(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    cursor.enter("trigger.card-type-partition.source-shape");
    cursor.require(
        sequence.source == LocalObjectReference::SourceSelf,
        "the current source entering",
    )?;

    let controller = controller();
    let revealed = bound_objects(REVEALED_TOP_CARDS_BINDING);
    let mut effects = vec![
        json!({
            "kind": "lookAtTopCards",
            "zone": library(controller.clone()),
            "count": integer(sequence.count),
            "bind": REVEALED_TOP_CARDS_BINDING,
        }),
        json!({
            "kind": "revealCards",
            "cards": revealed,
        }),
    ];
    let (choice_effects, remainder) = lower_card_type_choices(&sequence.card_types);
    effects.extend(choice_effects);
    effects.push(json!({
        "kind": "moveCards",
        "cards": remainder,
        "to": {
            "kind": "library",
            "player": controller,
            "position": "bottom",
        },
        "order": { "kind": "random" },
    }));

    let reminder_note = sequence.reminder_card_types.as_ref().map_or_else(
        || "No documentary card-type reminder was present".to_string(),
        |card_types| {
            format!(
                "Recognized {} documentary card types without using the reminder as the rules universe",
                card_types.len()
            )
        },
    );
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "effects": effects,
        }),
        &[
            "Bind and reveal one bounded top-library collection",
            "Use the exhaustive shared Oracle card-type vocabulary",
            "Choose zero or one matching card from the exact remaining set per type",
            "Move each chosen card with the generic hand zone primitive",
            "Move the exact remainder to the library bottom in random order",
            &reminder_note,
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "card_type_partition_tests.rs"]
mod tests;
