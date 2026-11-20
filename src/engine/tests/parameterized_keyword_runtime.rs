//! Parameterized keyword abilities (CR 702) compiled from real Oracle text run
//! on the ordinary primitives they expand to: an upkeep trigger with a
//! scaled or fixed payment (cumulative upkeep, echo), a casting-time optional
//! additional cost (buyback) and a casting-cost reduction (affinity).
use super::*;

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, text: &str) -> Vec<Value> {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(
        parsed.status, "canonical",
        "{name}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("every ability has a rule"))
        .inspect(|rule| assert!(rule_is_executable(rule), "{name}: {rule:#}"))
        .collect()
}

fn card(id: &str, type_line: &str, mana_cost: &str, rules: Vec<Value>) -> CardDefinition {
    let mut definition = test_definition(id, type_line);
    definition.mana_cost = mana_cost.to_string();
    definition.rules = rules;
    definition
}

/// Pays or declines every offered resolution cost.
struct Payer(bool);

impl DecisionProvider for Payer {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let wanted = if self.0 {
            ActionKind::ChooseResolution
        } else {
            ActionKind::DeclinePayment
        };
        Ok(request
            .options
            .iter()
            .position(|option| option.kind == wanted)
            .unwrap_or(0))
    }
}

fn upkeep_engine(permanent: CardDefinition, mana: &[&str]) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    let mut entering = test_instance("keyword-permanent", permanent, "player-0");
    engine
        .apply_enter_replacements(&mut entering, &mut Payer(true))
        .expect("the permanent enters");
    engine.state.players[0].battlefield = vec![entering];
    for (index, symbols) in mana.iter().enumerate() {
        engine.state.players[0].battlefield.push(mana_source(
            &format!("mana-{index}"),
            "Basic Land - Wastes",
            symbols,
            None,
        ));
    }
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

/// Begins the controller's upkeep and resolves the triggers it put on the
/// stack. Returns how many triggers resolved.
fn run_upkeep(engine: &mut GameEngine, pay: bool) -> usize {
    engine.enqueue_step_triggers("upkeep");
    let triggers = engine.state.stack.len();
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut Payer(pay))
            .expect("the upkeep trigger resolves");
    }
    triggers
}

fn on_battlefield(engine: &GameEngine, id: &str) -> bool {
    engine.permanent_position(id).is_some()
}

#[test]
fn echo_is_due_only_in_the_first_upkeep_after_control_and_sacrifices_unless_paid() {
    let shivan_raptor = || {
        card(
            "shivan-raptor",
            "Creature - Dinosaur",
            "{2}{R}",
            parsed_rules(
                "Shivan Raptor",
                "Creature \u{2014} Dinosaur",
                "{2}{R}",
                "First strike, haste\nEcho {2}{R} (At the beginning of your upkeep, if this came under your control since the beginning of your last upkeep, sacrifice it unless you pay its echo cost.)",
            ),
        )
    };

    let mut paid = upkeep_engine(shivan_raptor(), &["{R}", "{R}", "{R}"]);
    assert_eq!(run_upkeep(&mut paid, true), 1);
    assert!(on_battlefield(&paid, "keyword-permanent"));
    assert!(
        paid.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.instance_id.starts_with("mana-"))
            .all(|land| land.tapped),
        "the echo cost was paid"
    );
    // CR 702.30a: the next upkeep is no longer the first one after control.
    assert_eq!(run_upkeep(&mut paid, true), 0);
    assert!(on_battlefield(&paid, "keyword-permanent"));

    let mut declined = upkeep_engine(shivan_raptor(), &["{R}", "{R}", "{R}"]);
    assert_eq!(run_upkeep(&mut declined, false), 1);
    assert!(!on_battlefield(&declined, "keyword-permanent"));
    assert!(
        declined.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "keyword-permanent")
    );
}

