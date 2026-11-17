use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn program(text: &str, face_type: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic typed trigger",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn typed_envelope_outcomes_are_not_inferred_from_consumed_token_count() {
    assert!(matches!(
        program("Draw a card.", "Creature"),
        GenericTriggerProgramAttempt::NotApplicable
    ));
    assert!(matches!(
        program("At the end of your turn, draw a card.", "Creature"),
        GenericTriggerProgramAttempt::RejectedCandidate(_)
    ));
    let GenericTriggerProgramAttempt::FailedCommitted(failure) =
        program("At the beginning of your upkeep draw a card.", "Creature")
    else {
        panic!("typed step envelope owns the missing separator")
    };
    assert_eq!(failure.state, "trigger.envelope.separator");
}

#[test]
fn recognized_event_commits_guard_program_and_face_failures() {
    for (text, face_type, state) in [
        (
            "When this creature enters, draw a card.",
            "Instant",
            "trigger.program.permanent-face",
        ),
        (
            "When this creature enters, you may destroy target artifact. Draw a card.",
            "Creature",
            "program.optional.scope",
        ),
        (
            "When this creature enters, if you control at least two other creatures, draw a card.",
            "Creature",
            "trigger.program.guard-other-binding",
        ),
    ] {
        let GenericTriggerProgramAttempt::FailedCommitted(failure) = program(text, face_type)
        else {
            panic!("a recognized event must preserve its owned failure: {text}")
        };
        assert_eq!(failure.state, state, "{text}");
    }
}

#[test]
fn complete_generic_program_remains_a_parsed_primitive_rule() {
    let GenericTriggerProgramAttempt::Parsed(draft) =
        program("When this creature enters, draw a card.", "Creature")
    else {
        panic!("complete shared program must still parse")
    };
    assert_eq!(draft.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"enterBattlefield","object":{"kind":"self"}})
    );
}

fn program_at_line(text: &str, face_type: &str, line_start: usize) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start,
        line_end: line_start,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic typed trigger",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn parsed_rule(text: &str, face_type: &str, line_start: usize) -> Value {
    match program_at_line(text, face_type, line_start) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule,
        GenericTriggerProgramAttempt::NotApplicable => panic!("not applicable: {text}"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure)
        | GenericTriggerProgramAttempt::FailedCommitted(failure) => {
            panic!("{text}: {} expected {}", failure.state, failure.expected)
        }
    }
}

#[test]
fn player_event_actor_verb_and_once_each_turn_limit_compose() {
    let controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
    // Bloombrute: the controller actor, a life-gain verb and a trigger limit
    // owned by the triggered ability (never an effect).
    assert_eq!(
        parsed_rule(
            "Whenever you gain life, draw a card. This ability triggers only once each turn.",
            "Creature",
            1,
        ),
        json!({"kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"lifeGained","player":controller.clone()},
            "effects":[{"kind":"drawCards","count":{"kind":"integer","value":1},
                "player":controller.clone()}],
            "triggerLimit":{"kind":"onceEachTurn","id":"onceEachTurn:1"}})
    );
    // Sibling: a third-person actor agrees with its verb; no limit is invented.
    assert_eq!(
        parsed_rule(
            "Whenever an opponent loses life, you gain 1 life.",
            "Enchantment",
            0
        ),
        json!({"kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"lifeLost","player":{"kind":"opponentsOf","player":controller.clone()}},
            "effects":[{"kind":"gainLife","amount":{"kind":"integer","value":1},
                "player":controller.clone()}]})
    );
    // New combination: any-player actor + limit on another printed line. The
    // limit id is unique per ability line so two limited abilities on one
    // face are counted separately.
    let rule = parsed_rule(
        "Whenever a player gains life, draw a card. This ability triggers only once each turn.",
        "Creature",
        3,
    );
    assert_eq!(
        rule["event"],
        json!({"kind":"lifeGained","player":{"kind":"eachPlayer"}})
    );
    assert_eq!(
        rule["triggerLimit"],
        json!({"kind":"onceEachTurn","id":"onceEachTurn:3"})
    );
}

