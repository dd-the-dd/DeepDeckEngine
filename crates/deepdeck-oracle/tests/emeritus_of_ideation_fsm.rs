use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance,
    parse_oracle_card,
};
use serde_json::{Value, json};

const FRONT_ORACLE: &str = "Flying, ward {2}\nThis creature enters prepared.\nWhenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared.";
const SPELL_ORACLE: &str = "Target player draws three cards.";

fn parse_prepare_card(front_oracle: &str, spell_oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Emeritus of Ideation // Ancestral Recall".to_string(),
        type_line: "Creature — Human Wizard // Instant".to_string(),
        mana_cost: Some("{3}{U}{U} // {U}".to_string()),
        oracle_text: None,
        layout: Some("prepare".to_string()),
        faces: vec![
            OracleCardFace {
                id: "emeritus-of-ideation".to_string(),
                name: "Emeritus of Ideation".to_string(),
                type_line: "Creature — Human Wizard".to_string(),
                mana_cost: Some("{3}{U}{U}".to_string()),
                oracle_text: front_oracle.to_string(),
                power: Some("5".to_string()),
                toughness: Some("5".to_string()),
                loyalty: None,
            },
            OracleCardFace {
                id: "ancestral-recall".to_string(),
                name: "Ancestral Recall".to_string(),
                type_line: "Instant".to_string(),
                mana_cost: Some("{U}".to_string()),
                oracle_text: spell_oracle.to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    })
}

fn parse_single(name: &str, type_line: &str, oracle: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{3}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn emeritus_and_linked_spell_are_fully_native_and_match_ground_truth() {
    let truth: Value = serde_json::from_str(include_str!(
        "../../../fixtures/oracle-ground-truth/cards/emeritus-of-ideation.json"
    ))
    .expect("Emeritus ground truth is valid JSON");
    let parsed = parse_prepare_card(FRONT_ORACLE, SPELL_ORACLE);

    // CR 715: the engine has no prepare-layout casting runtime yet, so the
    // card is not executable coverage although every ability is recognized.
    assert_eq!(
        parsed.status,
        "unsupported",
        "{}",
        serde_json::to_string_pretty(&parsed).expect("serialize parser diagnostics")
    );
    assert_eq!(parsed.abilities.len(), 4);
    for (index, ability) in parsed.abilities[..3].iter().enumerate() {
        assert_eq!(ability.status, "canonical");
        assert_eq!(ability.recognizer, RecognizerProvenance::Native);
        assert_eq!(
            ability.rule,
            Some(truth["abilities"][index]["expectedRule"].clone())
        );
    }
    let prepared_spell = &parsed.abilities[3];
    assert_eq!(prepared_spell.recognizer, RecognizerProvenance::Native);
    assert_eq!(prepared_spell.status, "unsupported");
    assert_eq!(prepared_spell.rule, None);
    assert_eq!(prepared_spell.diagnostics.len(), 1);
    assert_eq!(
        prepared_spell.diagnostics[0].code,
        "card_layout_runtime_missing"
    );
    assert_eq!(
        prepared_spell.diagnostics[0].state.as_deref(),
        Some("card-structure.layout-runtime")
    );

    // The linked spell's grammar still lowers to its ground-truth IR.
    let spell = parse_single("Ancestral Recall", "Instant", SPELL_ORACLE);
    assert_eq!(spell.status, "canonical");
    assert_eq!(spell.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        spell.abilities[0].rule,
        Some(truth["abilities"][3]["expectedRule"].clone())
    );
    let unknowns = parsed
        .abilities
        .iter()
        .flat_map(|ability| &ability.entities)
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();
    assert!(unknowns.is_empty(), "unresolved tokens: {unknowns:?}");
}

#[test]
fn sibling_composes_a_different_count_anaphor_and_source_state() {
    let parsed = parse_single(
        "Patient Engine",
        "Artifact Creature — Construct",
        "Whenever this artifact attacks, you may exile three cards from your graveyard. If you do, it becomes tapped.",
    );

    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
    let rule = parsed.abilities[0].rule.as_ref().expect("native rule");
    assert_eq!(rule["effects"][0]["action"]["count"], 3);
    assert_eq!(rule["effects"][0]["onPerformed"][0]["kind"], "tapPermanent");
    assert_eq!(
        rule["effects"][0]["onPerformed"][0]["permanent"],
        json!({ "kind": "self" })
    );

    let draw = parse_single("Small Insight", "Sorcery", "Target player draws a card.");
    assert_eq!(draw.status, "canonical");
    assert_eq!(draw.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        draw.abilities[0].rule.as_ref().expect("draw rule")["effects"][0]["count"],
        json!({ "kind": "integer", "value": 1 })
    );
}

#[test]
fn malformed_continuation_and_draw_noun_fail_at_the_first_local_state() {
    let continuation = parse_single(
        "Patient Engine",
        "Artifact Creature — Construct",
        "Whenever this artifact attacks, you may exile three cards from your graveyard. If you do, it becomes ready.",
    );
    let ability = &continuation.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    // The shared trigger program reports its own committed failure first;
    // the optional zone-action family's precise state stays visible.
    assert!(
        ability.diagnostics.iter().any(|diagnostic| {
            diagnostic.automaton.as_deref() == Some("optional-zone-action-trigger")
                && diagnostic.state.as_deref() == Some("effect.source-state.state")
                && diagnostic.found.as_deref() == Some("ready")
        }),
        "{:#?}",
        ability.diagnostics
    );

    let draw = parse_single(
        "Wrong Insight",
        "Sorcery",
        "Target player draws three spells.",
    );
    let ability = &draw.abilities[0];
    assert_eq!(ability.status, "unsupported");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("collection.card-noun")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("spells"));
}

#[test]
fn simple_target_draw_claim_does_not_capture_a_larger_effect_sequence() {
    let parsed = parse_single(
        "Costly Insight",
        "Sorcery",
        "Target player draws two cards and loses 2 life.",
    );

    // The simple draw family does not claim it; the shared sequence composes
    // both actions over the one declared target player.
    assert_eq!(parsed.status, "canonical");
    let target = json!({ "kind": "chosenTarget", "id": "targetPlayer" });
    assert_eq!(
        parsed.abilities[0].rule.as_ref(),
        Some(&json!({
            "kind": "spellAbility",
            "source": { "kind": "self" },
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [{
                    "kind": "chooseTargets",
                    "id": "targetPlayer",
                    "minimum": 1,
                    "maximum": 1,
                    "candidates": { "kind": "players" },
                }],
            },
            "effects": [
                {
                    "kind": "drawCards",
                    "player": target.clone(),
                    "count": { "kind": "integer", "value": 2 },
                },
                {
                    "kind": "loseLife",
                    "player": target,
                    "amount": { "kind": "integer", "value": 2 },
                },
            ],
        }))
    );
}
