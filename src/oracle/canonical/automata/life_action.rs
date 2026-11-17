//! Subject-independent life action tail shared by singular and quantified players.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericExpression, TargetBinding, TargetPlayerLifeAction, recognize_numeric_expression,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LifeVerbForm {
    Base,
    ThirdPersonSingular,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LifeActionNode {
    pub action: TargetPlayerLifeAction,
    pub amount: NumericExpression,
}

pub(in crate::oracle::canonical) fn recognize_life_action(
    cursor: &mut OracleTokenCursor<'_>,
    form: LifeVerbForm,
    antecedent: Option<TargetBinding>,
) -> MachineResult<LifeActionNode> {
    cursor.enter("effect.life.action");
    let verb = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("a life-total action"))?;
    let action = match (form, verb.as_str()) {
        (LifeVerbForm::Base, "gain") | (LifeVerbForm::ThirdPersonSingular, "gains") => {
            TargetPlayerLifeAction::Gain
        }
        (LifeVerbForm::Base, "lose") | (LifeVerbForm::ThirdPersonSingular, "loses") => {
            TargetPlayerLifeAction::Lose
        }
        _ => return Err(cursor.fail("`gain`, `gains`, `lose`, or `loses`")),
    };
    cursor.consume(&verb)?;
    cursor.enter("effect.life.amount-or-resource");
    let amount = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("life"))
    {
        cursor.consume("life")?;
        cursor.enter("effect.life.amount-after-resource");
        cursor.require(
            cursor
                .peek()
                .is_some_and(|raw| raw.eq_ignore_ascii_case("equal")),
            "`equal to` followed by a numeric expression",
        )?;
        recognize_numeric_expression(cursor, antecedent)?
    } else {
        cursor.enter("effect.life.amount-before-resource");
        let amount = recognize_numeric_expression(cursor, antecedent)?;
        cursor.enter("effect.life.resource-after-amount");
        cursor.consume("life")?;
        amount
    };
    Ok(LifeActionNode { action, amount })
}
