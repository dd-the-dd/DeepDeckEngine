use deepdeck_oracle::oracle::{
    OracleCardParseRequest, RecognizerProvenance, parse_oracle_card, parse_oracle_card_native,
};

fn request(name: &str, type_line: &str, oracle_text: &str) -> OracleCardParseRequest {
    OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    }
}

#[test]
fn native_only_preserves_native_ir_without_legacy_classification() {
    let request = request(
        "Thought Vessel",
        "Artifact",
        "You have no maximum hand size.",
    );
    let full = parse_oracle_card(request.clone());
    let native = parse_oracle_card_native(request);

    assert_eq!(native.status, "canonical");
    assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        native.abilities[0].ability_type,
        full.abilities[0].ability_type
    );
    assert_eq!(native.abilities[0].rule, full.abilities[0].rule);
    assert_eq!(
        serde_json::to_value(&native.abilities[0].diagnostics).unwrap(),
        serde_json::to_value(&full.abilities[0].diagnostics).unwrap()
    );
}

#[test]
fn both_public_entry_points_reject_an_unclaimed_static_rule() {
    // A static anthem is now owned by the shared static-continuous parent;
    // both entry points produce the same primitive rule.
    let anthem = request(
        "Legacy Static Bonus",
        "Enchantment",
        "Creatures you control get +1/+1.",
    );
    let full = parse_oracle_card(anthem.clone());
    let native = parse_oracle_card_native(anthem);
    assert_eq!(full.status, "canonical");
    assert_eq!(
        full.abilities[0].rule.as_ref().expect("anthem rule")["modifiers"][0]["kind"],
        "modifyPowerToughness"
    );
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&native).unwrap()
    );

    let request = request(
        "Legacy Static Bonus",
        "Enchantment",
        // `are blue` and `every creature type` are now changes the static
        // frame lowers (CR 105.3, 205.3m); `every color` still has no
        // type-change reader.
        "Creatures you control are every color.",
    );
    let full = parse_oracle_card(request.clone());
    let native = parse_oracle_card_native(request);

    // CR 113.3: the static frame owns the ability and reports where its
    // type-change predicate stopped; nothing is left unclaimed.
    assert_eq!(full.status, "unsupported");
    assert_eq!(full.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(full.abilities[0].status, "unsupported");
    assert!(full.abilities[0].rule.is_none());
    assert_eq!(
        serde_json::to_value(&full).unwrap(),
        serde_json::to_value(&native).unwrap()
    );
    assert_eq!(native.status, "unsupported");
    assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(native.abilities[0].ability_type, "staticAbility");
    assert_eq!(native.abilities[0].status, "unsupported");
    assert!(native.abilities[0].rule.is_none());
    let diagnostic = &native.abilities[0].diagnostics[0];
    assert_eq!(diagnostic.code, "oracle_machine_failure");
    assert_eq!(
        diagnostic.automaton.as_deref(),
        Some("static-continuous-effect")
    );
    assert_eq!(
        diagnostic.state.as_deref(),
        Some("static.continuous.type-change")
    );
}

#[test]
fn native_only_preserves_committed_machine_failure_diagnostics() {
    let request = request("Near Miss", "Artifact", "You have no minimum hand size.");
    let full = parse_oracle_card(request.clone());
    let native = parse_oracle_card_native(request);

    assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(native.abilities[0].status, "unsupported");
    assert_eq!(
        serde_json::to_value(&native.abilities[0].diagnostics).unwrap(),
        serde_json::to_value(&full.abilities[0].diagnostics).unwrap()
    );
    assert_eq!(
        native.abilities[0].diagnostics[0].state.as_deref(),
        Some("modifier.no-maximum-hand-size.maximum")
    );
}

#[test]
fn native_only_keeps_class_level_context_for_following_native_rules() {
    let parsed = parse_oracle_card_native(request(
        "Native Class Context",
        "Enchantment — Class",
        "{1}{B}: Level 2\nYou have no maximum hand size.",
    ));

    assert_eq!(parsed.abilities.len(), 2);
    assert_eq!(parsed.abilities[1].recognizer, RecognizerProvenance::Native);
    assert_eq!(
        parsed.abilities[1].rule.as_ref().unwrap()["minimumClassLevel"],
        2
    );
}

#[test]
fn native_only_preserves_overload_keyword_when_spell_sibling_is_unsupported() {
    // Mind Rake's real body (`Target player discards two cards.`) and a
    // distributive `for each creature they control` are now canonical; an
    // unsupported verb keeps the spell sibling unsupported while the
    // overload keyword stays independent.
    let request = request(
        "Test Mind Rake",
        "Sorcery",
        "Target player seeks two cards.\nOverload {1}{B} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")",
    );
    let native = parse_oracle_card_native(request);

    assert_eq!(native.status, "unsupported");
    assert_eq!(native.abilities[0].recognizer, RecognizerProvenance::Native);
    assert_eq!(native.abilities[0].status, "unsupported");
    assert!(native.abilities[0].rule.is_none());
    assert_eq!(native.abilities[1].recognizer, RecognizerProvenance::Native);
    assert_eq!(native.abilities[1].status, "canonical");
    // Both public entry points retain the unsupported spell body.
    // This is independent keyword recognition, never complete-card coverage.
    let keyword = native.abilities[1].rule.as_ref().unwrap();
    assert_eq!(keyword["ability"]["kind"], "alternativeCost");
    assert_eq!(keyword["ability"]["mode"], "overload");
    assert_eq!(keyword["ability"]["costs"][0]["manaCost"], "{1}{B}");
    assert!(
        native.abilities[1].rule.as_ref().unwrap()["ability"]
            .get("compileDirective")
            .is_none()
    );
}

#[test]
fn native_only_requires_a_spell_body_before_overload_can_be_composed() {
    for text in ["Overload {1}{B}", "You gain 2 life.\nOverload {1}{B}"] {
        let parsed = parse_oracle_card_native(request("Missing Target Source", "Sorcery", text));
        assert_eq!(parsed.status, "unsupported");
        let keyword = parsed.abilities.last().unwrap();
        assert_eq!(keyword.status, "unsupported");
        assert!(keyword.rule.is_none());
        assert!(keyword.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "oracle_card_composition_failure"
                && diagnostic.state.as_deref() == Some("composition.target-rule")
        }));
    }
}
