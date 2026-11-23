//! Runtime proofs for supported cards the smoke harness found doing nothing
//! (`runtime_smoke`): each test plays the real card text through the
//! compiler and the engine and checks the state it promises.
use super::*;

fn compile(name: &str, type_line: &str, mana_cost: &str, text: &str) -> CardDefinition {
    let (status, card) = compile_with_status(name, type_line, mana_cost, text);
    assert_eq!(status, "executable", "{name}");
    card
}

fn compile_with_status(
    name: &str,
    type_line: &str,
    mana_cost: &str,
    text: &str,
) -> (String, CardDefinition) {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let is_creature = type_line.contains("Creature");
    let compiled = std::thread::Builder::new()
        .name("plural-subject-compiler".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            compile_playable_card(PlayableCardInput {
                id: "subject-source".to_string(),
                face_id: None,
                is_token: false,
                is_game_piece: false,
                is_sideboard: false,
                power: is_creature.then(|| "2".to_string()),
                toughness: is_creature.then(|| "2".to_string()),
                oracle: request,
            })
        })
        .expect("compiler thread starts")
        .join()
        .expect("compiler thread completes")
        .expect("the card compiles");
    (compiled.engine_status, compiled.card)
}

fn creature(id: &str, controller: &str, cost: &str) -> CardInstance {
    let mut definition = test_definition(id, "Creature \u{2014} Giant");
    definition.power = Some("4".to_string());
    definition.toughness = Some("5".to_string());
    definition.mana_cost = cost.to_string();
    test_instance(id, definition, controller)
}

fn permanent(id: &str, type_line: &str, controller: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), controller)
}

/// Two giants and an artifact on each side.
fn board() -> GameEngine {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    for (index, (player, tag)) in [("player-0", "own"), ("player-1", "opp")]
        .into_iter()
        .enumerate()
    {
        engine.state.players[index].battlefield = vec![
            creature(&format!("{tag}-giant-a"), player, "{3}{G}"),
            creature(&format!("{tag}-giant-b"), player, "{3}{B}"),
            permanent(&format!("{tag}-relic"), "Artifact", player),
        ];
    }
    engine
}

/// Casts the compiled instant or sorcery (for free) declaring exactly the
/// permanents `wanted`, through the legal cast actions, and resolves it.
fn cast_and_resolve(engine: &mut GameEngine, card: &CardDefinition, wanted: &[&str]) {
    let mut free = card.clone();
    free.mana_cost = "{0}".to_string();
    engine.state.players[0]
        .hand
        .push(test_instance("subject-source", free, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let wanted = wanted
        .iter()
        .map(|id| id.to_string())
        .collect::<BTreeSet<_>>();
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.targets.len() == wanted.len()
                && action
                    .targets
                    .values()
                    .filter_map(|target| match target {
                        TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>()
                    == wanted
        })
        .unwrap_or_else(|| panic!("no cast declaring exactly {wanted:?}"));
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell is cast: {}", error.message));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the spell resolves: {}", error.message));
}

fn damage_on(engine: &GameEngine, id: &str) -> i32 {
    let (player, index) = engine
        .permanent_position(id)
        .expect("still on the battlefield");
    engine.state.players[player].battlefield[index].damage_marked
}

