use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    AddManaEffectNode, BoundObjectConditionNode, CreateEmblemEffectNode, DrawCardsEffectNode,
    LoyaltyCostNode, ReturnTargetCardNode, TargetBinding, TemporaryProtectedAttackTriggerNode,
    recognize_add_mana_effect, recognize_bound_object_color_condition,
    recognize_create_emblem_effect, recognize_draw_cards_effect, recognize_loyalty_cost,
    recognize_return_target_card_from_own_graveyard, recognize_temporary_protected_attack_trigger,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundConditionalEffectNode {
    pub condition: BoundObjectConditionNode,
    pub then_effects: Vec<LoyaltyAtomicEffectNode>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LoyaltyAtomicEffectNode {
    InstallTemporaryTrigger(TemporaryProtectedAttackTriggerNode),
    ReturnTargetCard(ReturnTargetCardNode),
    AddMana(AddManaEffectNode),
    DrawCards(DrawCardsEffectNode),
    CreateEmblem(CreateEmblemEffectNode),
    Conditional(BoundConditionalEffectNode),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LoyaltyAbilityNode {
    pub cost: LoyaltyCostNode,
    pub effects: Vec<LoyaltyAtomicEffectNode>,
}

/// Recognizes the loyalty envelope and delegates every body concept to an
/// independent child. It deliberately contains no card-name or full-sentence
/// branch.
pub(in crate::oracle::canonical) fn recognize_loyalty_ability(
    cursor: &mut OracleTokenCursor<'_>,
    _name_links: &[ResolvedNameLink],
    _name_failure: Option<&NameResolutionFailure>,
) -> MachineResult<LoyaltyAbilityNode> {
    cursor.enter("loyalty-ability.cost");
    let cost = recognize_loyalty_cost(cursor)?;
    cursor.enter("loyalty-ability.separator");
    cursor.consume(":")?;

    let effects = match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
        Some("until") => vec![LoyaltyAtomicEffectNode::InstallTemporaryTrigger(
            recognize_temporary_protected_attack_trigger(cursor)?,
        )],
        Some("return") => {
            let returned = recognize_return_target_card_from_own_graveyard(cursor)?;
            cursor.enter("loyalty-ability.return.sentence-boundary");
            cursor.consume(".")?;
            let binding = TargetBinding {
                effect_index: 0,
                slot_index: 0,
            };
            let condition = recognize_bound_object_color_condition(cursor, binding)?;
            let mana = recognize_add_mana_effect(cursor)?;
            cursor.enter("loyalty-ability.conditional.terminator");
            cursor.consume(".")?;
            vec![
                LoyaltyAtomicEffectNode::ReturnTargetCard(returned),
                LoyaltyAtomicEffectNode::Conditional(BoundConditionalEffectNode {
                    condition,
                    then_effects: vec![LoyaltyAtomicEffectNode::AddMana(mana)],
                }),
            ]
        }
        Some("draw") => {
            let draw = recognize_draw_cards_effect(cursor)?;
            cursor.enter("loyalty-ability.draw.sentence-boundary");
            cursor.consume(".")?;
            let emblem = recognize_create_emblem_effect(cursor)?;
            vec![
                LoyaltyAtomicEffectNode::DrawCards(draw),
                LoyaltyAtomicEffectNode::CreateEmblem(emblem),
            ]
        }
        _ => {
            return Err(cursor.fail(
                "a temporary trigger, targeted card return, or rounded draw loyalty effect",
            ));
        }
    };
    cursor.enter("loyalty-ability.complete");
    cursor.finish()?;
    Ok(LoyaltyAbilityNode { cost, effects })
}

#[cfg(test)]
#[path = "loyalty_ability_tests.rs"]
mod tests;
