//! Target lists and permission scopes compiled from real Oracle text.
//! CR 601.2c / 115.3: every instance of `target` is its own declaration with
//! its own count; CR 608.2b: an illegal target is unaffected while the legal
//! ones still are; CR 608.2c: a permission owns its own sentence only.
use super::*;
use std::collections::BTreeSet;

fn parsed_spell_rule(name: &str, type_line: &str, text: &str) -> Value {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some(text.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .name("target-collection-runtime-parser".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("runtime parser thread starts")
        .join()
        .expect("runtime parser thread completes");
    assert_eq!(parsed.status, "canonical", "{:#?}", parsed.abilities);
    let rule = parsed.abilities[0].rule.clone().expect("primitive IR");
    assert!(rule_is_executable(&rule), "{rule:#}");
    rule
}

fn permanent(id: &str, type_line: &str, owner: &str) -> CardInstance {
    test_instance(id, test_definition(id, type_line), owner)
}

/// An engine in player 0's main phase with the parsed spell in hand.
fn engine_with_spell(rule: Value, battlefield: [Vec<CardInstance>; 2]) -> GameEngine {
    let mut spell = test_definition("spell", "Sorcery");
    spell.rules = vec![rule];
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    engine.state.players[0].hand = vec![test_instance("spell", spell, "player-0")];
    let [mine, theirs] = battlefield;
    engine.state.players[0].battlefield = mine;
    engine.state.players[1].battlefield = theirs;
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    engine
}

fn targeted_ids(action: &LegalAction) -> BTreeSet<String> {
    action
        .targets
        .values()
        .filter_map(|target| match target {
            TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
            _ => None,
        })
        .collect()
}

fn cast_with(engine: &GameEngine, ids: &[&str]) -> LegalAction {
    let wanted = ids.iter().map(|id| id.to_string()).collect::<BTreeSet<_>>();
    engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| {
            action.kind == ActionKind::CastSpell
                && action.targets.len() == ids.len()
                && targeted_ids(action) == wanted
        })
        .unwrap_or_else(|| panic!("no cast declaring exactly {ids:?}"))
}

fn cast_target_counts(engine: &GameEngine) -> BTreeSet<usize> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| action.kind == ActionKind::CastSpell)
        .map(|action| action.targets.len())
        .collect()
}

fn in_zone(cards: &[CardInstance], id: &str) -> bool {
    cards.iter().any(|card| card.instance_id == id)
}

fn remove_from_battlefield(engine: &mut GameEngine, player: usize, id: &str) {
    let position = engine.state.players[player]
        .battlefield
        .iter()
        .position(|card| card.instance_id == id)
        .expect("permanent on the battlefield");
    let card = engine.state.players[player].battlefield.remove(position);
    engine.state.players[player].graveyard.push(card);
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn two_target_slots_are_destroyed_together_and_both_lose_regeneration() {
    let rule = parsed_spell_rule(
        "Plague Spores",
        "Sorcery",
        "Destroy target nonblack creature and target land. They can't be regenerated.",
    );
    let decisions = rule["declaration"]["decisions"].as_array().unwrap();
    assert_eq!(decisions.len(), 2, "one declaration per instance of target");
    assert_eq!(rule["effects"].as_array().unwrap().len(), 1);
    assert_eq!(rule["effects"][0]["permanent"]["kind"], "union");
    assert_eq!(rule["effects"][0]["cannotRegenerate"], true);

    let mut engine = engine_with_spell(
        rule,
        [
            vec![],
            vec![
                permanent("bear", "Creature - Bear", "player-1"),
                permanent("forest", "Basic Land - Forest", "player-1"),
            ],
        ],
    );
    let cast = cast_with(&engine, &["bear", "forest"]);
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(7))
        .expect("cast with both declarations");
    engine.state.rule_modifiers.push(json!({
        "kind": "regenerationShield",
        "cardInstanceId": "bear",
    }));
    engine
        .resolve_top_stack(&mut RandomAi::seeded(8))
        .expect("resolve the collection destruction");
    let graveyard = &engine.state.players[1].graveyard;
    assert!(in_zone(graveyard, "bear") && in_zone(graveyard, "forest"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn an_illegal_member_of_a_plural_exile_leaves_the_other_member_affected() {
    let rule = parsed_spell_rule("Dust to Dust", "Sorcery", "Exile two target artifacts.");
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 2);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 2);
    let mut engine = engine_with_spell(
        rule,
        [
            vec![],
            vec![
                permanent("bauble", "Artifact", "player-1"),
                permanent("relic", "Artifact", "player-1"),
            ],
        ],
    );
    assert_eq!(cast_target_counts(&engine), BTreeSet::from([2]));
    let cast = cast_with(&engine, &["bauble", "relic"]);
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(9))
        .expect("cast with two targets");
    remove_from_battlefield(&mut engine, 1, "bauble");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(10))
        .expect("resolve with one illegal target");
    assert!(in_zone(&engine.state.players[1].exile, "relic"));
    assert!(in_zone(&engine.state.players[1].graveyard, "bauble"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn up_to_two_returned_targets_go_to_each_owners_hand() {
    let rule = parsed_spell_rule(
        "Into the Void",
        "Sorcery",
        "Return up to two target creatures to their owners' hands.",
    );
    let mut engine = engine_with_spell(
        rule,
        [
            vec![permanent("my-bear", "Creature - Bear", "player-0")],
            vec![permanent("their-elf", "Creature - Elf", "player-1")],
        ],
    );
    assert_eq!(cast_target_counts(&engine), BTreeSet::from([0, 1, 2]));
    let cast = cast_with(&engine, &["my-bear", "their-elf"]);
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(11))
        .expect("cast with two targets");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(12))
        .expect("resolve the collection return");
    assert!(in_zone(&engine.state.players[0].hand, "my-bear"));
    assert!(in_zone(&engine.state.players[1].hand, "their-elf"));
}

