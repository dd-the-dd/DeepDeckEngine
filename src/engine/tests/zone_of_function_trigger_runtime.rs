//! Real Oracle text -> primitive IR -> engine behavior for triggered
//! abilities that function from a graveyard (CR 113.6, 603.4, 400.7) and
//! for Miracle (CR 702.94a):
//! - a graveyard card observes a spell cast, a step, a life gain, a draw and
//!   a card put into a graveyard with the matcher battlefield observers use;
//! - the same rule on a permanent does not function (its zone of function
//!   is the graveyard), and a rule without zone evidence stays a
//!   battlefield rule that a graveyard card never triggers;
//! - the first card drawn in a turn with Miracle is revealed and may be cast
//!   for its miracle cost; a later draw of the turn offers nothing.
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

fn card(
    instance_id: &str,
    name: &str,
    type_line: &str,
    mana_cost: &str,
    oracle: &str,
    owner: &str,
) -> CardInstance {
    let mut definition = test_definition(instance_id, type_line);
    definition.name = name.to_string();
    definition.mana_cost = mana_cost.to_string();
    definition.rules = parsed_rules(name, type_line, mana_cost, oracle);
    test_instance(instance_id, definition, owner)
}

fn triggered_rule(source: &CardInstance) -> Value {
    source
        .definition
        .rules
        .iter()
        .find(|rule| rule["kind"] == "triggeredAbility")
        .cloned()
        .expect("a triggered ability")
}

fn pool(symbols: &[&str]) -> Vec<FloatingMana> {
    symbols
        .iter()
        .map(|symbol| FloatingMana {
            symbol: (*symbol).to_string(),
            spend_restriction: None,
        })
        .collect()
}

fn main_phase_engine() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].hand.clear();
    engine.state.players[0].graveyard.clear();
    engine.state.players[0].battlefield.clear();
    engine.state.players[1].battlefield.clear();
    engine
}

fn admit(engine: &mut GameEngine) {
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
}

/// Resolves the whole stack, accepting every resolution option.
fn resolve_stack(engine: &mut GameEngine) {
    let mut guard = 0;
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut EmeritusDecisionProvider)
            .expect("the stack resolves");
        guard += 1;
        assert!(guard < 32, "the stack terminates");
    }
}

/// Casts a card of player 0's hand through its legal action.
fn cast(engine: &mut GameEngine, instance_id: &str) {
    let action = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some(instance_id)
        })
        .unwrap_or_else(|| panic!("{instance_id} can be cast"));
    engine
        .apply_priority_action(&action, &mut CastSpellDecisionProvider)
        .expect("the spell is cast");
}

fn in_zone(cards: &[CardInstance], instance_id: &str) -> bool {
    cards.iter().any(|card| card.instance_id == instance_id)
}

fn vanilla_creature(instance_id: &str) -> CardInstance {
    test_instance(
        instance_id,
        test_definition(instance_id, "Creature - Bear"),
        "player-0",
    )
}

const VENGEVINE: &str = "Haste\nWhenever you cast a spell, if it's the second creature spell you cast this turn, you may return this card from your graveyard to the battlefield.";

const ARCLIGHT_PHOENIX: &str = "Flying, haste\nAt the beginning of combat on your turn, if you've cast three or more instant and sorcery spells this turn, return this card from your graveyard to the battlefield.";

const BRIDGE_FROM_BELOW: &str = "Whenever a nontoken creature is put into your graveyard from the battlefield, if this card is in your graveyard, create a 2/2 black Zombie creature token.\nWhen a creature is put into an opponent's graveyard from the battlefield, if this card is in your graveyard, exile this card.";

const PUNISHING_FIRE: &str = "Punishing Fire deals 2 damage to any target.\nWhenever an opponent gains life, you may pay {R}. If you do, return this card from your graveyard to your hand.";

const TEMPORAL_MASTERY: &str = "Take an extra turn after this one. Exile Temporal Mastery.\nMiracle {1}{U} (You may cast this card for its miracle cost when you draw it if it's the first card you drew this turn.)";

fn vengevine(owner: &str) -> CardInstance {
    card(
        "vengevine",
        "Vengevine",
        "Creature - Elemental",
        "{2}{G}{G}",
        VENGEVINE,
        owner,
    )
}

