use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};
use serde_json::json;

fn parse_spell(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("owner-library-parser-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

#[test]
fn chaos_warp_compiles_to_owner_bound_primitive_effects() {
    let parsed = parse_spell(
        "Chaos Warp",
        "The owner of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0]
        .rule
        .as_ref()
        .expect("the owner-library sequence should produce primitive IR");
    assert_eq!(
        rule,
        &json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "kind": "chooseTargets",
                    "id": "targetPermanent",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "permanents",
                        "where": null,
                    },
                }],
            },
            "effects": [
                {
                    "kind": "bind",
                    "id": "targetOwner",
                    "value": {
                        "kind": "ownerOf",
                        "object": { "kind": "chosenTarget", "id": "targetPermanent" },
                    },
                },
                {
                    "kind": "putPermanentOnOwnerLibrary",
                    "permanent": { "kind": "chosenTarget", "id": "targetPermanent" },
                    "position": "top",
                },
                {
                    "kind": "shuffleZone",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "boundValue", "id": "targetOwner" },
                    },
                },
                {
                    "kind": "lookAtTopCards",
                    "zone": {
                        "kind": "library",
                        "player": { "kind": "boundValue", "id": "targetOwner" },
                    },
                    "count": { "kind": "integer", "value": 1 },
                    "bind": "revealedTopCard",
                },
                {
                    "kind": "revealCards",
                    "cards": { "kind": "boundObjects", "binding": "revealedTopCard" },
                },
                {
                    "kind": "moveCards",
                    "cards": {
                        "kind": "filterObjects",
                        "objects": { "kind": "boundObjects", "binding": "revealedTopCard" },
                        "where": { "kind": "isPermanentCard" },
                    },
                    "to": {
                        "kind": "battlefield",
                        "player": { "kind": "boundValue", "id": "targetOwner" },
                        "tapped": false,
                    },
                },
            ],
        })
    );
    let serialized = rule.to_string();
    assert!(!serialized.contains("resolveSpellInstruction"));
    assert!(!serialized.contains("chaosWarp"));
}

#[test]
fn target_and_revealed_card_criteria_are_independent_generic_slots() {
    let parsed = parse_spell(
        "Test Owner Reveal",
        "The owner of target artifact shuffles it into their library, then reveals the top card of their library. If it's a creature card, they put that card onto the battlefield.",
    );

    assert_eq!(parsed.status, "canonical");
    let rule = parsed.abilities[0].rule.as_ref().expect("generic sibling");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        rule["effects"][5]["cards"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
    assert_eq!(
        rule["effects"][5]["to"]["player"],
        json!({ "kind": "boundValue", "id": "targetOwner" })
    );
}

#[test]
fn controller_wording_is_not_approximated_as_owner_wording() {
    let parsed = parse_spell(
        "Test Controller Reveal",
        "The controller of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}

#[test]
fn mismatched_library_pronoun_does_not_rebind_the_target_owner() {
    let parsed = parse_spell(
        "Test Mismatched Owner Reveal",
        "The owner of target permanent shuffles it into their library, then reveals the top card of your library. If it's a permanent card, they put it onto the battlefield.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[0].rule.is_none());
}
