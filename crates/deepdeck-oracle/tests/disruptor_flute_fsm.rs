use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const EXACT_ORACLE: &str = "Flash\nAs this artifact enters, choose a card name.\nSpells with the chosen name cost {3} more to cast.\nActivated abilities of sources with the chosen name can't be activated unless they're mana abilities.";

fn parse_artifact(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn disruptor_flute_is_four_native_primitive_abilities() {
    let parsed = parse_artifact("Disruptor Flute", EXACT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 4);
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
            "kind": "keywordAbility",
            "source": { "kind": "self" },
            "ability": { "kind": "flash" },
        }))
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&json!({
            "kind": "replacementEffect",
            "source": { "kind": "self" },
            "event": {
                "kind": "wouldEnterBattlefield",
                "object": { "kind": "self" },
            },
            "decisions": [{
                "id": "chosenCardName",
                "kind": "chooseCardName",
                "where": null,
                "lookAtOpponentHand": false,
            }],
            "replacement": [{
                "kind": "storeDecision",
                "decisionId": "chosenCardName",
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[2].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "additionalCastingCost",
                "players": { "kind": "eachPlayer" },
                "where": {
                    "kind": "nameEquals",
                    "value": {
                        "kind": "storedDecision",
                        "object": { "kind": "self" },
                        "decisionId": "chosenCardName",
                    },
                },
                "cost": { "kind": "payMana", "manaCost": "{3}" },
            }],
        }))
    );
    assert_eq!(
        parsed.abilities[3].rule.as_ref(),
        Some(&json!({
            "kind": "staticAbility",
            "source": { "kind": "self" },
            "activeWhile": {
                "kind": "inZone",
                "object": { "kind": "self" },
                "zone": { "kind": "battlefield" },
            },
            "modifiers": [{
                "kind": "prohibitActivation",
                "abilities": {
                    "kind": "activatedAbilities",
                    "sourceWhere": {
                        "kind": "nameEquals",
                        "value": {
                            "kind": "storedDecision",
                            "object": { "kind": "self" },
                            "decisionId": "chosenCardName",
                        },
                    },
                    "where": {
                        "kind": "not",
                        "operand": { "kind": "isManaAbility" },
                    },
                },
            }],
        }))
    );
}

#[test]
fn source_name_card_domain_tax_amount_and_mana_exception_are_generic_children() {
    let parsed = parse_artifact(
        "Test Needle",
        "As Test Needle enters, choose any card name.\nSpells with the chosen name cost {2} more to cast.\nActivated abilities of sources with the chosen name can't be activated.",
    );

    assert_eq!(parsed.status, "canonical");
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["modifiers"][0]["cost"]["manaCost"],
        "{2}"
    );
    assert!(
        parsed.abilities[2].rule.as_ref().unwrap()["modifiers"][0]["abilities"]["where"].is_null()
    );

    let land_choice = parse_artifact(
        "Test Reliquary",
        "As this land enters, choose a land card name.",
    );
    assert_eq!(land_choice.status, "canonical");
    assert_eq!(
        land_choice.abilities[0].rule.as_ref().unwrap()["decisions"][0]["where"],
        json!({ "kind": "cardTypeContains", "value": "Land" })
    );
}

#[test]
fn claimed_near_misses_fail_in_the_responsible_local_machine() {
    for (text, automaton, state, found) in [
        // The card-name claim is typed on its `card name` head; another head
        // is owned by the entry-replacement frame, which names the choice
        // it cannot read (CR 614.12).
        (
            "As this artifact enters, choose a card title.",
            "ability-frame",
            "ability-frame.entry-replacement.modification",
            "choose",
        ),
        (
            "Spells with the chosen name cost approximately three more to cast.",
            "chosen-name-spell-tax",
            "cost.mana.start",
            "approximately",
        ),
        (
            "Activated abilities of sources with the chosen name can't be activated unless they're triggered abilities.",
            "activation-prohibition",
            "static.activation-prohibition.exception-ability-kind",
            "triggered",
        ),
    ] {
        let parsed = parse_artifact("Test Flute", text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(diagnostic.automaton.as_deref(), Some(automaton));
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}
