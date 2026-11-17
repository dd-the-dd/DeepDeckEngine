//! Intervening-if guards (CR 603.4) read by the shared sentence-condition
//! atoms: one concept per assertion, with the near-misses that must stay
//! explicit failures.
use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn attempt(text: &str) -> GenericTriggerProgramAttempt {
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    try_parse_generic_trigger_program(&AbilityInput {
        face_name: "Synthetic guarded trigger",
        face_type_line: "Creature",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

fn guard(text: &str) -> Value {
    match attempt(text) {
        GenericTriggerProgramAttempt::Parsed(draft) => draft.rule["condition"].clone(),
        GenericTriggerProgramAttempt::NotApplicable => panic!("not applicable: {text}"),
        GenericTriggerProgramAttempt::RejectedCandidate(failure)
        | GenericTriggerProgramAttempt::FailedCommitted(failure) => {
            panic!("{text}: {} expected {}", failure.state, failure.expected)
        }
    }
}

fn failure_state(text: &str) -> String {
    match attempt(text) {
        GenericTriggerProgramAttempt::FailedCommitted(failure) => failure.state.to_string(),
        _ => panic!("expected a committed failure: {text}"),
    }
}

fn compare_ge(left: Value, right: i64) -> Value {
    json!({"kind":"compare","operator":">=","left":left,
        "right":{"kind":"integer","value":right}})
}

#[test]
fn controller_history_and_life_guards_share_the_static_atoms() {
    let controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
    assert_eq!(
        guard("At the beginning of your end step, if you gained life this turn, draw a card."),
        compare_ge(
            json!({"kind":"lifeGainedThisTurn","player":controller.clone()}),
            1
        )
    );
    // Sibling: the same history family with a different resource and verb.
    assert_eq!(
        guard("At the beginning of your end step, if you've lost life this turn, draw a card."),
        compare_ge(
            json!({"kind":"lifeLostThisTurn","player":controller.clone()}),
            1
        )
    );
    assert_eq!(
        guard("At the beginning of your upkeep, if you have 30 or more life, draw a card."),
        compare_ge(json!({"kind":"lifeTotal","player":controller.clone()}), 30)
    );
    // The combined proposition is a Boolean program over independent atoms.
    let combined = guard(
        "At the beginning of your upkeep, if you have 5 or less life or you've cast two or more spells this turn, draw a card.",
    );
    assert_eq!(combined["kind"], "or");
    assert_eq!(combined["operands"][0]["operator"], "<=");
    assert_eq!(combined["operands"][1]["left"]["event"], json!("spellCast"));
}

#[test]
fn attack_and_counted_life_gain_history_compose_with_the_controller() {
    let controller = json!({"kind":"controllerOf","object":{"kind":"self"}});
    assert_eq!(
        guard("At the beginning of your end step, if you attacked this turn, draw a card."),
        compare_ge(
            json!({"kind":"countEventsThisTurn","event":"attackerDeclared",
                "player":controller.clone(),"where":null}),
            1
        )
    );
    assert_eq!(
        guard(
            "At the beginning of each end step, if you gained 3 or more life this turn, draw a card."
        ),
        compare_ge(json!({"kind":"lifeGainedThisTurn","player":controller}), 3)
    );
    assert_eq!(
        failure_state(
            "At the beginning of each end step, if you gained 3 or fewer life this turn, draw a card."
        ),
        "condition.static.count.direction"
    );
}

#[test]
fn this_turn_events_of_other_players_lower_to_event_counts() {
    assert_eq!(
        guard(
            "At the beginning of your end step, if an opponent lost life this turn, draw a card."
        ),
        compare_ge(
            json!({"kind":"countOpponentsWhoLostLifeThisTurn",
                "player":{"kind":"controllerOf","object":{"kind":"self"}}}),
            1
        )
    );
    assert_eq!(
        guard(
            "At the beginning of each end step, if three or more creatures died this turn, draw a card."
        ),
        compare_ge(
            json!({"kind":"countEventsThisTurn","event":"permanentDied",
                "where":{"kind":"cardTypeContains","value":"Creature"}}),
            3
        )
    );
    // Sibling: the article form and a subtype in place of a card type.
    assert_eq!(
        guard("At the beginning of each end step, if an Elf died this turn, draw a card.")["left"]
            ["where"],
        json!({"kind":"subtypeContains","value":"Elf"})
    );
}

#[test]
fn last_turn_spell_casts_are_a_distinct_turn_scope() {
    assert_eq!(
        guard("At the beginning of each upkeep, if no spells were cast last turn, draw a card."),
        json!({"kind":"compare","operator":"==",
            "left":{"kind":"countEventsPreviousTurn","event":"spellCast","aggregate":"total"},
            "right":{"kind":"integer","value":0}})
    );
    assert_eq!(
        guard(
            "At the beginning of each upkeep, if a player cast two or more spells last turn, draw a card."
        ),
        compare_ge(
            json!({"kind":"countEventsPreviousTurn","event":"spellCast",
                "aggregate":"greatestPerPlayer"}),
            2
        )
    );
}

#[test]
fn guard_atoms_that_cannot_be_resolved_stay_explicit_failures() {
    // `it` has no antecedent in a guard of a non-source event.
    assert_eq!(
        failure_state("Whenever you gain life, if it's tapped, draw a card."),
        "condition.sentence.object.antecedent"
    );
    // A death count with a controller relation is not a bare type count.
    assert_eq!(
        failure_state(
            "At the beginning of each end step, if a creature you control died this turn, draw a card."
        ),
        "selector.predicate.complete"
    );
    // A direction that the count does not support.
    assert_eq!(
        failure_state(
            "At the beginning of each end step, if two or fewer creatures died this turn, draw a card."
        ),
        "condition.static.count.direction"
    );
}
