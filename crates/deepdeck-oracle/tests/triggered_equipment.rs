use deepdeck_oracle::oracle::{OracleCardParseRequest, parse_oracle_card};

fn parse_equipment_ability(oracle_text: &str) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_named_equipment_ability("Test Equipment", oracle_text)
}

fn parse_named_equipment_ability(
    card_name: &str,
    oracle_text: &str,
) -> deepdeck_oracle::oracle::OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: "Artifact — Equipment".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle_text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

#[test]
fn equipment_entry_attach_then_untap_lowers_to_independent_primitives() {
    for (recipient, expected_filter_kind) in [
        ("creature you control", "cardTypeContains"),
        ("artifact creature you control", "and"),
    ] {
        let parsed = parse_equipment_ability(&format!(
            "When this Equipment enters, attach it to target {recipient}. Untap that creature."
        ));

        assert_eq!(parsed.status, "canonical");
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the entry trigger should produce primitive IR");
        assert_eq!(rule["event"]["kind"], "enterBattlefield");
        assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
        assert_eq!(
            rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
            expected_filter_kind
        );
        assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
        assert_eq!(rule["effects"][1]["kind"], "untapPermanent");
        assert_eq!(rule["effects"][0]["to"], rule["effects"][1]["permanent"]);
        assert!(rule.to_string().find("twinBladesEnter").is_none());
        assert!(
            rule.to_string()
                .find("resolveTriggeredInstruction")
                .is_none()
        );
    }
}

#[test]
fn equipment_entry_attach_then_keyword_reuses_the_same_target_binding() {
    for (card_name, keyword_text, expected_keyword) in [
        ("Twin Blades", "double strike", "doubleStrike"),
        ("Test Harness", "trample", "trample"),
    ] {
        let parsed = parse_named_equipment_ability(
            card_name,
            &format!(
                "When this Equipment enters, attach it to target creature you control. That creature gains {keyword_text} until end of turn."
            ),
        );

        assert_eq!(parsed.status, "canonical");
        let rule = parsed.abilities[0]
            .rule
            .as_ref()
            .expect("the entry trigger should produce primitive IR");
        assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
        assert_eq!(rule["effects"][1]["kind"], "grantKeyword");
        assert_eq!(rule["effects"][1]["keyword"], expected_keyword);
        assert_eq!(rule["effects"][0]["to"], rule["effects"][1]["object"]);
        assert_eq!(
            rule["effects"][1]["duration"]["kind"],
            "untilEndOfCurrentTurn"
        );
        assert!(rule.to_string().find("twinBladesEnter").is_none());
        assert!(
            rule.to_string()
                .find("resolveTriggeredInstruction")
                .is_none()
        );
    }
}

#[test]
fn unresolved_that_creature_reference_is_rejected() {
    for oracle_text in [
        "When this Equipment enters, untap that creature.",
        "When this Equipment enters, attach it to target land you control. Untap that creature.",
        "When this Equipment enters, attach it to target land you control. That creature gains flying until end of turn.",
        "When this Equipment enters, target player draws a card. Untap that permanent.",
        "When this Equipment enters, target player draws a card. That permanent gains flying until end of turn.",
    ] {
        let parsed = parse_equipment_ability(oracle_text);

        assert_eq!(parsed.status, "unsupported");
        assert!(parsed.abilities[0].rule.is_none());
    }
}
