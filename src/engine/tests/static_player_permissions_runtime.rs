//! Runtime proofs of the static player permission frame: the card's own
//! casting permission from a graveyard, a once-per-turn graveyard window, a
//! timing permission with a cost increase, an exert option at the attack
//! declaration with its reflexive ability, and combat damage assigned as
//! though the attacker weren't blocked.
use super::super::static_player_permissions::{AS_THOUGH_UNBLOCKED, FLASH_TIMING_MANA_COST};
use super::*;

struct ChooseOption(usize);

impl DecisionProvider for ChooseOption {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(self.0.min(request.options.len().saturating_sub(1)))
    }
}

fn self_ref() -> Value {
    json!({ "kind": "self" })
}

fn controller() -> Value {
    json!({ "kind": "controllerOf", "object": self_ref() })
}

fn static_rule(zone: &str, modifier: Value) -> Value {
    json!({
        "kind": "staticAbility",
        "source": self_ref(),
        "activeWhile": { "kind": "inZone", "object": self_ref(), "zone": { "kind": zone } },
        "modifiers": [modifier],
    })
}

fn creature(id: &str, power: &str, rules: Vec<Value>, owner: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature - Test");
    definition.power = Some(power.to_string());
    definition.toughness = Some(power.to_string());
    definition.rules = rules;
    let mut instance = test_instance(id, definition, owner);
    instance.summoning_sick = false;
    instance
}

