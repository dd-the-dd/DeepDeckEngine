//! Lowering of the typed riders of one mana result: the spending
//! restriction (CR 106.6) and the per-unit retention (CR 106.4) are
//! attributes of the produced mana units.
use super::super::*;

/// The produced units' attributes. A restriction names its use (`kind`);
/// riders without one carry only unit attributes and restrict no use.
pub(in crate::oracle::canonical) fn lower_mana_result_riders(
    policy: &ManaSpendingPolicyNode,
) -> Value {
    let mut attributes = match &policy.restriction {
        // CR 106.6: a permitted use (`only to cast`) or a barred one
        // (`can't be spent to cast`) over the same spell predicate.
        Some(restriction) => {
            let mut lowered = json!({
                "kind": if restriction.prohibited {
                    "castSpellProhibited"
                } else if restriction.also_activates_abilities() {
                    // `or activate abilities of` the same objects.
                    "castSpellOrActivateAbility"
                } else {
                    "castSpell"
                },
                "where": lower_spell_object_predicate(&restriction.spells).unwrap_or(Value::Null),
            });
            if restriction.spell_cant_be_countered {
                lowered["cantBeCountered"] = json!(true);
            }
            lowered
        }
        None => json!({}),
    };
    if policy.retained_until_end_of_turn {
        attributes["retainedUntilEndOfTurn"] = json!(true);
    }
    attributes
}
