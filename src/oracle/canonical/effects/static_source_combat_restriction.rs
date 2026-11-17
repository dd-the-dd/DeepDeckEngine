use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) enum StaticSourceCombatRestrictionAttempt {
    NotClaimed,
    Parsed(CanonicalRuleDraft),
}

/// Full-envelope static parent. Conditional, temporary and multi-sentence
/// forms remain available to their own parents; none is made unconditional.
pub(in crate::oracle::canonical) fn try_parse_static_source_combat_restriction(
    input: &AbilityInput<'_>,
) -> StaticSourceCombatRestrictionAttempt {
    if !type_line_has_permanent_card_type(input.face_type_line) {
        return StaticSourceCombatRestrictionAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("static-source-combat-restriction", input.tokens);
    let parsed = (|| -> MachineResult<GrantedKeyword> {
        cursor.enter("static.combat-restriction.source");
        recognize_source_reference(&mut cursor, input.name_links, input.name_resolution_failure)?;
        cursor.enter("static.combat-restriction.prohibition");
        let word = cursor
            .peek()
            .filter(|raw| {
                raw.eq_ignore_ascii_case("can't") || raw.eq_ignore_ascii_case("can\u{2019}t")
            })
            .ok_or_else(|| cursor.fail("the prohibition `can't`"))?
            .to_owned();
        cursor.consume(&word)?;
        let restriction = recognize_combat_restriction_kind(&mut cursor)?;
        if cursor.peek() == Some(".") {
            cursor.consume(".")?;
        }
        cursor.enter("static.combat-restriction.complete");
        cursor.finish()?;
        Ok(restriction)
    })();
    let Ok(restriction) = parsed else {
        return StaticSourceCombatRestrictionAttempt::NotClaimed;
    };
    StaticSourceCombatRestrictionAttempt::Parsed(draft(
        json!({
            "kind":"staticAbility", "source":self_ref(),
            "activeWhile":active_while_battlefield(),
            "modifiers":[{
                "kind":"grantKeyword", "objects":self_ref(),
                "keyword":restriction.canonical_name(),
            }],
        }),
        &[
            "Resolve the permanent source through the shared source-reference automaton",
            "Reuse the independent combat prohibition child without a temporary duration",
            "Lower the existing internal restriction marker, not defender or a fabricated Magic keyword",
        ],
    ))
}

#[cfg(test)]
#[path = "static_source_combat_restriction_tests.rs"]
mod tests;