/// Vengevine (CR 113.6, 603.4): in the graveyard, the second creature spell
/// of the turn returns it; the first does not.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_card_observes_the_second_creature_spell_cast() {
    let vengevine = vengevine("player-0");
    assert_eq!(triggered_rule(&vengevine)["triggerZone"], "graveyard");
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![vengevine];
    engine.state.players[0].hand = vec![vanilla_creature("bear-1"), vanilla_creature("bear-2")];
    admit(&mut engine);

    cast(&mut engine, "bear-1");
    assert_eq!(
        engine.state.stack.len(),
        1,
        "the first creature spell alone"
    );
    resolve_stack(&mut engine);
    assert!(in_zone(&engine.state.players[0].graveyard, "vengevine"));

    cast(&mut engine, "bear-2");
    assert_eq!(
        engine.state.stack.len(),
        2,
        "the second creature spell triggers the card in the graveyard"
    );
    resolve_stack(&mut engine);
    assert!(in_zone(&engine.state.players[0].battlefield, "vengevine"));
    assert!(!in_zone(&engine.state.players[0].graveyard, "vengevine"));
}

/// The same rule on a permanent does not function: its zone of function is
/// the graveyard (CR 113.6).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_rule_does_not_function_on_the_battlefield() {
    let mut engine = main_phase_engine();
    engine.state.players[0].battlefield = vec![vengevine("player-0")];
    engine.state.players[0].hand = vec![vanilla_creature("bear-1"), vanilla_creature("bear-2")];
    admit(&mut engine);
    cast(&mut engine, "bear-1");
    resolve_stack(&mut engine);
    cast(&mut engine, "bear-2");
    assert_eq!(engine.state.stack.len(), 1, "only the creature spell");
}

/// A card that left the graveyard before its trigger resolves is a new
/// object: the return finds nothing (CR 400.7).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_trigger_does_nothing_once_its_card_left_the_zone() {
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![vengevine("player-0")];
    engine.state.players[0].hand = vec![vanilla_creature("bear-1"), vanilla_creature("bear-2")];
    admit(&mut engine);
    cast(&mut engine, "bear-1");
    resolve_stack(&mut engine);
    cast(&mut engine, "bear-2");
    assert_eq!(engine.state.stack.len(), 2);
    let moved = engine.state.players[0].graveyard.remove(0);
    engine.state.players[0].exile.push(moved);
    resolve_stack(&mut engine);
    assert!(in_zone(&engine.state.players[0].exile, "vengevine"));
    assert!(!in_zone(&engine.state.players[0].battlefield, "vengevine"));
}

/// Arclight Phoenix: a step trigger from the graveyard with an intervening
/// spell count (CR 603.4); three instants return it, two do not.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_step_trigger_reads_its_intervening_spell_count() {
    for (instants, returned) in [(3, true), (2, false)] {
        let phoenix = card(
            "phoenix",
            "Arclight Phoenix",
            "Creature - Phoenix",
            "{3}{R}",
            ARCLIGHT_PHOENIX,
            "player-0",
        );
        let rule = triggered_rule(&phoenix);
        assert_eq!(rule["triggerZone"], "graveyard");
        assert_eq!(rule["event"]["kind"], "stepBegan");
        let step = rule["event"]["step"].as_str().expect("a step").to_string();
        let mut engine = main_phase_engine();
        engine.state.players[0].graveyard = vec![phoenix];
        engine.state.players[0].hand = (0..instants)
            .map(|index| {
                card(
                    &format!("insight-{index}"),
                    "Probe Insight",
                    "Instant",
                    "{0}",
                    "You gain 1 life.",
                    "player-0",
                )
            })
            .collect();
        admit(&mut engine);
        for index in 0..instants {
            cast(&mut engine, &format!("insight-{index}"));
            resolve_stack(&mut engine);
        }
        engine.enqueue_step_triggers(&step);
        resolve_stack(&mut engine);
        assert_eq!(
            in_zone(&engine.state.players[0].battlefield, "phoenix"),
            returned,
            "{instants} instants"
        );
    }
}

/// Bridge from Below: cards arriving in a graveyard are observed from the
/// graveyard; a token is not a nontoken creature, and an opponent's
/// creature exiles the card (its second ability).
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_card_observes_cards_put_into_a_graveyard() {
    let bridge = card(
        "bridge",
        "Bridge from Below",
        "Enchantment",
        "{B}{B}{B}",
        BRIDGE_FROM_BELOW,
        "player-0",
    );
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![bridge];
    let mut token = vanilla_creature("token-bear");
    token.definition.is_token = true;
    engine.state.players[0].battlefield = vec![vanilla_creature("bear"), token];
    admit(&mut engine);

    let zombies = |engine: &GameEngine| {
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|permanent| {
                permanent.definition.is_token && permanent.instance_id != "token-bear"
            })
            .count()
    };
    engine.state.players[0].battlefield[0].damage_marked = 2;
    engine.check_state_based_actions();
    resolve_stack(&mut engine);
    assert_eq!(zombies(&engine), 1, "a nontoken creature died");

    let token_index = engine.state.players[0]
        .battlefield
        .iter()
        .position(|permanent| permanent.instance_id == "token-bear")
        .expect("the token is on the battlefield");
    engine.state.players[0].battlefield[token_index].damage_marked = 2;
    engine.check_state_based_actions();
    resolve_stack(&mut engine);
    assert_eq!(zombies(&engine), 1, "a token is not a nontoken creature");

    let mut opposing = vanilla_creature("opposing-bear");
    opposing.owner = "player-1".to_string();
    opposing.controller = "player-1".to_string();
    engine.state.players[1].battlefield = vec![opposing];
    admit(&mut engine);
    engine.state.players[1].battlefield[0].damage_marked = 2;
    engine.check_state_based_actions();
    resolve_stack(&mut engine);
    assert!(in_zone(&engine.state.players[0].exile, "bridge"));
}

