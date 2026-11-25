//! Real Oracle text -> primitive IR -> engine behavior for program
//! conditions that read an earlier instruction or a casting choice, and for
//! the zone a triggered ability functions from:
//! - `If you do` after an option a sentence conditional guards (CR 608.2c):
//!   false when the guard failed or the option was declined;
//! - a target a kicker branch alone acts on is declared only for a kicked
//!   cast (CR 601.2c);
//! - a triggered ability that returns its card from the graveyard functions
//!   from the graveyard (CR 113.6k).
use super::*;

fn parsed_rules(card_name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: card_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
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
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

fn triggered_rule(rules: &[Value]) -> Value {
    rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .cloned()
        .expect("a triggered ability")
}

fn ability_stack(source: &CardInstance, rule: &Value) -> StackObject {
    StackObject {
        id: "stack:condition-guard".to_string(),
        controller: "player-0".to_string(),
        card: source.clone(),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("triggeredAbility".to_string()),
        ability_rule: Some(rule.clone()),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

/// Declines every resolution option (`you may ...`); any other decision is
/// the default test provider's.
struct DecliningProvider;

impl DecisionProvider for DecliningProvider {
    fn choose(
        &mut self,
        state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if matches!(request.kind, DecisionKind::ResolutionChoice) {
            return Ok(0);
        }
        EmeritusDecisionProvider.choose(state, request)
    }
}

const PROBE_SEER: &str = "At the beginning of your upkeep, look at the top card of your library. If it's a land card, you may reveal it. If you do, draw a card.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_guarded_option_outcome_needs_the_guard_and_the_performed_option() {
    let rule = triggered_rule(&parsed_rules(
        "Probe Seer",
        "Creature - Human Wizard",
        "{2}{U}",
        PROBE_SEER,
    ));
    // (top card, the option is accepted, cards drawn)
    for (top, accept, drawn) in [
        ("Land - Forest", true, 1),
        ("Land - Forest", false, 0),
        ("Creature - Elf", true, 0),
    ] {
        let mut engine = test_engine(2);
        let source = test_instance(
            "probe-seer",
            test_definition("probe-seer", "Creature - Human Wizard"),
            "player-0",
        );
        let player = &mut engine.state.players[0];
        player.hand.clear();
        player.battlefield = vec![source.clone()];
        player.library = vec![
            test_instance("top-card", test_definition("top-card", top), "player-0"),
            test_instance(
                "second-card",
                test_definition("second-card", "Instant"),
                "player-0",
            ),
        ];
        let stack = ability_stack(&source, &rule);
        let result = if accept {
            execute_effects_at_resolution_start(
                &mut engine,
                &rule["effects"],
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut EmeritusDecisionProvider,
            )
        } else {
            execute_effects_at_resolution_start(
                &mut engine,
                &rule["effects"],
                &stack,
                &mut BTreeMap::new(),
                &mut BTreeMap::new(),
                &mut DecliningProvider,
            )
        };
        result.expect("the guarded option resolves");
        assert_eq!(
            engine.state.players[0].hand.len(),
            drawn,
            "top {top}, accepted {accept}: {rule:#}"
        );
    }
}

fn kicked(declaration: &CastingDeclaration) -> bool {
    declaration.decisions.get("kicked").and_then(Value::as_bool) == Some(true)
        || declaration
            .decisions
            .iter()
            .any(|(id, value)| id.starts_with("kickerPaid:") && value.as_bool() == Some(true))
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_kicker_branch_target_is_declared_only_for_a_kicked_cast() {
    let rules = parsed_rules(
        "Probe Shock",
        "Instant",
        "{1}{U}",
        "Kicker {2}\nDraw a card. If this spell was kicked, tap target creature.",
    );
    let mut engine = test_engine(2);
    engine.state.players[1].battlefield.push(test_instance(
        "tap-candidate",
        test_definition("tap-candidate", "Creature - Bear"),
        "player-1",
    ));
    let mut definition = test_definition("probe-shock", "Instant");
    definition.mana_cost = "{1}{U}".to_string();
    definition.rules = rules;
    let declarations = engine
        .declarations_for_card(&definition, "player-0")
        .expect("the kicker spell has declarations");
    assert!(
        declarations
            .iter()
            .any(|declaration| !kicked(declaration) && declaration.targets.is_empty()),
        "an unkicked cast declares no target"
    );
    assert!(
        declarations
            .iter()
            .filter(|declaration| !kicked(declaration))
            .all(|declaration| declaration.targets.is_empty()),
        "no unkicked cast declares the kicker branch's target"
    );
    let kicked_casts = declarations
        .iter()
        .filter(|declaration| kicked(declaration))
        .collect::<Vec<_>>();
    assert!(!kicked_casts.is_empty());
    assert!(
        kicked_casts
            .iter()
            .all(|declaration| declaration.targets.len() == 1),
        "every kicked cast declares the target its branch acts on"
    );
}

const SPIT_FLAME: &str = "Spit Flame deals 4 damage to target creature.\nWhenever a Dragon you control enters, you may pay {R}. If you do, return this card from your graveyard to your hand.";

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_trigger_that_returns_its_card_functions_from_the_graveyard() {
    let rules = parsed_rules("Spit Flame", "Instant", "{2}{R}", SPIT_FLAME);
    let rule = triggered_rule(&rules);
    assert_eq!(rule["triggerZone"], "graveyard", "{rule:#}");
    let mut engine = test_engine(2);
    let mut definition = test_definition("spit-flame", "Instant");
    definition.mana_cost = "{2}{R}".to_string();
    definition.rules = rules;
    engine.state.players[0]
        .graveyard
        .push(test_instance("spit-flame", definition, "player-0"));
    engine.state.players[0].mana_pool = vec![FloatingMana {
        symbol: "R".to_string(),
        spend_restriction: None,
    }];
    let dragon = test_instance(
        "entered-dragon",
        test_definition("entered-dragon", "Creature - Dragon"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(dragon.clone());

    engine.enqueue_enter_battlefield_triggers(&dragon);
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the card in the graveyard observes the entry"
    );
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the graveyard trigger resolves");
    assert!(engine.state.players[0].graveyard.is_empty());
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "spit-flame")
    );

    // The same card in a hand observes nothing: it functions only from the
    // graveyard (CR 113.6).
    let mut engine = test_engine(2);
    let mut definition = test_definition("spit-flame", "Instant");
    definition.rules = parsed_rules("Spit Flame", "Instant", "{2}{R}", SPIT_FLAME);
    engine.state.players[0].hand = vec![test_instance("spit-flame", definition, "player-0")];
    engine.state.players[0].battlefield.push(dragon.clone());
    engine.enqueue_enter_battlefield_triggers(&dragon);
    assert!(engine.state.stack.is_empty());
}
