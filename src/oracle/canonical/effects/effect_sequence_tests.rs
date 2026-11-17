use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

#[test]
fn initial_another_modifier_retains_source_exclusion_without_prior_targets() {
    for text in [
        "Another target creature gets +2/+2 until end of turn.",
        "Draw a card. Another target creature gets +2/+2 until end of turn.",
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("initial-another-modifier-test", &tokens);
        let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect("shared sequence");
        let EffectSequenceLoweringAttempt::Lowered(lowered) =
            try_lower_local_effect_sequence(&mut cursor, &sequence)
        else {
            panic!("first target still means another object than the source");
        };
        assert_eq!(lowered.decisions.len(), 1);
        assert_eq!(lowered.decisions[0]["candidates"]["excludeSource"], true);
    }
}

#[test]
fn another_modifier_after_prior_targets_is_a_typed_relation_support_limit() {
    for text in [
        "Target creature gets +2/+2 until end of turn. Another target creature gets -2/-2 until end of turn.",
        "Destroy target artifact. Another target creature gets +2/+2 until end of turn.",
        "This spell deals 1 damage to target creature. Another target creature gets +2/+2 until end of turn.",
        "Target player loses 1 life. Another target creature gets +2/+2 until end of turn.",
        "Return target creature card from your graveyard to your hand. Another target creature gets +2/+2 until end of turn.",
    ] {
        let tokens = recognize_oracle_entities(text);
        let mut cursor = OracleTokenCursor::new("later-another-modifier-test", &tokens);
        let sequence = recognize_effect_sequence(&mut cursor, &[], None).expect(text);
        let EffectSequenceLoweringAttempt::FailedSupport(failure) =
            try_lower_local_effect_sequence(&mut cursor, &sequence)
        else {
            panic!("missing relation must not become source exclusion: {text}");
        };
        assert_eq!(failure.state, "lower.modify-stats.another-target-relation");
        assert!(failure.expected.contains("distinct-target relation"));
    }
}

fn source(text: &str) -> AbilitySource {
    AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    }
}

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = source(text);
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Modifier",
        face_type_line: "Instant",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    // The shared spell program owns this grammar; use the public dispatcher.
    match parse_canonical_rule(&input, "spellAbility") {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => Err(failure),
        CanonicalParseOutcome::Unsupported => panic!("no native owner for {text}"),
    }
}

#[test]
fn standalone_modifier_is_one_ordinary_sequence_action() {
    let rule = parse("Target creature gets -2/-2 until end of turn.")
        .expect("standalone modifier parses")
        .rule;

    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
    assert_eq!(rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][0]["power"]["value"], -2);
    assert_eq!(rule["effects"][0]["toughness"]["value"], -2);
}

#[test]
fn untap_sibling_reuses_the_exact_modifier_target_binding() {
    let rule = parse("Target creature gets +2/+2 and gains reach until end of turn. Untap it.")
        .expect("modifier followed by bound untap parses")
        .rule;

    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("target ID");
    assert_eq!(rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][2]["kind"], "untapPermanent");
    assert_eq!(rule["effects"][2]["permanent"]["id"], target_id);
}

