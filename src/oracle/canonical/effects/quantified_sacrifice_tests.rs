use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

fn parse(text: &str, kind: &str) -> Result<Value, String> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Quantified Sacrifice Source",
        face_type_line: "Creature — Demon",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule_native(&input, Some(kind)) {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. } => Err(format!("{failure:?}")),
        _ => Err("unsupported or failed with candidate evidence".to_string()),
    }
}

fn effects(text: &str) -> Vec<Value> {
    parse(text, "triggeredAbility").unwrap_or_else(|failure| panic!("{text}: {failure}"))["effects"]
        .as_array()
        .unwrap()
        .clone()
}

fn assert_sacrifice_program(effects: &[Value], players: Value, count: i64, predicate: Value) {
    assert_eq!(effects.len(), 2, "per-player choice, then one batch");
    let iteration = &effects[0];
    assert_eq!(iteration["kind"], "forEachPlayer");
    assert_eq!(iteration["players"], players);
    let choice = &iteration["effects"][0];
    assert_eq!(choice["kind"], "chooseCards");
    assert_eq!(choice["player"]["kind"], "boundValue");
    assert_eq!(choice["player"]["id"], iteration["bind"]);
    assert_eq!(choice["from"]["kind"], "eachPermanent");
    assert_eq!(choice["from"]["player"], choice["player"]);
    assert_eq!(choice["from"]["where"], predicate);
    assert_eq!(choice["count"], json!({"kind":"integer","value":count}));
    assert_eq!(iteration["collect"]["objects"]["decisionId"], choice["id"]);
    let batch = &effects[1];
    assert_eq!(batch["kind"], "sacrificePermanents");
    assert_eq!(batch["player"], players);
    assert_eq!(batch["objects"]["binding"], iteration["collect"]["bind"]);
}

fn creature() -> Value {
    json!({"kind":"cardTypeContains","value":"Creature"})
}

#[test]
fn each_player_sacrifices_a_creature_of_their_choice() {
    let effects =
        effects("When this creature dies, each player sacrifices a creature of their choice.");
    assert_sacrifice_program(&effects, json!({"kind":"eachPlayer"}), 1, creature());
}

#[test]
fn chooser_marker_and_plural_quantity_compose_with_every_population() {
    // Previously unused combinations of the same independent concepts.
    let opponents = json!({"kind":"opponentsOf","player":controller()});
    for (text, players, count) in [
        (
            "Whenever this creature attacks, each opponent sacrifices two creatures of their choice.",
            opponents.clone(),
            2,
        ),
        (
            "Whenever this creature attacks, each opponent sacrifices two creatures.",
            opponents.clone(),
            2,
        ),
        (
            "Whenever a creature you control dies, each other player sacrifices a creature of their choice.",
            opponents,
            1,
        ),
    ] {
        assert_sacrifice_program(&effects(text), players, count, creature());
    }
}

#[test]
fn identities_are_scoped_per_sequence_position() {
    let program = effects(
        "When this creature enters, each player sacrifices a creature of their choice. Then each player sacrifices a land of their choice.",
    );
    assert_eq!(program.len(), 4);
    assert_ne!(program[0]["bind"], program[2]["bind"]);
    assert_ne!(
        program[0]["effects"][0]["id"],
        program[2]["effects"][0]["id"]
    );
    assert_eq!(program[2]["effects"][0]["from"]["where"]["value"], "Land");
}

#[test]
fn each_player_discards_reuses_the_discard_child_without_targets() {
    let program = effects("When this creature enters, each player discards a card.");
    assert_eq!(program.len(), 1);
    assert_eq!(program[0]["kind"], "forEachPlayer");
    assert_eq!(program[0]["players"], json!({"kind":"eachPlayer"}));
    assert_eq!(program[0]["effects"][0]["kind"], "discardCards");
    assert_eq!(program[0]["effects"][0]["player"]["kind"], "boundValue");
}

#[test]
fn near_misses_stay_explicit_failures() {
    for text in [
        // A controller relation contradicts the iteration binding.
        "When this creature dies, each player sacrifices a creature you control of their choice.",
        // The marker is `their`, never `your`, for a third-person actor.
        "When this creature dies, each player sacrifices a creature of your choice.",
        // A trailing payment continuation is not swallowed by the sacrifice.
        "When this creature dies, each opponent sacrifices a creature of their choice unless they pay {1}.",
    ] {
        assert!(parse(text, "triggeredAbility").is_err(), "{text}");
    }
}
