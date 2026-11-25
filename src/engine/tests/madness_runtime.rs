//! Real Oracle text -> `madness` keyword IR -> engine behavior (CR 702.35a):
//! a discarded madness card is exiled instead of put into its owner's
//! graveyard, and the linked trigger lets its owner cast it for its madness
//! cost while the trigger resolves, or puts it into that graveyard.
use super::*;

const MADNESS_REMINDER: &str = "(If you discard this card, discard it into exile. When you do, cast it for its madness cost or put it into your graveyard.)";

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
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

fn reckless_wurm(instance_id: &str, owner: &str) -> CardInstance {
    let mut definition = test_definition(instance_id, "Creature — Wurm");
    definition.name = "Reckless Wurm".to_string();
    definition.mana_cost = "{3}{R}{R}".to_string();
    definition.power = Some("4".to_string());
    definition.toughness = Some("4".to_string());
    definition.rules = parsed_rules(
        "Reckless Wurm",
        "Creature — Wurm",
        "{3}{R}{R}",
        &format!("Trample\nMadness {{2}}{{R}} {MADNESS_REMINDER}"),
    );
    test_instance(instance_id, definition, owner)
}

/// Casts every resolution-time spell it is offered; passes otherwise.
struct CastWhenOffered;

impl DecisionProvider for CastWhenOffered {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let selected = match request.kind {
            DecisionKind::Priority => request
                .options
                .iter()
                .position(|action| action.kind == ActionKind::PassPriority),
            DecisionKind::ResolutionChoice => request
                .options
                .iter()
                .position(|action| action.kind == ActionKind::CastSpell)
                .or_else(|| request.options.len().checked_sub(1)),
            _ => Some(0),
        };
        selected.ok_or_else(|| EngineError::new("test decision has no matching option"))
    }
}

fn in_zone(cards: &[CardInstance], instance_id: &str) -> bool {
    cards.iter().any(|card| card.instance_id == instance_id)
}

fn madness_triggers(engine: &GameEngine, instance_id: &str) -> usize {
    engine
        .state
        .events
        .iter()
        .filter(|event| {
            event.kind == "madnessTriggered"
                && event.card_instance_id.as_deref() == Some(instance_id)
        })
        .count()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_discarded_madness_card_is_exiled_then_cast_for_its_madness_cost() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![reckless_wurm("wurm", "player-0")];
    engine.state.players[0].battlefield = vec![
        mana_source("mountain-a", "Basic Land - Mountain", "{R}", None),
        mana_source("mountain-b", "Basic Land - Mountain", "{R}", None),
        mana_source("mountain-c", "Basic Land - Mountain", "{R}", None),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    engine
        .discard_card_as_cost("player-0", "wurm")
        .expect("the madness card is discarded");
    assert!(
        in_zone(&engine.state.players[0].exile, "wurm"),
        "discarded into exile (CR 702.35a)"
    );
    assert!(!in_zone(&engine.state.players[0].graveyard, "wurm"));
    assert_eq!(madness_triggers(&engine, "wurm"), 1);
    assert_eq!(engine.state.stack.len(), 1, "the linked trigger");

    // The trigger resolves: its owner casts the card for {2}{R}, not its
    // {3}{R}{R} mana cost, even though it is a creature spell.
    engine
        .resolve_top_stack(&mut CastWhenOffered)
        .expect("the madness trigger resolves");
    let spell = engine
        .state
        .stack
        .iter()
        .find(|object| object.card.instance_id == "wurm")
        .expect("the madness card is a spell on the stack");
    assert_eq!(
        spell.decisions.get("alternativeCostMode"),
        Some(&json!("madness"))
    );
    assert_eq!(
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|permanent| permanent.tapped)
            .count(),
        3,
        "three mana paid for {{2}}{{R}}"
    );
    assert!(!in_zone(&engine.state.players[0].exile, "wurm"));

    engine
        .resolve_top_stack(&mut CastWhenOffered)
        .expect("the creature spell resolves");
    assert!(in_zone(&engine.state.players[0].battlefield, "wurm"));
    assert!(!in_zone(&engine.state.players[0].graveyard, "wurm"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_uncast_madness_card_goes_from_exile_to_its_owners_graveyard() {
    // No mana to pay the madness cost: the owner can't cast it, so the
    // trigger's second instruction puts it into the graveyard.
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![reckless_wurm("wurm", "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    engine
        .discard_card_as_cost("player-0", "wurm")
        .expect("the madness card is discarded");
    assert!(in_zone(&engine.state.players[0].exile, "wurm"));
    engine
        .resolve_top_stack(&mut CastWhenOffered)
        .expect("the madness trigger resolves");
    assert!(engine.state.stack.is_empty(), "nothing was cast");
    assert!(!in_zone(&engine.state.players[0].exile, "wurm"));
    assert!(in_zone(&engine.state.players[0].graveyard, "wurm"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_declined_madness_cast_and_a_card_without_madness_reach_the_graveyard() {
    // EmeritusDecisionProvider declines every resolution choice; a card
    // without madness is never exiled and triggers nothing.
    let mut engine = test_engine(2);
    engine.state.players[0].hand = vec![
        reckless_wurm("wurm", "player-0"),
        test_instance(
            "plain-bear",
            test_definition("plain-bear", "Creature — Bear"),
            "player-0",
        ),
    ];
    engine.state.players[0].battlefield = vec![
        mana_source("mountain-a", "Basic Land - Mountain", "{R}", None),
        mana_source("mountain-b", "Basic Land - Mountain", "{R}", None),
        mana_source("mountain-c", "Basic Land - Mountain", "{R}", None),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    engine
        .discard_card_as_cost("player-0", "plain-bear")
        .expect("the plain card is discarded");
    assert!(in_zone(&engine.state.players[0].graveyard, "plain-bear"));
    assert_eq!(madness_triggers(&engine, "plain-bear"), 0);
    assert!(engine.state.stack.is_empty());

    engine
        .discard_card_as_cost("player-0", "wurm")
        .expect("the madness card is discarded");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the madness trigger resolves");
    assert!(engine.state.stack.is_empty(), "the cast was declined");
    assert!(in_zone(&engine.state.players[0].graveyard, "wurm"));
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|permanent| !permanent.tapped),
        "no madness cost was paid"
    );
}
