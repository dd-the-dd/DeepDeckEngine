//! The effect head is `SUBJECT x LEMMA x MODALITY`, read from the verb
//! lexicon: every verb of a head is a lexicon lemma, the subject kinds are
//! the ones the dispatcher distinguishes, and nothing is consumed.
use super::*;

use crate::oracle::syntax::recognize_entities;

use super::super::super::OracleTokenCursor;
use super::super::{ClauseModality, VerbForm, VerbLemma};

fn head(text: &str) -> EffectHead {
    let tokens = recognize_entities(text);
    let cursor = OracleTokenCursor::new("test.effect-head", &tokens);
    let head = read_effect_head(&cursor);
    assert_eq!(cursor.position(), 0, "{text}: the head consumes nothing");
    head
}

#[test]
fn an_imperative_head_is_a_lexicon_lemma_in_its_base_form() {
    for (text, lemma) in [
        ("Destroy target creature.", VerbLemma::Destroy),
        ("Counter target spell.", VerbLemma::Counter),
        ("Scry 2.", VerbLemma::Scry),
        ("Prevent the next 3 damage.", VerbLemma::Prevent),
        ("Add {G}.", VerbLemma::Add),
        ("Exile target creature.", VerbLemma::Exile),
        ("Put a +1/+1 counter on target creature.", VerbLemma::Put),
    ] {
        let head = head(text);
        assert_eq!(head.subject, EffectSubjectKind::Imperative, "{text}");
        assert_eq!(head.modality, ClauseModality::Indicative, "{text}");
        assert_eq!(head.imperative(), Some(lemma), "{text}");
        assert!(head.base(lemma), "{text}");
        assert_eq!(head.verb_offset, 0, "{text}");
    }
}

#[test]
fn an_inflected_verb_or_a_plural_noun_is_never_an_imperative() {
    // `Counters` is the plural noun (or the third person), never the
    // imperative of `counter`.
    let counters = head("Counters can't be put on it.");
    assert_ne!(counters.subject, EffectSubjectKind::Imperative);
    assert_eq!(counters.imperative(), None);
}

#[test]
fn the_controller_subject_and_its_modality_place_the_verb() {
    let indicative = head("You draw two cards.");
    assert_eq!(indicative.subject, EffectSubjectKind::Controller);
    assert_eq!(indicative.modality, ClauseModality::Indicative);
    assert_eq!(indicative.verb_offset, 1);
    assert!(indicative.base(VerbLemma::Draw));

    let permission = head("You may search your library for a card.");
    assert_eq!(permission.subject, EffectSubjectKind::Controller);
    assert_eq!(permission.modality, ClauseModality::May);
    assert_eq!(permission.verb_offset, 2);
    assert!(permission.base(VerbLemma::Search));
}

#[test]
fn possessed_and_object_related_players_are_distinct_subjects() {
    let possessed = head("That player may draw a card.");
    assert_eq!(possessed.subject, EffectSubjectKind::Possessed);
    assert_eq!(possessed.modality, ClauseModality::May);
    assert_eq!(possessed.verb_offset, 3);
    assert!(possessed.base(VerbLemma::Draw));

    let related = head("Its controller creates a 1/1 white Spirit creature token.");
    assert_eq!(related.subject, EffectSubjectKind::ObjectRelatedPlayer);
    assert_eq!(related.verb_offset, 2);
    assert!(related.is(VerbLemma::Create, VerbForm::ThirdPersonSingular));
}

#[test]
fn noun_phrase_subjects_carry_no_lemma() {
    for (text, subject) in [
        (
            "Target player mills two cards.",
            EffectSubjectKind::Declared,
        ),
        (
            "Up to two target creatures get +1/+1 until end of turn.",
            EffectSubjectKind::Declared,
        ),
        ("Each opponent loses 2 life.", EffectSubjectKind::Quantified),
        ("It deals 2 damage to any target.", EffectSubjectKind::Other),
    ] {
        let head = head(text);
        assert_eq!(head.subject, subject, "{text}");
        assert_eq!(head.lemma, None, "{text}");
    }
}

#[test]
fn a_preposed_color_iteration_is_the_mana_output_head() {
    let head = head("For each color, add one mana of that color.");
    assert_eq!(head.subject, EffectSubjectKind::ColorIteration);
    assert_eq!(head.lemma, Some(VerbLemma::Add));
}

#[test]
fn operand_determiners_are_one_closed_table() {
    let tokens = recognize_entities("destroy all those that it target creature");
    let cursor = OracleTokenCursor::new("test.effect-head", &tokens);
    assert_eq!(operand_determiner(&cursor, 1), OperandDeterminer::Universal);
    assert_eq!(operand_determiner(&cursor, 2), OperandDeterminer::Those);
    assert_eq!(operand_determiner(&cursor, 3), OperandDeterminer::That);
    assert_eq!(operand_determiner(&cursor, 4), OperandDeterminer::It);
    assert_eq!(operand_determiner(&cursor, 5), OperandDeterminer::Target);
    assert_eq!(operand_determiner(&cursor, 6), OperandDeterminer::Other);
    assert_eq!(operand_determiner(&cursor, 40), OperandDeterminer::Absent);
}