#[test]
fn own_graveyard_permission_admits_only_its_own_card() {
    let mut engine = test_engine(2);
    let permission = static_rule(
        "graveyard",
        json!({
            "kind": "castingPermission", "players": controller(), "sourceZone": "graveyard",
            "where": Value::Null, "object": self_ref(),
            "withoutPayingManaCost": false, "asThoughFlash": false,
        }),
    );
    assert!(rule_is_executable(&permission));
    let recursive = creature("recursive", "2", vec![permission], "player-0");
    let other = creature("other", "2", Vec::new(), "player-0");
    engine.state.players[0].graveyard = vec![recursive.clone(), other.clone()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let player = engine.state.players[0].clone();
    assert!(
        engine
            .casting_route(&player, &recursive, "graveyard", false)
            .is_some(),
        "the card's own permission admits it from its graveyard"
    );
    assert!(
        engine
            .casting_route(&player, &other, "graveyard", false)
            .is_none(),
        "the permission names only its own card"
    );
    // CR 113.6: the permission functions from the graveyard only.
    engine.state.players[0].graveyard.clear();
    engine.state.players[0].exile = vec![recursive.clone()];
    let player = engine.state.players[0].clone();
    assert!(
        engine
            .casting_route(&player, &recursive, "exile", false)
            .is_none()
    );
}

#[test]
fn once_each_turn_graveyard_window_is_usable_once_during_your_turn() {
    let mut engine = test_engine(2);
    let window = static_rule(
        "battlefield",
        json!({
            "kind": "castingPermission", "players": controller(), "sourceZone": "graveyard",
            "where": { "kind": "subtypeContains", "value": "Aura" },
            "withoutPayingManaCost": false, "asThoughFlash": false,
            "duringControllerTurnOnly": true, "onceEachTurn": true,
        }),
    );
    assert!(rule_is_executable(&window));
    engine.state.players[0].battlefield = vec![creature("danitha", "2", vec![window], "player-0")];
    let aura = test_instance(
        "aura",
        test_definition("aura", "Enchantment \u{2014} Aura"),
        "player-0",
    );
    let second = test_instance(
        "second-aura",
        test_definition("second-aura", "Enchantment \u{2014} Aura"),
        "player-0",
    );
    engine.state.players[0].graveyard = vec![aura.clone(), second.clone()];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.state.active_player = 1;
    let player = engine.state.players[0].clone();
    assert!(
        engine
            .casting_route(&player, &aura, "graveyard", false)
            .is_none(),
        "the window is closed during another player's turn"
    );
    engine.state.active_player = 0;
    assert!(
        engine
            .casting_route(&player, &aura, "graveyard", false)
            .is_some()
    );
    engine.record_event(
        "spellCast",
        Some("player-0".to_string()),
        Some("aura".to_string()),
        json!({ "decisions": { "staticCastingPermissionSource": "danitha" } }),
    );
    assert!(
        engine
            .casting_route(&player, &second, "graveyard", false)
            .is_none(),
        "the source's permission was used this turn"
    );
}

#[test]
fn timing_permission_with_a_cost_increase_is_paid_only_through_the_permission() {
    let engine = test_engine(2);
    let mut costly = test_definition("ghitu-fire", "Sorcery");
    costly.rules.push(static_rule(
        "hand",
        json!({ "kind": "castAsThoughFlash", "object": self_ref(), "additionalManaCost": "{2}" }),
    ));
    assert!(costly.rules.iter().all(rule_is_executable));
    assert_eq!(
        engine.printed_self_flash_cost("player-0", &costly, "hand"),
        Some("{2}".to_string())
    );
    assert!(!engine.printed_self_flash_permission("player-0", &costly, "hand"));
    // The permission functions from the hand only.
    assert_eq!(
        engine.printed_self_flash_cost("player-0", &costly, "graveyard"),
        None
    );
    let mut free = test_definition("spinner", "Creature - Faerie");
    free.rules.push(static_rule(
        "hand",
        json!({ "kind": "castAsThoughFlash", "object": self_ref() }),
    ));
    assert!(engine.printed_self_flash_permission("player-0", &free, "hand"));
    let decisions = BTreeMap::from([(FLASH_TIMING_MANA_COST.to_string(), json!("{2}"))]);
    assert_eq!(
        static_player_permissions::static_permission_mana_costs(&decisions),
        vec!["{2}".to_string()]
    );
    assert!(static_player_permissions::static_permission_mana_costs(&BTreeMap::new()).is_empty());
}

fn thorn_combat(choice: usize) -> GameEngine {
    let mut engine = test_engine(2);
    let manner = static_rule(
        "battlefield",
        json!({ "kind": "assignCombatDamageAsThoughUnblocked", "object": self_ref() }),
    );
    assert!(rule_is_executable(&manner));
    engine.state.players[0].battlefield = vec![creature("thorn", "7", vec![manner], "player-0")];
    engine.state.players[1].battlefield = vec![creature("wall", "2", Vec::new(), "player-1")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let attacks = BTreeMap::from([(
        "thorn".to_string(),
        AttackAssignment {
            defender: TargetRef::Player {
                player_id: "player-1".to_string(),
            },
            defending_player_id: "player-1".to_string(),
        },
    )]);
    let blocks = BTreeMap::from([("thorn".to_string(), vec!["wall".to_string()])]);
    let assignments = engine
        .choose_combat_damage_assignments(
            &mut ChooseOption(choice),
            &attacks,
            &blocks,
            CombatDamageStep::All,
        )
        .expect("combat damage is assigned");
    assert_eq!(
        assignments["thorn"].contains_key(AS_THOUGH_UNBLOCKED),
        choice == 0
    );
    engine
        .resolve_combat_damage(&attacks, &blocks, &assignments)
        .expect("combat damage resolves");
    engine
}

#[test]
fn blocked_attacker_may_assign_its_damage_as_though_it_were_unblocked() {
    let engine = thorn_combat(0);
    assert_eq!(engine.state.players[1].life, 13);
    let (player_index, card_index) = engine.permanent_position("thorn").expect("thorn survives");
    // The blocker still deals its damage (CR 510.1c).
    assert_eq!(
        engine.state.players[player_index].battlefield[card_index].damage_marked,
        2
    );
}

#[test]
fn declining_the_manner_assigns_damage_to_the_blockers() {
    let engine = thorn_combat(1);
    assert_eq!(engine.state.players[1].life, 20);
    assert!(engine.permanent_position("wall").is_none());
}

#[test]
fn exert_is_an_attack_option_with_its_reflexive_ability() {
    let mut engine = test_engine(2);
    let reflexive = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
        "effects": [{
            "kind": "drawCards",
            "player": controller(),
            "count": { "kind": "integer", "value": 1 },
        }],
    });
    let exert = static_rule(
        "battlefield",
        json!({ "kind": "exertAsAttacks", "object": self_ref(), "reflexive": reflexive }),
    );
    assert!(rule_is_executable(&exert));
    engine.state.players[0].battlefield = vec![
        creature("naga", "2", vec![exert], "player-0"),
        creature("plain", "2", Vec::new(), "player-0"),
    ];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine.state.active_player = 0;
    let actions = engine.legal_attacker_actions(&BTreeSet::new());
    let exerting = actions
        .iter()
        .filter(|action| {
            action
                .decisions
                .get(static_player_permissions::EXERT_AS_ATTACKS)
                .and_then(Value::as_bool)
                == Some(true)
        })
        .collect::<Vec<_>>();
    assert!(!exerting.is_empty(), "the exert option is offered");
    assert!(
        exerting
            .iter()
            .all(|action| action.attacker_id.as_deref() == Some("naga")),
        "only the creature with the permission may exert"
    );
    let action = exerting[0].clone();
    let mut selected = BTreeSet::new();
    let mut attacks = BTreeMap::new();
    let mut tapped = Vec::new();
    engine
        .apply_attacker_declaration(
            &action,
            &mut selected,
            &mut attacks,
            &mut tapped,
            &mut ChooseOption(0),
        )
        .expect("the exerted attacker is declared");
    assert!(engine.state.rule_modifiers.iter().any(|modifier| {
        value_kind(modifier) == Some("skipNextUntap")
            && modifier["cardInstanceId"].as_str() == Some("naga")
    }));
    assert!(
        engine
            .state
            .events
            .iter()
            .any(|event| event.kind == "permanentExerted")
    );
    let stack_before = engine.state.stack.len();
    engine.enqueue_exert_reflexive_triggers();
    assert_eq!(engine.state.stack.len(), stack_before + 1);
    assert_eq!(
        engine
            .state
            .stack
            .last()
            .expect("reflexive trigger")
            .ability_rule
            .as_ref()
            .map(|rule| rule["event"]["kind"].clone()),
        Some(json!("reflexiveTriggerCreated"))
    );
    // The pending reflexive ability is published once.
    engine.enqueue_exert_reflexive_triggers();
    assert_eq!(engine.state.stack.len(), stack_before + 1);
}

