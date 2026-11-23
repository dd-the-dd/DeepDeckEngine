use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Counter target noncreature spell. Its controller creates two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")";

fn parse_offer(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "An Offer You Can't Refuse".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn offer_is_a_native_sequence_of_three_independent_primitives() {
    let parsed = parse_offer(EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown")
    );
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetSpell",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "spells",
                        "where": {
                            "kind": "not",
                            "operand": { "kind": "cardTypeContains", "value": "Creature" },
                        },
                    },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "counteredSpellController",
                    "value": {
                        "kind": "controllerOf",
                        "object": { "kind": "chosenTarget", "id": "targetSpell" },
                    },
                },
                {
                    "kind": "counterSpell",
                    "spell": { "kind": "chosenTarget", "id": "targetSpell" },
                },
                {
                    "kind": "createTokens",
                    "controller": {
                        "kind": "boundValue",
                        "id": "counteredSpellController",
                    },
                    "quantity": { "kind": "integer", "value": 2 },
                    "token": { "kind": "namedToken", "name": "Treasure" },
                },
            ],
        }))
    );
}

#[test]
fn target_predicate_quantity_and_optional_reminder_are_independent_children() {
    let parsed =
        parse_offer("Counter target artifact spell. Its controller creates three Clue tokens.");

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        rule["effects"][2]["quantity"],
        json!({ "kind": "integer", "value": 3 })
    );
    assert_eq!(
        rule["effects"][2]["token"],
        json!({ "kind": "namedToken", "name": "Clue" })
    );

    let map =
        parse_offer("Counter target noncreature spell. Its controller creates two Map tokens.");
    assert_eq!(map.status, "canonical");
    assert_eq!(
        map.abilities[0].rule.as_ref().unwrap()["effects"][2]["token"],
        json!({ "kind": "namedToken", "name": "Map" })
    );
}

#[test]
fn claimed_near_misses_report_the_responsible_local_state() {
    // The countered spell's owner is now the performer of the ordinary token
    // creation (CR 608.2c), no longer a near miss of this family.
    let owner =
        parse_offer("Counter target noncreature spell. Its owner creates two Treasure tokens.");
    assert_eq!(owner.status, "canonical");
    let effects = &owner.abilities[0].rule.as_ref().unwrap()["effects"];
    assert!(
        effects
            .as_array()
            .unwrap()
            .iter()
            .any(|effect| effect["performer"]["kind"] == "ownerOf"),
        "{effects:#}"
    );
    // The family's reminder check no longer decides the card: the shared
    // sequence reads the performer's token creation and records reminder
    // text without rules meaning (CR 207.2), as for every other sentence.
    let reminder = parse_offer(
        "Counter target noncreature spell. Its controller creates two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add two mana of any color.\")",
    );
    assert_eq!(reminder.status, "canonical");
}
