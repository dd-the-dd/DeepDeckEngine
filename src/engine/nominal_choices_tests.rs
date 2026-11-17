use super::creature_type_options;
use serde_json::json;

#[test]
fn unrestricted_choices_include_complete_compound_identities_without_world_input() {
    let options = creature_type_options(None).unwrap();
    let complete = deepdeck_card_domain::all_creature_subtypes()
        .unwrap()
        .iter()
        .map(|id| id.ir_name().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        options, complete,
        "unrestricted choice exposes the entire stable domain"
    );
    for name in ["Time Lord", "Assembly-Worker", "Human", "Elf"] {
        assert_eq!(
            options
                .iter()
                .filter(|value| value.as_str() == name)
                .count(),
            1,
            "{name}"
        );
    }
    for fragment_or_family in ["Time", "Lord", "Worker", "Equipment", "Power-Plant", "Urza"] {
        assert!(
            !options.iter().any(|value| value == fragment_or_family),
            "{fragment_or_family}"
        );
    }
    assert_eq!(options, creature_type_options(None).unwrap());
}

#[test]
fn explicit_options_are_restricted_canonical_unique_and_in_registry_order() {
    let forward = creature_type_options(Some(&json!(["Time Lord", "Elf", "Time Lord"]))).unwrap();
    let reverse = creature_type_options(Some(&json!(["Elf", "Time Lord"]))).unwrap();
    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 2);
    assert!(
        forward
            .iter()
            .all(|name| name == "Elf" || name == "Time Lord")
    );
    assert!(!forward.iter().any(|name| name == "Human"));
    let straight = creature_type_options(Some(&json!(["C'tan"]))).unwrap();
    let curly = creature_type_options(Some(&json!(["C’tan"]))).unwrap();
    assert_eq!(straight, curly);
    assert_eq!(straight.len(), 1);
}

#[test]
fn malformed_unknown_noncreature_and_plural_options_are_errors() {
    for options in [
        json!(null),
        json!("Elf"),
        json!([]),
        json!([3]),
        json!([""]),
        json!(["FutureUnknownType"]),
        json!(["Equipment"]),
        json!(["Power-Plant"]),
        json!(["Elves"]),
        json!([" Elf"]),
        json!(["Elf "]),
        json!(["Time  Lord"]),
    ] {
        assert!(creature_type_options(Some(&options)).is_err(), "{options}");
    }
}

#[test]
fn both_effect_and_entry_contracts_validate_the_same_explicit_creature_options() {
    for (options, valid) in [
        (json!(["Elf"]), true),
        (json!(["Elf", "Time Lord"]), true),
        (json!([]), false),
        (json!(["Equipment"]), false),
        (json!(["FutureUnknownType"]), false),
        (json!([3]), false),
        (json!(["Elf "]), false),
    ] {
        let effect = json!({"kind":"chooseCreatureType","id":"tribe",
            "player":{"kind":"abilityController"},"options":options});
        assert_eq!(
            super::super::effect_supported(&effect),
            valid,
            "effect: {options}"
        );
        let replacement = json!({"kind":"replacementEffect","source":{"kind":"self"},
            "event":{"kind":"wouldEnterBattlefield","object":{"kind":"self"}},
            "decisions":[{"kind":"chooseCreatureType","id":"tribe","options":options}],
            "replacement":[{"kind":"storeDecision","decisionId":"tribe"}]});
        assert_eq!(
            super::super::rule_is_executable(&replacement),
            valid,
            "entry: {options}"
        );
    }
}

use super::super::{
    CanonicalRuleExecution, CardDefinition, DecisionProvider, EngineDecisionRequest, EngineError,
    GameEngine, GameSetup, GameState, PlayerDeck, card_instance, stored_decision_value,
};
use std::collections::BTreeMap;

