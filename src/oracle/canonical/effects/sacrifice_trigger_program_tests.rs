use super::*;
use crate::oracle::model::AbilitySource;
use crate::oracle::syntax::{
    ExactNameTokenIndex, recognize_oracle_entities, resolve_named_token_spans,
};

fn program(text: &str) -> SacrificeTriggerProgramParseAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let mut tokens = recognize_oracle_entities(text);
    let names = ExactNameTokenIndex::from_names(
        crate::card_catalog::named_token_names().expect("local catalog"),
    );
    resolve_named_token_spans(&mut tokens, &names);
    try_parse_promoted_sacrifice_trigger_program(&AbilityInput {
        face_name: "Synthetic sacrifice witness",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn any_player_event_and_damage_keep_source_and_target_declaration() {
    let SacrificeTriggerProgramParseAttempt::Parsed(draft) = program(
        "Whenever a player sacrifices a permanent, this creature deals 1 damage to any target.",
    ) else {
        panic!("one shared event and one shared action must compose")
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"permanentSacrificed","where":null})
    );
    assert_eq!(
        draft.rule["declaration"]["decisions"][0],
        target_decision("targetDamageable", json!({"kind":"anyTarget"}), 1, 1)
    );
    assert_eq!(
        draft.rule["effects"],
        json!([{"kind":"dealDamage","source":self_ref(),"amount":integer(1),"recipient":chosen_target("targetDamageable")}])
    );
}

#[test]
fn query_grammar_stays_shared_and_characteristics_read_the_departure_snapshot() {
    let text =
        "Whenever you sacrifice another artifact or creature, draw two cards and gain 1 life.";
    let tokens = recognize_oracle_entities(text);
    let mut cursor = OracleTokenCursor::new("sacrifice-query-syntax-test", &tokens);
    let SacrificeEventAttempt::Recognized(event) = try_recognize_sacrifice_event(&mut cursor)
    else {
        panic!("ObjectQuery syntax must remain available")
    };
    assert_eq!(event.actor, EventActorScope::Controller);
    assert!(event.exclude_source);
    assert_eq!(
        event
            .object
            .as_ref()
            .expect("the event names a sacrificed-object query")
            .predicate,
        ObjectPredicate::Union(vec![
            ObjectPredicate::CardType(ObjectCardType::Artifact),
            ObjectPredicate::CardType(ObjectCardType::Creature),
        ])
    );
    // The engine reads the predicate on the effective characteristics it
    // captures as the object leaves the battlefield (event model).
    let SacrificeTriggerProgramParseAttempt::Parsed(draft) = program(text) else {
        panic!("card types are captured in the departure snapshot")
    };
    assert_eq!(draft.rule["event"]["kind"], "permanentSacrificed");
    assert_eq!(draft.rule["event"]["excludeSource"], true);

    let SacrificeTriggerProgramParseAttempt::Parsed(draft) =
        program("Whenever you sacrifice another permanent, draw two cards and gain 1 life.")
    else {
        panic!("identity exclusion and bare permanent domain remain supported")
    };
    assert_eq!(
        draft.rule["event"],
        json!({"kind":"permanentSacrificed","player":controller(),"where":null,"excludeSource":true})
    );
    assert_eq!(draft.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draft.rule["effects"][1]["kind"], "gainLife");
}

#[test]
fn unproved_actor_numeric_context_and_intervening_conditions_are_explicit() {
    for (text, state) in [
        (
            "Whenever an opponent sacrifices a permanent, draw a card.",
            "trigger.sacrifice.actor-scope-support",
        ),
        (
            "Whenever you sacrifice a creature with power 4 or greater, draw a card.",
            "trigger.sacrifice.numeric-snapshot-support",
        ),
        (
            "Whenever you sacrifice a permanent, if you control a creature, draw a card.",
            "trigger.sacrifice.intervening-condition",
        ),
    ] {
        let SacrificeTriggerProgramParseAttempt::FailedSupport(failure) = program(text) else {
            panic!("must keep the support boundary: {text}")
        };
        assert_eq!(failure.state, state);
    }
}

#[test]
fn unrelated_events_restore_checkpoint_and_committed_envelopes_do_not_drop_suffixes() {
    let tokens = recognize_oracle_entities("prefix Whenever you draw a card, gain 1 life.");
    let mut cursor = OracleTokenCursor::new("sacrifice-rollback-test", &tokens);
    cursor.consume("prefix").unwrap();
    let position = cursor.position();
    assert!(matches!(
        try_recognize_sacrifice_event(&mut cursor),
        SacrificeEventAttempt::NotClaimed
    ));
    assert_eq!(cursor.position(), position);
    for text in [
        "Whenever you sacrifice one or more permanents, draw a card.",
        "Whenever you sacrifice a card, draw a card.",
        "Whenever you sacrifice a permanent, draw a card. This ability triggers only once each turn.",
    ] {
        assert!(!matches!(
            program(text),
            SacrificeTriggerProgramParseAttempt::Parsed(_)
        ));
    }
}
