use super::super::*;
use super::FrozenGraveyardDestination;

struct SelectAll;
impl DecisionProvider for SelectAll {
    fn choose(&mut self, _: &GameState, _: &EngineDecisionRequest) -> Result<usize, EngineError> {
        Ok(0)
    }
    fn choose_card_instance_ids(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<Vec<String>, EngineError> {
        match request.choice.as_ref() {
            Some(DecisionChoice::CardSelection {
                candidate_card_instance_ids,
                ..
            }) => Ok(candidate_card_instance_ids.clone()),
            _ => Err(EngineError::new("unexpected group B card-choice context")),
        }
    }
}

fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"manaCost":"{0}",
        "power":types.contains("Creature").then_some("2"),
        "toughness":types.contains("Creature").then_some("2")}))
    .unwrap()
}
fn engine() -> GameEngine {
    let mut e = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("Player {i}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Basic Land - Plains"); 4],
                })
                .collect(),
        },
        261115,
    )
    .unwrap();
    e.state.players[0].battlefield.push(card_instance(
        "p0",
        &definition("executor", "Artifact"),
        901,
    ));
    e
}
fn insert(e: &mut GameEngine, zone: &str, id: &str, types: &str, index: usize) -> String {
    let card = card_instance("p0", &definition(id, types), index);
    let result = card.instance_id.clone();
    match zone {
        "hand" => e.state.players[0].hand.push(card),
        "graveyard" => e.state.players[0].graveyard.push(card),
        "battlefield" => e.state.players[0].battlefield.push(card),
        _ => panic!("unsupported construction zone"),
    }
    result
}
fn admit(e: &mut GameEngine) {
    e.admit_explicit_test_fixture_objects()
        .expect("construction admission before references");
}
fn identity(card: &CardInstance) -> object_identity::ObjectIdentity {
    object_identity::object_identity_of(card).unwrap()
}
fn set_printed_rules(card: &mut CardInstance, rules: Vec<Value>) {
    assert!(
        rules.iter().all(rule_is_executable),
        "positive construction requires executable rules"
    );
    card.definition.rules = rules;
    card.printed_definition = Some(card.definition.clone());
}
fn execute(
    e: &mut GameEngine,
    source: &str,
    effects: Vec<Value>,
    targets: BTreeMap<String, TargetRef>,
) -> Result<(), EngineError> {
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":effects});
    assert!(rule_is_executable(&rule));
    e.execute_canonical_rule(
        CanonicalRuleExecution {
            controller_id: "p0".into(),
            source_definition_id: source.into(),
            rule,
            decisions: BTreeMap::new(),
            targets,
        },
        &mut SelectAll,
    )
}
fn policy(e: &mut GameEngine, kind: &str) {
    let id = insert(e, "battlefield", "entry-policy", "Artifact", 903);
    let (_, i) = e.permanent_position(&id).unwrap();
    let modifier = match kind {
        "prohibited" => {
            json!({"kind":"prohibitEnterBattlefieldFromZones","zones":["hand","graveyard","exile"],"where":{"kind":"cardTypeContains","value":"Creature"}})
        }
        "exile" => {
            json!({"kind":"prohibitUncastBattlefieldEntry","where":{"kind":"cardTypeContains","value":"Creature"},"destination":"exile"})
        }
        _ => panic!(),
    };
    set_printed_rules(
        &mut e.state.players[0].battlefield[i],
        vec![json!({"kind":"staticAbility","source":{"kind":"self"},
        "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"modifiers":[modifier]})],
    );
}

#[test]
fn departure_captures_old_counter_lki_then_installs_one_destination_incarnation() {
    for exile in [false, true] {
        let mut e = engine();
        let id = insert(&mut e, "battlefield", "departing", "Creature - Bear", 902);
        e.state.players[0]
            .battlefield
            .last_mut()
            .unwrap()
            .counters
            .insert("charge".into(), 3);
        admit(&mut e);
        let old = e.state.players[0].battlefield.last().unwrap().clone();
        let old_identity = identity(&old);
        let clock = e.state.battlefield_incarnation_sequence;
        let action: LegalAction = serde_json::from_value(json!({"id":"group-b-observer","kind":"activateAbility","playerId":"p0","label":"observe actual departure","cardInstanceId":id})).unwrap();
        e.with_immediate_source_context(&action, &old, |e, frame| {
            let destination = if exile {
                FrozenGraveyardDestination::Exile {
                    replacement_source_id: "replacement".into(),
                    replacement_controller_id: Some("p0".into()),
                }
            } else {
                FrozenGraveyardDestination::Graveyard
            };
            let installed = e
                .install_permanent_graveyard_move(&id, destination)?
                .unwrap();
            assert_eq!(identity(&installed.permanent), old_identity);
            assert_eq!(installed.permanent.counters.get("charge"), Some(&3));
            let departed = e.immediate_source_context(frame)?;
            assert!(source_lki::source_has_departed(&departed));
            assert_eq!(identity(&departed.card), old_identity);
            assert_eq!(
                e.checked_runtime_numeric_expression(
                    &json!({"kind":"countCounters","object":{"kind":"self"},"counter":"charge"}),
                    &departed,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    "writer-group-b.lki-counter"
                )?,
                3
            );
            let actual = e.card_outside_battlefield(&id).unwrap();
            assert_ne!(identity(actual), old_identity);
            assert!(actual.counters.is_empty());
            assert_eq!(e.state.battlefield_incarnation_sequence, clock + 1);
            assert_eq!(
                installed.destination,
                if exile { "exile" } else { "graveyard" }
            );
            Ok(())
        })
        .unwrap();
    }
}

#[test]
fn discard_cost_commits_actual_graveyard_replacement_once_before_discard_notification() {
    let mut e = engine();
    let id = insert(&mut e, "hand", "discarded", "Creature - Bear", 902);
    let replacement = insert(&mut e, "battlefield", "graveyard-policy", "Artifact", 903);
    let (_, i) = e.permanent_position(&replacement).unwrap();
    set_printed_rules(
        &mut e.state.players[0].battlefield[i],
        vec![
            json!({"kind":"staticAbility","source":{"kind":"self"},"activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},"condition":{"kind":"compare","operator":"==","left":{"kind":"countCards","zone":{"kind":"hand","player":{"kind":"controllerOf","object":{"kind":"self"}}},"where":null},"right":1},"modifiers":[{"kind":"replaceGraveyardDestination","graveyardOwners":{"kind":"eachPlayer"},"where":null,"from":{"kind":"anyZone"},"to":{"kind":"exile"}}]}),
        ],
    );
    admit(&mut e);
    let old = identity(e.card_outside_battlefield(&id).unwrap());
    let clock = e.state.battlefield_incarnation_sequence;
    e.discard_card_as_cost("p0", &id).unwrap();
    let actual = e.state.players[0]
        .exile
        .iter()
        .find(|c| c.instance_id == id)
        .unwrap();
    assert_ne!(identity(actual), old);
    assert_eq!(e.state.battlefield_incarnation_sequence, clock + 1);
    assert!(e.state.players[0].graveyard.is_empty());
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|v| v.kind == "cardDiscarded")
            .count(),
        1
    );
    assert_eq!(
        e.state
            .events
            .iter()
            .filter(|v| v.kind == "graveyardDestinationReplaced")
            .count(),
        1
    );
}

#[test]
fn entry_writers_share_admission_actual_destination_and_one_tick_before_trigger_capture() {
    for path in ["reanimate", "source", "hand"] {
        for outcome in ["allowed", "prohibited", "exile"] {
            let mut e = engine();
            let zone = if path == "hand" { "hand" } else { "graveyard" };
            let id = insert(&mut e, zone, "entrant", "Creature - Bear", 902);
            let entry_rule = json!({"kind":"triggeredAbility","source":{"kind":"self"},"event":{"kind":"enterBattlefield","object":{"kind":"self"}},"effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]});
            let card = if zone == "hand" {
                e.state.players[0].hand.last_mut().unwrap()
            } else {
                e.state.players[0].graveyard.last_mut().unwrap()
            };
            set_printed_rules(card, vec![entry_rule]);
            if outcome != "allowed" {
                policy(&mut e, outcome);
            }
            admit(&mut e);
            let old = identity(e.card_outside_battlefield(&id).unwrap());
            let clock = e.state.battlefield_incarnation_sequence;
            let effect = match path {
                "reanimate" => {
                    json!({"kind":"returnCardsFromGraveyard","player":{"kind":"abilityController"},"controller":{"kind":"abilityController"},"where":{"kind":"cardTypeContains","value":"Creature"}})
                }
                "source" => {
                    json!({"kind":"moveAbilitySourceToBattlefield","from":"graveyard","tapped":false})
                }
                _ => {
                    json!({"kind":"putCardsFromHandOntoBattlefield","player":{"kind":"abilityController"},"where":{"kind":"cardTypeContains","value":"Creature"}})
                }
            };
            execute(
                &mut e,
                if path == "source" {
                    "entrant"
                } else {
                    "executor"
                },
                vec![effect],
                BTreeMap::new(),
            )
            .unwrap();
            if outcome == "allowed" {
                let (p, c) = e.permanent_position(&id).unwrap();
                let actual = identity(&e.state.players[p].battlefield[c]);
                assert_ne!(actual, old);
                assert_eq!(
                    e.state.battlefield_incarnation_sequence,
                    clock + 1,
                    "{path}"
                );
                assert_eq!(
                    e.state.stack.len(),
                    1,
                    "entry event must capture committed identity"
                );
                assert_eq!(identity(&e.state.stack[0].card), actual);
            } else {
                assert!(e.permanent_position(&id).is_none());
                let actual = identity(e.card_outside_battlefield(&id).unwrap());
                if outcome == "prohibited" {
                    assert_eq!(actual, old);
                    assert_eq!(e.state.battlefield_incarnation_sequence, clock);
                } else {
                    assert!(e.state.players[0].exile.iter().any(|c| c.instance_id == id));
                    assert_ne!(actual, old);
                    assert_eq!(e.state.battlefield_incarnation_sequence, clock + 1);
                }
                assert!(e.state.stack.is_empty());
                assert!(
                    !e.state
                        .events
                        .iter()
                        .any(|v| v.kind == "permanentEnteredBattlefield")
                );
            }
        }
    }
}

#[test]
fn blink_return_prohibition_preserves_exile_object_and_replacement_reexile_creates_another() {
    for outcome in ["allowed", "prohibited", "exile"] {
        let mut e = engine();
        let id = insert(&mut e, "battlefield", "blinking", "Creature - Bear", 902);
        if outcome != "allowed" {
            policy(&mut e, outcome);
        }
        admit(&mut e);
        let (p, c) = e.permanent_position(&id).unwrap();
        let old = identity(&e.state.players[p].battlefield[c]);
        let clock = e.state.battlefield_incarnation_sequence;
        execute(
            &mut e,
            "executor",
            vec![json!({"kind":"blinkPermanent","permanent":{"kind":"chosenTarget","id":"blink"}})],
            BTreeMap::from([(
                "blink".into(),
                TargetRef::Permanent {
                    instance_id: id.clone(),
                },
            )]),
        )
        .unwrap();
        let ticks = if outcome == "prohibited" { 1 } else { 2 };
        assert_eq!(e.state.battlefield_incarnation_sequence, clock + ticks);
        let actual = if let Some((p, c)) = e.permanent_position(&id) {
            &e.state.players[p].battlefield[c]
        } else {
            e.card_outside_battlefield(&id).unwrap()
        };
        assert_ne!(identity(actual), old);
        assert_eq!(e.permanent_position(&id).is_some(), outcome == "allowed");
        assert_eq!(
            e.state
                .events
                .iter()
                .filter(|v| v.kind == "permanentEnteredBattlefield")
                .count(),
            usize::from(outcome == "allowed")
        );
    }
}

#[test]
fn exhausted_entry_rolls_back_earlier_effect_and_extracted_carrier_without_incidental_trigger_risk()
{
    let mut e = engine();
    insert(&mut e, "graveyard", "entrant", "Creature - Bear", 902);
    admit(&mut e);
    e.state.battlefield_incarnation_sequence = usize::MAX;
    let before = serde_json::to_value(&e.state).unwrap();
    let mut rng = e.rng.clone();
    let error = execute(&mut e, "executor", vec![json!({"kind":"gainLife","player":{"kind":"abilityController"},"amount":4}),json!({"kind":"returnCardsFromGraveyard","player":{"kind":"abilityController"},"controller":{"kind":"abilityController"},"where":{"kind":"cardTypeContains","value":"Creature"}})], BTreeMap::new()).unwrap_err();
    assert!(
        error
            .message
            .contains("object incarnation sequence exhausted")
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(
        rand::RngCore::next_u64(&mut e.rng),
        rand::RngCore::next_u64(&mut rng)
    );
}

#[test]
fn exhausted_sacrifice_cost_restores_old_live_lki_observer_and_clock() {
    let mut e = engine();
    let id = insert(&mut e, "battlefield", "cost-source", "Creature - Bear", 902);
    admit(&mut e);
    let old = e.state.players[0].battlefield.last().unwrap().clone();
    e.state.battlefield_incarnation_sequence = usize::MAX;
    let action: LegalAction = serde_json::from_value(json!({"id":"cost-lki","kind":"activateAbility","playerId":"p0","label":"observe cost","cardInstanceId":id})).unwrap();
    e.with_immediate_source_context(&action, &old, |e, index| {
        let before = serde_json::to_value(&e.state).unwrap();
        let error = e
            .sacrifice_permanent_as_cost("p0", "executor", &id)
            .unwrap_err();
        assert!(
            error
                .message
                .contains("object incarnation sequence exhausted")
        );
        assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
        let observer = e.immediate_source_context(index)?;
        assert!(!source_lki::source_has_departed(&observer));
        assert_eq!(identity(&observer.card), identity(&old));
        Ok(())
    })
    .unwrap();
}

#[test]
fn evaluate_live_uses_the_predeparture_policy_and_never_a_removed_previous_source() {
    for remove_source_first in [false, true] {
        let mut e = engine();
        let policy_id = insert(
            &mut e,
            "battlefield",
            "self-graveyard-policy",
            "Artifact",
            902,
        );
        let victim = insert(
            &mut e,
            "battlefield",
            "ordinary-victim",
            "Creature - Bear",
            903,
        );
        let (_, index) = e.permanent_position(&policy_id).unwrap();
        set_printed_rules(
            &mut e.state.players[0].battlefield[index],
            vec![json!({
                "kind":"staticAbility","source":{"kind":"self"},
                "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
                "modifiers":[{"kind":"replaceGraveyardDestination","graveyardOwners":{"kind":"eachPlayer"},"where":null,"from":{"kind":"anyZone"},"to":{"kind":"exile"}}]
            })],
        );
        admit(&mut e);
        let moved = if remove_source_first {
            e.exile_permanent(&policy_id).unwrap();
            victim
        } else {
            execute(&mut e, "executor", vec![json!({
                "kind":"becomeCreature","object":{"kind":"chosenTarget","id":"animate"},
                "addTypes":["Creature"],"addSubtypes":[],"addColors":[],
                "basePower":{"kind":"integer","value":2},"baseToughness":{"kind":"integer","value":2},
                "retainExistingTypes":true,"duration":{"kind":"untilEndOfCurrentTurn"}
            })], BTreeMap::from([("animate".into(), TargetRef::Permanent { instance_id:policy_id.clone() })])).unwrap();
            let (player, card) = e.permanent_position(&policy_id).unwrap();
            assert!(e.permanent_is_creature(&e.state.players[player].battlefield[card]));
            policy_id
        };
        let clock = e.state.battlefield_incarnation_sequence;
        // Non-sacrifice caller follows the real legacy EvaluateLive path.
        e.move_permanent_to_graveyard(&moved, "state-based")
            .unwrap();
        assert_eq!(e.state.battlefield_incarnation_sequence, clock + 1);
        assert_eq!(
            e.state.players[0]
                .exile
                .iter()
                .any(|c| c.instance_id == moved),
            !remove_source_first
        );
        assert_eq!(
            e.state.players[0]
                .graveyard
                .iter()
                .any(|c| c.instance_id == moved),
            remove_source_first
        );
        assert_eq!(
            e.state
                .events
                .iter()
                .filter(|v| v.kind == "graveyardDestinationReplaced")
                .count(),
            usize::from(!remove_source_first)
        );
    }
}

#[test]
fn late_cost_failure_restores_trigger_sequence_as_well_as_public_state() {
    let mut e = engine();
    let first = insert(&mut e, "battlefield", "first-cost", "Creature - Bear", 902);
    let second = insert(&mut e, "battlefield", "second-cost", "Creature - Bear", 903);
    let (_, index) = e.permanent_position(&first).unwrap();
    set_printed_rules(
        &mut e.state.players[0].battlefield[index],
        vec![json!({
            "kind":"triggeredAbility","source":{"kind":"self"},
            "event":{"kind":"permanentDied","object":{"kind":"self"}},
            "effects":[{"kind":"gainLife","player":{"kind":"abilityController"},"amount":1}]
        })],
    );
    admit(&mut e);
    let executor = e.state.players[0].battlefield[0].instance_id.clone();
    let mut action: LegalAction = serde_json::from_value(json!({
        "id":"two-costs","kind":"activateAbility","playerId":"p0", "label":"pay two real sacrifice costs",
        "cardInstanceId":executor,"decisions":{"activatedLifePayment":2}
    })).unwrap();
    // Cost resources are not declared spell targets or Ward producers.
    action
        .targets
        .insert("cost-a".into(), TargetRef::Permanent { instance_id: first });
    action.targets.insert(
        "cost-b".into(),
        TargetRef::Permanent {
            instance_id: second,
        },
    );
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[
        {"kind":"payLife","player":{"kind":"controllerOf","object":{"kind":"self"}},"amount":2},
        {"kind":"sacrificePermanent","permanent":{"kind":"chosenObject","id":"cost-a"}},
        {"kind":"sacrificePermanent","permanent":{"kind":"chosenObject","id":"cost-b"}}
    ],"effects":[]});
    let payment = e.prepare_activated_cost_payment(&action, &rule).unwrap();
    // First real departure can install its object and queue its death trigger;
    // only the second allocation fails. A GameState-only snapshot leaks the
    // private stack sequence advanced by that first trigger.
    e.state.battlefield_incarnation_sequence = usize::MAX - 1;
    let before = serde_json::to_value(&e.state).unwrap();
    let stack_sequence = e.stack_sequence;
    let copy_sequence = e.copy_sequence;
    let error = e
        .commit_activated_cost_payment(&payment)
        .err()
        .expect("second allocation must fail");
    assert!(
        error
            .message
            .contains("object incarnation sequence exhausted")
    );
    assert_eq!(serde_json::to_value(&e.state).unwrap(), before);
    assert_eq!(e.stack_sequence, stack_sequence);
    assert_eq!(e.copy_sequence, copy_sequence);
    assert!(e.check_quantity_failure().is_ok());
}
