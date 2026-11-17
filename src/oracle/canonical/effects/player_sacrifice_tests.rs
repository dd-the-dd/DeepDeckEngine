use super::*;
use crate::oracle::{
    model::AbilitySource,
    syntax::{AbilityInput, recognize_oracle_entities},
};

fn parse(text: &str) -> Result<Value, String> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_oracle_entities(text);
    let input = AbilityInput {
        face_name: "Player Action Source",
        face_type_line: "Creature — Demon",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    };
    match parse_canonical_rule_native(&input, Some("triggeredAbility")) {
        CanonicalParseOutcome::Parsed { draft, .. } => Ok(draft.rule),
        CanonicalParseOutcome::Failed { failure, .. } => Err(format!("{failure:?}")),
        _ => Err("unsupported or failed with candidate evidence".to_string()),
    }
}

fn effects(text: &str) -> Vec<Value> {
    parse(text).unwrap_or_else(|failure| panic!("{text}: {failure}"))["effects"]
        .as_array()
        .unwrap()
        .clone()
}

fn kinds(effects: &[Value]) -> Vec<&str> {
    effects
        .iter()
        .map(|effect| effect["kind"].as_str().unwrap())
        .collect()
}

fn target() -> Value {
    json!({"kind":"chosenTarget","id":"targetPlayer"})
}

fn assert_target_sacrifice(effects: &[Value], offset: usize, count: i64, type_name: &str) {
    let choice = &effects[offset];
    assert_eq!(choice["kind"], "chooseCards");
    assert_eq!(choice["player"], target());
    assert_eq!(choice["from"]["kind"], "eachPermanent");
    assert_eq!(choice["from"]["player"], target());
    assert_eq!(choice["from"]["where"]["value"], type_name);
    assert_eq!(choice["count"], json!({"kind":"integer","value":count}));
    let batch = &effects[offset + 1];
    assert_eq!(batch["kind"], "sacrificePermanents");
    assert_eq!(batch["player"], target());
    assert_eq!(batch["objects"]["decisionId"], choice["id"]);
}

#[test]
fn target_opponent_sacrifices_a_creature_of_their_choice() {
    let rule =
        parse("When this creature enters, target opponent sacrifices a creature of their choice.")
            .unwrap();
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["id"], "targetPlayer");
    assert_eq!(decisions[0]["candidates"]["kind"], "players");
    let effects = rule["effects"].as_array().unwrap();
    assert_eq!(kinds(effects), ["chooseCards", "sacrificePermanents"]);
    assert_target_sacrifice(effects, 0, 1, "Creature");
}

#[test]
fn singular_subject_composes_with_new_quantity_query_and_discard_selection() {
    let effects = effects(
        "When this creature enters, target player discards two cards at random and sacrifices an artifact.",
    );
    assert_eq!(
        kinds(&effects),
        [
            "chooseCards",
            "discardCards",
            "chooseCards",
            "sacrificePermanents"
        ]
    );
    assert_eq!(effects[1]["player"], target());
    assert_target_sacrifice(&effects, 2, 1, "Artifact");
    assert_ne!(effects[0]["id"], effects[2]["id"]);
}

#[test]
fn one_player_subject_is_shared_by_a_coordinated_action_list() {
    let effects = effects(
        "When this creature attacks, target opponent sacrifices a creature or planeswalker of their choice, discards a card, and loses 3 life. You draw a card and gain 3 life.",
    );
    assert_eq!(
        kinds(&effects),
        [
            "chooseCards",
            "sacrificePermanents",
            "discardCards",
            "loseLife",
            "drawCards",
            "gainLife"
        ]
    );
    assert_eq!(effects[2]["player"], target());
    assert_eq!(effects[3]["player"], target());
    assert_eq!(effects[4]["player"], controller());
    assert_eq!(effects[5]["player"], controller());
}

#[test]
fn that_player_resolves_only_through_the_declared_antecedent() {
    let effects = effects(
        "When this creature enters, target player gains 2 life. That player sacrifices a land.",
    );
    assert_eq!(
        kinds(&effects),
        ["gainLife", "chooseCards", "sacrificePermanents"]
    );
    assert_target_sacrifice(&effects, 1, 1, "Land");
}

#[test]
fn singular_near_misses_stay_explicit_failures() {
    for text in [
        // No earlier player binding: `that player` is never guessed.
        "When this creature enters, that player sacrifices a creature.",
        // Agreement of the third-person verb.
        "When this creature enters, target player sacrifice a creature.",
        // A controller relation contradicts the acting player's own permanents.
        "When this creature enters, target player sacrifices a creature you control.",
        // Only players act; an object subject is not a player subject.
        "When this creature enters, target creature sacrifices a creature.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}

#[test]
fn quantified_subject_shares_actions_and_finishes_each_before_the_next() {
    let program = effects(
        "When this creature dies, each player sacrifices a creature, discards a card, and loses 4 life.",
    );
    // CR 608.2c / 101.4: choice loop, sacrifice batch, discard loop, one loss.
    assert_eq!(
        kinds(&program),
        [
            "forEachPlayer",
            "sacrificePermanents",
            "forEachPlayer",
            "loseLife"
        ]
    );
    assert_eq!(program[0]["effects"][0]["kind"], "chooseCards");
    assert_eq!(program[2]["effects"][0]["kind"], "discardCards");
    assert_eq!(program[3]["player"], json!({"kind":"eachPlayer"}));
    assert_ne!(program[0]["bind"], program[2]["bind"]);
}

#[test]
fn quantified_actions_compose_across_draw_mill_life_and_discard() {
    let opponents = json!({"kind":"opponentsOf","player":controller()});
    let program =
        effects("When this creature dies, each opponent discards a card and loses 2 life.");
    assert_eq!(kinds(&program), ["forEachPlayer", "loseLife"]);
    assert_eq!(program[1]["player"], opponents);
    let program = effects(
        "When this creature dies, each player draws two cards, mills a card, and gains 2 life.",
    );
    assert_eq!(
        kinds(&program),
        ["forEachPlayer", "forEachPlayer", "forEachPlayer"]
    );
    assert_eq!(program[0]["effects"][0]["kind"], "drawCards");
    assert_eq!(program[1]["effects"][0]["kind"], "mill");
    assert_eq!(program[2]["effects"][0]["kind"], "gainLife");
}

#[test]
fn wider_populations_lower_life_actions() {
    let program = effects("When this creature dies, each player loses 2 life.");
    assert_eq!(
        program,
        [
            json!({"kind":"loseLife","player":{"kind":"eachPlayer"},"amount":{"kind":"integer","value":2}})
        ]
    );
    let program = effects("When this creature dies, each other player gains 2 life.");
    assert_eq!(program[0]["kind"], "forEachPlayer");
    assert_eq!(
        program[0]["players"],
        json!({"kind":"opponentsOf","player":controller()})
    );
}

#[test]
fn quantified_list_ends_before_a_controller_clause() {
    let program =
        effects("When this creature dies, each player sacrifices a creature and you gain 2 life.");
    assert_eq!(
        kinds(&program),
        ["forEachPlayer", "sacrificePermanents", "gainLife"]
    );
    assert_eq!(program[2]["player"], controller());
}

#[test]
fn quantified_list_near_misses_stay_explicit_failures() {
    for text in [
        "When this creature dies, each player sacrifices a creature and loses.",
        "When this creature dies, each opponent discards a card and sings a song.",
        "When this creature dies, each player sacrifices a creature, then loses 2 life.",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}
