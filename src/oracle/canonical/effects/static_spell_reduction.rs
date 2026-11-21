use super::super::*;
use crate::oracle::{
    characteristic_vocabulary::type_line_has_permanent_card_type, syntax::AbilityInput,
};

pub(in crate::oracle::canonical) enum StaticSpellReductionParseAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) fn try_parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> StaticSpellReductionParseAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticSpellReductionParseAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    match try_recognize_casting_cost_reduction(&mut cursor) {
        CastingReductionReadAttempt::NotClaimed => StaticSpellReductionParseAttempt::NotClaimed,
        CastingReductionReadAttempt::FailedCommitted(failure) => {
            StaticSpellReductionParseAttempt::Failed(failure)
        }
        CastingReductionReadAttempt::Parsed(node) => {
            if !matches!(&node.subject, CastingReductionSubject::SpellSet(_)) {
                return StaticSpellReductionParseAttempt::NotClaimed;
            }
            match lower_casting_cost_reduction(input, &mut cursor, node) {
                Ok(draft) => StaticSpellReductionParseAttempt::Parsed(draft),
                Err(failure) => StaticSpellReductionParseAttempt::Failed(failure),
            }
        }
    }
}

pub(in crate::oracle::canonical) fn parse_promoted_static_spell_reduction(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("static-spell-reduction", input.tokens);
    let node = recognize_static_spell_reduction(&mut cursor)?;
    lower_casting_cost_reduction(input, &mut cursor, node)
}

#[cfg(test)]
#[path = "static_spell_reduction_tests.rs"]
mod tests;