#[test]
fn cumulative_upkeep_adds_an_age_counter_then_scales_life_or_generic_mana() {
    // Glacial Chasm: pay 2 life per age counter.
    let glacial_chasm = parsed_rules(
        "Glacial Chasm",
        "Land",
        "",
        "Cumulative upkeep\u{2014}Pay 2 life. (At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it.)",
    );
    let mut engine = upkeep_engine(card("glacial-chasm", "Land", "", glacial_chasm), &[]);
    for expected_life in [18, 14] {
        assert_eq!(run_upkeep(&mut engine, true), 1);
        assert_eq!(engine.state.players[0].life, expected_life);
    }
    let (player, index) = engine
        .permanent_position("keyword-permanent")
        .expect("paid twice");
    assert_eq!(
        engine.state.players[player].battlefield[index]
            .counters
            .get("age"),
        Some(&2)
    );
    run_upkeep(&mut engine, false);
    assert!(!on_battlefield(&engine, "keyword-permanent"));
    assert_eq!(engine.state.players[0].life, 14);

    // Ancestral Knowledge: {1} per age counter; the third upkeep needs three
    // mana while only three lands exist in total, so it cannot be paid.
    let ancestral_knowledge = parsed_rules(
        "Ancestral Knowledge",
        "Enchantment",
        "{1}{U}",
        "Cumulative upkeep {1} (At the beginning of your upkeep, put an age counter on this permanent, then sacrifice it unless you pay its upkeep cost for each age counter on it.)",
    );
    let mut engine = upkeep_engine(
        card("ancestral", "Enchantment", "{1}{U}", ancestral_knowledge),
        &["{C}", "{C}", "{C}"],
    );
    let untapped = |engine: &GameEngine| {
        engine.state.players[0]
            .battlefield
            .iter()
            .filter(|card| card.instance_id.starts_with("mana-") && !card.tapped)
            .count()
    };
    run_upkeep(&mut engine, true);
    assert_eq!(untapped(&engine), 2);
    run_upkeep(&mut engine, true);
    assert_eq!(untapped(&engine), 0);
    assert!(on_battlefield(&engine, "keyword-permanent"));
    run_upkeep(&mut engine, true);
    assert!(!on_battlefield(&engine, "keyword-permanent"));
}

fn casting_engine(hand: CardDefinition, battlefield: Vec<CardInstance>) -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.active_player = 0;
    engine.state.priority_player = Some(0);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand = vec![test_instance("keyword-spell", hand, "player-0")];
    engine.state.players[0].battlefield = battlefield;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn cast_actions(engine: &GameEngine) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("keyword-spell")
        })
        .collect()
}

fn swamps(count: usize) -> Vec<CardInstance> {
    (0..count)
        .map(|index| mana_source(&format!("swamp-{index}"), "Basic Land - Swamp", "{B}", None))
        .collect()
}

fn buyback_chosen(action: &LegalAction) -> bool {
    action.decisions["buybackMode"] == json!(["buyback"])
        || action.decisions["buybackMode"] == json!("buyback")
}

#[test]
fn buyback_returns_the_spell_to_its_owners_hand_only_when_its_cost_was_paid() {
    let lab_rats = || {
        card(
            "lab-rats",
            "Sorcery",
            "{B}",
            parsed_rules(
                "Lab Rats",
                "Sorcery",
                "{B}",
                "Buyback {4} (You may pay an additional {4} as you cast this spell. If you do, put this card into your hand as it resolves.)\nCreate a 1/1 black Rat creature token.",
            ),
        )
    };

    // One land pays only the spell: no buyback declaration is offered.
    let short = casting_engine(lab_rats(), swamps(1));
    let actions = cast_actions(&short);
    assert!(!actions.is_empty());
    assert!(!actions.iter().any(buyback_chosen));

    let mut engine = casting_engine(lab_rats(), swamps(5));
    let cast = cast_actions(&engine)
        .into_iter()
        .find(buyback_chosen)
        .expect("five lands pay the spell and its buyback cost");
    engine
        .apply_priority_action(&cast, &mut Payer(true))
        .expect("the buyback cast pays five mana");
    assert!(
        engine.state.players[0]
            .battlefield
            .iter()
            .all(|land| land.tapped)
    );
    engine
        .resolve_top_stack(&mut Payer(true))
        .expect("Lab Rats resolves");
    assert!(
        engine.state.players[0]
            .hand
            .iter()
            .any(|card| card.instance_id == "keyword-spell")
    );
    assert_eq!(
        engine.state.players[0].battlefield.len(),
        6,
        "one Rat token"
    );

    let mut declined = casting_engine(lab_rats(), swamps(5));
    let cast = cast_actions(&declined)
        .into_iter()
        .find(|action| !buyback_chosen(action))
        .expect("the buyback cost may be declined");
    declined
        .apply_priority_action(&cast, &mut Payer(true))
        .expect("the plain cast pays one mana");
    declined
        .resolve_top_stack(&mut Payer(true))
        .expect("Lab Rats resolves");
    assert!(
        declined.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "keyword-spell")
    );
}

