use super::super::*;

/// Lowers one independently recognized mana-production effect. Ability
/// envelopes remain responsible for deciding whether the effect belongs to a
/// spell, an activated ability, or a mana ability.
pub(in crate::oracle::canonical) fn lower_add_mana_effect(effect: &AddManaEffectNode) -> Value {
    let mana = match effect {
        AddManaEffectNode::Fixed(effect) => json!(effect.mana.as_oracle_cost()),
        AddManaEffectNode::Choice(effect) => json!({
            "kind": "chooseOne",
            "options": effect
                .options
                .iter()
                .map(ManaCostNode::as_oracle_cost)
                .collect::<Vec<_>>(),
        }),
        AddManaEffectNode::AnyColor(effect) => json!({
            "kind": "chooseColor",
            "amount": effect.amount,
        }),
        AddManaEffectNode::ScopedColor(effect) => match effect.domain {
            ManaChoiceDomain::CommanderColorIdentity => json!({
                "kind": "chooseCommanderColor",
                "amount": effect.amount,
            }),
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
                ObjectControllerScope::NotAbilityController,
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
