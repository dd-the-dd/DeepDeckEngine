use super::super::*;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::syntax::AbilityInput;

/// Claims the generic `draw ..., then put ...` spell envelope. The claim only
/// selects the promoted grammar family; child automata own counts, zones,
/// destination position, ordering, and committed diagnostics.
pub(in crate::oracle::canonical) fn claims_promoted_draw_move_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("draw"))
        && tokens.windows(3).any(|window| {
            window[0].raw == ","
                && window[1].raw.eq_ignore_ascii_case("then")
                && is_verb_form(&window[2].raw, VerbLemma::Put, VerbForm::Base)
        })
        && tokens.last().is_some_and(|token| token.raw == ".")
}

fn controller_zone(zone: LocalZone, player: Value) -> Value {
    match zone {
        LocalZone::Hand => hand(player),
        LocalZone::Library => library(player),
    }
}

fn library_position(position: LibraryPosition) -> &'static str {
    match position {
        LibraryPosition::Top => "top",
        LibraryPosition::Bottom => "bottom",
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_draw_move(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("effect-sequence", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    cursor.enter("sequence.draw-then-move.validation");
    cursor.require(
        sequence.connectors == [EffectConnector::Then],
        "an ordered `then` effect connector",
    )?;
    let [
        LocalEffectNode::DrawCards(draw),
        LocalEffectNode::MoveCards(move_cards),
    ] = sequence.effects.as_slice()
    else {
        return Err(cursor.fail("a draw effect followed by a move-cards effect"));
    };
    cursor.require(
        draw.player == move_cards.player,
        "one player shared by draw, selection, and movement",
    )?;
    cursor.require(
        move_cards.source == LocalZone::Hand && move_cards.destination == LocalZone::Library,
        "a hand-to-library move",
    )?;
    cursor.require(
        move_cards.position == LibraryPosition::Top,
        "a top-of-library destination",
    )?;
    cursor.require(
        move_cards.order == CollectionOrder::Chosen,
        "a controller-chosen collection order",
    )?;

    let player = match draw.player {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOfQualified(_) | LocalPlayerReference::Bound(_) => {
            return Err(cursor.fail("an unqualified draw-move actor"));
        }
        LocalPlayerReference::ControllerOf(_) => {
            unreachable!("draw-move grammar has no target-player antecedent")
        }
    };
    let draw_count = lower_numeric_expression(&draw.count)
        .expect("draw-move grammar has no target-property binding");
    let move_count = lower_numeric_atom(move_cards.count.clone());
    let source = controller_zone(move_cards.source, player.clone());
    let mut destination = controller_zone(move_cards.destination, player.clone());
    destination["position"] = Value::String(library_position(move_cards.position).to_string());
    let uses_x = matches!(&draw.count, NumericExpression::Atom(NumericAtom::VariableX))
        || numeric_atom_uses_x(&move_cards.count);

    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": [
            {
                "kind": "drawCards",
                "player": player.clone(),
                "count": draw_count,
            },
            {
                "kind": "chooseCards",
                "id": "cardsForLibrary",
                "player": player.clone(),
                "minimum": move_count.clone(),
                "maximum": move_count,
                "candidates": {
                    "kind": "cards",
                    "zone": source,
                    "where": Value::Null,
                },
            },
            {
                "kind": "chooseOrder",
                "id": "cardsForLibraryOrder",
                "player": player.clone(),
                "objects": decision_result("cardsForLibrary"),
            },
            {
                "kind": "moveCards",
                "cards": decision_result("cardsForLibraryOrder"),
                "to": destination,
                "order": {
                    "kind": "decisionOrder",
                    "decisionId": "cardsForLibraryOrder",
                },
            },
        ],
    });
    if uses_x {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [x_value()],
        });
    }

    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize independent draw and move effects from one token stream",
            "Resolve numeric atoms, controller zones, library position, and collection order through child automata",
            "Lower selection, ordering, and movement to independent primitive IR operations",
            &trace,
        ],
    ))
}
