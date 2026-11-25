//! Effect coordination with the enclosing frame's scope: finite gapped
//! conjuncts, `or` alternatives (CR 608.2d), the elided library of a gapped
//! library position and the `Then,` connector variant.
use super::super::super::OracleTokenCursor;
use super::super::{
    EffectConnector, EffectRecognitionContext, EffectSequenceNode, LocalEffectNode, VerbAgreement,
    VerbForm, VerbLemma, recognize_effect_sequence_with_context,
};
use crate::oracle::syntax::recognize_entities;

fn parse_sequence(text: &str) -> Result<EffectSequenceNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("coordination-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
}

#[test]
fn or_joins_two_alternatives_and_a_gapped_operand_reads_the_elided_verb() {
    // Catastrophe: the second operand is an argument frame of `destroy`.
    let sequence = parse_sequence("Destroy all lands or all creatures.").unwrap();
    assert_eq!(sequence.connectors, [EffectConnector::Or]);
    assert!(matches!(
        sequence.effects.as_slice(),
        [
            LocalEffectNode::DestroyCollection(_),
            LocalEffectNode::DestroyCollection(_)
        ]
    ));
    // Two complete imperatives are alternatives too (Plague Boiler's shape).
    let sequence =
        parse_sequence("Put a +1/+1 counter on target creature or remove a +1/+1 counter from it.")
            .unwrap();
    assert_eq!(sequence.connectors, [EffectConnector::Or]);
}

#[test]
fn a_restated_regeneration_amendment_covers_every_destroy_alternative() {
    let sequence = parse_sequence(
        "Destroy all lands or all creatures. Creatures destroyed this way can't be regenerated.",
    )
    .unwrap();
    assert!(sequence.effects.iter().all(|effect| matches!(
        effect,
        LocalEffectNode::DestroyCollection(collection) if collection.cannot_regenerate
    )));
}

#[test]
fn a_selector_union_stays_inside_its_operand() {
    let sequence = parse_sequence("Destroy target artifact or creature.").unwrap();
    assert_eq!(sequence.effects.len(), 1);
    assert!(sequence.connectors.is_empty());
}

#[test]
fn alternatives_of_an_ordinary_player_subject_have_no_chooser() {
    // The target player would choose; no written performer names them.
    assert!(parse_sequence("Target player draws a card or gains 3 life.").is_err());
}

#[test]
fn then_with_a_comma_opens_the_next_sentence() {
    let sequence = parse_sequence("Draw a card. Then, draw a card.").unwrap();
    assert_eq!(sequence.connectors, [EffectConnector::Then]);
    assert_eq!(sequence.effects.len(), 2);
}

#[test]
fn the_finite_verb_of_a_player_clause_is_its_subject_agreement_form() {
    let tokens = recognize_entities("Target player sacrifices an artifact");
    let cursor = OracleTokenCursor::new("finite-verb-unit", &tokens);
    assert_eq!(
        super::finite_clause_verb(&cursor, 0, tokens.len(), VerbAgreement::ThirdPersonSingular),
        Some((VerbLemma::Sacrifice, VerbForm::ThirdPersonSingular))
    );
    // An imperative clause has no finite verb in the subject's form.
    let tokens = recognize_entities("Sacrifice an artifact");
    let cursor = OracleTokenCursor::new("finite-verb-unit", &tokens);
    assert_eq!(
        super::finite_clause_verb(&cursor, 0, tokens.len(), VerbAgreement::ThirdPersonSingular),
        None
    );
}

#[test]
fn an_elided_library_position_restates_its_antecedent_library() {
    let antecedent = recognize_entities("put that card on top of your library");
    let conjunct = recognize_entities("and the rest on the bottom in any order.");
    let (view, inserted_at, inserted) =
        super::elided_library_tokens(&conjunct, 1, &antecedent).expect("an elided library");
    let words = view
        .iter()
        .map(|token| token.raw.as_str())
        .collect::<Vec<_>>();
    assert_eq!(inserted, 3);
    assert_eq!(
        &words[inserted_at - 1..inserted_at + inserted],
        ["bottom", "of", "your", "library"]
    );
    // An explicit library is never restated.
    let conjunct = recognize_entities("and the rest on the bottom of your library.");
    assert!(super::elided_library_tokens(&conjunct, 1, &antecedent).is_none());
}