fn path_definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"manaCost":"{0}"})).unwrap()
}
fn path_engine(hidden_type: &str) -> GameEngine {
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|index| PlayerDeck {
                    id: format!("p{index}"),
                    name: format!("Player{index}"),
                    starting_life: 20,
                    cards: if index == 0 {
                        vec![
                            path_definition("chooser", "Artifact"),
                            path_definition("hidden", hidden_type),
                        ]
                    } else {
                        vec![path_definition("filler", "Basic Land - Forest"); 2]
                    },
                })
                .collect(),
        },
        261115,
    )
    .unwrap()
}
fn entry_choice_rule(options: Option<serde_json::Value>) -> serde_json::Value {
    let mut rule = json!({"kind":"replacementEffect","source":{"kind":"self"},
        "event":{"kind":"wouldEnterBattlefield","object":{"kind":"self"}},
        "decisions":[{"kind":"chooseCreatureType","id":"tribe"}],
        "replacement":[{"kind":"storeDecision","decisionId":"tribe"}]});
    if let Some(options) = options {
        rule["decisions"][0]["options"] = options;
    }
    rule
}
#[derive(Default)]
struct TimeLordProvider {
    menus: Vec<Vec<String>>,
}
impl DecisionProvider for TimeLordProvider {
    fn choose(
        &mut self,
        _: &GameState,
        request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        let menu = request
            .options
            .iter()
            .map(|action| {
                action
                    .decisions
                    .values()
                    .find_map(|value| {
                        value.as_str().or_else(|| {
                            value
                                .as_array()
                                .filter(|values| values.len() == 1)
                                .and_then(|values| values[0].as_str())
                        })
                    })
                    .map(str::to_string)
                    .ok_or_else(|| EngineError::new("unexpected nominal test choice shape"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let index = menu
            .iter()
            .position(|value| value == "Time Lord")
            .ok_or_else(|| {
                EngineError::new("Time Lord must be selectable without appearing in world")
            })?;
        self.menus.push(menu);
        Ok(index)
    }
}

#[test]
fn entry_callback_returns_and_stores_time_lord_from_complete_world_independent_menu() {
    let expected = creature_type_options(None).unwrap();
    for hidden in ["Creature - Elf", "Creature - Goblin"] {
        let mut engine = path_engine(hidden);
        let mut definition = path_definition("entry-chooser", "Land");
        definition.rules = vec![entry_choice_rule(None)];
        let mut card = card_instance("p0", &definition, 900);
        let mut provider = TimeLordProvider::default();
        engine
            .apply_enter_replacements(&mut card, &mut provider)
            .unwrap();
        assert_eq!(provider.menus, vec![expected.clone()]);
        assert_eq!(
            stored_decision_value(&card.definition, "tribe"),
            Some("Time Lord")
        );
    }
}

#[test]
fn resolution_callback_returns_time_lord_through_public_instruction_and_records_exact_value() {
    let expected = creature_type_options(None).unwrap();
    for hidden in ["Creature - Elf", "Creature - Goblin"] {
        let mut engine = path_engine(hidden);
        let mut provider = TimeLordProvider::default();
        let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
            {"kind":"chooseCreatureType","id":"tribe","player":{"kind":"abilityController"}}]});
        engine
            .execute_canonical_rule(
                CanonicalRuleExecution {
                    controller_id: "p0".into(),
                    source_definition_id: "chooser".into(),
                    rule,
                    decisions: BTreeMap::new(),
                    targets: BTreeMap::new(),
                },
                &mut provider,
            )
            .unwrap();
        assert_eq!(provider.menus, vec![expected.clone()]);
        let events = engine
            .state
            .events
            .iter()
            .filter(|event| event.kind == "creatureTypeChosen")
            .collect::<Vec<_>>();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].detail["creatureType"], json!("Time Lord"));
        assert_eq!(events[0].detail["decisionId"], json!("tribe"));
    }
}

