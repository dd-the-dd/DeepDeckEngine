use super::super::*;

/// Lowers one independently recognized mana-production effect. Ability
/// envelopes remain responsible for deciding whether the effect belongs to a
/// spell, an activated ability, or a mana ability.
pub(in crate::oracle::canonical) fn lower_add_mana_effect(effect: &AddManaEffectNode) -> Value {
    let mana = match effect {
        AddManaEffectNode::Qualified { production, policy } => {
            let mut lowered = lower_add_mana_effect(production);
            let mut restriction = json!({
                "kind":"castSpell",
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
                ObjectControllerScope::NotAbilityController | ObjectControllerScope::EventPlayer,
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
