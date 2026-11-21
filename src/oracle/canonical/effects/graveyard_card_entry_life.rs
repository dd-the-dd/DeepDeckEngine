use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

const TARGET_GRAVEYARD_CARD_ID: &str = "targetGraveyardCard";
const TARGET_MANA_VALUE_BINDING: &str = "targetManaValue";

/// Broad token-shape boundary for a targeted card moving from the shared
/// graveyard domain followed by a controller life-loss sentence. The child
/// automata validate every selector, relation, reference, and property.
pub(in crate::oracle::canonical) fn claims_promoted_graveyard_card_entry_life_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| is_verb_form(&token.raw, VerbLemma::Put, VerbForm::Base))
        && tokens
            .get(1)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("target"))
        && tokens
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("graveyard"))
        && tokens.windows(3).any(|window| {
            window[0].raw == "."
                && window[1].raw.eq_ignore_ascii_case("you")
                && is_verb_form(&window[2].raw, VerbLemma::Lose, VerbForm::Base)
        })
}

pub(in crate::oracle::canonical) fn parse_promoted_graveyard_card_entry_life(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("graveyard-card-entry-life", input.tokens);
    let sequence = recognize_graveyard_card_entry_life(&mut cursor)?;
    cursor.enter("lower.graveyard-card-entry-life.validation");
    cursor.require(
        sequence.entry.source == GraveyardCardSourceNode::AnyGraveyard,
        "the shared graveyard card domain",
    )?;
    cursor.require(
        sequence.entry.destination == CardEntryDestinationNode::Battlefield,
        "the battlefield destination",
    )?;
    cursor.require(
        sequence.entry.controller == LocalPlayerReference::Controller,
        "the ability controller as destination controller",
    )?;
    cursor.require(
        sequence.life.player == LocalPlayerReference::Controller,
        "the ability controller as life-loss player",
    )?;
    cursor.require(
        sequence
            .life
            .amount
            .is_target_mana_value(sequence.target_binding),
        "the target mana-value binding",
    )?;

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    TARGET_GRAVEYARD_CARD_ID,
                    json!({
                        "kind": "cards",
                        "zone": { "kind": "anyGraveyard" },
                        "where": lower_card_object_predicate(&sequence.entry.predicate),
                    }),
                    1,
                    1,
                )],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": TARGET_MANA_VALUE_BINDING,
                    "value": {
                        "kind": "manaValueOf",
                        "object": chosen_target(TARGET_GRAVEYARD_CARD_ID),
                    },
                },
                {
                    "kind": "moveCards",
                    "cards": {
                        "kind": "chosenTargets",
                        "id": TARGET_GRAVEYARD_CARD_ID,
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": controller(),
                        "tapped": false,
                    },
                },
                {
                    "kind": "loseLife",
                    "player": controller(),
                    "amount": {
                        "kind": "boundValue",
                        "id": TARGET_MANA_VALUE_BINDING,
                    },
                },
            ],
        }),
        &[
            "Recognize the target card selector and any-graveyard source with local automata",
            "Resolve the later mana-value anaphor to the target binding",
            "Capture mana value before changing the target's zone",
            "Move the card under the ability controller without changing ownership",
            "Lose the captured amount of life as an independent primitive",
            &trace,
        ],
    ))
}
