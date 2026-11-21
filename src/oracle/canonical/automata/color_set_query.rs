//! Shared color-set grammar. A color set is neither color identity nor a set
//! of mana types; the mana and characteristic parents consume the same query.
use super::super::{MachineResult, OracleTokenCursor};
use super::{ObjectControllerScope, ObjectKind, ObjectPredicate, recognize_object_collection};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ColorSetQueryNode {
    PermanentsControlledByAbilityController,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ColorSetQueryState {
    Quantifier,
    Color,
    Relation,
    Objects,
    Complete,
}

/// `for each color among permanents you control` leaves punctuation to its
/// caller. The preserved controller is a role, not the current source's owner.
pub(in crate::oracle::canonical) fn recognize_color_set_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ColorSetQueryNode> {
    let mut state = ColorSetQueryState::Quantifier;
    loop {
        state = match state {
            ColorSetQueryState::Quantifier => {
                cursor.enter("query.colors.quantifier");
                cursor.consume("for")?;
                cursor.consume("each")?;
                ColorSetQueryState::Color
            }
            ColorSetQueryState::Color => {
                cursor.enter("query.colors.color-domain");
                let word = cursor.consume_type("colorNoun")?;
                cursor.require(
                    word.raw.eq_ignore_ascii_case("color"),
                    "a color, not a mana type or color identity",
                )?;
                ColorSetQueryState::Relation
            }
            ColorSetQueryState::Relation => {
                cursor.enter("query.colors.relation");
                cursor.consume("among")?;
                ColorSetQueryState::Objects
            }
            ColorSetQueryState::Objects => {
                cursor.enter("query.colors.permanent-domain");
                let objects = recognize_object_collection(cursor)?;
                cursor.require(
                    objects.predicate == ObjectPredicate::ObjectKind(ObjectKind::Permanent)
                        && objects.controller == Some(ObjectControllerScope::AbilityController),
                    "the supported unfiltered permanent set controlled by the ability controller",
                )?;
                ColorSetQueryState::Complete
            }
            ColorSetQueryState::Complete => {
                cursor.enter("query.colors.complete");
                return Ok(ColorSetQueryNode::PermanentsControlledByAbilityController);
            }
        }
    }
}

#[cfg(test)]
#[path = "color_set_query_tests.rs"]
mod tests;
