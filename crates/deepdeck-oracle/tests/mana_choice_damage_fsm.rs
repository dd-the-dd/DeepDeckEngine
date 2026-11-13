use deepdeck_oracle::oracle::{
    OracleCardParseRequest, OracleCardParseResult, RecognizerProvenance, parse_oracle_card,
};
use serde_json::{Value, json};

fn parse_artifact(card_name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    })
}

fn choice_damage_rule(options: &[&str], amount: i64) -> Value {
    json!({
        "kind": "manaAbility",
        "source": { "kind": "self" },
        "costs": [{ "kind": "tap", "object": { "kind": "self" } }],
        "effects": [
            {
                "kind": "addMana",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
                "mana": { "kind": "chooseOne", "options": options },
            },
            {
                "kind": "dealDamage",
                "source": { "kind": "self" },
                "amount": { "kind": "integer", "value": amount },
                "recipient": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
        ],
    })
}

#[test]
fn two_ability_artifact_preserves_fixed_mana_and_natively_composes_choice_with_damage() {
    let parsed = parse_artifact(
        "Talisman of Dominance",
        "{T}: Add {C}.\n{T}: Add {U} or {B}. This artifact deals 1 damage to you.",
    );
    assert_eq!(parsed.status, "canonical");
    assert_eq!(parsed.abilities.len(), 2);
    assert!(
        parsed
            .abilities
            .iter()
            .all(|ability| ability.recognizer == RecognizerProvenance::Native)
    );
    assert_eq!(
        parsed.abilities[0].rule.as_ref().unwrap()["effects"][0]["mana"],
        "{C}"
    );
    assert_eq!(
        parsed.abilities[1].rule.as_ref(),
        Some(&choice_damage_rule(&["{U}", "{B}"], 1))
    );
    assert!(
        !parsed.abilities[1]
            .rule
            .as_ref()
            .unwrap()
            .to_string()
            .contains("resolveSpellInstruction")
    );
}

#[test]
fn mana_choice_and_damage_amount_generalize_independently() {
    let sibling = parse_artifact(
        "Painful Prism",
        "{T}: Add {R} or {G}. Painful Prism deals 2 damage to you.",
    );
    assert_eq!(sibling.status, "canonical");
    assert_eq!(
        sibling.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(
        sibling.abilities[0].rule.as_ref(),
        Some(&choice_damage_rule(&["{R}", "{G}"], 2))
    );

    let choice_only = parse_artifact("Dual Source", "{T}: Add {G} or {U}.");
    assert_eq!(choice_only.status, "canonical");
    assert_eq!(
        choice_only.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    let rule = choice_only.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        rule["effects"][0]["mana"],
        json!({ "kind": "chooseOne", "options": ["{G}", "{U}"] })
    );
    assert_eq!(rule["effects"].as_array().unwrap().len(), 1);
}

#[test]
fn typed_this_source_reference_is_shared_across_permanent_kinds() {
    for source_noun in ["artifact", "creature", "enchantment", "land", "permanent"] {
        let parsed = parse_artifact(
            "Typed Source",
            &format!("{{T}}: Add {{U}} or {{B}}. This {source_noun} deals 1 damage to you."),
        );
        assert_eq!(parsed.status, "canonical", "{source_noun}");
        assert_eq!(
            parsed.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{source_noun}"
        );
        assert_eq!(
            parsed.abilities[0].rule.as_ref(),
            Some(&choice_damage_rule(&["{U}", "{B}"], 1)),
            "{source_noun}"
        );
    }
}

#[test]
fn every_recognized_mana_output_can_compose_shared_targetless_effects() {
    let fixed_damage = parse_artifact(
        "Painful Obelisk",
        "{T}: Add {R}. Painful Obelisk deals 2 damage to you.",
    );
    let rule = fixed_damage.abilities[0].rule.as_ref().unwrap();
    assert_eq!(
        fixed_damage.abilities[0].recognizer,
        RecognizerProvenance::Native
    );
    assert_eq!(rule["effects"][0]["mana"], "{R}");
    assert_eq!(rule["effects"][1]["kind"], "dealDamage");
    assert_eq!(
        rule["effects"][1]["amount"],
        json!({ "kind": "integer", "value": 2 })
    );

    let gain = parse_artifact("Kindly Relic", "{T}: Add {W}. You gain 2 life.");
    let rule = gain.abilities[0].rule.as_ref().unwrap();
    assert_eq!(gain.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(rule["effects"][0]["mana"], "{W}");
    assert_eq!(rule["effects"][1]["kind"], "gainLife");
    assert_eq!(
        rule["effects"][1]["player"],
        json!({ "kind": "controllerOf", "object": { "kind": "self" } })
    );

    let draw = parse_artifact(
        "Scholar's Relic",
        "{T}: Add one mana of any color. Draw a card.",
    );
    let rule = draw.abilities[0].rule.as_ref().unwrap();
    assert_eq!(draw.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        rule["effects"][0]["mana"],
        json!({ "kind": "chooseColor", "amount": 1 })
    );
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
    assert_eq!(
        rule["effects"][1]["count"],
        json!({ "kind": "integer", "value": 1 })
    );
}

#[test]
fn claimed_mana_choice_near_misses_fail_at_the_responsible_child() {
    for (oracle_text, state, found) in [
        (
            "{T}: Add {U} or mana. Broken Source deals 1 damage to you.",
            "mana.fixed.start",
            "mana",
        ),
        (
            "{T}: Add {U} or {B}. Broken Source deals damage to you.",
            "numeric.atom.start",
            "damage",
        ),
    ] {
        let parsed = parse_artifact("Broken Source", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        assert_eq!(
            ability.diagnostics[0].automaton.as_deref(),
            Some("activated-mana-ability")
        );
        assert_eq!(ability.diagnostics[0].state.as_deref(), Some(state));
        assert_eq!(ability.diagnostics[0].found.as_deref(), Some(found));
    }
}

#[test]
fn followups_that_need_a_target_or_declaration_fail_structurally() {
    for (oracle_text, expected) in [
        (
            "{T}: Add {U}. Broken Source deals 1 damage to target creature.",
            "a targetless effect requiring no target declaration",
        ),
        (
            "{T}: Add {U}. Broken Source deals X damage to you.",
            "a targetless effect requiring no unavailable value declaration",
        ),
    ] {
        let parsed = parse_artifact("Broken Source", oracle_text);
        let ability = &parsed.abilities[0];
        assert_eq!(
            ability.recognizer,
            RecognizerProvenance::Native,
            "{oracle_text}"
        );
        assert_eq!(ability.status, "unsupported", "{oracle_text}");
        assert_eq!(
            ability.diagnostics[0].state.as_deref(),
            Some("activation.mana.followup.targetless-lowering")
        );
        assert_eq!(ability.diagnostics[0].expected.as_deref(), Some(expected));
    }
}