#[test]
fn invalid_entry_options_and_initial_bindings_fail_before_provider_or_entry_mutation() {
    for (options, initial) in [
        (json!(["Equipment"]), BTreeMap::new()),
        (
            json!(["Elf", "Time Lord"]),
            BTreeMap::from([("tribe".into(), json!("Equipment"))]),
        ),
        (
            json!(["Elf"]),
            BTreeMap::from([("tribe".into(), json!("Time Lord"))]),
        ),
    ] {
        let mut engine = path_engine("Creature - Elf");
        // A valid existing mutation would tap this card if preflight ran late.
        let mut policy = path_definition("tap-policy", "Artifact");
        policy.rules = vec![json!({"kind":"staticAbility","source":{"kind":"self"},
            "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
            "modifiers":[{"kind":"permanentsEnterTapped","objects":{"kind":"permanents","where":null}}]})];
        engine.state.players[0]
            .battlefield
            .push(card_instance("p0", &policy, 901));
        engine.admit_explicit_test_fixture_objects().unwrap();
        let mut definition = path_definition("entry-chooser", "Land");
        definition.rules = vec![entry_choice_rule(Some(options))];
        let mut card = card_instance("p0", &definition, 900);
        let before_card = card.clone();
        let before_state = serde_json::to_value(&engine.state).unwrap();
        let branch = engine.branching_decision_sequence.get();
        let mut provider = TimeLordProvider::default();
        let error = engine
            .apply_enter_replacements_with_decisions(&mut card, &mut provider, &initial)
            .unwrap_err();
        assert!(error.message.starts_with("nominal."));
        assert!(provider.menus.is_empty());
        assert_eq!(card, before_card);
        assert_eq!(serde_json::to_value(&engine.state).unwrap(), before_state);
        assert_eq!(engine.branching_decision_sequence.get(), branch);
    }
    // Public instruction execution rejects malformed effect IR at its contract
    // gate, distinct from the checked entry-input error tested above.
    let mut engine = path_engine("Creature - Elf");
    let mut provider = TimeLordProvider::default();
    let before = serde_json::to_value(&engine.state).unwrap();
    let rule = json!({"kind":"spellAbility","source":{"kind":"self"},"effects":[
        {"kind":"gainLife","player":{"kind":"abilityController"},"amount":3},
        {"kind":"chooseCreatureType","id":"tribe","player":{"kind":"abilityController"},"options":["Equipment"]}]});
    let error = engine
        .execute_canonical_rule(
            CanonicalRuleExecution {
                controller_id: "p0".into(),
                source_definition_id: "chooser".into(),
                rule,
                decisions: BTreeMap::new(),
                targets: BTreeMap::new(),
            },
            &mut provider,
        )
        .unwrap_err();
    assert!(error.message.contains("unexecutable"));
    assert!(provider.menus.is_empty());
    assert_eq!(serde_json::to_value(&engine.state).unwrap(), before);
}

#[test]
fn initial_creature_choice_alias_is_canonicalized_and_ambiguous_reuse_is_explicit() {
    let mut definition = path_definition("entry-chooser", "Land");
    definition.rules = vec![entry_choice_rule(Some(json!(["C’tan", "Time Lord"])))];
    let initial = BTreeMap::from([("tribe".into(), json!(["C'tan"]))]);
    let prepared = super::prepare_entry_creature_type_bindings(&definition, &initial).unwrap();
    assert_eq!(
        prepared["tribe"],
        json!(
            deepdeck_card_domain::resolve_creature_choice("C’tan")
                .unwrap()
                .ir_name()
        )
    );
    definition
        .rules
        .push(entry_choice_rule(Some(json!(["Elf", "Time Lord"]))));
    let reused = BTreeMap::from([("tribe".into(), json!("Time Lord"))]);
    let error = super::prepare_entry_creature_type_bindings(&definition, &reused).unwrap_err();
    assert!(error.message.contains("binding-scope"));
    // No incoming value means the existing independent per-rule choices remain.
    assert!(super::prepare_entry_creature_type_bindings(&definition, &BTreeMap::new()).is_ok());
}

