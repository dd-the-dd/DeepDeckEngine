use super::super::*;
use crate::oracle::syntax::AbilityInput;

pub(in crate::oracle::canonical) fn claims_promoted_rebound_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    rebound_keyword_starts(&OracleTokenCursor::new("rebound-claim", input.tokens))
}

pub(in crate::oracle::canonical) fn parse_promoted_rebound_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("keyword-rebound", input.tokens);
    recognize_rebound_keyword(&mut cursor)?;
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": { "kind": "rebound" },
        }),
        &[
            "Recognize Rebound (CR 702.88a); the runtime expands it into the shared delayed upkeep trigger",
        ],
    ))
}