/// Punishing Fire: an opponent's life gain, observed from the graveyard,
/// with an optional payment that returns the card to its owner's hand.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_graveyard_card_observes_an_opponents_life_gain() {
    let fire = card(
        "fire",
        "Punishing Fire",
        "Instant",
        "{1}{R}",
        PUNISHING_FIRE,
        "player-0",
    );
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![fire];
    engine.state.players[0].mana_pool = pool(&["R"]);
    admit(&mut engine);
    engine.gain_life("player-0", 2).expect("own life gain");
    assert!(
        engine.state.stack.is_empty(),
        "its owner's gain is not observed"
    );
    engine.gain_life("player-1", 2).expect("opponent life gain");
    assert_eq!(engine.state.stack.len(), 1);
    resolve_stack(&mut engine);
    assert!(in_zone(&engine.state.players[0].hand, "fire"));
    assert!(engine.state.players[0].mana_pool.is_empty(), "{{R}} paid");
}

/// Unused combinations of supported concepts, without new production code:
/// a landfall graveyard trigger and a second-draw graveyard trigger, each
/// with an intervening zone guard and a life gain.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn unused_graveyard_trigger_combinations_execute() {
    let landfall = card(
        "probe-landfall",
        "Probe Cairn",
        "Creature - Spirit",
        "{1}{G}",
        "Landfall — Whenever a land you control enters, if this card is in your graveyard, you gain 2 life.",
        "player-0",
    );
    assert_eq!(triggered_rule(&landfall)["triggerZone"], "graveyard");
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![landfall];
    let land = test_instance(
        "entered-land",
        test_definition("entered-land", "Land - Forest"),
        "player-0",
    );
    engine.state.players[0].battlefield.push(land.clone());
    admit(&mut engine);
    let life = engine.state.players[0].life;
    engine.enqueue_enter_battlefield_triggers(&land);
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, life + 2);

    let drawer = card(
        "probe-drawer",
        "Probe Scholar",
        "Creature - Spirit",
        "{1}{U}",
        "Whenever you draw your second card each turn, if this card is in your graveyard, you gain 2 life.",
        "player-0",
    );
    assert_eq!(triggered_rule(&drawer)["triggerZone"], "graveyard");
    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![drawer];
    admit(&mut engine);
    let life = engine.state.players[0].life;
    engine.draw_cards("player-0", 1).expect("first draw");
    assert!(
        engine.state.stack.is_empty(),
        "the first draw is not the second"
    );
    engine.draw_cards("player-0", 1).expect("second draw");
    resolve_stack(&mut engine);
    assert_eq!(engine.state.players[0].life, life + 2);
}

/// Near misses: a rule without zone evidence stays a battlefield rule that a
/// graveyard card never triggers, and returning another card from a
/// graveyard declares no zone of function.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn rules_without_zone_evidence_stay_battlefield_rules() {
    let witness = card(
        "witness",
        "Second Spell Witness",
        "Creature - Elemental",
        "{2}{G}{G}",
        "Whenever you cast a spell, if it's the second creature spell you cast this turn, you gain 2 life.",
        "player-0",
    );
    assert!(triggered_rule(&witness).get("triggerZone").is_none());
    let raiser = card(
        "raiser",
        "Probe Raiser",
        "Creature - Zombie",
        "{2}{B}",
        "Whenever you cast a creature spell, return target creature card from your graveyard to your hand.",
        "player-0",
    );
    assert!(triggered_rule(&raiser).get("triggerZone").is_none());

    let mut engine = main_phase_engine();
    engine.state.players[0].graveyard = vec![witness];
    engine.state.players[0].hand = vec![vanilla_creature("bear-1"), vanilla_creature("bear-2")];
    admit(&mut engine);
    cast(&mut engine, "bear-1");
    resolve_stack(&mut engine);
    cast(&mut engine, "bear-2");
    assert_eq!(
        engine.state.stack.len(),
        1,
        "a battlefield rule does not trigger from a graveyard"
    );
}

