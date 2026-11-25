//! Reconfigure (CR 702.151a-b) compiled from its rules definition: two
//! ordinary activated abilities (attach to another creature you control,
//! unattach), and the runtime rule that an attached Equipment with
//! reconfigure is not a creature until it becomes unattached.
use super::*;

const LIZARD_BLADES: &str = "Double strike\nEquipped creature has double strike.\nReconfigure {2} ({2}: Attach to target creature you control; or unattach from a creature. Reconfigure only as a sorcery. While attached, this isn't a creature.)";

fn lizard_blades() -> CardInstance {
    let compilation = compile_playable_card(PlayableCardInput {
        id: "lizard-blades".to_string(),
        face_id: None,
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: Some("1".to_string()),
        toughness: Some("1".to_string()),
        oracle: OracleCardParseRequest {
            card_name: "Lizard Blades".to_string(),
            type_line: "Artifact Creature \u{2014} Equipment Lizard".to_string(),
            mana_cost: Some("{1}{R}".to_string()),
            oracle_text: Some(LIZARD_BLADES.to_string()),
            layout: Some("normal".to_string()),
            faces: Vec::new(),
        },
    })
    .expect("the card compiles");
    assert_eq!(
        compilation.engine_status, "executable",
        "{:#?}",
        compilation.parser_result.diagnostics
    );
    let reconfigure = compilation
        .card
        .rules
        .iter()
        .filter(|rule| rule["keywordProvenance"]["keyword"] == "reconfigure")
        .count();
    assert_eq!(reconfigure, 2, "attach and unattach");
    test_instance("blades", compilation.card, "player-0")
}

fn permanent<'a>(engine: &'a GameEngine, id: &str) -> &'a CardInstance {
    let (player, index) = engine.permanent_position(id).expect("on the battlefield");
    &engine.state.players[player].battlefield[index]
}

fn reconfigure_actions(engine: &GameEngine) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::ActivateAbility
                && action.card_instance_id.as_deref() == Some("blades")
        })
        .collect()
}

fn add_two_mana(engine: &mut GameEngine) {
    engine.state.players[0].mana_pool = ["C", "C"]
        .into_iter()
        .map(|symbol| FloatingMana {
            symbol: symbol.to_string(),
            spend_restriction: None,
        })
        .collect();
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn an_attached_reconfigure_equipment_is_not_a_creature_until_unattached() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    add_two_mana(&mut engine);
    let mut bear = test_definition("bear", "Creature \u{2014} Bear");
    bear.power = Some("2".to_string());
    engine.state.players[0].battlefield =
        vec![lizard_blades(), test_instance("bear", bear, "player-0")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(engine.permanent_is_creature(permanent(&engine, "blades")));

    // Only the attach ability is legal while unattached; it targets
    // another creature, never the Equipment itself.
    let attach = reconfigure_actions(&engine)
        .into_iter()
        .find(|action| {
            action.targets.values().any(|target| {
                matches!(target, TargetRef::Permanent { instance_id } if instance_id == "bear")
            })
        })
        .expect("attach to the bear");
    assert!(reconfigure_actions(&engine).iter().all(|action| {
        !action.targets.values().any(|target| {
            matches!(target, TargetRef::Permanent { instance_id } if instance_id == "blades")
        })
    }));
    engine
        .apply_priority_action(&attach, &mut EmeritusDecisionProvider)
        .expect("activate reconfigure");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("attach resolves");
    let blades = permanent(&engine, "blades");
    assert_eq!(blades.attached_to.as_deref(), Some("bear"));
    assert!(
        !engine.permanent_is_creature(blades),
        "CR 702.151b: no longer a creature while attached"
    );

    // Unattach: a creature again.
    add_two_mana(&mut engine);
    let unattach = reconfigure_actions(&engine)
        .into_iter()
        .find(|action| action.targets.is_empty())
        .expect("the unattach ability is legal while attached");
    engine
        .apply_priority_action(&unattach, &mut EmeritusDecisionProvider)
        .expect("activate unattach");
    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("unattach resolves");
    let blades = permanent(&engine, "blades");
    assert!(blades.attached_to.is_none());
    assert!(engine.permanent_is_creature(blades));
}
