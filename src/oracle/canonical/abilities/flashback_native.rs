use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_flashback_family(
    input: &AbilityInput<'_>,
) -> bool {
    input
        .tokens
        .first()
        .is_some_and(|token| token.raw.eq_ignore_ascii_case("flashback"))
}
pub(in crate::oracle::canonical) fn parse_promoted_flashback(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-flashback", input.tokens);
    cursor.enter("keyword.flashback.source-context");
    cursor.require(
        spell_face(input.face_type_line),
        "an instant or sorcery source face for Flashback",
    )?;
    let flashback = recognize_flashback_keyword(&mut cursor)?;
    Ok(draft(
        json!({"kind":"keywordAbility", "source":self_ref(),
            "ability":{"kind":"flashback","cost":{"kind":"payMana","manaCost":flashback.cost.as_oracle_cost()}}
        }),
        &[
            "Recognize the registered Flashback keyword and retain a shared typed mana cost",
            "Require complete cost/reminder consumption and a fixed payment without an invented X producer",
            "Reuse the existing graveyard-cast and stack-leave exile permission implementation",
        ],
    ))
}
