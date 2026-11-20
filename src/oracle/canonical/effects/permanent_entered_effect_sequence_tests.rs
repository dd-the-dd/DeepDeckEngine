use crate::oracle::{
    model::{AbilitySource, OracleCardParseRequest, RecognizerProvenance},
    parse_oracle_card, parse_oracle_card_native,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

fn parse(text: &str) -> PermanentEnteredParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_promoted_permanent_entered_effect_sequence(&AbilityInput {
        face_name: "Generic Entry Source",
        face_type_line: "Creature — Druid",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed(text: &str) -> CanonicalRuleDraft {
    match parse(text) {
        PermanentEnteredParseAttempt::Parsed(draft) => draft,
        PermanentEnteredParseAttempt::NotClaimed => panic!("expected a claimed native family"),
        PermanentEnteredParseAttempt::Unpromoted(failure) => {
            panic!("expected a fully promoted native body: {failure}")
        }
        PermanentEnteredParseAttempt::Failed(failure) => panic!("unexpected failure: {failure}"),
    }
}

#[test]
fn lowers_tatyova_as_controller_scoped_event_and_independent_actions() {
    let rule =
        parsed("Landfall — Whenever a land you control enters, you gain 1 life and draw a card.")
            .rule;
    assert_eq!(rule["event"]["kind"], "permanentEntered");
    assert_eq!(rule["event"]["player"], controller());
    assert_eq!(rule["event"]["where"]["value"], "Land");
    assert_eq!(rule["effects"][0]["kind"], "gainLife");
    assert_eq!(rule["effects"][1]["kind"], "drawCards");
}

#[test]
fn another_and_optional_are_generic_envelope_properties() {
    let another = parsed("When another creature enters, you gain 1 life.").rule;
    assert_eq!(another["event"]["anyController"], true);
    assert_eq!(another["event"]["excludeSource"], true);

    let optional = parsed("Whenever a Beast enters, you may draw a card.").rule;
    assert_eq!(optional["effects"][0]["kind"], "optionalEffects");
    assert_eq!(optional["effects"][0]["effects"][0]["kind"], "drawCards");
}

#[test]
fn targeted_child_preserves_its_declaration() {
    let rule = parsed(
        "Whenever an enchantment you control enters, target creature gets +1/+1 until end of turn.",
    )
    .rule;
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPermanent");
    assert_eq!(rule["effects"][0]["object"]["id"], "targetPermanent");
}

#[test]
fn unsupported_first_child_is_unpromoted_but_committed_near_miss_fails() {
    assert!(matches!(
        parse("Whenever a land you control enters, incubate 2."),
        PermanentEnteredParseAttempt::Unpromoted(_)
    ));
    assert!(matches!(
        parse("Whenever a land you control enters, draw target card."),
        PermanentEnteredParseAttempt::Failed(_)
    ));
}

#[test]
fn first_boundary_rejects_event_anaphora_compound_events_and_wide_optional_scope() {
    for text in [
        "Whenever a creature enters, put a +1/+1 counter on it.",
        "Whenever this creature or another creature enters, draw a card.",
        "Whenever one or more creatures enter, draw a card.",
        "Whenever a land enters or attacks, draw a card.",
        "Whenever a creature your team controls enters, draw a card.",
        "Whenever a legendary you control enters, draw a card.",
        "Whenever a creature enters, you may draw a card and gain 1 life.",
    ] {
        assert!(
            !matches!(parse(text), PermanentEnteredParseAttempt::Parsed(_)),
            "{text}"
        );
    }
    assert!(matches!(
        parse("Whenever a creature enters, you may draw a card and gain 1 life."),
        PermanentEnteredParseAttempt::Unpromoted(_)
    ));
}

#[test]
fn optional_outcome_has_one_native_and_full_reading() {
    // The `If you do` outcome is the shared optional-action program; native
    // and full parsing agree on it instead of a fallback reading.
    for (name, text) in [(
        "Skyswimmer Koi",
        "Whenever an artifact you control enters, you may draw a card. If you do, discard a card.",
    )] {
        let request = OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: "Creature".to_string(),
            mana_cost: None,
            oracle_text: Some(text.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        };

        let native = parse_oracle_card_native(request.clone());
        let full = parse_oracle_card(request);

        assert_eq!(native.status, "canonical", "{name}");
        assert_eq!(full.status, "canonical", "{name}");
        assert_eq!(
            native.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{name}"
        );
        assert_eq!(native.abilities[0].rule, full.abilities[0].rule, "{name}");
        let rule = native.abilities[0].rule.as_ref().expect("rule");
        assert_eq!(rule["effects"][0]["kind"], "optionalAction", "{name}");
    }
}

#[test]
fn entry_sequence_composes_independent_draw_and_discard_actions() {
    let request = OracleCardParseRequest {
        card_name: "Generic Entry Source".into(), type_line: "Creature".into(),
        mana_cost: None, layout: Some("normal".into()), faces: Vec::new(),
        oracle_text: Some("Whenever another legendary Elf you control enters, draw two cards, then discard a card.".into()),
    };
    for parsed in [
        parse_oracle_card_native(request.clone()),
        parse_oracle_card(request),
    ] {
        assert_eq!(parsed.status, "canonical");
        assert_eq!(parsed.abilities[0].recognizer, RecognizerProvenance::Native);
        let rule = parsed.abilities[0].rule.as_ref().unwrap();
        assert_eq!(rule["effects"][0]["kind"], "drawCards");
        assert_eq!(rule["effects"][1]["kind"], "discardCards");
    }
}
