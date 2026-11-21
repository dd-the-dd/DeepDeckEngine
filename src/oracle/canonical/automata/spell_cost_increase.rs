use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ManaCostNode, SpellCastQueryNode, SpellCasterScope, recognize_mana_cost,
    recognize_spell_cast_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SpellCostIncreaseNode {
    pub spells: SpellCastQueryNode,
    pub cost: ManaCostNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpellCostIncreaseState {
    Spells,
    CasterScope,
    CostAction,
    Cost,
    Increase,
    CastAction,
    Terminator,
    Complete,
}

/// Recognizes a fixed additional casting cost applied to a typed collection
/// of spells. The object collection child owns card characteristics, Boolean
/// composition, the spell domain, and the optional caster relation. The mana
/// child owns the complete cost atom. This parent owns only the cost-increase
/// envelope and its punctuation.
pub(in crate::oracle::canonical) fn recognize_spell_cost_increase(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<SpellCostIncreaseNode> {
    let mut state = SpellCostIncreaseState::Spells;
    let mut spells = None;
    let mut cost = None;

    loop {
        state = match state {
            SpellCostIncreaseState::Spells => {
                cursor.enter("static.spell-cost-increase.spells");
                spells = Some(recognize_spell_cast_collection(cursor)?);
                SpellCostIncreaseState::CasterScope
            }
            SpellCostIncreaseState::CasterScope => {
                cursor.enter("static.spell-cost-increase.caster-scope");
                let query = spells
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated spell collection"))?;
                cursor.require(
                    !matches!(query.caster, Some(SpellCasterScope::NotAbilityController)),
                    "all players, the ability controller, or opponents of the ability controller",
                )?;
                SpellCostIncreaseState::CostAction
            }
            SpellCostIncreaseState::CostAction => {
                cursor.enter("static.spell-cost-increase.cost-action");
                cursor.consume("cost")?;
                SpellCostIncreaseState::Cost
            }
            SpellCostIncreaseState::Cost => {
                cursor.enter("static.spell-cost-increase.cost");
                let parsed = recognize_mana_cost(cursor)?;
                cursor.require(
                    !parsed.contains_variable_x(),
                    "a fixed additional mana cost without {X}",
                )?;
                cost = Some(parsed);
                SpellCostIncreaseState::Increase
            }
            SpellCostIncreaseState::Increase => {
                cursor.enter("static.spell-cost-increase.increase");
                cursor.consume("more")?;
                SpellCostIncreaseState::CastAction
            }
            SpellCostIncreaseState::CastAction => {
                cursor.enter("static.spell-cost-increase.cast-action");
                cursor.consume_phrase(&["to", "cast"])?;
                SpellCostIncreaseState::Terminator
            }
            SpellCostIncreaseState::Terminator => {
                cursor.enter("static.spell-cost-increase.terminator");
                cursor.consume(".")?;
                SpellCostIncreaseState::Complete
            }
            SpellCostIncreaseState::Complete => {
                cursor.enter("static.spell-cost-increase.complete");
                cursor.finish()?;
                return Ok(SpellCostIncreaseNode {
                    spells: cursor.require_value(spells, "a populated spell collection")?,
                    cost: cursor.require_value(cost, "a populated additional mana cost")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "spell_cost_increase_tests.rs"]
mod tests;
