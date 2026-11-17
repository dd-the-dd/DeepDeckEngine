use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const CHAOS_WARP_ORACLE: &str = "The owner of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield.";

fn parse_spell(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{2}{R}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("chaos-warp-fsm-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

#[test]
fn chaos_warp_is_native_and_preserves_owner_bound_primitive_ir() {
    let parsed = parse_spell("Chaos Warp", CHAOS_WARP_ORACLE);

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
    let rule = ability.rule.as_ref().expect("primitive owner sequence");
    assert_eq!(
        rule["effects"][0],
        json!({
            "kind": "bind",
            "id": "targetOwner",
            "value": {
                "kind": "ownerOf",
                "object": { "kind": "chosenTarget", "id": "targetPermanent" },
            },
        })
    );
    assert_eq!(rule["effects"][1]["kind"], "putPermanentOnOwnerLibrary");
    assert_eq!(rule["effects"][1]["position"], "top");
    assert_eq!(
        rule["effects"][2]["zone"]["player"],
        json!({ "kind": "boundValue", "id": "targetOwner" })
    );
    assert_eq!(rule["effects"][3]["kind"], "lookAtTopCards");
    assert_eq!(rule["effects"][3]["bind"], "revealedTopCard");
    assert_eq!(rule["effects"][4]["kind"], "revealCards");
    assert_eq!(
        rule["effects"][5]["cards"]["where"],
        json!({ "kind": "isPermanentCard" })
    );
    assert_eq!(
        rule["effects"][5]["to"]["player"],
        json!({ "kind": "boundValue", "id": "targetOwner" })
    );
    assert!(!rule.to_string().contains("resolveSpellInstruction"));
    assert!(!rule.to_string().contains("chaosWarp"));
}

#[test]
fn target_and_revealed_card_predicates_are_independent_native_children() {
    let parsed = parse_spell(
        "Test Owner Reveal",
        "The owner of target artifact shuffles it into their library, then reveals the top card of their library. If it's a creature card, they put that card onto the battlefield.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("native sibling");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Artifact" })
    );
    assert_eq!(
        rule["effects"][5]["cards"]["where"],
        json!({ "kind": "cardTypeContains", "value": "Creature" })
    );
}

#[test]
fn committed_owner_sequence_rejects_controller_as_a_different_player_relation() {
    let parsed = parse_spell(
        "Test Controller Reveal",
        "The controller of target permanent shuffles it into their library, then reveals the top card of their library. If it's a permanent card, they put it onto the battlefield.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    let diagnostic = ability
        .diagnostics
        .first()
        .expect("committed native failure is surfaced");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("owner-library-reveal-sequence")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("owner-library.subject.player-role")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("controller"));
}
