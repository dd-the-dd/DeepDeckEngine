use super::*;
fn engine() -> GameEngine {
    let card: CardDefinition =
        serde_json::from_value(json!({"id":"filler","name":"Filler","typeLine":"Land","rules":[]}))
            .unwrap();
    GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("P{i}"),
                    starting_life: 20,
                    cards: vec![card.clone(); 4],
                })
                .collect(),
        },
        31517,
    )
    .unwrap()
}
#[test]
fn ability_actor_physical_controller_and_owner_are_distinct_player_roles() {
    let e = engine();
    let d: CardDefinition = serde_json::from_value(
        json!({"id":"source","name":"Source","typeLine":"Artifact","rules":[]}),
    )
    .unwrap();
    let mut source = card_instance("p0", &d, 800);
    source.controller = "p1".into();
    let context = PlayerQueryContext::source("p0", Some(&source));
    assert_eq!(
        context
            .resolve_ref(&e, &json!({"kind":"controllerOf","object":{"kind":"self"}}))
            .unwrap(),
        "p0"
    );
    assert_eq!(
        context
            .resolve_ref(
                &e,
                &json!({"kind":"controllerOfObject","object":{"kind":"self"}})
            )
            .unwrap(),
        "p1"
    );
    assert_eq!(
        context
            .resolve_ref(&e, &json!({"kind":"ownerOf","object":{"kind":"self"}}))
            .unwrap(),
        "p0"
    );
    assert_eq!(
        context
            .resolve_population(
                &e,
                &json!({"kind":"playersExcept","player":{"kind":"abilityController"}})
            )
            .unwrap(),
        vec!["p1"]
    );
}
#[test]
fn invalid_holder_shape_and_missing_producer_are_not_hidden_by_empty_zones() {
    let e = engine();
    let context = PlayerQueryContext::source("p0", None);
    let zone = json!({"kind":"graveyard","player":{"kind":"abilityController"},"players":{"kind":"eachPlayer"}});
    assert!(!zone_holders_supported(&zone));
    assert!(
        context
            .zone_holders(&e, &zone)
            .unwrap_err()
            .message
            .contains("mutually exclusive")
    );
    assert!(
        context
            .resolve_ref(&e, &json!({"kind":"boundValue","id":"missing"}))
            .unwrap_err()
            .message
            .contains("unresolved binding")
    );
    assert_eq!(
        context
            .zone_holders(
                &e,
                &json!({"kind":"graveyard","players":{"kind":"eachPlayer"}})
            )
            .unwrap(),
        vec!["p0", "p1"]
    );
}
#[test]
fn direct_binding_requires_player_even_when_object_relationship_would_have_a_controller() {
    let e = engine();
    let source = card_instance("p0", &e.state.players[0].library[0].definition, 801);
    let frame = StackObject {
        id: "scope".into(),
        controller: "p0".into(),
        card: source,
        cant_be_countered: false,
        exile_on_leave_stack: false,
        ability_kind: Some("activatedAbility".into()),
        ability_rule: None,
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
    };
    for binding in [
        RuntimeBinding::Objects(vec![]),
        RuntimeBinding::Target(TargetRef::Permanent {
            instance_id: "absent".into(),
        }),
    ] {
        let bindings = BTreeMap::from([("holder".into(), binding)]);
        assert!(
            PlayerQueryContext::runtime(&frame, &bindings)
                .resolve_ref(&e, &json!({"kind":"boundValue","id":"holder"}))
                .unwrap_err()
                .message
                .contains("binding is not a player")
        );
    }
    let bindings = BTreeMap::from([("holder".into(), RuntimeBinding::Player("p1".into()))]);
    assert_eq!(
        PlayerQueryContext::runtime(&frame, &bindings)
            .resolve_ref(&e, &json!({"kind":"boundValue","id":"holder"}))
            .unwrap(),
        "p1"
    );
}