#[test]
fn affinity_reduces_generic_mana_by_each_controlled_permanent_of_its_quality() {
    let frogmite = || {
        card(
            "frogmite",
            "Artifact Creature - Frog",
            "{4}",
            parsed_rules(
                "Frogmite",
                "Artifact Creature \u{2014} Frog",
                "{4}",
                "Affinity for artifacts (This spell costs {1} less to cast for each artifact you control.)",
            ),
        )
    };
    let artifact = |id: &str| test_instance(id, test_definition(id, "Artifact"), "player-0");

    // Two artifacts and two lands pay {4} reduced by two.
    let mut battlefield = vec![artifact("ornithopter-a"), artifact("ornithopter-b")];
    battlefield.extend(swamps(2));
    assert!(!cast_actions(&casting_engine(frogmite(), battlefield)).is_empty());

    // An opponent's artifact does not count (`you control`).
    let mut battlefield = vec![artifact("ornithopter-a")];
    battlefield.extend(swamps(2));
    let mut engine = casting_engine(frogmite(), battlefield);
    engine.state.players[1].battlefield.push(test_instance(
        "foreign",
        test_definition("foreign", "Artifact"),
        "player-1",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(cast_actions(&engine).is_empty());
}

#[test]
fn count_and_trigger_keywords_compile_to_executable_rules() {
    for (name, type_line, mana_cost, text, kind) in [
        (
            "Karumonix, the Rat King",
            "Legendary Creature \u{2014} Phyrexian Rat",
            "{1}{B}{B}",
            "Toxic 1 (Players dealt combat damage by this creature also get a poison counter.)",
            "keywordAbility",
        ),
        (
            "Artisan of Kozilek",
            "Creature \u{2014} Eldrazi",
            "{9}",
            "Annihilator 2 (Whenever this creature attacks, defending player sacrifices two permanents of their choice.)",
            "keywordAbility",
        ),
        (
            "Seraph of the Scales",
            "Creature \u{2014} Angel",
            "{2}{W}{B}",
            "Afterlife 2 (When this creature dies, create two 1/1 white and black Spirit creature tokens with flying.)",
            "triggeredAbility",
        ),
        (
            "Moonlit Strider",
            "Creature \u{2014} Spirit",
            "{3}{W}",
            "Soulshift 3 (When this creature dies, you may return target Spirit card with mana value 3 or less from your graveyard to your hand.)",
            "triggeredAbility",
        ),
        (
            "Batterskull",
            "Artifact \u{2014} Equipment",
            "{5}",
            "Living weapon (When this Equipment enters, create a 0/0 black Phyrexian Germ creature token, then attach this to it.)",
            "triggeredAbility",
        ),
    ] {
        let rules = parsed_rules(name, type_line, mana_cost, text);
        assert_eq!(rules[0]["kind"], kind, "{name}");
    }
}

/// Picks the entry choice whose label names `label`.
struct EntryChoice(&'static str);

impl DecisionProvider for EntryChoice {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        assert_eq!(request.kind, DecisionKind::ReplacementChoice);
        request
            .options
            .iter()
            .position(|option| option.label.contains(self.0))
            .ok_or_else(|| EngineError::new("missing scripted Riot choice"))
    }
}

#[test]
fn printed_riot_enters_with_the_chosen_counter_or_haste() {
    // Zhur-Taa Goblin.
    let rules = parsed_rules(
        "Zhur-Taa Goblin",
        "Creature \u{2014} Goblin Berserker",
        "{R}{G}",
        "Riot (This creature enters with your choice of a +1/+1 counter or haste.)",
    );
    for (choice, counters, haste) in [("+1/+1 counter", Some(&1), false), ("haste", None, true)] {
        let engine = test_engine(2);
        let mut goblin = test_instance(
            "zhur-taa-goblin",
            card(
                "zhur-taa-goblin",
                "Creature - Goblin Berserker",
                "{R}{G}",
                rules.clone(),
            ),
            "player-0",
        );
        let mut engine = engine;
        engine
            .apply_enter_replacements(&mut goblin, &mut EntryChoice(choice))
            .expect("the Riot choice applies");
        assert_eq!(goblin.counters.get("+1/+1"), counters, "{choice}");
        assert_eq!(has_keyword(&goblin.definition, "haste"), haste, "{choice}");
    }
}

/// Attacks with `attacker`, blocks it with `blocker`, then passes priority.
struct BlockScript {
    attacker: &'static str,
    blocker: &'static str,
    attacked: bool,
    blocked: bool,
}

impl BlockScript {
    fn new(attacker: &'static str, blocker: &'static str) -> Self {
        Self {
            attacker,
            blocker,
            attacked: false,
            blocked: false,
        }
    }
}

impl DecisionProvider for BlockScript {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let position = |kind: ActionKind| {
            request
                .options
                .iter()
                .position(|action| action.kind == kind)
                .ok_or_else(|| EngineError::new(format!("{kind:?} is unavailable")))
        };
        match request.kind {
            DecisionKind::Attackers if !self.attacked => {
                self.attacked = true;
                request
                    .options
                    .iter()
                    .position(|action| {
                        action.kind == ActionKind::DeclareAttacker
                            && action.attacker_id.as_deref() == Some(self.attacker)
                    })
                    .ok_or_else(|| EngineError::new("attacker action is unavailable"))
            }
            DecisionKind::Attackers => position(ActionKind::FinishAttackers),
            DecisionKind::Blockers if !self.blocked => {
                self.blocked = true;
                request
                    .options
                    .iter()
                    .position(|action| {
                        action.kind == ActionKind::DeclareBlocker
                            && action.blocker_id.as_deref() == Some(self.blocker)
                    })
                    .ok_or_else(|| EngineError::new("blocker action is unavailable"))
            }
            DecisionKind::Blockers => position(ActionKind::FinishBlockers),
            DecisionKind::Priority => position(ActionKind::PassPriority),
            _ => Ok(0),
        }
    }

    fn requests_explicit_priority_pass(&self, _player_id: &str) -> bool {
        true
    }
}

fn creature(id: &str, owner: &str, power: &str, text: Option<&str>) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Samurai");
    definition.power = Some(power.to_string());
    definition.toughness = Some(power.to_string());
    if let Some(text) = text {
        definition.rules = parsed_rules(id, "Creature \u{2014} Human Samurai", "{2}", text);
    }
    test_instance(id, definition, owner)
}

