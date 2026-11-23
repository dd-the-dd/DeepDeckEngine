use super::super::{NumericAtom, ObjectCardType, ObjectControllerScope, ObjectPredicate};
use super::*;
use crate::oracle::syntax::recognize_entities;

const BATTLEFIELD_OR_STACK: &[ObjectKind] = &[ObjectKind::Permanent, ObjectKind::Spell];

fn read_phrase(text: &str, domains: &[ObjectKind]) -> (ObjectNounPhrase, usize, usize) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("object-noun-phrase", &tokens);
    let phrase = recognize_object_noun_phrase(
        &mut cursor,
        ObjectNounPhraseExpectation::in_domains(domains),
    )
    .unwrap_or_else(|failure| panic!("{text}: {failure}"));
    (phrase, cursor.position(), tokens.len())
}

#[test]
fn one_noun_phrase_reads_every_determiner_family() {
    let exactly_one_target = Some(ObjectDeterminer::Target {
        cardinality: ExpressedTargetCardinality::EXACTLY_ONE,
        exclude_source: false,
    });
    for (text, targeted, universal) in [
        ("target creature", true, false),
        ("two target creatures", true, false),
        ("up to X target artifacts", true, false),
        ("another target creature", true, false),
        ("a creature you control", false, false),
        ("up to two creatures", false, false),
        ("each creature you control", false, true),
        ("all artifacts and creatures", false, true),
        ("creatures you control", false, true),
    ] {
        let (phrase, end, len) = read_phrase(text, BATTLEFIELD_OR_STACK);
        assert_eq!(end, len, "{text}: the whole phrase");
        assert_eq!(phrase.is_targeted(), targeted, "{text}");
        assert_eq!(phrase.is_universal(), universal, "{text}");
        assert_eq!(phrase.domain, Some(ObjectKind::Permanent), "{text}");
    }
    let (single, _, _) = read_phrase("target creature", BATTLEFIELD_OR_STACK);
    assert_eq!(single.determiner, exactly_one_target);
    let (bounded, _, _) = read_phrase("up to X target artifacts", BATTLEFIELD_OR_STACK);
    assert_eq!(
        bounded.determiner,
        Some(ObjectDeterminer::Target {
            cardinality: ExpressedTargetCardinality {
                minimum: NumericAtom::Integer(0),
                maximum: NumericAtom::VariableX,
            },
            exclude_source: false,
        })
    );
}

#[test]
fn source_exclusion_is_read_once_whatever_the_determiner() {
    for text in [
        "another target creature",
        "two other target creatures",
        "another creature you control",
        "two other Elves you control",
        "each other creature you control",
        "other Elves you control",
    ] {
        let (phrase, end, len) = read_phrase(text, BATTLEFIELD_OR_STACK);
        assert_eq!(end, len, "{text}");
        assert!(phrase.exclude_source, "{text}");
    }
}

#[test]
fn domain_is_inferred_once_and_left_to_the_parent_to_require() {
    // CR 109.2a: a subtype head names a permanent.
    let (goblins, _, _) = read_phrase("Goblins you control", BATTLEFIELD_OR_STACK);
    assert_eq!(goblins.domain, Some(ObjectKind::Permanent));
    // Near misses: an explicit card or spell noun keeps its own domain.
    let (elf_card, _, _) = read_phrase("an Elf card", &[ObjectKind::Permanent, ObjectKind::Card]);
    assert_eq!(elf_card.domain, Some(ObjectKind::Card));
    let (spells, _, _) = read_phrase("spells you control", BATTLEFIELD_OR_STACK);
    assert_eq!(spells.domain, Some(ObjectKind::Spell));
    // A noun the permanent domain admits, related by control (CR 108.4).
    let (tokens, _, _) = read_phrase("tokens you control", BATTLEFIELD_OR_STACK);
    assert_eq!(tokens.domain, Some(ObjectKind::Permanent));
    let (bare_tokens, _, _) = read_phrase("each token", BATTLEFIELD_OR_STACK);
    assert_eq!(bare_tokens.domain, None);
    // A card noun is not in a battlefield-only expectation.
    let (card, _, _) = read_phrase("a creature card", &[ObjectKind::Permanent]);
    assert_eq!(card.domain, None);
}

#[test]
fn absent_determiner_consumes_nothing() {
    for text in ["creature you control", "Equipment", "target player"] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("object-determiner", &tokens);
        let determiner = recognize_object_determiner(&mut cursor).expect(text);
        if text == "target player" {
            // `target` is a determiner; the player noun is the parent's.
            assert!(determiner.is_some_and(|determiner| determiner.is_targeted()));
        } else {
            assert_eq!(determiner, None, "{text}");
            assert_eq!(cursor.position(), 0, "{text}");
        }
    }
}

#[test]
fn chosen_count_is_the_shared_numeric_expression() {
    let (phrase, _, _) = read_phrase("two other Elves you control", BATTLEFIELD_OR_STACK);
    let Some(ObjectDeterminer::Chosen {
        quantity: ChosenObjectQuantity::Exactly(count),
        another: false,
    }) = &phrase.determiner
    else {
        panic!("a chosen count");
    };
    assert_eq!(count, &NumericExpression::from(2));
    let tokens = recognize_entities("x");
    let mut cursor = OracleTokenCursor::new("object-noun-phrase-project", &tokens);
    let objects = phrase
        .query
        .clone()
        .into_object_query(&mut cursor)
        .expect("controlled Elves");
    assert_eq!(
        objects.controller,
        Some(ObjectControllerScope::AbilityController)
    );
    assert!(matches!(objects.predicate, ObjectPredicate::Subtype(_)));
    let (any_number, _, _) = read_phrase("any number of creature cards", &[ObjectKind::Card]);
    assert_eq!(
        any_number.determiner,
        Some(ObjectDeterminer::Chosen {
            quantity: ChosenObjectQuantity::AnyNumber,
            another: false,
        })
    );
    assert!(
        any_number
            .query
            .predicate()
            .guarantees_card_type(ObjectCardType::Creature)
    );
}
