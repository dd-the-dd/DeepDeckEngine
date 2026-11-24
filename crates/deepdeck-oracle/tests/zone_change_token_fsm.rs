use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const GENEROUS_GIFT_ORACLE: &str =
    "Destroy target permanent. Its controller creates a 3/3 green Elephant creature token.";

fn parse_instant(card_name: &str, mana_cost: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn generous_gift_is_native_without_unknown_and_snapshots_controller_before_destroying() {
    let parsed = parse_instant("Generous Gift", "{2}{W}", GENEROUS_GIFT_ORACLE);

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 1);
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert!(
        ability
            .entities
            .iter()
            .all(|entity| entity.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:#?}",
        ability.entities
    );
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetPermanent",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": { "kind": "permanents", "where": null },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "changedPermanentController",
                    "value": {
                        "kind": "controllerOf",
                        "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                    },
                },
                {
                    "kind": "destroyPermanent",
                    "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
                },
                {
                    "kind": "createTokens",
                    "controller": {
                        "kind": "boundValue",
                        "id": "changedPermanentController",
                    },
                    "quantity": { "kind": "integer", "value": 1 },
                    "token": {
                        "colors": ["Green"],
                        "types": ["Creature"],
                        "subtypes": ["Elephant"],
                        "power": 3,
                        "toughness": 3,
                        "abilities": [],
                    },
                },
            ],
        }))
    );
}

#[test]
fn zone_change_and_vanilla_token_characteristics_are_reusable() {
    let beast = parse_instant(
        "Beast Within",
        "{2}{G}",
        "Destroy target permanent. Its controller creates a 3/3 green Beast creature token.",
    );
    assert_eq!(beast.status, "canonical");
    assert_eq!(beast.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        beast.abilities[0].rule.as_ref().unwrap()["effects"][2]["token"]["subtypes"],
        json!(["Beast"])
    );

    let exile = parse_instant(
        "Example Transformation",
        "{2}{R}",
        "Exile target artifact. Its controller creates two 1/1 red Goblin creature tokens.",
    );
    assert_eq!(exile.status, "canonical");
    assert_eq!(exile.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = exile.abilities[0].rule.as_ref().unwrap();
    assert_eq!(rule["effects"][1]["kind"], "exilePermanent");
    assert_eq!(rule["effects"][2]["quantity"]["value"], 2);
    assert_eq!(rule["effects"][2]["token"]["power"], 1);
    assert_eq!(rule["effects"][2]["token"]["colors"], json!(["Red"]));
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
}

#[test]
fn multiple_registered_subtypes_remain_separate_ir_values() {
    let parsed = parse_instant(
        "Example Conversion",
        "{2}{R}",
        "Destroy target permanent. Its controller creates two 1/1 red Phyrexian Goblin creature tokens.",
    );
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][2]["token"]["subtypes"],
        json!(["Phyrexian", "Goblin"])
    );
}

/// The shared written token description reads a description without a color
/// word as colorless (CR 105.2c) and keyword abilities after `with`
/// (CR 111.4), so these former near-misses are complete descriptions.
#[test]
fn colorless_and_keyword_descriptions_use_the_shared_token_description() {
    let colorless = parse_instant(
        "Example Transformation",
        "{2}{G}",
        "Destroy target permanent. Its controller creates a 3/3 Beast creature token.",
    );
    assert_eq!(colorless.status, "canonical");
    let rule = colorless.abilities[0].rule.as_ref().unwrap();
    assert_eq!(rule["effects"][2]["token"]["colors"], json!([]));
    assert_eq!(rule["effects"][2]["token"]["subtypes"], json!(["Beast"]));

    let trample = parse_instant(
        "Example Transformation",
        "{2}{G}",
        "Destroy target permanent. Its controller creates a 3/3 green Beast creature token with trample.",
    );
    assert_eq!(trample.status, "canonical");
    assert_eq!(
        trample.abilities[0].rule.as_ref().unwrap()["effects"][2]["controller"]["kind"],
        "boundValue"
    );
    assert_eq!(
        trample.abilities[0].rule.as_ref().unwrap()["effects"][2]["token"]["abilities"],
        json!([{ "kind": "trample" }])
    );
}

#[test]
fn claimed_near_misses_fail_at_the_responsible_local_state() {
    // An unknown player noun never enters the zone-change/token family; the
    // shared contextual program owns that subject failure.
    for (oracle_text, automaton, state, found) in [
        (
            "Destroy target permanent. Their controller creates a 3/3 green Beast creature token.",
            "contextual-effect-program",
            "effect.start",
            "Their",
        ),
        (
            "Destroy target permanent. Its controller creates a three-three green Beast creature token.",
            "zone-change-create-token",
            "token.named.catalog-identity",
            "three-three",
        ),
        (
            "Destroy target permanent. Its controller creates a 3/3 green Beast artifact token.",
            "zone-change-create-token",
            "token.descriptor.types",
            "token",
        ),
        (
            "Destroy target permanent. Its controller creates a 3/3 green Imaginarytype creature token.",
            "zone-change-create-token",
            "token.descriptor.subtypes",
            "Imaginarytype",
        ),
    ] {
        let parsed = parse_instant("Example Transformation", "{2}{G}", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some(automaton),
            "{oracle_text}"
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state), "{oracle_text}");
        assert_eq!(diagnostic.found.as_deref(), Some(found), "{oracle_text}");
    }
}
