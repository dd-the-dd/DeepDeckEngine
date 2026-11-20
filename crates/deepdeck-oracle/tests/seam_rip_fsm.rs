use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const SEAM_RIP_ORACLE: &str = "When this enchantment enters, exile target nonland permanent an opponent controls with mana value 2 or less until this enchantment leaves the battlefield.";

fn parse_permanent(card_name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseResult {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{W}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    std::thread::Builder::new()
        .name("seam-rip-fsm-test".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser test thread starts")
        .join()
        .expect("parser test thread completes")
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Seam Rip; see docs/coverage-targets.md"
)]
#[test]
fn seam_rip_is_native_linked_entry_exile_with_composed_target_predicates() {
    let parsed = parse_permanent("Seam Rip", "Enchantment", SEAM_RIP_ORACLE);

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

    let rule = ability.rule.as_ref().expect("native linked-exile rule");
    assert_eq!(rule["kind"], "triggeredAbility");
    assert_eq!(
        rule["event"],
        json!({ "kind": "enterBattlefield", "object": { "kind": "self" } })
    );

    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["kind"], "chooseTargets");
    assert_eq!(decision["id"], "exileTarget");
    assert_eq!(decision["minimum"], 1);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["kind"], "permanents");
    assert_eq!(
        decision["candidates"]["controller"],
        json!({
            "kind": "opponentsOf",
            "player": { "kind": "controllerOf", "object": { "kind": "self" } },
        })
    );
    assert_eq!(
        decision["candidates"]["where"],
        json!({
            "kind": "and",
            "operands": [
                {
                    "kind": "not",
                    "operand": { "kind": "cardTypeContains", "value": "Land" },
                },
                {
                    "kind": "compare",
                    "left": {
                        "kind": "manaValueOf",
                        "object": { "kind": "candidate" },
                    },
                    "operator": "<=",
                    "right": { "kind": "integer", "value": 2 },
                },
            ],
        })
    );

    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "exilePermanentWithSource");
    assert_eq!(
        effect["permanent"],
        json!({ "kind": "chosenTarget", "id": "exileTarget" })
    );
    assert_eq!(effect["minimum"], 1);
    assert_eq!(effect["maximum"], 1);
    assert_eq!(effect["returnWhenSourceLeaves"], true);
    assert!(!rule.to_string().contains("resolveTriggeredInstruction"));
    assert!(!rule.to_string().contains("seamRip"));
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Seam Rip; see docs/coverage-targets.md"
)]
#[test]
fn linked_entry_exile_reuses_cardinality_controller_and_numeric_selector_children() {
    let parsed = parse_permanent(
        "Test Containment Device",
        "Artifact",
        "When this artifact enters, exile up to one target creature you control with mana value 4 or greater until this artifact leaves the battlefield.",
    );

    assert_eq!(parsed.status, "canonical");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("native sibling rule");
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 0);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(
        decision["candidates"]["controller"],
        json!({ "kind": "controllerOf", "object": { "kind": "self" } })
    );
    let predicate = decision["candidates"]["where"].to_string();
    assert!(predicate.contains("Creature"));
    assert!(predicate.contains("manaValueOf"));
    assert!(predicate.contains("\"operator\":\">=\""));
    assert!(predicate.contains("\"value\":4"));
    assert_eq!(rule["effects"][0]["returnWhenSourceLeaves"], true);
}

#[test]
fn committed_linked_exile_rejects_a_duration_bound_to_a_different_source_kind() {
    let parsed = parse_permanent(
        "Test Broken Seal",
        "Enchantment",
        "When this enchantment enters, exile target creature until this artifact leaves the battlefield.",
    );

    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    // The shared trigger program reports its own committed failure first;
    // the linked-exile family's precise duration state stays visible.
    let diagnostic = ability
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.automaton.as_deref() == Some("linked-entry-exile"))
        .expect("committed linked-exile failure is surfaced");
    assert_eq!(diagnostic.state.as_deref(), Some("duration.source-kind"));
    assert_eq!(diagnostic.found.as_deref(), Some("artifact"));
}