#[test]
fn draw_sibling_uses_the_shared_targetless_action_lowerer() {
    let rule = parse("Target creature gets +3/+3 until end of turn. Draw a card.")
        .expect("modifier followed by draw parses")
        .rule;

    assert_eq!(rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
    assert_eq!(rule["effects"][1]["count"]["value"], 1);
}

#[test]
fn distinct_target_slots_receive_distinct_ids() {
    let rule = parse("Target creature and target land get +1/+1 until end of turn.")
        .expect("two independent target slots parse")
        .rule;

    assert_eq!(
        rule["declaration"]["decisions"][0]["id"],
        "targetPermanent1"
    );
    assert_eq!(
        rule["declaration"]["decisions"][1]["id"],
        "targetPermanent2"
    );
    assert_eq!(rule["effects"][0]["object"]["id"], "targetPermanent1");
    assert_eq!(rule["effects"][1]["object"]["id"], "targetPermanent2");
}

#[test]
fn cardinalized_slot_lowers_one_plural_target_reference_for_every_action() {
    // `N` is the shared numeric atom: words and digits give the same IR.
    let words = parse("Up to two target creatures get +1/+1 and gain flying until end of turn.")
        .expect("a permanent target collection lowers through chosenTargets")
        .rule;
    let digits = parse("Up to 2 target creatures get +1/+1 and gain flying until end of turn.")
        .expect("a digit cardinality is the same numeric atom")
        .rule;
    assert_eq!(words, digits);
    let decision = &words["declaration"]["decisions"][0];
    assert_eq!(
        (decision["minimum"].as_i64(), decision["maximum"].as_i64()),
        (Some(0), Some(2))
    );
    let plural = serde_json::json!({"kind":"chosenTargets","id":decision["id"].clone()});
    assert_eq!(words["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(words["effects"][0]["object"], plural);
    assert_eq!(words["effects"][1]["kind"], "grantKeyword");
    assert_eq!(words["effects"][1]["object"], plural);

    let exact = parse("Three target creatures get -1/-1 until end of turn.")
        .expect("an exact cardinality is a bounded collection")
        .rule;
    let decision = &exact["declaration"]["decisions"][0];
    assert_eq!(
        (decision["minimum"].as_i64(), decision["maximum"].as_i64()),
        (Some(3), Some(3))
    );
    // A single target remains a scalar reference.
    let scalar = parse("Up to one target creature gets +1/+1 until end of turn.")
        .expect("up to one remains scalar")
        .rule;
    assert_eq!(scalar["effects"][0]["object"]["kind"], "chosenTarget");
}

#[test]
fn plural_target_slot_never_binds_a_singular_anaphor() {
    let text = "Up to two target creatures get +1/+1 until end of turn. Untap it.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.plural-anaphor", &tokens);
    let Ok(sequence) = recognize_effect_sequence(&mut cursor, &[], None) else {
        return; // grammar refusal is also an explicit non-claim
    };
    assert!(
        lower_local_effect_sequence(&mut cursor, &sequence).is_err(),
        "`it` must not silently denote one member of a target collection"
    );
}

#[test]
fn two_targeted_card_moves_allocate_distinct_target_bindings() {
    let text = "Return target artifact card from your graveyard to your hand. Then put target card from a graveyard on top of its owner's library.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.two-targeted-card-moves", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("two independent targeted card moves should compose");
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)
        .expect("each move should retain its own declaration binding");

    assert_eq!(lowered.decisions[0]["id"], "targetCard1");
    assert_eq!(lowered.decisions[1]["id"], "targetCard2");
    assert_eq!(lowered.effects[0]["cards"]["id"], "targetCard1");
    assert_eq!(lowered.effects[1]["cards"]["id"], "targetCard2");
    assert_eq!(
        lowered.effects[1]["to"]["player"]["object"]["id"],
        "targetCard2"
    );
}

#[test]
fn card_target_cannot_bind_a_battlefield_untap_anaphor() {
    let text = "Return target creature card from your graveyard to your hand. Then untap it.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.card-target-untap-domain", &tokens);
    let failure = recognize_effect_sequence(&mut cursor, &[], None)
        .expect_err("a card-zone target cannot establish a permanent anaphor");

    assert_eq!(failure.state, "effect.untap-target.reference");
    assert_eq!(
        failure.expected,
        "a preceding singular target or observed event-object binding"
    );
}

#[test]
fn targeted_damage_and_zone_change_use_distinct_shared_target_ids() {
    let text = "This spell deals 3 damage to target creature. Then destroy target artifact.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.damage-zone-sequence", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("targeted damage and destruction should compose");
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)
        .expect("both ordinary targeted actions should lower");

    assert_eq!(lowered.decisions[0]["id"], "targetPermanent1");
    assert_eq!(lowered.decisions[1]["id"], "targetPermanent2");
    assert_eq!(lowered.effects[0]["recipient"]["id"], "targetPermanent1");
    assert_eq!(lowered.effects[1]["permanent"]["id"], "targetPermanent2");
}

#[test]
fn amended_destroy_is_an_ordinary_sequence_action() {
    let text = "Destroy target creature. It can't be regenerated. You gain 2 life.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.amended-destroy-sequence", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("amended destroy and sibling action should compose");
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)
        .expect("amended destroy should lower through the common sequence");
    assert_eq!(lowered.effects[0]["cannotRegenerate"], true);
    assert_eq!(lowered.effects[1]["kind"], "gainLife");
}

#[test]
fn two_destroy_targets_keep_distinct_ids_and_one_policy() {
    let text = "Destroy target artifact. Then destroy target creature. It can't be regenerated.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("test.two-destroy-policies", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("two independent destroys should compose");
    let lowered = lower_local_effect_sequence(&mut cursor, &sequence)
        .expect("both destroys should lower with distinct decisions");
    assert_eq!(lowered.decisions[0]["id"], "targetPermanent1");
    assert_eq!(lowered.decisions[1]["id"], "targetPermanent2");
    assert!(lowered.effects[0].get("cannotRegenerate").is_none());
    assert_eq!(lowered.effects[1]["cannotRegenerate"], true);
}

