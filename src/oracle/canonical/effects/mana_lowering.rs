use super::super::*;

/// Lowers one independently recognized mana-production effect. Ability
/// envelopes remain responsible for deciding whether the effect belongs to a
/// spell, an activated ability, or a mana ability.
pub(in crate::oracle::canonical) fn lower_add_mana_effect(effect: &AddManaEffectNode) -> Value {
    let mana = match effect {
        AddManaEffectNode::Qualified { production, policy } => {
            let mut lowered = lower_add_mana_effect(production);
            // CR 106.6: a permitted use (`only to cast`) or a barred one
            // (`can't be spent to cast`) over the same spell predicate.
            let mut restriction = json!({
                "kind": if policy.prohibited {
                    "castSpellProhibited"
                } else if policy.also_activates_abilities() {
                    // `or activate abilities of` the same objects.
                    "castSpellOrActivateAbility"
                } else {
                    "castSpell"
                },
                "where":lower_spell_object_predicate(&policy.spells).unwrap_or(Value::Null),
            });
            if policy.spell_cant_be_countered {
                restriction["cantBeCountered"] = json!(true);
            }
            lowered["spendRestriction"] = restriction;
            return lowered;
        }
        AddManaEffectNode::Fixed(effect) => json!(effect.mana.as_oracle_cost()),
        AddManaEffectNode::Counted(effect) => json!({
            "kind": "fixedMana",
            "symbol": effect.symbol.to_string(),
            "amount": lower_numeric_expression(&effect.amount)
                .expect("counted mana is built only from engine-evaluable counts"),
        }),
        AddManaEffectNode::Choice(effect) => json!({
            "kind": "chooseOne",
            "options": effect
                .options
                .iter()
                .map(ManaCostNode::as_oracle_cost)
                .collect::<Vec<_>>(),
        }),
        AddManaEffectNode::AnyColor(effect) => json!({
            "kind": effect.runtime_mana_kind(),
            "amount": effect.amount,
        }),
        AddManaEffectNode::AnyColorAmount(effect) => json!({
            "kind": effect.runtime_mana_kind(),
            "amount": lower_numeric_expression(&effect.amount)
                .expect("an expression mana amount is admitted only when it lowers"),
        }),
        AddManaEffectNode::StoredColor(effect) => {
            // The resolution color id stays symbolic; the card-level binding
            // rebinds it to the source's entry choice (CR 614.12).
            let stored = json!({
                "kind": "storedColor",
                "decisionId": crate::oracle::contextual_binding::RESOLUTION_COLOR_DECISION,
                "amount": effect.amount,
            });
            if effect.alternatives.is_empty() {
                stored
            } else {
                let mut options = effect
                    .alternatives
                    .iter()
                    .map(|mana| json!(mana.as_oracle_cost()))
                    .collect::<Vec<_>>();
                options.push(stored);
                json!({ "kind": "chooseOne", "options": options })
            }
        }
        AddManaEffectNode::EachColor(effect) => {
            let mut colors = lower_color_set_query(&effect.colors);
            if effect.amount != 1 {
                colors["amount"] = json!(effect.amount);
            }
            colors
        }
        AddManaEffectNode::ScopedColor(effect) => match effect.domain {
            ManaChoiceDomain::CommanderColorIdentity => json!({
                "kind": "chooseCommanderColor",
                "amount": effect.amount,
            }),
            ManaChoiceDomain::LandTypesControlledByAbilityController => {
                json!({"kind":"manaTypesLandsYouControlCouldProduce","amount":effect.amount})
            }
            // The triggering mana ability's record carries what it produced.
            ManaChoiceDomain::EventProducedTypes => {
                json!({"kind":"manaTypesProducedByEvent","amount":effect.amount})
            }
            ManaChoiceDomain::LandColorsControlledBy(
                ObjectControllerScope::OpponentOfAbilityController,
            ) => json!({
                "kind": "manaColorsLandsOpponentsControlCouldProduce",
                "amount": effect.amount,
            }),
            ManaChoiceDomain::LandColorsControlledBy(ObjectControllerScope::AbilityController) => {
                unreachable!("the land-color-domain automaton currently requires an opponent")
            }
            ManaChoiceDomain::LandColorsControlledBy(
                ObjectControllerScope::NotAbilityController
                | ObjectControllerScope::EventPlayer
                | ObjectControllerScope::Player(_),
            ) => {
                unreachable!(
                    "the land-color-domain automaton does not accept a negative controller scope"
                )
            }
        },
    };

    json!({
        "kind": "addMana",
        "player": controller(),
        "mana": mana,
    })
}

#[cfg(test)]
#[path = "mana_lowering_tests.rs"]
mod tests;
