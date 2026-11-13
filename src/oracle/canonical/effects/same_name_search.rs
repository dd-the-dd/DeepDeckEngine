use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims a choose-target/search envelope from token topology only. Exact
/// target predicates, zones, name references and sequence punctuation remain
/// responsibilities of the committed child automaton.
pub(in crate::oracle::canonical) fn claims_promoted_targeted_same_name_search_family(
    input: &AbilityInput<'_>,
) -> bool {
    let tokens = input.tokens;
    let first_sentence_end = tokens.iter().position(|token| token.raw == ".");
    let same_name_relation = tokens.windows(3).any(|window| {
        window[0].raw.eq_ignore_ascii_case("same")
            && window[1].raw.eq_ignore_ascii_case("name")
            && window[2].raw.eq_ignore_ascii_case("as")
    });
    let owner_reference = tokens.windows(2).any(|window| {
        window[0].raw.eq_ignore_ascii_case("its")
            && (window[1].raw.eq_ignore_ascii_case("owner's")
                || window[1].raw.eq_ignore_ascii_case("owner’s"))
    });

    tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("choose"))
        && tokens
            .get(1)
            .is_some_and(|token| token.token_type == "targetMarker")
        && first_sentence_end
            .and_then(|boundary| tokens.get(boundary + 1))
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("search"))
        && same_name_relation
        && owner_reference
}

pub(in crate::oracle::canonical) fn parse_promoted_targeted_same_name_search(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("targeted-same-name-search", input.tokens);
    let search = recognize_targeted_same_name_search(&mut cursor)?;
    let target_id = "targetNamedCard";
    let target = chosen_target(target_id);
    let excluded = lower_card_object_predicate(&search.excluded_target);
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
                    target_id,
                    json!({
                        "kind": "cards",
                        "zone": { "kind": "anyGraveyard" },
                        "where": not(excluded),
                    }),
                    1,
                    1,
                )],
            },
            "effects": [{
                "kind": "exileNamedCardsFromZones",
                "player": target.clone(),
                "chooser": controller(),
                "nameFromCard": target,
                "zones": ["graveyard", "hand", "library"],
                "all": search.quantity == SameNameSearchQuantity::All,
                "shuffleLibrary": true,
            }],
        }),
        &[
            "Recognize one card target in any graveyard",
            "Delegate the excluded characteristic to ObjectPredicate",
            "Bind the target's owner and card name before searching zones",
            "Search only graveyard, hand, and library of the bound owner",
            "Exile the same-name selection and shuffle that library",
            &trace,
        ],
    ))
}