fn tapped(engine: &GameEngine, id: &str) -> bool {
    let (player, index) = engine
        .permanent_position(id)
        .expect("still on the battlefield");
    engine.state.players[player].battlefield[index].tapped
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn each_of_two_target_creatures_deals_the_full_amount_to_each_target() {
    // Jagged Lightning, and the `up to` sibling (Dual Shot).
    for (name, text, amount) in [
        (
            "Jagged Lightning",
            "Jagged Lightning deals 3 damage to each of two target creatures.",
            3,
        ),
        (
            "Dual Shot",
            "Dual Shot deals 1 damage to each of up to two target creatures.",
            1,
        ),
    ] {
        let card = compile(name, "Sorcery", "{3}{R}{R}", text);
        let rule = &card.rules[0];
        let decision = &rule["declaration"]["decisions"][0];
        assert_eq!(decision["maximum"], 2, "{name}");
        let id = decision["id"].as_str().expect("target slot id").to_string();
        assert_eq!(
            rule["effects"][0]["recipient"],
            json!({ "kind": "chosenTargets", "id": id }),
            "{name}"
        );
        let mut engine = board();
        cast_and_resolve(&mut engine, &card, &["opp-giant-a", "opp-giant-b"]);
        assert_eq!(damage_on(&engine, "opp-giant-a"), amount, "{name}");
        assert_eq!(damage_on(&engine, "opp-giant-b"), amount, "{name}");
        assert_eq!(damage_on(&engine, "own-giant-a"), 0, "{name}");
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_single_damage_target_keeps_its_singular_recipient() {
    // Near miss: one target slot is still one chosen target.
    let card = compile(
        "Lightning Strike",
        "Instant",
        "{1}{R}",
        "Lightning Strike deals 3 damage to any target.",
    );
    assert_eq!(
        card.rules[0]["effects"][0]["recipient"]["kind"],
        "chosenTarget"
    );
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn each_permanent_actions_read_the_population_and_the_whole_collection() {
    // Plague Wind destroys only what its controller doesn't control; Metal
    // Fatigue taps the whole collection it names (Roaring Slagwurm's shape).
    let plague_wind = compile(
        "Plague Wind",
        "Sorcery",
        "{7}{B}{B}",
        "Destroy all creatures you don't control. They can't be regenerated.",
    );
    let mut engine = board();
    cast_and_resolve(&mut engine, &plague_wind, &[]);
    assert!(engine.permanent_position("own-giant-a").is_some());
    assert!(engine.permanent_position("own-giant-b").is_some());
    assert!(engine.permanent_position("opp-giant-a").is_none());
    assert!(engine.permanent_position("opp-giant-b").is_none());

    let tap = compile("Metal Fatigue", "Instant", "{2}{W}", "Tap all artifacts.");
    let mut engine = board();
    cast_and_resolve(&mut engine, &tap, &[]);
    assert!(tapped(&engine, "opp-relic"));
    assert!(tapped(&engine, "own-relic"));
    assert!(!tapped(&engine, "own-giant-a"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn another_target_of_a_triggered_ability_excludes_its_source() {
    // Relief Captain: `up to three other target creatures`.
    let card = compile(
        "Relief Captain",
        "Creature \u{2014} Kor Knight",
        "{2}{W}{W}",
        "Kicker {2}{W}\nWhen this creature enters, if it was kicked, put a +1/+1 counter on each of up to three other target creatures.",
    );
    let rule = card
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("triggeredAbility"))
        .expect("the enter trigger");
    let mut engine = board();
    engine.state.players[0].battlefield.push(test_instance(
        "subject-source",
        card.clone(),
        "player-0",
    ));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let (player, index) = engine.permanent_position("subject-source").expect("source");
    let source = engine.state.players[player].battlefield[index].clone();
    let declarations = engine
        .declarations_for_triggered_rule(rule, "player-0", &source, &BTreeMap::new())
        .expect("declarations")
        .expect("a declaration");
    assert!(!declarations.is_empty());
    for declaration in &declarations {
        assert!(declaration.targets.values().all(|target| !matches!(
            target,
            TargetRef::Permanent { instance_id } if instance_id == "subject-source"
        )));
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn an_alternative_cost_condition_reads_absent_cast_decisions_as_not_paid() {
    // Vandalblast cast for its mana cost: the overload branch is not taken,
    // and missing cast decisions are not a panic.
    let card = compile(
        "Vandalblast",
        "Sorcery",
        "{R}",
        "Destroy target artifact you don't control.\nOverload {4}{R} (You may cast this spell for its overload cost. If you do, change \"target\" in its text to \"each.\")",
    );
    let rule = card
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("spellAbility"))
        .expect("the spell ability");
    let id = rule["declaration"]["decisions"][0]["id"]
        .as_str()
        .expect("target slot id")
        .to_string();
    let mut engine = board();
    cast_and_resolve(&mut engine, &card, &["opp-relic"]);
    assert!(engine.permanent_position("opp-relic").is_none());
    assert!(engine.permanent_position("own-relic").is_some());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_card_zone_query_reading_battlefield_state_is_refused_before_play() {
    // The card-zone domain reads definition data: mana value is executable
    // (Raise the Past); power, which the zone query cannot read, is refused
    // by the validator instead of failing at resolution (Draw Team Lines).
    let (status, _) = compile_with_status(
        "Raise the Past",
        "Sorcery",
        "{2}{W}{W}",
        "Return all creature cards with mana value 2 or less from your graveyard to the battlefield.",
    );
    assert_eq!(status, "executable");
    let (status, _) = compile_with_status(
        "Test Recall",
        "Sorcery",
        "{2}{W}{W}",
        "Return all creature cards with power 3 or less from your graveyard to the battlefield.",
    );
    assert_ne!(status, "executable");
}

/// The legal casts of `card` (for free) on the board, with the target slot
/// ids of its spell ability.
fn casts_of(card: &CardDefinition) -> (Vec<LegalAction>, Vec<String>) {
    let mut engine = board();
    let mut free = card.clone();
    free.mana_cost = "{0}".to_string();
    engine.state.players[0]
        .hand
        .push(test_instance("subject-source", free, "player-0"));
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let casts = engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| action.kind == ActionKind::CastSpell)
        .collect::<Vec<_>>();
    let slots = card.rules[0]["declaration"]["decisions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|decision| value_kind(decision) == Some("chooseTargets"))
        .filter_map(|decision| decision["id"].as_str().map(str::to_string))
        .collect();
    (casts, slots)
}

/// The objects a cast names for the slot `slot` (`slot` or `slot:index`).
fn slot_objects(cast: &LegalAction, slot: &str) -> Vec<TargetRef> {
    cast.targets
        .iter()
        .filter(|(id, _)| id.split(':').next() == Some(slot))
        .map(|(_, target)| target.clone())
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn another_target_after_a_target_names_a_different_object() {
    // Deadshot (`another target creature`), Betrayal at the Vault (`each of
    // two other target creatures`) and its `up to` sibling, where a single
    // object chosen for the second slot is checked too.
    for (name, text) in [
        (
            "Deadshot",
            "Tap target creature. It deals damage equal to its power to another target creature.",
        ),
        (
            "Betrayal at the Vault",
            "Target creature you control deals damage equal to its power to each of two other target creatures.",
        ),
        (
            "Test Volley",
            "Target creature you control deals damage equal to its power to each of up to two other target creatures.",
        ),
    ] {
        let card = compile(name, "Instant", "{3}{R}", text);
        let (casts, slots) = casts_of(&card);
        let rule = &card.rules[0];
        assert_eq!(
            rule["declaration"]["decisions"][1]["distinctFrom"],
            json!([slots[0]]),
            "{name}"
        );
        assert!(!rule.to_string().contains("otherThanEarlierTargets"));
        assert!(!casts.is_empty(), "{name}");
        for cast in &casts {
            let first = slot_objects(cast, &slots[0]);
            assert!(
                slot_objects(cast, &slots[1])
                    .iter()
                    .all(|object| !first.contains(object)),
                "{name}: {cast:?}"
            );
        }
        if name == "Test Volley" {
            assert!(
                casts
                    .iter()
                    .any(|cast| slot_objects(cast, &slots[1]).len() == 1),
                "a cast names one object for the `up to` slot"
            );
        }
    }
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn another_names_the_source_only_without_an_earlier_target() {
    // Near misses: a lone `another target creature you control` excludes the
    // source and relates to no slot; after an earlier slot (Wayta), `another`
    // relates to that slot and the source may be named.
    let single = compile(
        "Test Rally",
        "Creature \u{2014} Human",
        "{1}{W}",
        "When this creature enters, put a +1/+1 counter on another target creature you control.",
    );
    let trigger = single
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("triggeredAbility"))
        .expect("the enter trigger");
    let decision = &trigger["declaration"]["decisions"][0];
    assert_eq!(decision["candidates"]["excludeSource"], true);
    assert!(decision.get("distinctFrom").is_none());
    assert!(!trigger.to_string().contains("otherThanEarlierTargets"));

    let wayta = compile(
        "Wayta, Trainer Prodigy",
        "Legendary Creature \u{2014} Human Warrior",
        "{R}{G}{W}",
        "{2}{G}, {T}: Target creature you control fights another target creature.",
    );
    let ability = wayta
        .rules
        .iter()
        .find(|rule| value_kind(rule) == Some("activatedAbility"))
        .expect("the fight ability");
    let decisions = ability["declaration"]["decisions"]
        .as_array()
        .expect("two target slots");
    assert_eq!(decisions[1]["distinctFrom"], json!([decisions[0]["id"]]));
    assert!(decisions[1]["candidates"].get("excludeSource").is_none());
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_distinct_relation_judges_only_the_rule_that_declared_the_targets() {
    // Near miss for a card with several spell rules: a rule whose slots the
    // declaration does not use does not judge it.
    let relation = json!({
        "kind": "spellAbility",
        "declaration": { "kind": "castingDeclaration", "decisions": [
            { "kind": "chooseTargets", "id": "first" },
            { "kind": "chooseTargets", "id": "second", "distinctFrom": ["first"] },
        ]},
    });
    let bear = TargetRef::Permanent {
        instance_id: "bear".to_string(),
    };
    let other_rule = BTreeMap::from([("targetPermanent".to_string(), bear.clone())]);
    assert!(!target_legality::rule_declares_targets(
        &relation,
        &other_rule
    ));
    let reused = BTreeMap::from([
        ("first".to_string(), bear.clone()),
        ("second:0".to_string(), bear.clone()),
    ]);
    assert!(target_legality::rule_declares_targets(&relation, &reused));
    assert!(!target_legality::declaration_respects_distinct_targets(
        &relation, &reused
    ));
    let single = BTreeMap::from([
        ("first".to_string(), bear.clone()),
        ("second".to_string(), bear),
    ]);
    assert!(!target_legality::declaration_respects_distinct_targets(
        &relation, &single
    ));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-4"))]
#[test]
fn a_tap_cost_beside_the_source_tap_names_another_permanent() {
    // Munda's Vanguard: `{T}, Tap an untapped Ally you control`. Alone, the
    // source cannot pay both; another Ally makes the activation legal.
    let card = compile(
        "Munda's Vanguard",
        "Creature \u{2014} Kor Knight Ally",
        "{4}{W}",
        "Cohort \u{2014} {T}, Tap an untapped Ally you control: Put a +1/+1 counter on each creature you control.",
    );
    let activations = |with_ally: bool| {
        let mut engine = board();
        let mut source = test_instance("subject-source", card.clone(), "player-0");
        source.summoning_sick = false;
        engine.state.players[0].battlefield.push(source);
        if with_ally {
            let mut ally = test_instance(
                "ally",
                test_definition("ally", "Creature \u{2014} Kor Ally"),
                "player-0",
            );
            ally.summoning_sick = false;
            engine.state.players[0].battlefield.push(ally);
        }
        engine
            .admit_explicit_test_fixture_objects()
            .expect("fixture objects are admitted");
        let actions = engine
            .legal_priority_actions(0)
            .into_iter()
            .filter(|action| {
                action.kind == ActionKind::ActivateAbility
                    && action.card_instance_id.as_deref() == Some("subject-source")
            })
            .collect::<Vec<_>>();
        (engine, actions)
    };
    let (_, alone) = activations(false);
    assert!(alone.is_empty(), "{alone:?}");
    let (mut engine, with_ally) = activations(true);
    let action = with_ally.first().expect("the activation with another Ally");
    engine
        .apply_priority_action(action, &mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the activation is paid: {}", error.message));
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .unwrap_or_else(|error| panic!("the ability resolves: {}", error.message));
    let (player, index) = engine.permanent_position("own-giant-a").expect("giant");
    assert_eq!(
        engine.state.players[player].battlefield[index]
            .counters
            .get("+1/+1"),
        Some(&1)
    );
}
