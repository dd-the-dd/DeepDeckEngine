use deepdeck_oracle::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse_aura(name: &str, oracle_text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: "Enchantment — Aura".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Test Infiltration; see docs/coverage-targets.md"
)]
#[test]
fn attached_object_can_copy_a_linked_exiled_permanent_for_the_source_duration() {
    let parsed = parse_aura(
        "Test Infiltration",
        "Enchant creature you control\nWhen this Aura enters, exile up to one target creature other than enchanted creature until this Aura leaves the battlefield. Enchanted creature becomes a copy of that creature until this Aura leaves the battlefield.\nEnchanted creature has ward {2}.",
    );

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    assert_eq!(parsed.abilities.len(), 3);
    let rule = parsed.abilities[1].rule.as_ref().expect("entry rule");
    let decision = &rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 0);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["where"]["value"], "Creature");
    assert_eq!(
        decision["candidates"]["exclude"]["kind"],
        "attachedPermanent"
    );
    assert_eq!(rule["effects"][0]["kind"], "exilePermanentWithSource");
    assert_eq!(rule["effects"][0]["bind"], "linkedExiledPermanent");
    assert_eq!(rule["effects"][1]["kind"], "becomeCopyOfPermanent");
    assert_eq!(rule["effects"][1]["object"]["kind"], "attachedPermanent");
    assert_eq!(rule["effects"][1]["copy"]["kind"], "singleBoundObject");
    assert_eq!(
        rule["effects"][1]["duration"]["kind"],
        "whileSourceOnBattlefield"
    );
    assert!(
        serde_json::to_string(parsed.abilities[2].rule.as_ref().unwrap())
            .expect("ward rule serializes")
            .to_ascii_lowercase()
            .contains("ward")
    );
}

#[cfg_attr(
    not(feature = "coverage-targets"),
    ignore = "coverage target: Test Reflection; see docs/coverage-targets.md"
)]
#[test]
fn target_filter_attached_subject_and_copy_noun_recompose_independently() {
    let parsed = parse_aura(
        "Test Reflection",
        "Enchant artifact you control\nWhen this Aura enters, exile target artifact other than enchanted permanent until this Aura leaves the battlefield. Enchanted permanent becomes a copy of that permanent until this Aura leaves the battlefield.",
    );

    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[1].rule.as_ref().expect("entry rule");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(
        rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Artifact"
    );
    assert_eq!(rule["effects"][1]["object"]["kind"], "attachedPermanent");
}

#[test]
fn mismatched_source_duration_is_not_silently_linked() {
    let parsed = parse_aura(
        "Test Reflection",
        "Enchant creature you control\nWhen this Aura enters, exile target creature other than enchanted creature until this enchantment leaves the battlefield. Enchanted creature becomes a copy of that creature until this Aura leaves the battlefield.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[1].rule.is_none());
}

#[test]
fn linked_copy_anaphora_rejects_a_different_object_kind() {
    let parsed = parse_aura(
        "Test Reflection",
        "Enchant creature you control\nWhen this Aura enters, exile target creature other than enchanted creature until this Aura leaves the battlefield. Enchanted creature becomes a copy of that card until this Aura leaves the battlefield.",
    );

    assert_eq!(parsed.status, "unsupported");
    assert!(parsed.abilities[1].rule.is_none());
}
