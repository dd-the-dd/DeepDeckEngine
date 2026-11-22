use super::super::*;
use super::deck_construction_keyword_supported;

fn keyword_rule(ability: Value) -> Value {
    json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": ability })
}

#[test]
fn partner_abilities_are_executable_without_in_game_behavior() {
    for ability in [
        json!({ "kind": "partner", "rulesScope": "deckConstruction" }),
        json!({ "kind": "partner", "variant": "Friends forever", "rulesScope": "deckConstruction" }),
        json!({ "kind": "chooseABackground", "rulesScope": "deckConstruction" }),
        json!({ "kind": "doctorsCompanion", "rulesScope": "deckConstruction" }),
    ] {
        assert!(deck_construction_keyword_supported(&ability), "{ability}");
        assert!(
            rule_is_executable(&keyword_rule(ability.clone())),
            "{ability}"
        );
    }
}

#[test]
fn unscoped_or_malformed_partner_abilities_stay_unexecutable() {
    for ability in [
        json!({ "kind": "partner" }),
        json!({ "kind": "partner", "rulesScope": "game" }),
        json!({ "kind": "partner", "variant": "  ", "rulesScope": "deckConstruction" }),
        json!({ "kind": "partner", "variant": 3, "rulesScope": "deckConstruction" }),
        json!({ "kind": "chooseABackground", "variant": "x", "rulesScope": "deckConstruction" }),
        json!({ "kind": "companion", "rulesScope": "deckConstruction" }),
    ] {
        assert!(
            !rule_is_executable(&keyword_rule(ability.clone())),
            "{ability}"
        );
    }
}
