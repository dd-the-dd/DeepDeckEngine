use super::super::{MachineResult, OracleTokenCursor};
use super::{
    NumericAtom, RelatedObjectQueryNode, recognize_numeric_atom,
    recognize_related_object_query_before_separator,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TriggerMultiplicityNode {
    pub sources: RelatedObjectQueryNode,
    pub additional: i32,
}

pub(in crate::oracle::canonical) fn recognize_trigger_multiplicity(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TriggerMultiplicityNode> {
    cursor.enter("trigger-multiplicity.subject");
    cursor.consume_phrase(&["if", "a", "triggered", "ability", "of"])?;
    let article = cursor
        .peek()
        .filter(|word| word.eq_ignore_ascii_case("a") || word.eq_ignore_ascii_case("an"))
        .ok_or_else(|| cursor.fail("an indefinite object-query article"))?
        .to_string();
    cursor.consume(&article)?;
    let sources = recognize_related_object_query_before_separator(cursor)?;
    cursor.enter("trigger-multiplicity.occurrence");
    cursor.consume_phrase(&["triggers", ",", "that", "ability", "triggers"])?;
    let additional = recognize_numeric_atom(cursor)?;
    cursor.enter("trigger-multiplicity.additional");
    let NumericAtom::Integer(value) = additional else {
        return Err(cursor.fail("a fixed positive additional occurrence count"));
    };
    let additional = i32::try_from(value)
        .ok()
        .filter(|v| *v > 0)
        .ok_or_else(|| cursor.fail("a positive executable additional occurrence count"))?;
    cursor.consume("additional")?;
    cursor.consume(if additional == 1 { "time" } else { "times" })?;
    cursor.consume(".")?;
    cursor.finish()?;
    Ok(TriggerMultiplicityNode {
        sources,
        additional,
    })
}
