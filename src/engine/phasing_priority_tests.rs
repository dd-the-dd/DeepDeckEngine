use super::*;

#[test]
fn phased_out_priority_sources_do_not_offer_activated_actions() {
    for any_player in [false, true] {
        let mut engine = test_engine(2);
        let mut definition = test_definition("phase-source", "Artifact");
        definition.rules.push(json!({
            "kind": "activatedAbility", "source": {"kind": "self"},
            "anyPlayerMayActivate": any_player,
            "costs": [{"kind": "tap", "object": {"kind": "self"}}],
            "effects": [{"kind": "gainLife", "amount": {"kind": "integer", "value": 1},
                "player": {"kind": "controllerOf", "object": {"kind": "self"}}}],
        }));
        engine.state.players[0].battlefield =
            vec![test_instance("phase-source", definition, "player-0")];
        engine.state.players[0].hand.clear();
        let actor = usize::from(any_player);
        let offered = engine.legal_priority_actions(actor);
        let activation = offered
            .iter()
            .find(|action| action.card_instance_id.as_deref() == Some("phase-source"))
            .expect("the phased-in ability is available, including any-player activations")
            .clone();
        engine.phase_out_permanent("phase-source");
        assert!(engine.state.players[0].battlefield[0].flags["phasedOut"]);
        assert!(
            engine
                .legal_priority_actions(actor)
                .iter()
                .all(|action| { action.card_instance_id.as_deref() != Some("phase-source") }),
            "CR702.26b: a phased-out source cannot activate its abilities"
        );
        assert!(
            engine
                .apply_priority_action(&activation, &mut RandomAi::seeded(1))
                .is_err(),
            "stale pre-phasing activations must still be rejected"
        );
        engine
            .start_turn(&mut RandomAi::seeded(1))
            .expect("phase-in succeeds");
        assert!(
            engine
                .legal_priority_actions(actor)
                .iter()
                .any(|action| { action.card_instance_id.as_deref() == Some("phase-source") }),
            "the same ability returns after phasing in"
        );
    }
}

#[test]
fn phased_out_priority_sources_do_not_offer_equipment_actions() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.players[0].hand.clear();
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Phase equipment".to_string(),
        type_line: "Artifact — Equipment".to_string(),
        mana_cost: Some("{0}".to_string()),
        oracle_text: Some("Equip {0}".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    let mut equipment = test_definition("phase-equipment", "Artifact — Equipment");
    equipment.rules = parsed
        .abilities
        .iter()
        .filter_map(|ability| ability.rule.clone())
        .collect();
    engine.state.players[0].battlefield = vec![
        test_instance("phase-equipment", equipment, "player-0"),
        test_instance(
            "bearer",
            test_definition("bearer", "Creature — Human"),
            "player-0",
        ),
    ];
    assert!(
        engine
            .legal_priority_actions(0)
            .iter()
            .any(|action| { action.decisions.get("equip") == Some(&Value::Bool(true)) })
    );
    engine.phase_out_permanent("phase-equipment");
    assert!(
        engine
            .legal_priority_actions(0)
            .iter()
            .all(|action| { action.card_instance_id.as_deref() != Some("phase-equipment") })
    );
}
