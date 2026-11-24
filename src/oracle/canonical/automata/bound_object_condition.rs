use super::super::{MachineResult, OracleTokenCursor};
use super::{
    BoundObjectConditionNode, NumericComparisonOperator, ObjectPredicate, SelectorNumericProperty,
    TargetBinding, card_color_from_oracle_word, recognize_numeric_atom,
};

/// Recognizes `If it's a COLOR card,` while resolving `it` through an object
/// binding owned by the surrounding effect sequence.
pub(in crate::oracle::canonical) fn recognize_bound_object_color_condition(
    cursor: &mut OracleTokenCursor<'_>,
    object: TargetBinding,
) -> MachineResult<BoundObjectConditionNode> {
    cursor.enter("condition.bound-object-color.if");
    cursor.consume("if")?;
    cursor.enter("condition.bound-object-color.anaphor");
    let anaphor = cursor.consume_type("referenceWord")?;
    cursor.require(
        anaphor.raw.eq_ignore_ascii_case("it's") || anaphor.raw.eq_ignore_ascii_case("it’s"),
        "the bound-object anaphor `it's`",
    )?;
    cursor.enter("condition.bound-object-color.article");
    cursor.consume("a")?;
    cursor.enter("condition.bound-object-color.color");
    let color_token = cursor
        .peek_token()
        .cloned()
        .ok_or_else(|| cursor.fail("a Magic color"))?;
    let color = card_color_from_oracle_word(&color_token.raw)
        .ok_or_else(|| cursor.fail("a Magic color"))?;
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("condition.bound-object-color.domain");
    cursor.consume("card")?;
    cursor.enter("condition.bound-object-color.separator");
    cursor.consume(",")?;
    cursor.enter("condition.bound-object-color.complete");
    Ok(BoundObjectConditionNode {
        object,
        predicate: ObjectPredicate::Color(color),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundNumericConditionState {
    If,
    Anaphor,
    Relation,
    Property,
    Amount,
    ComparisonConnector,
    Direction,
    Complete,
}

/// Recognizes a numeric property condition over an object binding owned by a
/// parent effect, for example `if it has mana value 2 or less`.
///
/// The condition deliberately does not alter the target selector: legality is
/// checked when the spell resolves, which differs from `target creature with
/// mana value 2 or less` where the comparison constrains target declaration.
pub(in crate::oracle::canonical) fn recognize_bound_object_numeric_condition(
    cursor: &mut OracleTokenCursor<'_>,
    object: TargetBinding,
) -> MachineResult<BoundObjectConditionNode> {
    let mut state = BoundNumericConditionState::If;
    let mut amount = None;
    let mut operator = None;
    loop {
        state = match state {
            BoundNumericConditionState::If => {
                cursor.enter("condition.bound-object-numeric.if");
                cursor.consume("if")?;
                BoundNumericConditionState::Anaphor
            }
            BoundNumericConditionState::Anaphor => {
                cursor.enter("condition.bound-object-numeric.anaphor");
                // Any other verb phrase of the pronoun (`its power is less
                // than or equal to EXPR`, `it's white`, `it shares a color
                // with ...`) is the shared object predicate of the condition
                // language over the same bound target.
                if super::bound_pronoun_predicate_starts(cursor)
                    && !(cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("it"))
                        && cursor
                            .peek_ahead(1)
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("has"))
                        && cursor
                            .peek_ahead(2)
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("mana")))
                {
                    let predicate = super::recognize_bound_pronoun_predicate(cursor)?;
                    cursor.enter("condition.bound-object-numeric.complete");
                    return Ok(BoundObjectConditionNode { object, predicate });
                }
                let anaphor = cursor.consume_type("referenceWord")?;
                cursor.require(
                    anaphor.raw.eq_ignore_ascii_case("it"),
                    "the bound-object anaphor `it`",
                )?;
                BoundNumericConditionState::Relation
            }
            BoundNumericConditionState::Relation => {
                cursor.enter("condition.bound-object-numeric.relation");
                cursor.consume("has")?;
                BoundNumericConditionState::Property
            }
            BoundNumericConditionState::Property => {
                cursor.enter("condition.bound-object-numeric.property");
                cursor.consume_phrase(&["mana", "value"])?;
                BoundNumericConditionState::Amount
            }
            BoundNumericConditionState::Amount => {
                cursor.enter("condition.bound-object-numeric.amount");
                amount = Some(recognize_numeric_atom(cursor)?);
                BoundNumericConditionState::ComparisonConnector
            }
            BoundNumericConditionState::ComparisonConnector => {
                cursor.enter("condition.bound-object-numeric.comparison-connector");
                cursor.consume("or")?;
                BoundNumericConditionState::Direction
            }
            BoundNumericConditionState::Direction => {
                cursor.enter("condition.bound-object-numeric.direction");
                operator = Some(
                    match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                        Some("less") => {
                            cursor.consume("less")?;
                            NumericComparisonOperator::LessThanOrEqual
                        }
                        Some("greater") => {
                            cursor.consume("greater")?;
                            NumericComparisonOperator::GreaterThanOrEqual
                        }
                        _ => return Err(cursor.fail("`less` or `greater`")),
                    },
                );
                BoundNumericConditionState::Complete
            }
            BoundNumericConditionState::Complete => {
                cursor.enter("condition.bound-object-numeric.complete");
                return Ok(BoundObjectConditionNode {
                    object,
                    predicate: ObjectPredicate::NumericComparison {
                        left: SelectorNumericProperty::ManaValue,
                        operator: cursor
                            .require_value(operator, "a numeric comparison operator")?,
                        right: Box::new(
                            cursor
                                .require_value(amount, "a numeric comparison amount")?
                                .into(),
                        ),
                    },
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "bound_object_condition_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "bound_object_numeric_condition_tests.rs"]
mod numeric_tests;
