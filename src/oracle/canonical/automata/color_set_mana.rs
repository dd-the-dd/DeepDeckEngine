use super::super::{MachineResult, OracleTokenCursor};
use super::{ColorSetQueryNode, NumericAtom, recognize_color_set_query, recognize_numeric_atom};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AddEachColorManaNode {
    pub colors: ColorSetQueryNode,
    pub amount: i64,
}

pub(in crate::oracle::canonical) fn recognize_add_each_color_mana(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AddEachColorManaNode> {
    cursor.enter("effect.mana-each-color.query");
    let colors = recognize_color_set_query(cursor)?;
    cursor.enter("effect.mana-each-color.body-separator");
    cursor.consume(",")?;
    cursor.enter("effect.mana-each-color.action");
    cursor.consume("add")?;
    cursor.enter("effect.mana-each-color.amount");
    let amount_token = cursor.peek_token().cloned();
    let amount = match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(n) if (1..=10).contains(&n) => n,
        _ => {
            let token = amount_token
                .as_ref()
                .ok_or_else(|| cursor.fail("a positive literal mana amount"))?;
            return Err(cursor.fail_with_span(
                "a positive literal amount between one and ten for each color",
                token.index,
                token.span_start,
                token.span_end,
                token.raw.clone(),
            ));
        }
    };
    cursor.enter("effect.mana-each-color.resource");
    cursor.consume("mana")?;
    cursor.consume("of")?;
    cursor.enter("effect.mana-each-color.bound-color");
    cursor.consume("that")?;
    cursor.consume("color")?;
    cursor.enter("effect.mana-each-color.complete");
    Ok(AddEachColorManaNode { colors, amount })
}
