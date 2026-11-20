use crate::oracle::{
    model::{AbilitySource, OracleCardParseRequest, RecognizerProvenance},
    parse_oracle_card, parse_oracle_card_native,
    syntax::{AbilityInput, recognize_oracle_entities},
};

use super::*;

fn parse(text: &str) -> PhaseTriggerParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    try_parse_promoted_phase_trigger_sequence(&AbilityInput {
        face_name: "Generic Phase Source",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed(text: &str) -> CanonicalRuleDraft {
    match parse(text) {
        PhaseTriggerParseAttempt::Parsed(draft) => draft,
        PhaseTriggerParseAttempt::NotClaimed => panic!("expected a claimed native family"),
        PhaseTriggerParseAttempt::Unpromoted(failure) => {
            panic!("expected a fully promoted native body: {failure}")
        }
        PhaseTriggerParseAttempt::Failed(failure) => panic!("unexpected failure: {failure}"),
    }
}

#[test]
fn lowers_phyrexian_arena_to_two_independent_primitives() {
    let rule = parsed("At the beginning of your upkeep, you draw a card and lose 1 life.").rule;
    assert_eq!(rule["event"]["kind"], "stepBegan");
    assert_eq!(rule["event"]["step"], "upkeep");
    assert_eq!(rule["event"]["player"], controller());
    assert_eq!(rule["effects"][0]["kind"], "drawCards");
    assert_eq!(rule["effects"][1]["kind"], "loseLife");
    assert_eq!(rule["effects"][1]["amount"], integer(1));
}

#[test]
fn phase_and_player_scopes_lower_without_collapsing_main_phases() {
    for (text, step, player_kind) in [
        (
            "At the beginning of each upkeep, draw a card.",
            "upkeep",
            "eachPlayer",
        ),
        (
            "At the beginning of the end step, draw a card.",
            "endStep",
            "eachPlayer",
        ),
        (
            "At the beginning of each end step, draw a card.",
            "endStep",
            "eachPlayer",
        ),
        (
            "At the beginning of combat on your turn, draw a card.",
            "beginCombat",
            "controllerOf",
        ),
        (
            "At the beginning of each combat, draw a card.",
            "beginCombat",
            "eachPlayer",
        ),
        (
            "At the beginning of your first main phase, draw a card.",
            "precombatMain",
            "controllerOf",
        ),
        (
            "At the beginning of your postcombat main phase, draw a card.",
            "postcombatMain",
            "controllerOf",
        ),
    ] {
        let event = parsed(text).rule["event"].clone();
        assert_eq!(event["step"], step, "{text}");
        assert_eq!(event["player"]["kind"], player_kind, "{text}");
    }
}

#[test]
fn curly_possessive_opponent_draw_step_preserves_typed_scope() {
    let event =
        parsed("At the beginning of each opponent’s draw step, you gain 1 life.").rule["event"]
            .clone();
    assert_eq!(event["step"], "draw");
    assert_eq!(event["player"]["kind"], "opponentsOf");
    assert_eq!(event["player"]["player"], controller());
}

#[test]
fn optional_single_targeted_action_keeps_declaration_outside_wrapper() {
    let rule = parsed("At the beginning of your end step, you may destroy target artifact.").rule;
    assert_eq!(rule["declaration"]["decisions"][0]["kind"], "chooseTargets");
    assert_eq!(rule["effects"][0]["kind"], "optionalEffects");
    assert_eq!(rule["effects"][0]["effects"][0]["kind"], "destroyPermanent");
}

#[test]
fn unsupported_boundaries_remain_unclaimed() {
    for text in [
        "At the beginning of your upkeep, if you control a creature, draw a card.",
        "At the beginning of the next end step, draw a card.",
        "At the end of turn, draw a card.",
        "At the beginning of your opponent's upkeep, draw a card.",
        "At the beginning of your second main phase, draw a card.",
        "At the beginning of your third main phase, draw a card.",
        "At the beginning of each of your main phases, draw a card.",
        "At the beginning of your upkeep and whenever a creature enters, draw a card.",
        "At the beginning of your upkeep draw a card.",
        "At the beginning of your upkeep, draw a card. This ability triggers only once each turn.",
    ] {
        assert!(
            matches!(parse(text), PhaseTriggerParseAttempt::NotClaimed),
            "{text}"
        );
    }
}

#[test]
fn third_person_life_verb_is_never_defaulted_to_controller() {
    // The coordinated predicate shares the declared target player; the second
    // verb must lower to that chosen target, never to the controller.
    let rule =
        parsed("At the beginning of your upkeep, target player gains 1 life and loses 1 life.")
            .rule;
    assert_eq!(rule["declaration"]["decisions"][0]["id"], "targetPlayer");
    assert_eq!(rule["effects"].as_array().map(Vec::len), Some(2));
    for (index, kind) in [(0, "gainLife"), (1, "loseLife")] {
        let effect = &rule["effects"][index];
        assert_eq!(effect["kind"], kind);
        assert_eq!(effect["player"]["kind"], "chosenTarget", "{effect}");
        assert_eq!(effect["player"]["id"], "targetPlayer", "{effect}");
    }
}

#[test]
fn unsupported_bodies_and_wide_optional_scope_are_unpromoted_candidates() {
    for text in [
        "At the beginning of your upkeep, incubate 2.",
        "At the beginning of your upkeep, you may draw a card and gain 1 life.",
    ] {
        assert!(
            matches!(parse(text), PhaseTriggerParseAttempt::Unpromoted(_)),
            "{text}"
        );
    }
}

#[test]
fn malformed_draw_and_dangling_connector_remain_committed_failures() {
    for text in [
        "At the beginning of your upkeep, you draw target card.",
        "At the beginning of your upkeep, draw a card and.",
    ] {
        assert!(
            matches!(parse(text), PhaseTriggerParseAttempt::Failed(_)),
            "{text}"
        );
    }
}

#[test]
fn quantified_opponent_phase_draw_is_complete_native_ir() {
    // Formerly the quantified-player failure case below: the shared
    // quantified-player action now composes, in both parser modes.
    let request = OracleCardParseRequest {
        card_name: "Master of the Feast".to_string(),
        type_line: "Enchantment Creature — Demon".to_string(),
        mana_cost: None,
        oracle_text: Some(
            "At the beginning of your upkeep, each opponent draws a card.".to_string(),
        ),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    for result in [
        parse_oracle_card_native(request.clone()),
        parse_oracle_card(request),
    ] {
        assert_eq!(result.status, "canonical");
        let ability = &result.abilities[0];
        assert!(ability.diagnostics.is_empty(), "{:?}", ability.diagnostics);
        let rule = ability.rule.as_ref().expect("complete rule");
        assert_eq!(rule["event"]["step"], "upkeep");
    }
}

#[test]
fn default_preserves_native_phase_failure_without_fallback() {
    for (name, type_line, text, primary_state, candidate_state) in [
        (
            "Sanctum of Calm Waters",
            "Legendary Enchantment — Shrine",
            "At the beginning of your first main phase, you may draw X cards, where X is the number of Shrines you control. If you do, discard a card.",
            "lower.program.optional-action.action",
            "condition.sentence.atom-kind",
        ),
        (
            "Sylvan Library",
            "Enchantment",
            "At the beginning of your draw step, you may draw two additional cards. If you do, choose two cards in your hand drawn this turn. For each of those cards, pay 4 life or put the card on top of your library.",
            "effect.draw-cards.unsupported-qualifier",
            "effect.draw-cards.unsupported-qualifier",
        ),
        (
            "Flame Blitz",
            "Enchantment",
            "At the beginning of your end step, this enchantment deals 5 damage to each planeswalker.",
            "lower.sentence-conditional.ranges",
            "lower.sentence-conditional.ranges",
        ),
    ] {
        let request = OracleCardParseRequest {
            card_name: name.to_string(),
            type_line: type_line.to_string(),
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
        assert_eq!(native.abilities[0].diagnostics.len(), 2, "{name}");
        assert_eq!(
            native.abilities[0].diagnostics[0].code, "oracle_machine_failure",
            "{name}"
        );

        assert_eq!(full.status, "unsupported", "{name}");
        assert_eq!(
            full.abilities[0].recognizer,
            RecognizerProvenance::Native,
            "{name}"
        );
        assert!(full.abilities[0].rule.is_none(), "{name}");
        assert_eq!(full.abilities[0].diagnostics.len(), 2, "{name}");
        assert_eq!(
            full.abilities[0].diagnostics[0].code, "oracle_machine_failure",
            "{name}"
        );
        for ability in [&native.abilities[0], &full.abilities[0]] {
            assert!(
                ability.rule.is_none(),
                "{name}: neither role creates partial IR"
            );
            assert_eq!(
                ability.diagnostics[0].automaton.as_deref(),
                Some("generic-trigger-program"),
                "{name}: owned primary"
            );
            assert_eq!(
                ability.diagnostics[0].state.as_deref(),
                Some(primary_state),
                "{name}"
            );
            assert_eq!(
                ability.diagnostics[1].code, "oracle_candidate_machine_failure",
                "{name}: explicitly candidate, not committed"
            );
            assert_eq!(
                ability.diagnostics[1].automaton.as_deref(),
                Some("phase-trigger-sequence"),
                "{name}: compatibility evidence retains its family"
            );
            assert_eq!(
                ability.diagnostics[1].state.as_deref(),
                Some(candidate_state),
                "{name}: preserve compatibility feedback"
            );
        }
        assert_eq!(
            serde_json::to_value(&native.abilities[0].diagnostics).unwrap(),
            serde_json::to_value(&full.abilities[0].diagnostics).unwrap(),
            "{name}: all diagnostic fields and roles match between modes"
        );
        assert_eq!(
            full.abilities[0].diagnostics[0].state, native.abilities[0].diagnostics[0].state,
            "{name}"
        );
        assert_eq!(
            full.abilities[0].diagnostics[0].expected, native.abilities[0].diagnostics[0].expected,
            "{name}"
        );
    }
}
