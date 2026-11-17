use super::super::*;

fn definition(id: &str, types: &str) -> CardDefinition {
    serde_json::from_value(json!({"id":id,"name":id,"typeLine":types,"manaCost":"{0}","power":"1","toughness":"1","rules":[]})).unwrap()
}

fn engine() -> GameEngine {
    GameEngine::new(
        GameSetup {
            players: (0..2)
                .map(|n| PlayerDeck {
                    id: format!("player-{n}"),
                    name: format!("Player {n}"),
                    starting_life: 20,
                    cards: vec![definition("filler", "Basic Land - Plains"); 8],
                })
                .collect(),
            opening_hand_size: 0,
            starting_player: 0,
        },
        1804,
    )
    .unwrap()
}

fn stack(id: &str, definition_id: &str, types: &str) -> StackObject {
    StackObject {
        id: id.to_string(),
        controller: "player-0".to_string(),
        card: card_instance("player-0", &definition(definition_id, types), 1805),
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: None,
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    }
}

#[test]
fn unique_stack_definition_resolves_through_the_common_stack_identity_resolver() {
    let mut engine = engine();
    engine
        .state
        .stack
        .push(stack("stack-unique", "witness", "Instant"));
    let selected = engine
        .resolve_canonical_target(&CanonicalTargetSelection::StackObjectDefinition {
            definition_id: "witness".to_string(),
        })
        .unwrap();
    assert_eq!(
        selected,
        TargetRef::StackObject {
            stack_id: "stack-unique".to_string()
        }
    );
}

#[test]
fn missing_stack_definition_is_an_explicit_error_without_mutation() {
    let engine = engine();
    let before = engine.state.clone();
    let error = engine
        .resolve_canonical_target(&CanonicalTargetSelection::StackObjectDefinition {
            definition_id: "missing".to_string(),
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exactly one stack object; found 0")
    );
    assert_eq!(engine.state, before);
}

#[test]
fn duplicate_stack_definition_is_ambiguous_even_when_one_object_is_a_copy() {
    let mut engine = engine();
    engine
        .state
        .stack
        .push(stack("original", "witness", "Instant"));
    engine.state.stack.push(stack("copy", "witness", "Instant"));
    let before = engine.state.clone();
    let error = engine
        .resolve_canonical_target(&CanonicalTargetSelection::StackObjectDefinition {
            definition_id: "witness".to_string(),
        })
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("exactly one stack object; found 2")
    );
    assert_eq!(engine.state, before);
}

#[test]
fn definition_selection_still_obeys_the_activated_abilitys_ordinary_target_predicate() {
    let mut engine = engine();
    let rule = json!({"kind":"activatedAbility","source":{"kind":"self"},"costs":[{"kind":"payMana","manaCost":"{0}"}],
        "declaration":{"kind":"castingDeclaration","decisions":[{"kind":"chooseTargets","id":"spell","minimum":1,"maximum":1,"candidates":{"kind":"spells","where":{"kind":"cardTypeContains","value":"Creature"}}}]},
        "effects":[{"kind":"counterSpell","spell":{"kind":"chosenTarget","id":"spell"}}]});
    assert!(rule_is_executable(&rule));
    let mut source = definition("helper", "Artifact");
    source.rules = vec![rule];
    engine.state.players[0]
        .battlefield
        .push(card_instance("player-0", &source, 1806));
    engine.state.step = GameStep::PrecombatMain;
    let activation = CanonicalActivatedAbilityActivation {
        controller_id: "player-0".to_string(),
        source_definition_id: "helper".to_string(),
        ability_rule_index: Some(0),
        targets: BTreeMap::from([(
            "spell".to_string(),
            CanonicalTargetSelection::StackObjectDefinition {
                definition_id: "witness".to_string(),
            },
        )]),
    };
    engine
        .state
        .stack
        .push(stack("instant", "witness", "Instant"));
    assert!(
        !engine
            .canonical_activated_ability_available(&activation)
            .unwrap()
    );
    engine.state.stack.clear();
    engine
        .state
        .stack
        .push(stack("creature", "witness", "Creature"));
    assert!(
        engine
            .canonical_activated_ability_available(&activation)
            .unwrap()
    );
}
