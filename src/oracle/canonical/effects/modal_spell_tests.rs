use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

use super::{
    MachineResult, claims_promoted_effect_sequence_modal_family,
    claims_promoted_primitive_modal_family, parse_promoted_effect_sequence_modal,
};

fn claims(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    claims_promoted_primitive_modal_family(&AbilityInput {
        face_name: "Test Command",
        face_type_line: "Instant",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn effect_sequence_input<'a>(
    source: &'a AbilitySource,
    tokens: &'a [crate::oracle::model::RecognizedEntity],
) -> AbilityInput<'a> {
    AbilityInput {
        face_name: "Generic Command",
        face_type_line: "Sorcery",
        source,
        tokens,
        name_links: &[],
        name_resolution_failure: None,
    }
}

#[test]
fn claim_accepts_only_when_every_kozilek_mode_has_a_promoted_shape() {
    assert!(claims(
        "Choose two — • Target player creates X 0/1 colorless Eldrazi Spawn creature tokens with \"Sacrifice this token: Add {C}.\" • Target player scries X, then draws a card. • Exile target creature with mana value X or less. • Exile up to X target cards from graveyards."
    ));
}

#[test]
fn claim_leaves_unimplemented_treasure_modes_on_the_legacy_path() {
    assert!(!claims(
        "Choose one — • Target player creates a Treasure token. • Exile target creature."
    ));
}

#[test]
fn claim_keeps_internal_near_misses_visible_to_the_committed_automaton() {
    assert!(claims(
        "Choose two — • Target player creates two 1/1 colorless Eldrazi Scion creature tokens with \"This token sings.\" • Target player scries two, then draws a card. • Exile target artifact with mana value two or less. • Exile up to two target cards from graveyards."
    ));
}

#[test]
fn claim_preserves_the_existing_choose_one_damage_and_grant_family() {
    assert!(claims(
        "Choose one — • This spell deals 3 damage to any target. • Target creature gains flying until end of turn."
    ));
}

#[test]
fn generic_modal_parent_lowers_choose_two_collection_modes_in_printed_order() {
    let text = "Choose two — • Destroy all artifacts. • Destroy all enchantments. • Destroy all creatures with mana value 3 or less. • Destroy all creatures with mana value 4 or greater.";
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    let input = effect_sequence_input(&source, &tokens);
    assert!(claims_promoted_effect_sequence_modal_family(&input));

    let rule = parse_promoted_effect_sequence_modal(&input)
        .expect("collection modes lower through the shared sequence")
        .rule;
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseModes");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 2);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"]["value"], 2);
    assert_eq!(rule["declaration"]["decisions"][0]["allowRepeated"], false);
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(4));
    assert_eq!(rule["effects"][0]["then"][0]["kind"], "destroyPermanent");
    assert_eq!(rule["effects"][3]["then"][0]["kind"], "destroyPermanent");
}

fn modal_rule(text: &str) -> MachineResult<serde_json::Value> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    let input = effect_sequence_input(&source, &tokens);
    assert!(
        claims_promoted_effect_sequence_modal_family(&input),
        "{text}"
    );
    parse_promoted_effect_sequence_modal(&input).map(|draft| draft.rule)
}

#[test]
fn widened_modal_headers_keep_the_printed_minimum_and_bound_the_maximum() {
    // CR 700.2: `one or both` chooses one or two of exactly two modes;
    // `one or more` chooses any nonempty subset of the printed modes.
    for (text, modes) in [
        (
            "Choose one or both — • Destroy all artifacts. • Destroy all creatures.",
            2,
        ),
        (
            "Choose one or more — • Destroy all artifacts. • Destroy all creatures. • Destroy all lands.",
            3,
        ),
    ] {
        let rule = modal_rule(text).expect(text);
        let options = (1..=modes)
            .map(|index| format!("mode{index}"))
            .collect::<Vec<_>>();
        assert_eq!(
            rule["declaration"]["decisions"],
            serde_json::json!([{"kind":"chooseModes","id":"chosenModes","minimum":1,
                "maximum":{"kind":"integer","value":modes},"allowRepeated":false,
                "options":options}]),
            "{text}"
        );
        let effects = rule["effects"].as_array().expect("one effect per mode");
        assert_eq!(effects.len(), modes);
        for (index, effect) in effects.iter().enumerate() {
            assert_eq!(effect["kind"], "conditional");
            assert_eq!(effect["condition"]["value"], format!("mode{}", index + 1));
            assert_eq!(effect["then"][0]["kind"], "destroyPermanent");
        }
    }
}

#[test]
fn generic_modal_claim_rejects_nonexact_and_repeated_choice_headers() {
    for text in [
        "Choose one or all — • Destroy all artifacts. • Destroy all creatures.",
        "Choose two. You may choose the same mode more than once. • Destroy all artifacts. • Destroy all creatures.",
    ] {
        let source = AbilitySource {
            face_id: None,
            text: text.to_string(),
            line_start: 0,
            line_end: 0,
        };
        let tokens = recognize_entities(text);
        assert!(!claims_promoted_effect_sequence_modal_family(
            &effect_sequence_input(&source, &tokens)
        ));
    }
    // `one or both` names exactly two modes; a third printed mode is never
    // silently widened into `one or more`.
    let failure = modal_rule(
        "Choose one or both — • Destroy all artifacts. • Destroy all creatures. • Destroy all lands.",
    )
    .expect_err("`one or both` over three modes");
    assert_eq!(failure.state, "modal.effect-sequence.complete");
    assert_eq!(failure.expected, "`one or both` over exactly two modes");
}