/// Previously unused combination: a literal range (`one or two`), the
/// controller relation, a plural return and its plural possessive.
#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_literal_target_range_declares_only_counts_inside_the_range() {
    let rule = parsed_spell_rule(
        "Range Probe",
        "Sorcery",
        "Return one or two target creatures you control to their owners' hands.",
    );
    assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 2);
    let mut engine = engine_with_spell(
        rule,
        [
            vec![
                permanent("bear", "Creature - Bear", "player-0"),
                permanent("elf", "Creature - Elf", "player-0"),
                permanent("wolf", "Creature - Wolf", "player-0"),
            ],
            vec![permanent("their-goblin", "Creature - Goblin", "player-1")],
        ],
    );
    assert_eq!(cast_target_counts(&engine), BTreeSet::from([1, 2]));
    assert!(
        engine
            .legal_priority_actions(0)
            .iter()
            .all(|action| !targeted_ids(action).contains("their-goblin"))
    );
    let cast = cast_with(&engine, &["elf"]);
    engine
        .apply_priority_action(&cast, &mut RandomAi::seeded(13))
        .expect("cast with one target");
    engine
        .resolve_top_stack(&mut RandomAi::seeded(14))
        .expect("resolve the ranged return");
    assert!(in_zone(&engine.state.players[0].hand, "elf"));
    assert!(in_zone(&engine.state.players[0].battlefield, "bear"));
}

/// Answers the permission with `self.0`; every other choice takes the first
/// legal option.
struct Permission(bool);
impl DecisionProvider for Permission {
    fn choose(
        &mut self,
        _state: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        if let Some(DecisionChoice::OptionSelection { options, .. }) = request.choice.as_ref()
            && let Some(index) = options.iter().position(|option| {
                option
                    == if self.0 {
                        "Perform the optional effects"
                    } else {
                        "Decline"
                    }
            })
        {
            return Ok(index);
        }
        Ok(0)
    }
}

fn summon_undead(perform: bool) -> GameEngine {
    let rule = parsed_spell_rule(
        "Summon Undead",
        "Sorcery",
        "You may mill three cards. Then return a creature card from your graveyard to the battlefield. (To mill a card, put the top card of your library into your graveyard.)",
    );
    assert_eq!(rule["effects"][0]["kind"], "optionalEffects");
    let mut engine = engine_with_spell(rule, [vec![], vec![]]);
    engine.state.players[0].library = ["l1", "l2", "l3", "l4"]
        .into_iter()
        .map(|id| permanent(id, "Instant", "player-0"))
        .collect();
    engine.state.players[0].graveyard = vec![permanent("zombie", "Creature - Zombie", "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let cast = engine
        .legal_priority_actions(0)
        .into_iter()
        .find(|action| action.kind == ActionKind::CastSpell)
        .expect("the spell is castable");
    engine
        .apply_priority_action(&cast, &mut Permission(perform))
        .expect("cast");
    engine
        .resolve_top_stack(&mut Permission(perform))
        .expect("resolve the permission and the independent step");
    engine
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-2"))]
#[test]
fn a_permission_scopes_its_sentence_and_the_next_sentence_always_runs() {
    let declined = summon_undead(false);
    assert_eq!(declined.state.players[0].library.len(), 4);
    assert!(in_zone(&declined.state.players[0].battlefield, "zombie"));

    let performed = summon_undead(true);
    assert_eq!(performed.state.players[0].library.len(), 1);
    assert!(in_zone(&performed.state.players[0].battlefield, "zombie"));
}
