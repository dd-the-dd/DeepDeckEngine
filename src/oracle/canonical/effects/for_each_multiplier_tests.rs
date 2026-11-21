use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;
use crate::oracle::canonical::{
    CanonicalParseOutcome, CanonicalRuleDraft, MachineResult, parse_canonical_rule,
};
use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

fn parse(text: &str) -> MachineResult<CanonicalRuleDraft> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Generic Multiplier",
        face_type_line: "Sorcery",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule(&input, "spellAbility") {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft),
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => Err(failure),
        CanonicalParseOutcome::Unsupported => panic!("no native owner for {text}"),
    }
}

#[test]
fn draw_for_each_color_among_permanents_is_the_shared_distinct_color_count() {
    let rule = parse("Draw a card for each color among permanents you control.")
        .expect("draw multiplied by a color count")
        .rule;
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
    let count = &rule["effects"][0]["count"];
    assert_eq!(count["kind"], "countDistinctColors");
    assert_eq!(count["player"]["kind"], "controllerOf");
}

#[test]
fn sibling_actions_reuse_the_same_count_child() {
    // A previously unused combination: life gain scaled by a permanent count.
    let rule = parse("You gain 1 life for each creature you control.")
        .expect("life gain multiplied by a permanent count")
        .rule;
    assert_eq!(rule["effects"][0]["kind"], "gainLife");
    assert_eq!(rule["effects"][0]["amount"]["kind"], "countPermanents");
    let rule = parse("Draw a card for each other creature you control.")
        .expect("draw multiplied by an excluding count")
        .rule;
    assert_eq!(rule["effects"][0]["count"]["excludeSource"], true);
}

#[test]
fn mana_for_each_counter_on_source_is_a_counted_fixed_mana_amount() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Generic Battery".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(
            "{T}: Put a charge counter on this enchantment.
{T}: Add {G} for each charge counter on this enchantment."
                .to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    });
    let ability = &parsed.abilities[1];
    assert_eq!(ability.status, "canonical", "{parsed:#?}");
    let effect = &ability.rule.as_ref().unwrap()["effects"][0];
    assert_eq!(effect["kind"], "addMana");
    assert_eq!(effect["mana"]["kind"], "fixedMana");
    assert_eq!(effect["mana"]["amount"]["kind"], "countCounters");
}

#[test]
fn unsupported_multipliers_fail_at_a_precise_state() {
    for text in [
        // Non-literal base quantity has no proved reading.
        "Draw X cards for each creature you control.",
        // Distinct colors are only evaluated for the ability controller.
        "Draw a card for each color among permanents your opponents control.",
        // Mana productions only take counts the mana runtime evaluates.
        "Add {G} for each color among permanents you control.",
        // An action without a shared numeric quantity refuses the multiplier.
        "Scry 1 for each creature you control.",
    ] {
        let failure = parse(text).expect_err(text);
        assert_ne!(failure.state, "sequence.connector-or-end", "{text}");
    }
}