fn in_graveyard(engine: &GameEngine, player: usize, id: &str) -> bool {
    engine.state.players[player]
        .graveyard
        .iter()
        .any(|card| card.instance_id == id)
}

#[test]
fn bushido_pumps_a_blocker_when_it_blocks() {
    // A 2/2 with bushido 2 blocks a 3/3: it fights as a 4/4.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature("raider", "player-0", "3", None)];
    engine.state.players[1].battlefield = vec![creature(
        "samurai",
        "player-1",
        "2",
        Some(
            "Bushido 2 (Whenever this creature blocks or becomes blocked, it gets +2/+2 until end of turn.)",
        ),
    )];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .run_combat(&mut BlockScript::new("raider", "samurai"))
        .expect("combat resolves");
    assert!(in_graveyard(&engine, 0, "raider"));
    assert!(!in_graveyard(&engine, 1, "samurai"));
}

#[test]
fn becoming_blocked_triggers_bushido_and_afflict_of_the_attacker() {
    // An unused combination: one attacker with bushido 1 and afflict 2 is
    // blocked by a 3/3. Bushido makes it a 3/3 that trades; afflict makes the
    // defending player lose two life although no combat damage reaches them.
    let mut engine = test_engine(2);
    engine.state.players[0].battlefield = vec![creature(
        "ronin",
        "player-0",
        "2",
        Some(
            "Bushido 1 (Whenever this creature blocks or becomes blocked, it gets +1/+1 until end of turn.)\nAfflict 2 (Whenever this creature becomes blocked, defending player loses 2 life.)",
        ),
    )];
    engine.state.players[1].battlefield = vec![creature("wall", "player-1", "3", None)];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
        .run_combat(&mut BlockScript::new("ronin", "wall"))
        .expect("combat resolves");
    assert_eq!(engine.state.players[1].life, 18);
    assert_eq!(engine.state.players[0].life, 20);
    assert!(
        in_graveyard(&engine, 1, "wall"),
        "the bushido attacker deals 3"
    );
    assert!(in_graveyard(&engine, 0, "ronin"));
}

#[test]
fn evoke_casts_for_its_alternative_cost_then_sacrifices_on_entry() {
    // Mulldrifter's evoke cost with a 2/2 body: one land pays only the evoke
    // cost, so the castable action is the alternative one.
    let elemental = card(
        "mulldrifter",
        "Creature - Elemental",
        "{4}{U}",
        parsed_rules(
            "Mulldrifter",
            "Creature \u{2014} Elemental",
            "{4}{U}",
            "Evoke {B} (You may cast this spell for its evoke cost. If you do, it's sacrificed when it enters.)",
        ),
    );
    let mut engine = casting_engine(elemental, swamps(1));
    let cast = cast_actions(&engine)
        .into_iter()
        .find(|action| action.decisions["alternativeCostMode"] == json!("evoke"))
        .expect("the evoke cost is castable with one land");
    engine
        .apply_priority_action(&cast, &mut Payer(true))
        .expect("the evoke cast pays {B}");
    while !engine.state.stack.is_empty() {
        engine
            .resolve_top_stack(&mut Payer(true))
            .expect("the spell and its evoke sacrifice resolve");
    }
    assert!(!on_battlefield(&engine, "keyword-spell"));
    assert!(
        engine.state.players[0]
            .graveyard
            .iter()
            .any(|card| card.instance_id == "keyword-spell")
    );
}
