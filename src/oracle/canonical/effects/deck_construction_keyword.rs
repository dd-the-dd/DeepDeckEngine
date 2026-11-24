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

#[cfg(test)]
#[path = "deck_construction_keyword_tests.rs"]
mod tests;
