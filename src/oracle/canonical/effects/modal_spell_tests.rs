use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{AbilityInput, recognize_entities};

use super::{
    MachineResult, claims_promoted_effect_sequence_modal_family,
    parse_promoted_effect_sequence_modal,
};

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

fn claims(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    claims_promoted_effect_sequence_modal_family(&effect_sequence_input(&source, &tokens))
}

#[test]
fn the_modal_claim_commits_only_on_structural_markers() {
    // `choose` alone is an ordinary instruction; `a`/`an` never open a header.
    assert!(!claims(
        "Choose a creature you control. It gains flying until end of turn."
    ));
    assert!(!claims("Destroy target creature. • Draw a card."));
    // Once `choose` and a bullet are present, the header owns every failure.
    assert!(claims(
        "Choose one or all \u{2014} \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures."
    ));
    let failure = modal_rule(
        "Choose one or all \u{2014} \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures.",
    )
    .expect_err("`or all` is not a mode count");
    assert_eq!(failure.state, "modal.header.mode-count-extension");
    // `one or both` names exactly two modes; a third printed mode is never
    // silently widened into `one or more`.
    let failure = modal_rule(
        "Choose one or both \u{2014} \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures. \u{2022} Destroy all lands.",
    )
    .expect_err("`one or both` over three modes");
    assert_eq!(failure.state, "modal.effect-sequence.complete");
    assert_eq!(failure.expected, "`one or both` over exactly two modes");
}

#[test]
fn unlowered_header_qualifiers_fail_explicitly() {
    for (text, state) in [
        (
            "Choose one that hasn't been chosen \u{2014} \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures.",
            "modal.header.qualifier",
        ),
        (
            "Choose one at random. \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures.",
            "modal.header.qualifier",
        ),
        (
            "Choose one. If you control a commander, you may choose both instead. \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures.",
            "modal.header.repeat-permission",
        ),
    ] {
        let failure = modal_rule(text).expect_err(text);
        assert_eq!(failure.state, state, "{text}");
    }
}

#[test]
fn repeatable_modes_lower_one_occurrence_per_possible_choice() {
    // CR 700.2d: a mode chosen twice is performed twice, each occurrence
    // with its own declarations, in printed order.
    let rule = modal_rule(
        "Choose two. You may choose the same mode more than once. \u{2022} Target player draws a card. \u{2022} Destroy all artifacts.",
    )
    .expect("a bounded repeatable header");
    let modes = &rule["declaration"]["decisions"][0];
    assert_eq!(modes["allowRepeated"], true);
    assert_eq!(modes["minimum"], 2);
    let effects = rule["effects"].as_array().expect("occurrences");
    assert_eq!(effects.len(), 4);
    assert_eq!(effects[0]["condition"]["kind"], "selectionCountAtLeast");
    assert_eq!(effects[1]["condition"]["minimum"]["value"], 2);
    assert_eq!(effects[1]["condition"]["value"], "mode1");
    // Each occurrence of the targeted mode declares its own target.
    let ids = rule["declaration"]["decisions"]
        .as_array()
        .expect("decisions")
        .iter()
        .filter_map(|decision| decision["id"].as_str())
        .collect::<Vec<_>>();
    assert!(ids.iter().any(|id| id.starts_with("mode1#1:")), "{ids:?}");
    assert!(ids.iter().any(|id| id.starts_with("mode1#2:")), "{ids:?}");
}

#[test]
fn an_up_to_header_keeps_its_zero_minimum_for_the_runtime_to_judge() {
    let rule = modal_rule(
        "Choose up to two \u{2014} \u{2022} Destroy all artifacts. \u{2022} Destroy all creatures. \u{2022} Destroy all lands.",
    )
    .expect("an up-to header");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"]["value"], 2);
}

#[test]
fn a_mode_is_a_whole_effect_program() {
    // An unused combination: a mode of two independent sentences, which the
    // old per-mode effect sequence could not hold.
    let rule = modal_rule(
        "Choose one \u{2014} \u{2022} Draw a card. Then discard a card. \u{2022} Destroy all artifacts.",
    )
    .expect("a multi-sentence mode is one program");
    assert!(
        rule["effects"][0]["then"]
            .as_array()
            .is_some_and(|effects| effects.len() >= 2),
        "{rule:#}"
    );
}

#[test]
fn spree_modes_carry_their_costs_as_mode_conditioned_additional_costs() {
    // CR 702.172a: one or more modes; each chosen mode's cost is paid as an
    // additional cost. The `+` markers delimit the shared mode bodies.
    let text = "Spree (Choose one or more additional costs.)\n+ {1} \u{2014} Destroy all artifacts.\n+ {2}{R} \u{2014} Destroy all creatures.";
    let rule = modal_rule(text).expect("a Spree body is a modal container");
    let modes = &rule["declaration"]["decisions"][0];
    assert_eq!(modes["kind"], "chooseModes");
    assert_eq!(modes["minimum"], 1);
    assert_eq!(modes["maximum"]["value"], 2);
    let costs = rule["declaration"]["additionalCosts"]
        .as_array()
        .expect("mode costs");
    assert_eq!(costs.len(), 2);
    assert_eq!(costs[0]["kind"], "conditional");
    assert_eq!(costs[0]["condition"]["value"], "mode1");
    assert_eq!(costs[1]["then"][0]["manaCost"], "{2}{R}");
}

#[test]
fn a_spree_near_miss_without_mode_costs_is_not_claimed() {
    assert!(!claims("Spree (Choose one or more additional costs.)"));
}
