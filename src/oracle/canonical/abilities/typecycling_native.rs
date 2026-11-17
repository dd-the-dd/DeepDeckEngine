use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// Claims the cycling keyword family: `Cycling [mana cost]` (CR 702.29a) and
/// `[Type]cycling [cost]` (CR 702.29e).
pub(in crate::oracle::canonical) fn claims_promoted_typecycling_family(
    input: &AbilityInput<'_>,
) -> bool {
    typecycling_label_starts(input.tokens) || plain_cycling_starts(input.tokens)
}

pub(in crate::oracle::canonical) fn parse_promoted_typecycling(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-typecycling", input.tokens);
    let cycling = recognize_typecycling_keyword(&mut cursor)?;
    let cost = json!({
        "kind": "payMana",
        "manaCost": cycling.cost.as_oracle_cost(),
    });
    let discard_self =
        lower_local_activation_cost(&LocalActivationCostNode::DiscardSelf(DiscardSelfCostNode));
    match &cycling.expansion {
        CyclingExpansion::Draw(draw) => {
            cursor.enter("keyword.cycling.lowering");
            let count = lower_numeric_expression(&draw.count)
                .ok_or_else(|| cursor.fail("a lowerable cycling draw count"))?;
            Ok(draft(
                json!({
                    "kind": "keywordAbility",
                    "source": self_ref(),
                    "ability": {
                        "kind": "cycling",
                        "cost": cost,
                        "activationZone": "hand",
                        "costs": [discard_self],
                        "effects": [{
                            "kind": "drawCards",
                            "player": controller(),
                            "count": count,
                        }],
                    },
                }),
                &[
                    "Recognize the cycling keyword and its shared mana cost",
                    "Expand to a hand-zone discard-self activation in which the controller draws one card",
                    "Validate the optional reminder against that expansion",
                ],
            ))
        }
        CyclingExpansion::Search(search) => {
            cursor.enter("keyword.typecycling.lowering");
            let where_filter = lower_card_object_predicate(&search.filter);
            cursor.require(
                !where_filter.is_null(),
                "a searched-card predicate the runtime card filter can express",
            )?;
            let effects = lower_library_search_effects(&mut cursor, search, controller())?;
            Ok(draft(
                json!({
                    "kind": "keywordAbility",
                    "source": self_ref(),
                    "ability": {
                        "kind": "typecycling",
                        "cost": cost,
                        "activationZone": "hand",
                        "costs": [discard_self],
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
    }
}
