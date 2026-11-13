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
        parse("Whenever a land you control enters, manifest dread."),
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
fn full_preserves_legacy_entry_sequences_after_native_candidate_rejection() {
    for (name, text) in [
        (
            "Thranduil, the Elvenking",
            "Whenever another legendary Elf you control enters, draw two cards, then discard a card.",
        ),
        (
            "Skyswimmer Koi",
            "Whenever an artifact you control enters, you may draw a card. If you do, discard a card.",
        ),
    ] {
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

        assert_eq!(native.status, "unsupported", "{name}");
        assert_eq!(
            native.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{name}"
        );
        assert_eq!(
            native.abilities[0].diagnostics[0].code, "oracle_machine_failure",
            "{name}"
        );

        assert_eq!(full.status, "canonical", "{name}");
        assert_eq!(
            full.abilities[0].recognizer,
            RecognizerProvenance::Legacy,
            "{name}"
        );
        assert!(full.abilities[0].rule.is_some(), "{name}");
        assert_eq!(
            full.abilities[0].diagnostics[0].code, "native_candidate_unpromoted",
            "{name}"
        );
        assert_eq!(
            full.abilities[0].diagnostics[0].state, native.abilities[0].diagnostics[0].state,
            "{name}"
        );
    }
}
