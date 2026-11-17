use crate::oracle::{OracleCardParseRequest, OracleCardParseResult, parse_oracle_card};

fn parse(type_line: &str, text: &str) -> OracleCardParseResult {
    parse_oracle_card(OracleCardParseRequest {
        card_name: "Test Card".to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    })
}

fn rule(result: &OracleCardParseResult) -> &serde_json::Value {
    result.abilities[0]
        .rule
        .as_ref()
        .expect("the ability should lower to primitive IR")
}

#[test]
fn equipment_entry_attaches_the_source_to_a_declared_target() {
    for (recipient, text) in [
        ("creature", "target creature you control"),
        ("Dwarf", "target Dwarf you control"),
    ] {
        let parsed = parse(
            "Artifact - Equipment",
            &format!("When this Equipment enters, attach it to {text}."),
        );
        assert_eq!(parsed.status, "canonical", "{recipient}");
        let rule = rule(&parsed);
        let effect = &rule["effects"][0];
        assert_eq!(effect["kind"], "attachPermanent");
        assert_eq!(effect["attachment"]["kind"], "self");
        assert_eq!(effect["to"]["kind"], "chosenTarget");
        assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
        assert_eq!(
            effect["to"]["id"],
            rule["declaration"]["decisions"][0]["id"]
        );
    }
}

#[test]
fn attach_composes_with_the_shared_anaphor_and_sentence_sequence() {
    let parsed = parse(
        "Artifact - Equipment",
        "When this Equipment enters, attach this Equipment to target creature you control. That creature gains first strike until end of turn.",
    );
    assert_eq!(
        parsed.status, "canonical",
        "{:?}",
        parsed.abilities[0].diagnostics
    );
    let rule = rule(&parsed);
    assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
    assert_eq!(rule["effects"][1]["kind"], "grantKeyword");
    assert_eq!(rule["effects"][0]["to"], rule["effects"][1]["object"]);
}

#[test]
fn attach_accepts_a_declared_attachment_and_an_optional_destination() {
    let parsed = parse(
        "Creature - Dwarf",
        "When this creature enters, attach target artifact you control to up to one target creature you control.",
    );
    assert_eq!(
        parsed.status, "canonical",
        "{:?}",
        parsed.abilities[0].diagnostics
    );
    let rule = rule(&parsed);
    let effect = &rule["effects"][0];
    assert_eq!(effect["kind"], "attachPermanent");
    assert_eq!(effect["attachment"]["kind"], "chosenTarget");
    assert_eq!(effect["to"]["kind"], "chosenTarget");
    assert_ne!(effect["attachment"]["id"], effect["to"]["id"]);
    assert_eq!(rule["declaration"]["decisions"][1]["minimum"], 0);
}

#[test]
fn attach_requires_a_resolvable_destination_and_individual_operands() {
    for text in [
        "When this Equipment enters, attach it.",
        "When this Equipment enters, attach it to.",
        "When this Equipment enters, attach it to each creature you control.",
        "When this Equipment enters, attach it to two target creatures you control.",
        "When this Equipment enters, attach each artifact you control to target creature you control.",
        "When this Equipment enters, attach it to the amassed Army.",
    ] {
        let parsed = parse("Artifact - Equipment", text);
        assert_ne!(parsed.status, "canonical", "{text}");
        assert!(parsed.abilities[0].rule.is_none(), "{text}");
    }
}

#[test]
fn nominal_anaphor_follows_the_attachment_destination_only_when_typed_compatibly() {
    for (text, expected) in [
        (
            "When this Equipment enters, attach it to target creature you control. Untap that creature.",
            true,
        ),
        (
            "When this Equipment enters, attach it to target creature you control. Untap that permanent.",
            true,
        ),
        (
            "When this Equipment enters, attach it to target creature you control. Untap that land.",
            false,
        ),
        (
            "When this Equipment enters, attach it to target land you control. Untap that creature.",
            false,
        ),
        ("When this Equipment enters, untap that creature.", false),
    ] {
        let parsed = parse("Artifact - Equipment", text);
        assert_eq!(parsed.status == "canonical", expected, "{text}");
        if expected {
            let rule = rule(&parsed);
            assert_eq!(rule["effects"][0]["kind"], "attachPermanent");
            assert_eq!(rule["effects"][1]["kind"], "untapPermanent");
            assert_eq!(rule["effects"][0]["to"], rule["effects"][1]["permanent"]);
        }
    }
}
