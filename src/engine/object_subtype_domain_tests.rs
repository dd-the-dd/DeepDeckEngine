use super::{DefinitionView, is_creature_subtype, matches_object_predicate};
use crate::engine::*;
use serde_json::json;

fn definition(types: &str, changeling: bool) -> CardDefinition {
    serde_json::from_value(json!({"id":"subtype-domain", "name":"Subtype domain", "typeLine":types,
        "manaCost":"{0}","power":"2","toughness":"2",
        "rules":if changeling {vec![json!({"kind":"keywordAbility","ability":{"kind":"changeling"}})]} else {vec![]}})).unwrap()
}

#[test]
fn nominal_membership_is_exact_canonical_not_plural_or_unknown() {
    for name in ["Elf", "ELF", "Time Lord", "assembly-worker"] {
        assert!(is_creature_subtype(name), "{name}");
    }
    for name in [
        "Equipment",
        "Vehicle",
        "Arcane",
        "Aura",
        "Plains",
        "Elves",
        "FutureUnknownSubtype",
        "",
    ] {
        assert!(!is_creature_subtype(name), "{name}");
    }
}

#[test]
fn definition_changeling_grants_only_creature_types_even_on_kindred_spells() {
    let card = definition("Kindred Instant - Shapeshifter", true);
    for (name, expected) in [
        ("Elf", true),
        ("Time Lord", true),
        ("Equipment", false),
        ("Vehicle", false),
        ("Arcane", false),
        ("Plains", false),
        ("FutureUnknownSubtype", false),
    ] {
        assert_eq!(
            matches_object_predicate(
                DefinitionView::new(&card),
                &json!({"kind":"subtypeContains","value":name})
            ),
            expected,
            "{name}"
        );
    }
}

#[test]
fn printed_noncreature_subtype_remains_valid_independent_of_changeling() {
    for changeling in [false, true] {
        let card = definition("Artifact Creature - Equipment Shapeshifter", changeling);
        assert!(matches_object_predicate(
            DefinitionView::new(&card),
            &json!({"kind":"subtypeContains","value":"Equipment"})
        ));
        assert!(!matches_object_predicate(
            DefinitionView::new(&card),
            &json!({"kind":"subtypeContains","value":"Vehicle"})
        ));
    }
}

#[test]
fn effective_permanent_query_uses_same_subtype_domain_as_definition() {
    let filler = definition("Land", false);
    let mut engine = GameEngine::new(
        GameSetup {
            opening_hand_size: 0,
            starting_player: 0,
            players: (0..2)
                .map(|i| PlayerDeck {
                    id: format!("p{i}"),
                    name: format!("Player{i}"),
                    starting_life: 20,
                    cards: vec![filler.clone(); 4],
                })
                .collect(),
        },
        261115,
    )
    .unwrap();
    let source = definition("Artifact Creature - Equipment Shapeshifter", true);
    engine.state.players[0]
        .battlefield
        .push(card_instance("p0", &source, 900));
    engine.admit_explicit_test_fixture_objects().unwrap();
    let permanent = &engine.state.players[0].battlefield[0];
    for (name, expected) in [
        ("Elf", true),
        ("Equipment", true),
        ("Vehicle", false),
        ("Arcane", false),
        ("Plains", false),
    ] {
        assert_eq!(
            engine.permanent_matches_effective_filter(
                permanent,
                &json!({"kind":"subtypeContains","value":name})
            ),
            expected,
            "{name}"
        );
    }
}
