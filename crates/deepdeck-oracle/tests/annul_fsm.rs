use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::json;

fn parse(oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Annul".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

#[test]
fn artifact_or_enchantment_spell_is_one_native_union_target() {
    let parsed = parse("Counter target artifact or enchantment spell.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
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
                            "kind": "or",
                            "operands": [
                                { "kind": "cardTypeContains", "value": "Artifact" },
                                { "kind": "cardTypeContains", "value": "Enchantment" },
                            ],
                        },
                    },
                }],
            },
            "effects": [{
                "kind": "counterSpell",
                "spell": { "kind": "chosenTarget", "id": "targetSpell" },
            }],
        }))
    );
}

#[test]
fn instant_or_sorcery_spell_reuses_the_same_union_and_counter_primitive() {
    let parsed = parse("Counter target instant or sorcery spell.");
    let ability = &parsed.abilities[0];

    assert_eq!(parsed.status, "canonical");
    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    let rule = ability.rule.as_ref().expect("sibling emits primitive IR");
    assert_eq!(rule["effects"][0]["kind"], "counterSpell");
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"],
        json!({
            "kind": "or",
            "operands": [
                { "kind": "cardTypeContains", "value": "Instant" },
                { "kind": "cardTypeContains", "value": "Sorcery" },
            ],
        })
    );
}

#[test]
fn missing_or_wrong_shared_spell_domain_is_a_committed_native_failure() {
    let parsed = parse("Counter target artifact or spell.");
    let ability = &parsed.abilities[0];

    assert_eq!(ability.recognizer, RecognizerProvenance::Native);
    assert_eq!(ability.status, "unsupported");
    assert!(ability.rule.is_none());
    assert_eq!(
        ability.diagnostics[0].automaton.as_deref(),
        Some("type-union-spell-counter")
    );
    assert_eq!(
        ability.diagnostics[0].state.as_deref(),
        Some("effect.counter.target-domain")
    );
    assert_eq!(ability.diagnostics[0].found.as_deref(), Some("."));
}

#[test]
fn union_boundary_does_not_claim_a_different_domain_or_compound_sentence() {
    for oracle_text in [
        "Counter target artifact or enchantment card.",
        "Counter target artifact or enchantment spell. Draw a card.",
    ] {
        let parsed = parse(oracle_text);
        let ability = &parsed.abilities[0];
        assert!(
            ability.diagnostics.iter().all(|diagnostic| {
                diagnostic.automaton.as_deref() != Some("type-union-spell-counter")
            }),
            "union boundary unexpectedly claimed: {oracle_text}"
        );
    }
}

#[test]
fn promoted_annul_vocabulary_has_no_unknown_tokens() {
    let parsed = parse("Counter target artifact or enchantment spell.");
    let unknown = parsed.abilities[0]
        .entities
        .iter()
        .filter(|entity| entity.token_type == "unknown")
        .map(|entity| entity.raw.as_str())
        .collect::<Vec<_>>();

    assert!(unknown.is_empty(), "unknown tokens: {unknown:?}");
}
