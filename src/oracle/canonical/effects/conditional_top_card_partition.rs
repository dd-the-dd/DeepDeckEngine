use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_conditional_top_cards_partition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let Some(boundary) = tokens.iter().position(|token| token.raw == ".") else {
        return false;
    };
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("look"))
        && tokens
            .get(boundary + 1)
            .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Put, VerbForm::Base))
        && tokens.last().is_some_and(|token| token.raw == ".")
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("rest"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("instead"))
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

fn minimum_selection_count(count: i64) -> Value {
    json!({
        "kind": "minimum",
        "operands": [
            integer(count),
            { "kind": "countObjects", "objects": { "kind": "boundObjects", "binding": "lookedCards" } },
        ],
    })
}

pub(in crate::oracle::canonical) fn parse_promoted_conditional_top_cards_partition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("conditional-top-cards-partition", input.tokens);
    let sequence = recognize_conditional_top_cards_partition_sequence(&mut cursor)?;
    cursor.enter("sequence.conditional-top-cards.validation");
    cursor.require(
        sequence.top.zone == LocalZone::Library
            && sequence.top.owner == TopCardsLibraryOwnerNode::Controller,
        "a controller library source",
    )?;
    cursor.require(
        sequence.default_partition.binding == sequence.top.binding
            && sequence.replacement_partition.binding == sequence.top.binding,
        "partitions of the previously bound collection",
    )?;
    cursor.require(
        sequence.default_partition.chosen_destination == LocalZone::Hand
            && sequence.default_partition.remainder_destination == LocalZone::Library
            && sequence.replacement_partition.chosen_destination == LocalZone::Hand
            && sequence.replacement_partition.remainder_destination == LocalZone::Library,
        "hand selection and library-bottom remainder destinations",
    )?;
    cursor.require(
        sequence.default_partition.order == sequence.replacement_partition.order,
        "a cardinality-only replacement with the same remainder ordering",
    )?;

    let selection_count = json!({
        "kind": "conditionalValue",
        "condition": lower_condition(&sequence.replacement_condition),
        "ifTrue": minimum_selection_count(sequence.replacement_partition.chosen_count),
        "ifFalse": minimum_selection_count(sequence.default_partition.chosen_count),
    });
    let mut effects = vec![json!({
        "kind": "lookAtTopCards",
        "zone": { "kind": "library", "player": controller() },
        "count": integer(sequence.top.count),
        "bind": "lookedCards",
    })];
    effects.extend(lower_top_cards_partition_effects(
        &sequence.default_partition,
        selection_count,
    ));
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    Ok(draft(
        json!({ "kind": "spellAbility", "source": self_ref(), "effects": effects }),
        &[
            "Bind one inspected top-card collection",
            "Evaluate a shared Boolean zone-presence condition",
            "Replace only the primitive selection cardinality",
            "Reuse one partition decision and one remainder move",
            &trace,
        ],
    ))
}
