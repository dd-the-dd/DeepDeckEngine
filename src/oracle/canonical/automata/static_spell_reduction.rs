use super::super::{MachineResult, OracleTokenCursor};
use super::{
    SpellCastQueryNode, SpellCasterScope, recognize_mana_cost, recognize_spell_cast_collection,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct StaticSpellReductionNode {
    pub spells: SpellCastQueryNode,
    pub amount: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StaticSpellReductionState {
    Quantifier,
    Spells,
    CasterScope,
    CostAction,
    Cost,
    Reduction,
    Terminator,
    Complete,
}

#[derive(Default)]
struct StaticSpellReductionFrame {
    quantified_each: bool,
    spells: Option<SpellCastQueryNode>,
    amount: Option<i64>,
}

/// Recognizes one fixed generic reduction for a controller-scoped spell set.
/// Object-query children own all type/subtype/color/keyword/Boolean grammar;
/// this parent owns only the quantifier, fixed reduction, and sentence envelope.
pub(in crate::oracle::canonical) fn recognize_static_spell_reduction(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<StaticSpellReductionNode> {
    let mut state = StaticSpellReductionState::Quantifier;
    let mut frame = StaticSpellReductionFrame::default();

    loop {
        state = match state {
            StaticSpellReductionState::Quantifier => {
                cursor.enter("static.spell-cost-reduction.quantifier");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
                {
                    cursor.consume("each")?;
                    frame.quantified_each = true;
                }
                StaticSpellReductionState::Spells
            }
            StaticSpellReductionState::Spells => {
                cursor.enter("static.spell-cost-reduction.spells");
                frame.spells = Some(recognize_spell_cast_collection(cursor)?);
                StaticSpellReductionState::CostAction
            }
            StaticSpellReductionState::CostAction => {
                cursor.enter("static.spell-cost-reduction.cost-action");
                cursor.consume(if frame.quantified_each {
                    "costs"
                } else {
                    "cost"
                })?;
                StaticSpellReductionState::CasterScope
            }
            StaticSpellReductionState::CasterScope => {
                cursor.enter("static.spell-cost-reduction.caster-scope");
                let query = frame
                    .spells
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated spell collection"))?;
                cursor.require(
                    query.caster == Some(SpellCasterScope::AbilityController),
                    "spells cast by the ability controller",
                )?;
                StaticSpellReductionState::Cost
            }
            StaticSpellReductionState::Cost => {
                cursor.enter("static.spell-cost-reduction.cost");
                let cost = recognize_mana_cost(cursor)?;
                // The child owns the mana-symbol syntax, but this parent owns
                // the semantic restriction to one fixed positive generic
                // amount. Restore the parent state before reporting that
                // restriction so diagnostics identify the committed family.
                cursor.enter("static.spell-cost-reduction.cost");
                frame.amount = cost
                    .fixed_generic_amount()
                    .filter(|amount| *amount > 0)
                    .map(i64::from);
                cursor.require(
                    frame.amount.is_some(),
                    "one positive generic mana symbol without {X} or colored mana",
                )?;
                StaticSpellReductionState::Reduction
            }
            StaticSpellReductionState::Reduction => {
                cursor.enter("static.spell-cost-reduction.reduction");
                cursor.consume_phrase(&["less", "to", "cast"])?;
                StaticSpellReductionState::Terminator
            }
            StaticSpellReductionState::Terminator => {
                cursor.enter("static.spell-cost-reduction.terminator");
                cursor.consume(".")?;
                StaticSpellReductionState::Complete
            }
            StaticSpellReductionState::Complete => {
                cursor.enter("static.spell-cost-reduction.complete");
                cursor.finish()?;
                return Ok(StaticSpellReductionNode {
                    spells: cursor.require_value(frame.spells, "a populated spell collection")?,
                    amount: cursor.require_value(frame.amount, "a positive generic reduction")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "static_spell_reduction_tests.rs"]
mod tests;
