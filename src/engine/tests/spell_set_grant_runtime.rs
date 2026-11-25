//! Runtime proofs of the spell-set relations of the shared query: a keyword
//! granted to the spells a player casts (`Artifact spells you cast have
//! convoke`) and a reduction read from the zone a spell is cast from
//! (`Spells you cast from your graveyard cost {1} less to cast`).
use super::*;

fn parsed_rules(name: &str, type_line: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

#[test]
fn chief_engineer_lets_creatures_convoke_artifact_spells_only() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    let mut engineer = test_definition("chief-engineer", "Creature - Human Artificer");
    engineer.rules = parsed_rules(
        "Chief Engineer",
        "Creature — Human Artificer",
        "Artifact spells you cast have convoke. (Your creatures can help cast those spells. Each creature you tap while casting an artifact spell pays for {1} or one mana of that creature's color.)",
    );
    let mut artifact = test_definition("artifact-spell", "Artifact");
    artifact.mana_cost = "{2}".to_string();
    let mut sorcery = test_definition("plain-sorcery", "Sorcery");
    sorcery.mana_cost = "{2}".to_string();
    sorcery.rules = vec![json!({
        "kind": "spellAbility",
        "source": { "kind": "self" },
        "effects": [],
    })];
    engine.state.players[0].hand = vec![
        test_instance("artifact-spell", artifact, "player-0"),
        test_instance("plain-sorcery", sorcery, "player-0"),
    ];
    let mut tapper = test_instance(
        "tapper",
        test_definition("tapper", "Creature - Golem"),
        "player-0",
    );
    tapper.summoning_sick = true;
    engine.state.players[0].battlefield = vec![
        test_instance("chief-engineer", engineer, "player-0"),
        tapper,
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    let actions = engine.legal_priority_actions(0);
    let convoked = actions
        .iter()
        .find(|action| {
            action.card_instance_id.as_deref() == Some("artifact-spell")
                && action.decisions["convokeCount"] == 2
        })
        .expect("both creatures convoke the artifact spell")
        .clone();
    assert!(
        !actions.iter().any(|action| {
            action.card_instance_id.as_deref() == Some("plain-sorcery")
                && action.decisions.get("convokeCount").is_some()
        }),
        "a sorcery is not an artifact spell"
    );
    engine
        .apply_priority_action(&convoked, &mut EmeritusDecisionProvider)
        .expect("the artifact spell is cast with convoke");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|permanent| permanent.tapped)
    );
}

#[test]
fn graveyard_cast_reduction_applies_only_to_the_casters_own_graveyard_zone() {
    let mut engine = test_engine(2);
    let mut lamia = test_definition("lamia", "Creature - Snake Lamia");
    lamia.rules = parsed_rules(
        "Gravebreaker Lamia",
        "Creature — Snake Lamia",
        "Spells you cast from your graveyard cost {1} less to cast.",
    );
    engine.state.players[0].battlefield = vec![test_instance("lamia", lamia, "player-0")];
    let mut spell = test_definition("spell", "Sorcery");
    spell.mana_cost = "{3}".to_string();
    let own = test_instance("own-card", spell.clone(), "player-0");
    let foreign = test_instance("foreign-card", spell.clone(), "player-1");
    let decisions = BTreeMap::new();
    assert_eq!(
        engine
            .checked_global_casting_cost_reduction(
                "player-0",
                &spell,
                "graveyard",
                &decisions,
                Some(&own)
            )
            .expect("checked reduction"),
        1
    );
    assert_eq!(
        engine
            .checked_global_casting_cost_reduction(
                "player-0",
                &spell,
                "hand",
                &decisions,
                Some(&own)
            )
            .expect("checked reduction"),
        0,
        "a spell cast from the hand is not cast from the graveyard"
    );
    assert_eq!(
        engine
            .checked_global_casting_cost_reduction(
                "player-0",
                &spell,
                "graveyard",
                &decisions,
                Some(&foreign),
            )
            .expect("checked reduction"),
        0,
        "a card of another player's graveyard is not in the caster's graveyard"
    );
}

/// `for each opponent who lost life this turn` as the parser lowers it: a
/// count of the controller-relative population members satisfying the
/// member condition, the same on the static and resolution paths.
#[test]
fn population_count_reads_each_members_own_history() {
    let mut engine = test_engine(4);
    let count = json!({
        "kind": "countPlayers",
        "players": {
            "kind": "filterPlayers",
            "players": {
                "kind": "opponentsOf",
                "player": { "kind": "controllerOf", "object": { "kind": "self" } },
            },
            "where": {
                "kind": "compare",
                "operator": ">=",
                "left": { "kind": "lifeLostThisTurn", "player": { "kind": "quantifiedPlayer" } },
                "right": { "kind": "integer", "value": 1 },
            },
        },
    });
    assert!(numeric_expression_supported(&count), "{count:#}");
    assert_eq!(engine.numeric_expression(&count, "player-0"), Some(0));
    engine.record_event(
        "lifeLost",
        Some("player-1".to_string()),
        None,
        json!({ "amount": 2 }),
    );
    // The controller's own loss is not an opponent's.
    engine.record_event(
        "lifeLost",
        Some("player-0".to_string()),
        None,
        json!({ "amount": 1 }),
    );
    engine.record_event(
        "lifeLost",
        Some("player-3".to_string()),
        None,
        json!({ "amount": 1 }),
    );
    assert_eq!(engine.numeric_expression(&count, "player-0"), Some(2));
    // Every player, the controller included.
    let mut every = count.clone();
    every["players"]["players"] = json!({ "kind": "eachPlayer" });
    assert_eq!(engine.numeric_expression(&every, "player-0"), Some(3));
    // An excluded bound player needs bindings a count does not read.
    let mut excluded = count.clone();
    excluded["players"]["except"] = json!({ "kind": "triggeringPlayer" });
    assert!(!numeric_expression_supported(&excluded));
}
