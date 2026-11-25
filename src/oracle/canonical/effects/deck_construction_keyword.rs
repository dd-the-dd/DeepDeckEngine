use serde_json::{Value, json};

use super::super::*;
use crate::oracle::syntax::AbilityInput;

/// A partner ability line (CR 702.124a) claims its own frame: once its name
/// is read, a malformed variant, reminder or `partner with` trigger is a
/// committed failure of this frame.
pub(in crate::oracle::canonical) fn claims_deck_construction_keyword_family(
    input: &AbilityInput<'_>,
) -> bool {
    deck_construction_keyword_starts(&OracleTokenCursor::new(
        "deck-construction-keyword-claim",
        input.tokens,
    ))
}

/// The keyword is a characteristic of the object with no in-game behavior:
/// `rulesScope` states that its rules apply to deck construction only.
fn lower_deck_construction_keyword(keyword: &DeckConstructionKeyword) -> Value {
    let mut ability = match keyword {
        DeckConstructionKeyword::Partner => json!({ "kind": "partner" }),
        DeckConstructionKeyword::PartnerVariant(name) => {
            json!({ "kind": "partner", "variant": name })
        }
        DeckConstructionKeyword::ChooseABackground => json!({ "kind": "chooseABackground" }),
        DeckConstructionKeyword::DoctorsCompanion => json!({ "kind": "doctorsCompanion" }),
        DeckConstructionKeyword::SpellCommander => json!({ "kind": "spellCommander" }),
    };
    ability["rulesScope"] = json!("deckConstruction");
    ability
}

pub(in crate::oracle::canonical) fn parse_deck_construction_keyword(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("deck-construction-keyword", input.tokens);
    let keyword = recognize_deck_construction_keyword(&mut cursor)?;
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": lower_deck_construction_keyword(&keyword),
        }),
        &[
            "Recognize a partner ability and its canonical reminder (CR 702.124)",
            "Type it as a deck-construction keyword with no in-game behavior",
        ],
    ))
}

/// A deck-construction rule of the card itself (CR 100.2a, 903.3d) claims
/// its frame from its sentence shape, before the static frame reads the
/// card name as a static subject.
pub(in crate::oracle::canonical) fn claims_deck_construction_rule_family(
    input: &AbilityInput<'_>,
) -> bool {
    deck_construction_rule_starts(
        &OracleTokenCursor::new("deck-construction-rule-claim", input.tokens),
        input.name_links,
        input.name_resolution_failure,
    )
}

/// The rule is a characteristic of the card scoped to deck construction,
/// typed like the partner keywords: the engine accepts it with no in-game
/// behavior.
fn lower_deck_construction_rule(rule: &DeckConstructionRule) -> Value {
    let mut ability = match rule {
        DeckConstructionRule::CommanderEligibility => json!({ "kind": "commanderEligibility" }),
        DeckConstructionRule::CopyLimit { maximum } => json!({
            "kind": "deckCopyLimit",
            "maximum": maximum.map_or(Value::Null, |maximum| json!(maximum)),
        }),
    };
    ability["rulesScope"] = json!("deckConstruction");
    ability
}

pub(in crate::oracle::canonical) fn parse_deck_construction_rule(
    input: &AbilityInput<'_>,
) -> MachineResult<CanonicalRuleDraft> {
    let mut cursor = OracleTokenCursor::new("deck-construction-rule", input.tokens);
    let rule = recognize_deck_construction_rule(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    )?;
    Ok(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": lower_deck_construction_rule(&rule),
        }),
        &[
            "Recognize the card's own deck-construction rule with the shared source reference (CR 100.2a, 903.3d)",
            "Type it as a deck-construction characteristic with no in-game behavior",
        ],
    ))
}

#[cfg(test)]
#[path = "deck_construction_keyword_tests.rs"]
mod tests;