#[test]
fn player_event_and_trigger_limit_near_misses_stay_explicit() {
    // Disagreeing verb form: the shared actor reader never claims `you gains`.
    assert!(!matches!(
        program("Whenever you gains life, draw a card.", "Creature"),
        GenericTriggerProgramAttempt::Parsed(_)
    ));
    let tokens = recognize_entities("you gains life");
    let mut cursor = OracleTokenCursor::new("player-event-near-miss", &tokens);
    let failure = recognize_player_event(&mut cursor).expect_err("verb agreement");
    assert_eq!(failure.state, "event.player.action");
    // Recognized event without a runtime matcher for this actor.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) =
        program("Whenever an opponent attacks, draw a card.", "Creature")
    else {
        panic!("opponent attack must not widen to the controller event")
    };
    assert_eq!(failure.state, "trigger.program.player-event-support");
    // Only `once each turn` is a represented trigger limit.
    let GenericTriggerProgramAttempt::FailedCommitted(failure) = program(
        "Whenever you gain life, draw a card. This ability triggers only twice each turn.",
        "Creature",
    ) else {
        panic!("an unrepresented trigger limit must not be dropped")
    };
    assert_eq!(failure.state, "trigger.limit.clause");
}

#[test]
fn modal_trigger_body_scopes_each_mode_target_to_its_selection() {
    // Glamermite (ECL): modes are chosen as the trigger is put on the stack
    // (CR 700.2b); tap and untap share one verb frame.
    let rule = parsed_rule(
        "When this creature enters, choose one \u{2014}\n\u{2022} Tap target creature.\n\u{2022} Untap target creature.",
        "Creature \u{2014} Faerie Rogue",
        2,
    );
    let selected = |mode: &str| json!({"kind":"selectionContains","selection":{"kind":"decisionResult","decisionId":"chosenModes"},"value":mode});
    let target = |mode: &str| {
        json!({"kind":"chooseTargets","id":format!("{mode}:targetPermanent"),"minimum":1,"maximum":1,
            "candidates":{"kind":"permanents","where":{"kind":"cardTypeContains","value":"Creature"}},
            "condition":selected(mode)})
    };
    let chosen = |mode: &str| json!({"kind":"chosenTarget","id":format!("{mode}:targetPermanent")});
    assert_eq!(
        rule,
        json!({"kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"enterBattlefield","object":{"kind":"self"}},
            "declaration":{"kind":"castingDeclaration","decisions":[
                {"kind":"chooseModes","id":"chosenModes","minimum":1,
                    "maximum":{"kind":"integer","value":1},"allowRepeated":false,
                    "options":["mode1","mode2"]},
                target("mode1"), target("mode2")]},
            "effects":[
                {"kind":"conditional","condition":selected("mode1"),
                    "then":[{"kind":"tapPermanent","permanent":chosen("mode1")}]},
                {"kind":"conditional","condition":selected("mode2"),
                    "then":[{"kind":"untapPermanent","permanent":chosen("mode2")}]}]})
    );
    // New combination: a player event with a widened modal header.
    let rule = parsed_rule(
        "Whenever you gain life, choose one or both \u{2014}\n\u{2022} Untap target land.\n\u{2022} Draw a card.",
        "Creature",
        0,
    );
    assert_eq!(rule["event"]["kind"], "lifeGained");
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"]["value"], 2);
    assert_eq!(rule["effects"][0]["then"][0]["kind"], "untapPermanent");
    // Near miss: an unknown header widening is never a modal trigger.
    assert!(!matches!(
        program(
            "When this creature enters, choose one or all \u{2014}\n\u{2022} Tap target creature.\n\u{2022} Untap target creature.",
            "Creature",
        ),
        GenericTriggerProgramAttempt::Parsed(_)
    ));
}
