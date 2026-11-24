//! Real Oracle text -> primitive IR -> engine behavior for player slots: a
//! declared target player or a chosen opponent performs the instruction and
//! owns its decisions (CR 608.2c, 608.2d, 109.5), and a search reads the
//! searched player's zones (CR 701.23a).
use super::*;

fn parsed_rule(type_line: &str, oracle: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: "Player Slot Probe".to_string(),
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
    assert_eq!(parsed.status, "canonical", "{oracle}");
    let rule = parsed
        .abilities
        .into_iter()
        .find_map(|ability| ability.rule)
        .expect("one rule");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

/// Answers like the shared test provider and records who was asked.
#[derive(Default)]
struct RecordingProvider {
    choosers: Vec<String>,
}

impl DecisionProvider for RecordingProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if request.kind == DecisionKind::ResolutionChoice {
            self.choosers.push(request.player_id.clone());
        }
        EmeritusDecisionProvider.choose(state, request)
    }
}

fn spell(rule: &Value, targets: BTreeMap<String, TargetRef>) -> StackObject {
    StackObject {
        id: "stack:probe".to_string(),
        controller: "player-0".to_string(),
        card: test_instance("probe", test_definition("probe", "Sorcery"), "player-0"),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets,
    }
}

fn declared_player_target(rule: &Value, player_id: &str) -> BTreeMap<String, TargetRef> {
    let target_id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("a declared player")
        .to_string();
    BTreeMap::from([(
        target_id,
        TargetRef::Player {
            player_id: player_id.to_string(),
        },
    )])
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_declared_performer_owns_its_may_decision() {
    // CR 608.2d: `target opponent may ...` asks the target, never the
    // controller, and the instruction is the target's own.
    let rule = parsed_rule("Sorcery", "Target opponent may draw a card.");
    let mut engine = test_engine(2);
    let hand_before = engine.state.players[1].hand.len();
    let controller_hand_before = engine.state.players[0].hand.len();
    let mut provider = RecordingProvider::default();
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &spell(&rule, declared_player_target(&rule, "player-1")),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the spell resolves");
    assert!(!provider.choosers.is_empty(), "the permission was asked");
    assert!(
        provider.choosers.iter().all(|player| player == "player-1"),
        "{:?}",
        provider.choosers
    );
    assert_eq!(engine.state.players[1].hand.len(), hand_before + 1);
    assert_eq!(engine.state.players[0].hand.len(), controller_hand_before);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_declared_performer_runs_the_ordinary_instruction_as_its_own() {
    // Unscrupulous Agent: the target exiles a card from its own hand.
    let rule = parsed_rule("Sorcery", "Target opponent exiles a card from their hand.");
    let mut engine = test_engine(2);
    let opponent_hand = engine.state.players[1].hand.len();
    let controller_hand = engine.state.players[0].hand.len();
    let mut provider = RecordingProvider::default();
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &spell(&rule, declared_player_target(&rule, "player-1")),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the spell resolves");
    assert_eq!(engine.state.players[1].hand.len(), opponent_hand - 1);
    assert_eq!(engine.state.players[1].exile.len(), 1);
    assert_eq!(engine.state.players[0].hand.len(), controller_hand);
    assert!(
        provider.choosers.iter().all(|player| player == "player-1"),
        "{:?}",
        provider.choosers
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_opponent_performer_is_chosen_once_by_the_controller() {
    // CR 608.2d: the controller chooses the opponent on resolution; the
    // chosen opponent draws.
    let rule = parsed_rule("Sorcery", "An opponent draws a card.");
    let mut engine = test_engine(3);
    let hands = (0..3)
        .map(|index| engine.state.players[index].hand.len())
        .collect::<Vec<_>>();
    let mut provider = RecordingProvider::default();
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &spell(&rule, BTreeMap::new()),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the spell resolves");
    assert_eq!(provider.choosers, ["player-0"], "the controller chooses");
    let drawn = (0..3)
        .filter(|index| engine.state.players[*index].hand.len() == hands[*index] + 1)
        .collect::<Vec<_>>();
    assert_eq!(drawn.len(), 1, "exactly one player drew");
    assert_ne!(drawn[0], 0, "an opponent drew");
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_search_reads_every_listed_zone_of_its_player() {
    // CR 701.23a: `your library and graveyard` is one choice over both
    // zones; a graveyard card is found and the library is shuffled.
    let rule = parsed_rule(
        "Sorcery",
        "Search your library and graveyard for a creature card, put it into your hand, then shuffle.",
    );
    let mut engine = test_engine(2);
    engine.state.players[0].graveyard.push(test_instance(
        "buried-bear",
        test_definition("buried-bear", "Creature - Bear"),
        "player-0",
    ));
    let mut provider = RecordingProvider::default();
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &spell(&rule, BTreeMap::new()),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the spell resolves");
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "buried-bear"),
        "the graveyard creature was found"
    );
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "zoneShuffled" && event.player_id.as_deref() == Some("player-0")
    }));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_declared_holders_library_is_searched_and_its_player_shuffles() {
    // Bitter Ordeal's search: the controller searches the target's library;
    // `that player shuffles` is the target's own instruction.
    let rule = parsed_rule(
        "Sorcery",
        "Search target player's library for a card and exile it. Then that player shuffles.",
    );
    let mut engine = test_engine(2);
    let library = engine.state.players[1].library.len();
    let controller_library = engine.state.players[0].library.len();
    let mut provider = RecordingProvider::default();
    execute_effects_at_resolution_start(
        &mut engine,
        &rule["effects"],
        &spell(&rule, declared_player_target(&rule, "player-1")),
        &mut BTreeMap::new(),
        &mut BTreeMap::new(),
        &mut provider,
    )
    .expect("the spell resolves");
    assert_eq!(engine.state.players[1].library.len(), library - 1);
    assert_eq!(engine.state.players[1].exile.len(), 1);
    assert_eq!(engine.state.players[0].library.len(), controller_library);
    assert!(
        provider.choosers.iter().all(|player| player == "player-0"),
        "the searcher chooses: {:?}",
        provider.choosers
    );
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "zoneShuffled" && event.player_id.as_deref() == Some("player-1")
    }));
}
