use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "As this land enters, choose a creature type.\n{T}: Add {C}.\n{T}: Add one mana of any color. Spend this mana only to cast a creature spell of the chosen type, and that spell can't be countered.";

fn parse_permanent(type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Cavern".to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn cavern_is_three_native_primitive_rules() {
    let parsed = parse_permanent("Land", EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 3);
    assert!(parsed.abilities.iter().all(|ability| {
        ability.recognizer == RecognizerProvenance::Native
            && ability
                .entities
                .iter()
                .all(|entity| entity.token_type != "unknown")
    }));
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "replacementEffect",
            "source": { "kind": "self" },
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": { "kind": "self" },
            },
            "decisions": [{
                "id": "chosenCreatureType",
                "kind": "chooseCreatureType",
            }],
            "replacement": [{
                "kind": "storeDecision",
                "decisionId": "chosenCreatureType",
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[2].rule.as_ref(),
        Some(&json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{
                "kind": "tap",
                "object": { "kind": "self" },
            }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "self" },
                },
                "mana": {
                    "kind": "chooseColor",
                    "amount": 1,
                    "spendRestriction": {
                        "kind": "castSpell",
                        "where": { "kind": "cardTypeContains", "value": "Creature" },
                        "chosenCreatureTypeFromSource": true,
                        "cantBeCountered": true,
                    },
                },
            }],
        }))
    );
}

#[test]
fn entry_object_and_literal_mana_amount_are_generic_children() {
    let parsed = parse_permanent(
        "Artifact",
        "As this artifact enters, choose a creature type.\n{T}: Add two mana of any color. Spend this mana only to cast a creature spell of the chosen type, and that spell can't be countered.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["mana"]["amount"],
        2
    );
}

#[test]
fn claimed_near_misses_fail_at_the_responsible_typed_state() {
    for (oracle_text, automaton, state, found) in [
        (
            "As this instant enters, choose a creature type.",
            "chosen-creature-type-entry",
            "reference.permanent-entry-object.noun",
            "instant",
        ),
        (
            "{T}: Add one mana of any color. Spend this mana only to cast the creature spell of the chosen type, and that spell can't be countered.",
            "restricted-mana-ability",
            "mana.spend-restriction.article",
            "the",
        ),
        // The shared spell predicate now admits `an artifact spell`; a chosen
        // creature type read by a non-creature spell domain has no executable
        // consumer, so the card-context binding rejects it.
        (
            "{T}: Add one mana of any color. Spend this mana only to cast an artifact spell of the chosen type, and that spell can't be countered.",
            "card-context-binding",
            "binding.chosen-creature-type.source",
            "a chosen creature type reference",
        ),
        (
            "{T}: Add one mana of any color. Spend this mana only to cast a creature spell of the chosen name, and that spell can't be countered.",
            "restricted-mana-ability",
            "mana.spend-restriction.choice-property",
            "name",
        ),
    ] {
        let parsed = parse_permanent("Land", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