/// Miracle (CR 702.94a): the first card drawn this turn is revealed and may
/// be cast for its miracle cost; drawn second, it offers nothing.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn only_the_first_card_drawn_in_a_turn_offers_its_miracle() {
    let mastery = || {
        card(
            "mastery",
            "Temporal Mastery",
            "Sorcery",
            "{5}{U}{U}",
            TEMPORAL_MASTERY,
            "player-0",
        )
    };
    let filler = test_instance(
        "filler-draw",
        test_definition("filler-draw", "Basic Land - Island"),
        "player-0",
    );

    let mut engine = test_engine(2);
    engine.state.step = GameStep::Draw;
    engine.state.active_player = 0;
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![filler.clone(), mastery()];
    engine.state.players[0].mana_pool = pool(&["U", "C"]);
    admit(&mut engine);
    engine.draw_cards("player-0", 1).expect("first draw");
    assert!(engine.state.events.iter().any(|event| {
        event.kind == "cardRevealed"
            && event.card_instance_id.as_deref() == Some("mastery")
            && event.detail["reason"] == "miracle"
    }));
    assert_eq!(engine.state.stack.len(), 1, "the miracle trigger");
    engine
        .resolve_top_stack(&mut CastSpellDecisionProvider)
        .expect("the miracle trigger resolves");
    assert!(
        engine.state.players[0].mana_pool.is_empty(),
        "{{1}}{{U}} paid"
    );
    assert!(
        engine
            .state
            .stack
            .iter()
            .any(|object| object.card.instance_id == "mastery" && object.ability_rule.is_none()),
        "the drawn card is cast"
    );
    resolve_stack(&mut engine);
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        modifier["kind"] == "extraTurnForPlayer" && modifier["playerId"] == "player-0"
    }));

    // Drawn second this turn: no reveal, no trigger.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::Draw;
    engine.state.active_player = 0;
    engine.state.players[0].hand.clear();
    engine.state.players[0].library = vec![mastery(), filler];
    engine.state.players[0].mana_pool = pool(&["U", "C"]);
    admit(&mut engine);
    engine.draw_cards("player-0", 2).expect("two draws");
    assert!(in_zone(&engine.state.players[0].hand, "mastery"));
    assert!(engine.state.stack.is_empty());
    assert!(
        !engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "cardRevealed")
    );
}

/// Attack batches (CR 508.1, 603.2c): one declaration is one event; the
/// threshold counts the attackers of the written controller relation (any
/// player's, an opponent's), and a graveyard card observes the batch of its
/// owner's attackers.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn attack_batch_thresholds_read_their_controller_relation() {
    let observer = |instance_id: &str, oracle: &str| {
        card(
            instance_id,
            "Probe Watchtower",
            "Enchantment",
            "{1}{W}",
            oracle,
            "player-0",
        )
    };
    let attackers = |owner: &str, count: usize| {
        (0..count)
            .map(|index| {
                let id = format!("{owner}-attacker-{index}");
                let mut attacker =
                    test_instance(&id, test_definition(&id, "Creature - Bear"), owner);
                attacker.controller = owner.to_string();
                attacker
            })
            .collect::<Vec<_>>()
    };
    // (attacking player, attackers, any-player observer, opponent observer,
    // graveyard observer of its owner's attack)
    for (attacker_owner, count, any, opponent, graveyard) in [
        ("player-1", 2, true, true, false),
        ("player-0", 2, true, false, true),
        ("player-1", 1, false, false, false),
    ] {
        let mut engine = main_phase_engine();
        engine.state.players[0].battlefield = vec![
            observer(
                "any-watch",
                "Whenever two or more creatures attack, you gain 1 life.",
            ),
            observer(
                "opponent-watch",
                "Whenever two or more creatures your opponents control attack, you gain 1 life.",
            ),
        ];
        engine.state.players[0].graveyard = vec![card(
            "grave-watch",
            "Probe Barrow",
            "Creature - Spirit",
            "{1}{B}",
            "Whenever you attack with two or more creatures, if this card is in your graveyard, you gain 1 life.",
            "player-0",
        )];
        let attacking = attackers(attacker_owner, count);
        let attacker_index = if attacker_owner == "player-0" { 0 } else { 1 };
        engine.state.players[attacker_index]
            .battlefield
            .extend(attacking.iter().cloned());
        admit(&mut engine);
        let ids = attacking
            .iter()
            .map(|attacker| attacker.instance_id.clone())
            .collect::<Vec<_>>();
        engine.enqueue_declared_attacker_triggers(&ids);
        let triggered = |source: &str| {
            engine
                .state
                .stack
                .iter()
                .any(|object| object.card.instance_id == source)
        };
        assert_eq!(triggered("any-watch"), any, "{attacker_owner} x{count}");
        assert_eq!(
            triggered("opponent-watch"),
            opponent,
            "{attacker_owner} x{count}"
        );
        assert_eq!(
            triggered("grave-watch"),
            graveyard,
            "{attacker_owner} x{count}"
        );
    }
}
