use super::super::super::super::{MachineResult, OracleTokenCursor};
use super::super::super::{NumericAtom, NumericExpression};

/// Consumes the Oracle card noun associated with a numeric expression. A
/// literal one requires the singular; variable and compound expressions keep
/// the plural surface without restricting their eventual runtime value.
pub(in crate::oracle::canonical::automata) fn recognize_card_count_noun(
    cursor: &mut OracleTokenCursor<'_>,
    count: &NumericExpression,
) -> MachineResult<()> {
    cursor.enter("counted-card.noun");
    let noun = cursor
        .peek_token()
        .ok_or_else(|| cursor.fail("`card` or `cards` matching the count"))?
        .clone();
    let singular = matches!(count, NumericExpression::Atom(NumericAtom::Integer(1)));
    let agrees = if singular {
        noun.raw.eq_ignore_ascii_case("card")
    } else {
        noun.raw.eq_ignore_ascii_case("cards")
    };
    if !agrees {
        return Err(cursor.fail_with_span(
            "card noun agreement with the numeric expression",
            noun.index,
            noun.span_start,
            noun.span_end,
            noun.raw,
        ));
    }
    cursor.advance_to(cursor.position() + 1)?;
    cursor.enter("counted-card.complete");
    Ok(())
}
