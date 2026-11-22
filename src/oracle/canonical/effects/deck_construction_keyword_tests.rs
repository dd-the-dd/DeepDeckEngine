//! Partner abilities (CR 702.124) are deck-construction keywords with no
//! in-game behavior; `partner with` keeps its trigger as an explicit failure.
use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::{Value, json};

fn parse(name: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Legendary Creature — Human Warrior".to_string(),
        mana_cost: None,
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn only_rule(parsed: &OracleCardParseResult) -> &Value {
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.diagnostics);
    assert_eq!(parsed.abilities.len(), 1);
    parsed.abilities[0].rule.as_ref().expect("a compiled rule")
}

fn keyword(ability: Value) -> Value {
    json!({ "kind": "keywordAbility", "source": { "kind": "self" }, "ability": ability })
}

#[test]
fn partner_with_its_reminder_is_a_deck_construction_keyword() {
    // The Dilu Horse, real Oracle text.
    let parsed = parse(
        "The Dilu Horse",
        "Partner (You can have two commanders if both have partner.)",
    );
    assert_eq!(
        only_rule(&parsed),
        &keyword(json!({ "kind": "partner", "rulesScope": "deckConstruction" }))
    );
}

#[test]
fn partner_variants_background_and_doctors_companion_share_the_frame() {
    for (text, ability) in [
        (
            "Partner—Friends forever (You can have two commanders if both have this ability.)",
            json!({ "kind": "partner", "variant": "Friends forever", "rulesScope": "deckConstruction" }),
        ),
        (
            "Partner—Father & son (You can have two commanders if both have this ability.)",
            json!({ "kind": "partner", "variant": "Father & son", "rulesScope": "deckConstruction" }),
        ),
        (
            "Partner—Character select",
            json!({ "kind": "partner", "variant": "Character select", "rulesScope": "deckConstruction" }),
        ),
        (
            "Choose a Background (You can have a Background as a second commander.)",
            json!({ "kind": "chooseABackground", "rulesScope": "deckConstruction" }),
        ),
        (
            "Doctor's companion (You can have two commanders if the other is the Doctor.)",
            json!({ "kind": "doctorsCompanion", "rulesScope": "deckConstruction" }),
        ),
    ] {
        assert_eq!(
            only_rule(&parse("Sibling", text)),
            &keyword(ability),
            "{text}"
        );
    }
}

#[test]
fn partner_with_a_name_and_a_wrong_reminder_fail_explicitly() {
    for (text, state) in [
        (
            "Partner with Trynn, Champion of Freedom (When this creature enters, target player may put Trynn into their hand from their library, then shuffle.)",
            "keyword.partner-with.trigger",
        ),
        (
            "Partner (You can have two commanders if both have this ability.)",
            "keyword.deck-construction.reminder",
        ),
        (
            "Partner—Friends forever. Draw a card.",
            "keyword.deck-construction.reminder-or-complete",
        ),
    ] {
        let parsed = parse("Near Miss", text);
        assert_eq!(parsed.status, "unsupported", "{text}");
        assert_eq!(
            parsed.diagnostics[0].state.as_deref(),
            Some(state),
            "{text}: {:#?}",
            parsed.diagnostics
        );
    }
}