#[test]
fn collection_destroy_keeps_priority_over_targeted_amendment_child() {
    let tokens = recognize_oracle_entities("Destroy all creatures.");
    let mut cursor = OracleTokenCursor::new("test.destroy-collection-priority", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("collection destroy remains owned by its existing child");
    assert!(matches!(
        sequence.effects.as_slice(),
        [LocalEffectNode::DestroyCollection(_)]
    ));
}

#[test]
fn deferred_card_x_context_lowers_references_without_inventing_a_producer() {
    let tokens = recognize_oracle_entities("Draw X cards. You gain X life.");
    let mut cursor = OracleTokenCursor::new("test.deferred-card-x", &tokens);
    let sequence = recognize_effect_sequence(&mut cursor, &[], None)
        .expect("X is syntactically recognized independently of its producer");

    assert!(matches!(
        try_lower_local_effect_sequence(&mut cursor, &sequence),
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(_)
    ));
    let EffectSequenceLoweringAttempt::Lowered(lowered) =
        try_lower_local_effect_sequence_with_context(
            &mut cursor,
            &sequence,
            EffectSequenceLoweringContext::ValidateXAfterCardComposition,
        )
    else {
        panic!("card context must retain the provisional X references");
    };
    assert!(
        lowered.decisions.is_empty(),
        "lowering cannot invent a producer"
    );
    assert_eq!(lowered.effects.len(), 2);
    assert_eq!(lowered.effects[0]["count"], decision_result("xValue"));
    assert_eq!(lowered.effects[1]["amount"], decision_result("xValue"));
    // Opting one caller into deferred binding must not change the old wrapper.
    assert!(matches!(
        try_lower_local_effect_sequence(&mut cursor, &sequence),
        EffectSequenceLoweringAttempt::UnsupportedSupportLimit(_)
    ));
}

#[test]
fn unimplemented_distinct_target_relation_is_unsupported_in_full_and_native_modes() {
    use crate::oracle::{OracleCardParseRequest, parse_oracle_card, parse_oracle_card_native};
    let request = OracleCardParseRequest {
        card_name: "Synthetic Hard Another Relation".into(), type_line: "Sorcery".into(),
        mana_cost: Some("{1}{G}".into()),
        oracle_text: Some("Target creature gets +2/+2 until end of turn. Another target creature gets -2/-2 until end of turn.".into()),
        layout: Some("normal".into()), faces: vec![],
    };
    for parsed in [
        parse_oracle_card_native(request.clone()),
        parse_oracle_card(request),
    ] {
        assert_eq!(parsed.status, "unsupported");
        assert!(
            parsed
                .abilities
                .iter()
                .all(|ability| ability.rule.is_none())
        );
        assert!(
            parsed
                .abilities
                .iter()
                .any(|ability| ability.diagnostics.iter().any(|diagnostic| {
                    diagnostic.state.as_deref()
                        == Some("lower.modify-stats.another-target-relation")
                }))
        );
    }
}

#[test]
fn targeted_untap_lowers_through_the_shared_tap_verb_frame() {
    let lands = serde_json::json!({"kind":"permanents",
        "where":{"kind":"cardTypeContains","value":"Land"}});
    // A bounded target slot lowers to one plural reference (CR 115.1).
    for (text, minimum, maximum) in [
        ("Untap up to three target lands.", 0, 3),
        ("Untap two target lands.", 2, 2),
    ] {
        let rule = parse(text).expect(text).rule;
        assert_eq!(
            rule["declaration"]["decisions"],
            serde_json::json!([{"kind":"chooseTargets","id":"targetPermanent",
                "minimum":minimum,"maximum":maximum,"candidates":lands.clone()}]),
            "{text}"
        );
        assert_eq!(
            rule["effects"],
            serde_json::json!([{"kind":"untapPermanents",
                "objects":{"kind":"chosenTargets","id":"targetPermanent"}}]),
            "{text}"
        );
    }
    // A single target stays a scalar reference and composes with later actions.
    let rule = parse("Untap target creature. Draw a card.")
        .expect("scalar untap")
        .rule;
    assert_eq!(
        rule["effects"][0],
        serde_json::json!({"kind":"untapPermanent",
            "permanent":{"kind":"chosenTarget","id":"targetPermanent"}})
    );
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
    // Near miss: a target slot without an object domain is never invented.
    assert!(parse("Untap up to three target.").is_err());
}
