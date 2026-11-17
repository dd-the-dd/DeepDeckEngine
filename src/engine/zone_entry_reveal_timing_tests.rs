use super::*;
use rand::RngCore;

#[test]
fn entry_observes_reveal_history_and_the_intact_library() {
    for operator in [">", "=="] {
        for include_library_condition in [false, true] {
            let mut engine = engine();
            let event_condition = json!({"kind":"compare","operator":operator,
                "left":{"kind":"countEventsThisTurn","event":"cardsRevealed"},"right":0});
            let condition = if include_library_condition {
                json!({"kind":"and","operands":[event_condition,
                    {"kind":"compare","operator":"==","right":2,
                     "left":{"kind":"countCards","zone":{"kind":"library",
                        "player":{"kind":"controllerOf","object":{"kind":"self"}}},"where":null}}
                ]})
            } else {
                event_condition
            };
            engine.state.players[0].battlefield[0].definition.rules[0]["condition"] = condition;
            assert!(rule_is_executable(
                &engine.state.players[0].battlefield[0].definition.rules[0]
            ));
            let a = card_instance("p0", &definition("entrant-a", "Creature", vec![]), 30);
            let b = card_instance("p0", &definition("entrant-b", "Creature", vec![]), 31);
            engine.state.players[0].library = vec![a.clone(), b.clone()];
            engine
                .admit_explicit_test_fixture_objects()
                .expect("fixture construction precedes references");
            let before_identity = engine.state.battlefield_incarnation_sequence;
            let denied = operator == ">";
            let mut expected_library = if denied {
                vec![b.clone(), a.clone()]
            } else {
                vec![]
            };
            let mut expected_rng = engine.rng.clone();
            expected_library.shuffle(&mut expected_rng);
            engine
                .reveal_creatures_to_battlefield("p0", 2, "source", &mut NoChoices)
                .unwrap();
            let reveals = engine
                .state
                .events
                .iter()
                .filter(|e| e.kind == "cardsRevealed")
                .collect::<Vec<_>>();
            assert_eq!(reveals.len(), 1);
            assert_eq!(
                reveals[0].detail["cardIds"],
                json!([b.instance_id, a.instance_id])
            );
            for card in [&a, &b] {
                assert_eq!(
                    engine.state.players[0]
                        .battlefield
                        .iter()
                        .any(|c| c.instance_id == card.instance_id),
                    !denied
                );
            }
            let ids = |cards: &[CardInstance]| {
                cards
                    .iter()
                    .map(|c| c.instance_id.clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                ids(&engine.state.players[0].library),
                ids(&expected_library)
            );
            assert_eq!(engine.rng.next_u64(), expected_rng.next_u64());
            if denied {
                assert_eq!(
                    engine.state.battlefield_incarnation_sequence,
                    before_identity
                );
            } else {
                let entrants = engine.state.players[0]
                    .battlefield
                    .iter()
                    .filter(|c| c.instance_id == a.instance_id || c.instance_id == b.instance_id)
                    .cloned()
                    .collect::<Vec<_>>();
                assert_eq!(ids(&entrants), vec![b.instance_id, a.instance_id]);
            }
        }
    }
}
