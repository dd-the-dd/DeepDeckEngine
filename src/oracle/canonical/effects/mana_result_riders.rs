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
            let spells = |predicate: &ObjectPredicate| {
                lower_spell_object_predicate(predicate).unwrap_or(Value::Null)
            };
            let mut lowered = match restriction.union() {
                // A barred cast use states its spells as its only use.
                ManaSpendUnion::CastSpell(predicate) if restriction.prohibited => {
                    json!({ "kind": "castSpellProhibited", "where": spells(predicate) })
                }
                ManaSpendUnion::CastSpell(predicate) => {
                    json!({ "kind": "castSpell", "where": spells(predicate) })
                }
                // `or activate abilities of` the same objects.
                ManaSpendUnion::CastSpellOrAbilityOfSameObjects(predicate) => {
                    json!({ "kind": "castSpellOrActivateAbility", "where": spells(predicate) })
                }
                // Any other union: the cast use's spells and the ability
                // use's sources (`any` when the ability use names none).
                ManaSpendUnion::Uses {
                    spells: cast,
                    abilities,
                } => {
                    let mut uses = json!({ "kind": "castSpellOrActivateAbilityOf" });
                    if let Some(predicate) = cast {
                        uses["spells"] = spells(predicate);
                    }
                    match abilities {
                        Some(Some(source)) => {
                            uses["abilitySources"] = lower_permanent_object_predicate(source);
                        }
                        Some(None) => uses["abilitySources"] = json!("any"),
                        None => {}
                    }
                    uses
                }
            };
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
