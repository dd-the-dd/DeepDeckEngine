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
    // The printed payment through the shared keyword-cost lowering; the
    // graveyard-cast runtime pays one of these alone, or a composite.
    cursor.enter("keyword.flashback.cost-support");
    let cost = lower_keyword_cost_sequence(&cursor, &flashback.costs)?;
    cursor.require(
        matches!(
            cost["kind"].as_str(),
            Some("payMana" | "payLife" | "sacrificePermanent" | "discardCard" | "compositeCost")
        ),
        "a Flashback payment the graveyard-cast runtime pays alone: mana, life, a sacrifice or a discard",
    )?;
    Ok(draft(
        json!({"kind":"keywordAbility", "source":self_ref(),
            "ability":{"kind":"flashback","cost":cost}
        }),
        &[
            "Recognize the registered Flashback keyword and its shared typed cost sequence",
            "Require complete cost/reminder consumption and a fixed payment without an invented X producer",
            "Lower the printed payments through the shared keyword-cost lowering",
            "Reuse the existing graveyard-cast and stack-leave exile permission implementation",
        ],
    ))
}
