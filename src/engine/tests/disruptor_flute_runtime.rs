use super::*;

const ORACLE: &str = "Flash\nAs this artifact enters, choose a card name.\nSpells with the chosen name cost {3} more to cast.\nActivated abilities of sources with the chosen name can't be activated unless they're mana abilities.";

struct NamedDeviceDecisionProvider;

impl DecisionProvider for NamedDeviceDecisionProvider {
    fn choose(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<usize, EngineError> {
        Ok(0)
    }

    fn choose_card_name(
        &mut self,
        _state: &GameState,
        _request: &EngineDecisionRequest,
    ) -> Result<String, EngineError> {
        Ok("Named Device".to_string())
    }
}

fn named_device_definition(name: &str) -> CardDefinition {
    let mut definition = test_definition(name, "Artifact");
    definition.name = name.to_string();
    definition.rules = vec![
        json!({
            "kind": "activatedAbility",
            "source": { "kind": "self" },
            "costs": [{ "kind": "payMana", "manaCost": "{1}" }],
            "effects": [{
                "kind": "gainLife",
                "player": { "kind": "abilityController" },
                "amount": { "kind": "integer", "value": 1 },
            }],
        }),
        json!({
            "kind": "manaAbility",
            "source": { "kind": "self" },
            "costs": [{
                "kind": "tap",
                "object": { "kind": "abilitySource" },
            }],
            "effects": [{
                "kind": "addMana",
                "player": {
                    "kind": "controllerOf",
                    "object": { "kind": "abilitySource" },
                },
                "mana": "{C}",
            }],
        }),
    ];
    definition
}

#[test]
fn chosen_name_persists_and_drives_tax_prohibition_and_mana_exception() {
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Disruptor Flute".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some(ORACLE.to_string()),
        layout: None,
        faces: Vec::new(),
    });
    assert_eq!(parsed.status, "canonical");

    let mut flute_definition = test_definition("disruptor-flute", "Artifact");
    flute_definition.name = "Disruptor Flute".to_string();
    flute_definition.rules = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.clone())
        .collect();
    let mut flute = test_instance("disruptor-flute", flute_definition, "player-0");

    let mut engine = test_engine(2);
    engine
        .apply_enter_replacements(&mut flute, &mut NamedDeviceDecisionProvider)
        .expect("the card-name entry choice applies");
    assert_eq!(
        stored_decision_value(&flute.definition, "chosenCardName"),
        Some("Named Device")
    );
    engine.state.players[0].battlefield.push(flute);

    let mut named_spell = test_definition("named-device-spell", "Artifact");
    named_spell.name = "Named Device".to_string();
    named_spell.mana_cost = "{1}".to_string();
    let mut other_spell = test_definition("other-device-spell", "Artifact");
    other_spell.name = "Other Device".to_string();
    other_spell.mana_cost = "{1}".to_string();
    assert_eq!(
        engine.casting_mana_costs(&named_spell, "player-1", &BTreeMap::new()),
        vec!["{1}".to_string(), "{3}".to_string()]
    );
    assert_eq!(
        engine.casting_mana_costs(&other_spell, "player-1", &BTreeMap::new()),
        vec!["{1}".to_string()]
    );

    engine.state.players[1].battlefield = vec![
        test_instance(
            "named-device",
            named_device_definition("Named Device"),
            "player-1",
        ),
        test_instance(
            "other-device",
            named_device_definition("Other Device"),
            "player-1",
        ),
        mana_source("opponent-land-1", "Basic Land - Plains", "{W}", None),
        mana_source("opponent-land-2", "Basic Land - Plains", "{W}", None),
    ];
    for permanent in &mut engine.state.players[1].battlefield {
        permanent.owner = "player-1".to_string();
        permanent.controller = "player-1".to_string();
    }
    engine.state.active_player = 1;
    engine.state.priority_player = Some(1);
    engine.state.step = GameStep::PrecombatMain;

    let actions = engine.legal_priority_actions(1);
    assert!(actions.iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some("named-device")
            && action.decisions.get("manaAbility") == Some(&Value::Bool(true))
    }));
    assert!(actions.iter().all(|action| {
        action.kind != ActionKind::ActivateAbility
            || action.card_instance_id.as_deref() != Some("named-device")
            || action.decisions.get("manaAbility") == Some(&Value::Bool(true))
    }));
    assert!(actions.iter().any(|action| {
        action.kind == ActionKind::ActivateAbility
            && action.card_instance_id.as_deref() == Some("other-device")
            && action.decisions.get("manaAbility") != Some(&Value::Bool(true))
    }));
}
