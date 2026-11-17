use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::recognize_entities;

fn program(text: &str, face_type: &str) -> SourceTriggerProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_promoted_source_trigger_program(&AbilityInput {
        face_name: "Synthetic source program",
        face_type_line: face_type,
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn event_wrapper_preserves_mandatory_effect_order() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) = program(
        "When this creature enters, draw a card and gain 2 life.",
        "Creature",
    ) else {
        panic!("two existing actions must compose through the source wrapper")
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"enterBattlefield","object":{"kind":"self"}})
    );
    assert_eq!(draft.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draft.rule["effects"][1]["kind"], "gainLife");
    assert!(draft.rule.get("declaration").is_none());
}

#[test]
fn exact_optional_draw_is_one_all_or_nothing_action() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) = program(
        "When this creature dies, you may draw two cards.",
        "Artifact Creature",
    ) else {
        panic!("optional exact amount must preserve its choice scope")
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"permanentDied","object":{"kind":"self"}})
    );
    assert_eq!(
        draft.rule["effects"],
        json!([{
            "kind":"optionalEffects", "player":{"kind":"controllerOf","object":{"kind":"self"}},
            "effects":[{"kind":"drawCards","player":{"kind":"controllerOf","object":{"kind":"self"}},"count":{"kind":"integer","value":2}}]
        }])
    );
}

#[test]
fn up_to_amount_is_selected_at_resolution_instead_of_becoming_all_or_nothing() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) = program(
        "When this creature dies, you may draw up to two cards.",
        "Creature",
    ) else {
        panic!("explicit up-to quantity must keep a numeric range")
    };
    assert_eq!(draft.rule["effects"][0]["kind"], "chooseNumber");
    assert_eq!(draft.rule["effects"][0]["minimum"], integer(0));
    assert_eq!(draft.rule["effects"][0]["maximum"], integer(2));
    assert_eq!(
        draft.rule["effects"][1]["count"],
        decision_result("optionalDrawCount0")
    );
}

#[test]
fn optional_search_reuses_search_move_shuffle_and_record_in_order() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) = program(
        "When this artifact enters, you may search your library for a basic land card, put that card onto the battlefield tapped, then shuffle.",
        "Artifact Creature",
    ) else {
        panic!("controller-owned optional search must use the shared search procedure")
    };
    let effects = draft.rule["effects"][0]["effects"].as_array().unwrap();
    assert_eq!(
        effects
            .iter()
            .map(|effect| effect["kind"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "chooseCards",
            "moveCards",
            "shuffleZone",
            "recordLibrarySearch"
        ]
    );
    assert_eq!(effects[1]["to"]["tapped"], true);
    assert_eq!(effects[0]["minimum"], 0);
}

#[test]
fn coordinated_permission_is_rejected_but_a_period_ends_the_scope() {
    for text in [
        "When this creature dies, you may draw a card and gain 1 life.",
        "When this creature dies, you may draw a card, then gain 1 life.",
    ] {
        let SourceTriggerProgramParseAttempt::Failed(failure) = program(text, "Creature") else {
            panic!("coordinated permission cannot become a mandatory second action: {text}")
        };
        assert_eq!(failure.state, "sequence.optional.permission-group");
    }
    let SourceTriggerProgramParseAttempt::Parsed(draft) = program(
        "When this creature dies, you may draw a card. You gain 1 life.",
        "Creature",
    ) else {
        panic!("the period ends only the first action's permission")
    };
    assert_eq!(draft.rule["effects"][0]["kind"], "optionalEffects");
    assert_eq!(draft.rule["effects"][1]["kind"], "gainLife");
}

#[test]
fn conditions_unsupported_actions_and_invalid_faces_are_explicit() {
    assert!(matches!(
        program(
            "When this creature enters, if you control a Forest, draw a card.",
            "Creature"
        ),
        SourceTriggerProgramParseAttempt::Unpromoted(_)
    ));
    assert!(matches!(
        program("When this creature enters, connive.", "Creature"),
        SourceTriggerProgramParseAttempt::Unpromoted(_)
    ));
    assert!(matches!(
        program("When this creature enters, draw a card.", "Instant"),
        SourceTriggerProgramParseAttempt::Unpromoted(_)
    ));
    assert!(!matches!(
        program("When this creature enters, draw X cards.", "Creature"),
        SourceTriggerProgramParseAttempt::Parsed(_)
    ));
}

#[test]
fn committed_bad_draw_and_relative_target_support_do_not_fall_back() {
    assert!(matches!(
        program(
            "When this creature dies, you may draw imaginary cards.",
            "Creature"
        ),
        SourceTriggerProgramParseAttempt::Failed(_)
    ));
    let SourceTriggerProgramParseAttempt::Failed(failure) = program(
        "When this creature enters, target creature gets +1/+1 until end of turn. Another target creature gets -1/-1 until end of turn.",
        "Creature",
    ) else {
        panic!("accepted hard support boundary must survive the source wrapper")
    };
    assert_eq!(failure.state, "lower.modify-stats.another-target-relation");
}

#[test]
fn rebased_source_program_keeps_the_shared_plural_surveil_expansion() {
    let SourceTriggerProgramParseAttempt::Parsed(draft) =
        program("When this creature dies, surveil 2.", "Creature")
    else {
        panic!("source program must reuse the accepted plural lowering boundary")
    };
    let effects = draft.rule["effects"].as_array().unwrap();
    assert_eq!(effects.len(), 7);
    assert_eq!(effects[0]["kind"], "lookAtTopCards");
    assert_eq!(effects[6]["kind"], "emitAbilitySignal");
    assert_eq!(effects[6]["abilityId"], "surveil");
    assert!(effects.iter().all(|effect| effect["kind"] != "surveil"));
}

#[test]
fn optional_draw_respects_runtime_numeric_bounds_and_trigger_scope() {
    assert!(matches!(
        program("When this creature dies, you may draw X cards.", "Creature"),
        SourceTriggerProgramParseAttempt::Unpromoted(_)
    ));
    assert!(matches!(
        program(
            "When this creature dies, you may draw 65 cards.",
            "Creature"
        ),
        SourceTriggerProgramParseAttempt::Parsed(_)
    ));
    let SourceTriggerProgramParseAttempt::Unpromoted(failure) = program(
        "When this creature dies, you may draw 2147483648 cards.",
        "Creature",
    ) else {
        panic!("an unrepresentable runtime literal cannot become a canonical trigger")
    };
    assert_eq!(failure.state, "lower.optional-draw.count");
}
