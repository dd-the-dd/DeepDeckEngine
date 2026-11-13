use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the searched-collection partition family from stable structural
/// markers. The child machines own every word after the claim, so malformed
/// anaphora, destinations, connectors, and terminators remain native failures.
pub(in crate::oracle::canonical) fn claims_promoted_library_search_partition_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let prefix = ["search", "your", "library", "for", "up", "to"];
    if !prefix.iter().enumerate().all(|(index, expected)| {
        tokens
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    }) {
        return false;
    }

    let reveal = tokens
        .iter()
        .position(|token| token.raw.eq_ignore_ascii_case("reveal"));
    let put_one = tokens.windows(2).position(|window| {
        window[0].raw.eq_ignore_ascii_case("put") && window[1].raw.eq_ignore_ascii_case("one")
    });
    let and_the = tokens.windows(2).position(|window| {
        window[0].raw.eq_ignore_ascii_case("and") && window[1].raw.eq_ignore_ascii_case("the")
    });
    matches!((reveal, put_one, and_the), (Some(a), Some(b), Some(c)) if a < b && b < c)
}

fn lower_collection_player(reference: &LocalPlayerReference) -> Value {
    match reference {
        LocalPlayerReference::Controller => controller(),
        LocalPlayerReference::ControllerOf(_) => {
            unreachable!("this grammar currently recognizes controller-owned searches")
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_library_search_partition(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("library-search-partition", input.tokens);
    let sequence = recognize_library_search_partition_sequence(&mut cursor)?;
    let player = lower_collection_player(&sequence.search.player);

    cursor.enter("library-search-partition.binding-validation");
    cursor.require(
        sequence.reveal.binding == sequence.search.binding
            && sequence.partition.binding == sequence.search.binding,
        "one collection binding shared by search, reveal, and partition",
    )?;
    cursor.require(
        sequence.shuffle.player == sequence.search.player
            && sequence.shuffle.zone == sequence.search.zone,
        "the searched library as the shuffled zone",
    )?;
    cursor.require(
        sequence.partition.selected_count == 1,
        "one selected card before the remainder",
    )?;
    let CollectionDestination::Battlefield { tapped } = sequence.partition.selected_destination
    else {
        return Err(cursor.fail("the battlefield as the selected-card destination"));
    };
    cursor.require(
        sequence.partition.remainder_destination == CollectionDestination::Hand,
        "the controller's hand as the remainder destination",
    )?;

    let searched = "searchedCards";
    let selected = "battlefieldCard";
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
            "effects": [
                {
                    "kind": "chooseCards",
                    "id": searched,
                    "player": player.clone(),
                    "minimum": i64::from(sequence.search.minimum),
                    "maximum": i64::from(sequence.search.maximum),
                    "candidates": {
                        "kind": "cards",
                        "zone": library(player.clone()),
                        "where": lower_card_object_predicate(&sequence.search.filter),
                    },
                },
                {
                    "kind": "revealCards",
                    "cards": decision_result(searched),
                },
                {
                    "kind": "chooseCards",
                    "id": selected,
                    "player": player.clone(),
                    "from": decision_result(searched),
                    "count": integer(i64::from(sequence.partition.selected_count)),
                },
                {
                    "kind": "moveCards",
                    "cards": decision_result(selected),
                    "to": {
                        "kind": "battlefield",
                        "player": player.clone(),
                        "tapped": tapped,
                    },
                },
                {
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": decision_result(searched),
                        "right": decision_result(selected),
                    },
                    "to": hand(player.clone()),
                },
                {
                    "kind": "shuffleZone",
                    "zone": library(player.clone()),
                },
                {
                    "kind": "recordLibrarySearch",
                    "player": player,
                    "cards": decision_result(searched),
                },
            ],
        }),
        &[
            "Recognize one searched collection from the shared token stream",
            "Resolve reveal and partition anaphora through one typed binding",
            "Keep partition `and` local instead of treating it as an effect connector",
            "Lower search, reveal, selection, moves, shuffle, and search record independently",
            &trace,
        ],
    ))
}
