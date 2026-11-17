use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims `[Type]cycling [cost]` keywords. Plain `Cycling` has no type label
/// and is left to its own family.
pub(in crate::oracle::canonical) fn claims_promoted_typecycling_family(
    input: &AbilityInput<'_>,
) -> bool {
    typecycling_label_starts(input.tokens)
}

pub(in crate::oracle::canonical) fn parse_promoted_typecycling(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-typecycling", input.tokens);
    let typecycling = recognize_typecycling_keyword(&mut cursor)?;
    cursor.enter("keyword.typecycling.lowering");
    let where_filter = lower_card_object_predicate(&typecycling.search.filter);
    cursor.require(
        !where_filter.is_null(),
        "a searched-card predicate the runtime card filter can express",
    )?;
    let effects = lower_library_search_effects(&mut cursor, &typecycling.search, controller())?;
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": {
                "kind": "typecycling",
                "cost": {
                    "kind": "payMana",
                    "manaCost": typecycling.cost.as_oracle_cost(),
                },
                "activationZone": "hand",
                "costs": [lower_local_activation_cost(&LocalActivationCostNode::DiscardSelf(
                    DiscardSelfCostNode,
                ))],
                "where": where_filter,
                "effects": effects,
            },
        }),
        &[
            "Recognize the typecycling label as a shared search-card selector",
            "Expand to a hand-zone discard-self activation with a controller library search",
            "Validate the optional reminder against that expansion",
        ],
    ))
}
