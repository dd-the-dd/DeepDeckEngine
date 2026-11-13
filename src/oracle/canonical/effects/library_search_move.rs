use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims direct controller-owned library searches. This is deliberately
/// structural: selector, result anaphora, reveal, destination, shuffle order,
/// and any later sequence action remain owned by typed child automata and
/// produce committed native failures.
pub(in crate::oracle::canonical) fn claims_promoted_library_search_move_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let prefix = ["search", "your", "library", "for"];
    if !prefix.iter().enumerate().all(|(index, expected)| {
        tokens
            .get(index)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
    }) || !tokens.get(prefix.len()).is_some_and(|token| {
        token.raw.eq_ignore_ascii_case("a") || token.raw.eq_ignore_ascii_case("an")
    }) {
        return false;
    }

    tokens.windows(2).any(|window| {
        window[0].raw == ","
            && matches!(
                window[1].raw.to_ascii_lowercase().as_str(),
                "put" | "reveal" | "then"
            )
    })
}

pub(in crate::oracle::canonical) fn lower_controller_library_search_effects(
    cursor: &mut OracleTokenCursor<'_>,
    search: &LibrarySearchEffectNode,
) -> MachineResult<Vec<Value>> {
    cursor.enter("lower.library-search.player");
    cursor.require(
        search.player == LocalPlayerReference::Controller,
        "the ability controller as the searching player",
    )?;
    cursor.enter("lower.library-search.cardinality");
    cursor.require(
        search.minimum <= search.maximum,
        "a search minimum no greater than its maximum",
    )?;

    let binding = lower_collection_binding(search.binding);
    let player = controller();
    let mut effects = vec![json!({
        "kind": "chooseCards",
        "id": binding.clone(),
        "player": player.clone(),
        "minimum": i64::from(search.minimum),
        "maximum": i64::from(search.maximum),
        "candidates": {
            "kind": "cards",
            "zone": library(player.clone()),
            "where": lower_card_object_predicate(&search.filter),
        },
    })];
    if search.reveal {
        effects.push(json!({
            "kind": "revealCards",
            "cards": decision_result(&binding),
        }));
    }

    let move_effect = json!({
        "kind": "moveCards",
        "cards": decision_result(&binding),
        "to": match search.destination {
            SearchDestination::Battlefield { tapped } => json!({
                "kind": "battlefield",
                "player": player.clone(),
                "tapped": tapped,
            }),
            SearchDestination::Hand => hand(player.clone()),
            SearchDestination::LibraryTop => json!({
                "kind": "library",
                "player": player.clone(),
                "position": "top",
            }),
        },
    });
    let shuffle_effect = match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => json!({
            "kind": "shuffleZone",
            "zone": library(player.clone()),
        }),
        SearchShuffleOrder::ShuffleThenMove => json!({
            "kind": "shuffleZone",
            "zone": library(player.clone()),
            "excludeCards": decision_result(&binding),
        }),
    };
    match search.shuffle_order {
        SearchShuffleOrder::MoveThenShuffle => effects.extend([move_effect, shuffle_effect]),
        SearchShuffleOrder::ShuffleThenMove => effects.extend([shuffle_effect, move_effect]),
    }
    effects.push(json!({
        "kind": "recordLibrarySearch",
        "player": player,
        "cards": decision_result(&binding),
    }));
    Ok(effects)
}

pub(in crate::oracle::canonical) fn parse_promoted_library_search_move(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("library-search-move", input.tokens);
    let sequence =
        recognize_effect_sequence(&mut cursor, input.name_links, input.name_resolution_failure)?;
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");

    let mut rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "effects": lowered.effects,
    });
    if !lowered.decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": lowered.decisions,
        });
    }

    Ok(draft(
        rule,
        &[
            "Recognize one controller-owned library search from the shared token stream",
            "Compose card predicates and Boolean alternatives in the shared selector automaton",
            "Resolve result anaphora, optional reveal, destination, and shuffle order independently",
            "Continue through the ordinary effect sequence for later actions",
            "Lower choose, reveal, move, shuffle, record, and later effects as separate primitives",
            &trace,
        ],
    ))
}

#[cfg(test)]
#[path = "library_search_move_tests.rs"]
mod tests;