/// `Any player may cast spells as though they had flash.`: the player-set
/// actor compiles to a hand-zone casting permission for each player, and
/// the runtime lifts the timing restriction for the opponent's sorcery as
/// well as the controller's, only while the source is on the battlefield.
#[test]
fn a_player_set_flash_permission_lifts_timing_for_every_player() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Any Player Flash Probe".to_string(),
        type_line: "Enchantment".to_string(),
        mana_cost: Some("{2}{U}".to_string()),
        oracle_text: Some("Any player may cast spells as though they had flash.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rules = parsed
        .abilities
        .into_iter()
        .map(|ability| ability.rule.expect("the ability has a rule"))
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect::<Vec<_>>();
    let mut engine = test_engine(2);
    let sorcery = test_definition("sorcery", "Sorcery");
    for player in ["player-0", "player-1"] {
        assert!(!engine.card_casts_with_static_flash(player, &sorcery, "hand"));
    }
    let mut source = test_definition("flash-source", "Enchantment");
    source.rules = rules;
    engine.state.players[0]
        .battlefield
        .push(test_instance("flash-source", source, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    for player in ["player-0", "player-1"] {
        assert!(
            engine.card_casts_with_static_flash(player, &sorcery, "hand"),
            "{player} casts with flash"
        );
        // The permission is for spells cast from the usual zone only.
        assert!(!engine.card_casts_with_static_flash(player, &sorcery, "graveyard"));
    }
}
