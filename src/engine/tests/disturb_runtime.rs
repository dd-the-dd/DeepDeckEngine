//! Real Oracle text -> `disturb` keyword IR -> engine behavior (CR
//! 702.146a-b, 712.8): a disturb card is cast from its owner's graveyard
//! with its back face up for its disturb cost, resolves with that face up,
//! and keeps its front face's mana value (CR 712.8e).
use super::*;

const DISTURB_REMINDER: &str =
    "(You may cast this card from your graveyard transformed for its disturb cost.)";

fn parsed_rules(name: &str, type_line: &str, mana_cost: &str, oracle: &str) -> Vec<Value> {
    let request = OracleCardParseRequest {
        card_name: name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: Some(mana_cost.to_string()),
        oracle_text: Some(oracle.to_string()),
        layout: Some("normal".to_string()),
        faces: Vec::new(),
    };
    let parsed = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card(request))
        .expect("parser thread starts")
        .join()
        .expect("parser thread completes");
    assert_eq!(
        parsed.status, "canonical",
        "{oracle}: {:#?}",
        parsed.abilities
    );
    parsed
        .abilities
        .into_iter()
        .filter_map(|ability| ability.rule)
        .inspect(|rule| assert!(rule_is_executable(rule), "{rule:#}"))
        .collect()
}

/// Lunarch Veteran // Luminous Phantom as the card model assembles a
/// transforming card: each face's rules tagged with its face index, and the
/// transform-faces marker.
fn lunarch_veteran(instance_id: &str) -> CardInstance {
    let front_text = format!(
        "Whenever another creature you control enters, you gain 1 life.\nDisturb {{1}}{{W}} {DISTURB_REMINDER}"
    );
    let back_text = "Flying\nWhenever another creature you control leaves the battlefield, you gain 1 life.\nIf Luminous Phantom would be put into a graveyard from anywhere, exile it instead.";
    let tag = |rules: Vec<Value>, face_index: u64| {
        rules
            .into_iter()
            .map(move |mut rule| {
                rule["activeFaceIndex"] = Value::from(face_index);
                rule
            })
            .collect::<Vec<_>>()
    };
    let mut rules = tag(
        parsed_rules(
            "Lunarch Veteran",
            "Creature — Human Cleric",
            "{W}",
            &front_text,
        ),
        0,
    );
    rules.extend(tag(
        parsed_rules(
            "Luminous Phantom",
            "Creature — Spirit Cleric",
            "",
            back_text,
        ),
        1,
    ));
    rules.push(json!({
        "kind": "rulesMarker",
        "text": "Transformable card faces.",
        "transformFaces": [
            {
                "id": "lunarch-veteran",
                "name": "Lunarch Veteran",
                "typeLine": "Creature — Human Cleric",
                "manaCost": "{W}",
                "oracleText": front_text,
                "power": "1",
                "toughness": "1",
                "loyalty": null,
            },
            {
                "id": "luminous-phantom",
                "name": "Luminous Phantom",
                "typeLine": "Creature — Spirit Cleric",
                "manaCost": "",
                "oracleText": back_text,
                "power": "1",
                "toughness": "1",
                "loyalty": null,
            },
        ],
    }));
    let mut definition = test_definition(instance_id, "Creature — Human Cleric");
    definition.name = "Lunarch Veteran".to_string();
    definition.mana_cost = "{W}".to_string();
    definition.power = Some("1".to_string());
    definition.toughness = Some("1".to_string());
    definition.rules = rules;
    let mut instance = card_instance("player-0", &definition, 0);
    instance.instance_id = instance_id.to_string();
    instance
}

fn pool(engine: &mut GameEngine, symbols: &[&str]) {
    engine.state.players[0].mana_pool = symbols
        .iter()
        .map(|symbol| FloatingMana {
            symbol: (*symbol).to_string(),
            spend_restriction: None,
        })
        .collect();
}

fn disturb_casts(engine: &GameEngine) -> Vec<LegalAction> {
    engine
        .legal_priority_actions(0)
        .into_iter()
        .filter(|action| {
            action.kind == ActionKind::CastSpell
                && action.card_instance_id.as_deref() == Some("veteran")
        })
        .collect()
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn a_disturb_card_is_cast_transformed_from_the_graveyard_and_resolves_back_face_up() {
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    pool(&mut engine, &["W", "W"]);
    engine.state.players[0].graveyard = vec![lunarch_veteran("veteran")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");

    let casts = disturb_casts(&engine);
    assert!(
        !casts.is_empty()
            && casts
                .iter()
                .all(|action| action.decisions.get("castTransformed") == Some(&json!(true))),
        "only the disturb route: {casts:#?}"
    );
    let cast = casts.into_iter().next().expect("one cast");
    assert_eq!(
        cast.decisions.get("alternativeCostMode"),
        Some(&json!("disturb"))
    );
    engine
        .apply_priority_action(&cast, &mut EmeritusDecisionProvider)
        .expect("cast with disturb");
    assert!(
        engine.state.players[0].mana_pool.is_empty(),
        "paid {{1}}{{W}}"
    );
    let spell = engine
        .state
        .stack
        .iter()
        .find(|object| object.card.instance_id == "veteran")
        .expect("the spell is on the stack");
    assert_eq!(spell.card.definition.name, "Luminous Phantom");
    assert_eq!(
        mana_value(&spell.card.definition),
        1,
        "the front face's mana value (CR 712.8e)"
    );

    engine
        .resolve_top_stack(&mut EmeritusDecisionProvider)
        .expect("the spell resolves");
    let permanent = engine.state.players[0]
        .battlefield
        .iter()
        .find(|permanent| permanent.instance_id == "veteran")
        .expect("it enters the battlefield");
    assert_eq!(permanent.definition.name, "Luminous Phantom");
    assert_eq!(permanent.flags.get("transformed"), Some(&true));
    assert!(has_keyword(&permanent.definition, "flying"));
}

#[cfg(any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3"))]
#[test]
fn disturb_needs_its_cost_and_never_casts_from_the_hand() {
    // One white mana pays neither {1}{W} nor anything from the graveyard.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    pool(&mut engine, &["W"]);
    engine.state.players[0].graveyard = vec![lunarch_veteran("veteran")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    assert!(disturb_casts(&engine).is_empty());

    // From the hand, the front face is cast for {W}, never transformed.
    let mut engine = test_engine(2);
    engine.state.step = GameStep::PrecombatMain;
    engine.state.active_player = 0;
    pool(&mut engine, &["W", "W"]);
    engine.state.players[0].hand = vec![lunarch_veteran("veteran")];
    engine
        .admit_explicit_test_fixture_objects()
        .expect("fixture objects are admitted");
    let casts = disturb_casts(&engine);
    assert!(!casts.is_empty());
    assert!(
        casts
            .iter()
            .all(|action| action.decisions.get("castTransformed").is_none())
    );
}