#[test]
fn one_entry_rule_rejects_duplicate_nominal_producers_before_callback_or_mutation() {
    for decisions in [
        json!([
            {"kind":"chooseCreatureType","id":"tribe","options":["Elf"]},
            {"kind":"chooseCreatureType","id":"tribe","options":["Time Lord"]}
        ]),
        json!([
            {"kind":"chooseCardType","id":"tribe","options":["Artifact"]},
            {"kind":"chooseCreatureType","id":"tribe","options":["Elf"]}
        ]),
        json!([
            {"kind":"chooseCreatureType","id":"tribe","options":["Time Lord"]},
            {"kind":"chooseCreatureType","id":"tribe","options":["Time Lord"]}
        ]),
    ] {
        let mut rule = entry_choice_rule(None);
        rule["decisions"] = decisions;
        assert!(super::validate_entry_creature_choice_scope(&rule).is_err());
        assert!(!super::super::rule_is_executable(&rule));
        let mut engine = path_engine("Creature - Elf");
        let mut policy = path_definition("tap-policy", "Artifact");
        policy.rules = vec![json!({"kind":"staticAbility","source":{"kind":"self"},
            "activeWhile":{"kind":"inZone","object":{"kind":"self"},"zone":{"kind":"battlefield"}},
            "modifiers":[{"kind":"permanentsEnterTapped","objects":{"kind":"permanents","where":null}}]})];
        engine.state.players[0]
            .battlefield
            .push(card_instance("p0", &policy, 901));
        engine.admit_explicit_test_fixture_objects().unwrap();
        let mut definition = path_definition("duplicate-entry-chooser", "Land");
        definition.rules = vec![rule];
        let mut card = card_instance("p0", &definition, 900);
        let before_card = card.clone();
        let before_state = serde_json::to_value(&engine.state).unwrap();
        let branch = engine.branching_decision_sequence.get();
        let mut provider = TimeLordProvider::default();
        let error = engine
            .apply_enter_replacements_with_decisions(&mut card, &mut provider, &BTreeMap::new())
            .unwrap_err();
        assert!(
            error
                .message
                .starts_with("nominal.entry-choice.binding-scope:")
        );
        assert!(provider.menus.is_empty());
        assert_eq!(card, before_card);
        assert_eq!(serde_json::to_value(&engine.state).unwrap(), before_state);
        assert_eq!(engine.branching_decision_sequence.get(), branch);
    }
}

#[test]
fn distinct_entry_rules_keep_separate_empty_input_nominal_scopes() {
    let mut definition = path_definition("independent-entry-chooser", "Land");
    definition.rules = vec![
        entry_choice_rule(Some(json!(["Elf", "Time Lord"]))),
        entry_choice_rule(Some(json!(["Time Lord", "Human"]))),
    ];
    assert!(
        definition
            .rules
            .iter()
            .all(super::super::rule_is_executable)
    );
    assert!(super::prepare_entry_creature_type_bindings(&definition, &BTreeMap::new()).is_ok());
    let mut engine = path_engine("Creature - Goblin");
    let mut card = card_instance("p0", &definition, 900);
    let mut provider = TimeLordProvider::default();
    engine
        .apply_enter_replacements_with_decisions(&mut card, &mut provider, &BTreeMap::new())
        .unwrap();
    assert_eq!(
        provider.menus,
        vec![
            creature_type_options(Some(&json!(["Elf", "Time Lord"]))).unwrap(),
            creature_type_options(Some(&json!(["Time Lord", "Human"]))).unwrap(),
        ]
    );
    assert_eq!(
        stored_decision_value(&card.definition, "tribe"),
        Some("Time Lord")
    );
    // The old shared incoming-value ambiguity remains a different boundary.
    let incoming = BTreeMap::from([("tribe".into(), json!("Time Lord"))]);
    assert!(super::prepare_entry_creature_type_bindings(&definition, &incoming).is_err());
}
