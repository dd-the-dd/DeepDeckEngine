use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

const FULL_ORACLE: &str = "{T}: Exile target card from a graveyard.\n{6}, {T}, Sacrifice this artifact: Put each creature card exiled with this artifact onto the battlefield under your control with a flying counter on it. Each of them is a 1/1 Spirit in addition to its other types. Activate only as a sorcery.";

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Ghost Vacuum".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{1}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn tap_to_exile_a_target_card_from_any_graveyard_is_native_and_primitive() {
    let parsed = parse("{T}: Exile target card from a graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.rule.as_ref(),
        Some(&json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "id": "targetCard",
                    "kind": "chooseTargets",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": {
                        "kind": "cards",
                        "zone": { "kind": "anyGraveyard" },
                        "where": null,
                    },
                }],
            },
            // Standalone (no linked reader on the face, CR 607) the retired
            // `exileTargetCardWithSource` composite is the ordinary exile.
            "effects": [{
                "kind": "exileTargetCards",
                "cards": { "kind": "chosenTargets", "id": "targetCard" },
            }],
        }))
    );
}

#[test]
fn complete_ghost_vacuum_routes_both_abilities_through_native_automata() {
    let parsed = parse(FULL_ORACLE);

    // CR 607.2a: the second ability reads cards exiled with this artifact.
    // The retired `exileTargetCardWithSource` composite has no primitive
    // replacement that records the source relation yet, so card composition
    // rejects the unlinked exile instead of emitting a reader that can never
    // find its cards. Both abilities are still routed natively.
    assert_eq!(parsed.status, "unsupported");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    let exile = &parsed.abilities[0];
    assert_eq!(exile.status, "unsupported");
    assert_eq!(exile.rule, None);
    assert_eq!(exile.diagnostics.len(), 1);
    assert_eq!(exile.diagnostics[0].code, "oracle_card_composition_failure");
    assert_eq!(
        exile.diagnostics[0].automaton.as_deref(),
        Some("linked-exile-composition")
    );
    assert_eq!(
        exile.diagnostics[0].state.as_deref(),
        Some("composition.linked-exile.relation")
    );

    let reader = &parsed.abilities[1];
    assert_eq!(reader.status, "canonical");
    let rule = reader.rule.as_ref().unwrap();
    assert_eq!(
        rule["activationCondition"],
        json!({ "kind": "sorceryTiming" })
    );
    assert_eq!(
        rule["effects"][0]["cards"],
        json!({
            "kind": "filterObjects",
            "objects": { "kind": "cardsExiledWithSource" },
            "where": { "kind": "cardTypeContains", "value": "Creature" },
        })
    );
    assert_eq!(rule["effects"][0]["kind"], "moveCards");
    assert_eq!(rule["effects"][0]["to"]["kind"], "battlefield");
    assert_eq!(
        rule["effects"][0]["to"]["player"],
        json!({ "kind": "controllerOf", "object": { "kind": "self" } })
    );
    assert_eq!(
        rule["effects"][0]["to"]["enterWithCounters"],
        json!([{ "counter": "flying", "count": { "kind": "integer", "value": 1 } }])
    );
    assert_eq!(rule["effects"][0]["to"]["addSubtypes"], json!(["Spirit"]));
    assert_eq!(rule["effects"][0]["to"]["retainExistingTypes"], true);
    assert_eq!(
        rule["effects"][0]["to"]["basePower"],
        json!({ "kind": "integer", "value": 1 })
    );
    assert_eq!(
        rule["effects"][0]["to"]["baseToughness"],
        json!({ "kind": "integer", "value": 1 })
    );
}

#[test]
fn mana_tap_and_life_costs_reuse_the_same_card_zone_change_automaton() {
    let parsed = parse("{2}, {T}, Pay 2 life: Exile target card from any graveyard.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling emits a rule");
    assert_eq!(
        rule["costs"],
        json!([
            { "kind": "payMana", "manaCost": "{2}" },
            { "kind": "tap", "object": { "kind": "self" } },
            {
                "kind": "payLife",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "amount": { "kind": "integer", "value": 2 },
            },
        ])
    );
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["zone"]["kind"],
        "anyGraveyard"
    );
}

#[test]
fn claimed_near_misses_report_the_responsible_state_without_fallback() {
    for (oracle_text, state, found) in [
        (
            "{T}: Exile target card beyond a graveyard.",
            "card-target.source-relation",
            "beyond",
        ),
        (
            "{T}: Exile target card from a library.",
            "card-target.source-capability",
            "library",
        ),
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(ability.status, "unsupported");
        let diagnostic = &ability.diagnostics[0];
        assert_eq!(
            diagnostic.automaton.as_deref(),
            Some("activated-card-zone-change")
        );
        assert_eq!(diagnostic.state.as_deref(), Some(state));
        assert_eq!(diagnostic.found.as_deref(), Some(found));
    }
}

#[test]
#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Ghost Vacuum missing-colon diagnostic; see docs/coverage-targets.md"
)]
fn missing_colon_activation_fails_at_the_family_separator() {
    let parsed = parse("{T} Exile target card from a graveyard.");
    let ability = &parsed.abilities[0];
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    let diagnostic = &ability.diagnostics[0];
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("activated-card-zone-change")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("activation.card-zone-change.effect-separator")
    );
    assert_eq!(diagnostic.found.as_deref(), Some("Exile"));
}

#[test]
fn promoted_vocabulary_has_no_unknown_tokens() {
    let parsed = parse("{T}: Exile target card from a graveyard.");
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
