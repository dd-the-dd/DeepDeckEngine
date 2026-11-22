//! The clause reader: frames select subject policy, modality, tense and the
//! amount slot; a subject commits nothing until the verb and its argument
//! frame are claimed.
use super::super::super::OracleTokenCursor;
use super::super::life_clause::LIFE_LEMMAS;
use super::*;
use crate::oracle::syntax::recognize_entities;

fn read(text: &str, frame: &ClauseFrame<'_>) -> (ClauseRecognition, usize) {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("clause-test", &tokens);
    let result = recognize_clause(&mut cursor, frame);
    (result, cursor.position())
}

fn frame(
    subject: ClauseSubjectPolicy<'static>,
    modality: ClauseModality,
    tense: ClauseTense,
    amount: ClauseAmountPolicy,
) -> ClauseFrame<'static> {
    ClauseFrame {
        subject,
        modality,
        tense,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(amount)],
        lemmas: LIFE_LEMMAS,
    }
}

fn effect_player() -> ClauseSubjectPolicy<'static> {
    ClauseSubjectPolicy::EffectPlayer(EffectSubjectContext {
        declared_target: Some(PlayerBinding(0)),
        ..EffectSubjectContext::default()
    })
}

#[test]
fn past_history_reads_scope_subject_threshold_and_time_frame() {
    let history = frame(
        ClauseSubjectPolicy::PlayerScope,
        ClauseModality::Indicative,
        ClauseTense::PastThisTurn,
        ClauseAmountPolicy::Threshold,
    );
    let (ClauseRecognition::Recognized(clause), _) =
        read("you gained 3 or more life this turn", &history)
    else {
        panic!("history clause");
    };
    assert_eq!(
        clause.subject,
        ClauseSubject::Scope {
            actor: EventActorScope::Controller,
            one_or_more: false
        }
    );
    assert_eq!(clause.lemma, VerbLemma::Gain);
    assert_eq!(
        clause.action,
        ClauseAction::LifeTotalChange {
            direction: TargetPlayerLifeAction::Gain,
            amount: ClauseAmount::Count {
                value: 3,
                direction: ControlledCountDirection::OrMore
            }
        }
    );
    // The same frame reads another scope and lemma with no new code.
    let (ClauseRecognition::Recognized(clause), _) =
        read("an opponent lost life this turn", &history)
    else {
        panic!("opponent history clause");
    };
    assert_eq!(
        clause.action,
        ClauseAction::LifeTotalChange {
            direction: TargetPlayerLifeAction::Lose,
            amount: ClauseAmount::Unspecified
        }
    );
    // A present verb is claimed by the lemma and fails in the past frame.
    assert!(matches!(
        read("you gain life this turn", &history).0,
        ClauseRecognition::Failed(_)
    ));
}

#[test]
fn modality_reads_the_bare_verb_and_restricts_the_lemmas() {
    let replacement = ClauseFrame {
        lemmas: &[VerbLemma::Gain],
        ..frame(
            ClauseSubjectPolicy::PlayerScope,
            ClauseModality::Would,
            ClauseTense::Present,
            ClauseAmountPolicy::Absent,
        )
    };
    assert!(matches!(
        read("an opponent would gain life", &replacement).0,
        ClauseRecognition::Recognized(Clause {
            modality: ClauseModality::Would,
            subject: ClauseSubject::Scope {
                actor: EventActorScope::Opponent,
                ..
            },
            ..
        })
    ));
    assert!(matches!(
        read("an opponent would lose life", &replacement),
        (ClauseRecognition::NotClaimed, 0)
    ));
    assert!(matches!(
        read("an opponent would gains life", &replacement).0,
        ClauseRecognition::Failed(_)
    ));
}

#[test]
fn a_subject_commits_nothing_until_the_verb_and_frame_are_claimed() {
    let effect = frame(
        effect_player(),
        ClauseModality::Indicative,
        ClauseTense::Present,
        ClauseAmountPolicy::Expression { antecedent: None },
    );
    // `that player` without a binding: another verb leaves no trace.
    assert!(matches!(
        read("that player draws a card", &effect),
        (ClauseRecognition::NotClaimed, 0)
    ));
    // Same subject, a claimed life verb: the binding failure is the frame's.
    assert!(matches!(
        read("that player loses 2 life", &effect).0,
        ClauseRecognition::Failed(_)
    ));
    // The lemma without its argument frame is another verb sense.
    for other_sense in [
        "you gain control of target creature",
        "target player loses the game",
        "gain control of target artifact",
    ] {
        assert!(
            matches!(
                read(other_sense, &effect),
                (ClauseRecognition::NotClaimed, 0)
            ),
            "{other_sense}"
        );
    }
}

#[test]
fn agreement_is_derived_from_the_subject() {
    let effect = frame(
        effect_player(),
        ClauseModality::Indicative,
        ClauseTense::Present,
        ClauseAmountPolicy::Expression { antecedent: None },
    );
    assert!(matches!(
        read("target opponent loses 2 life", &effect).0,
        ClauseRecognition::Recognized(Clause {
            subject: ClauseSubject::Player(PlayerEffectSubject::DeclaredTarget { .. }),
            ..
        })
    ));
    assert!(matches!(
        read("gain 2 life", &effect).0,
        ClauseRecognition::Recognized(Clause {
            subject: ClauseSubject::Imperative,
            ..
        })
    ));
    for mismatch in [
        "target opponent lose 2 life",
        "you gains 2 life",
        "gains 2 life",
    ] {
        assert!(
            matches!(read(mismatch, &effect).0, ClauseRecognition::Failed(_)),
            "{mismatch}"
        );
    }
    let causative = frame(
        ClauseSubjectPolicy::EffectPlayer(EffectSubjectContext {
            declared_target: Some(PlayerBinding(0)),
            causative: true,
            ..EffectSubjectContext::default()
        }),
        ClauseModality::Indicative,
        ClauseTense::Present,
        ClauseAmountPolicy::Expression { antecedent: None },
    );
    assert!(matches!(
        read("target opponent lose 2 life", &causative).0,
        ClauseRecognition::Recognized(_)
    ));
    // A causative names its performer: no imperative reading.
    assert!(matches!(
        read("lose 2 life", &causative),
        (ClauseRecognition::NotClaimed, 0)
    ));
}

#[test]
fn the_frame_is_chosen_from_the_tokens_after_the_verb() {
    // A parent accepting the life frame and the (declared, unread) control
    // frame of the same lemma: each sentence selects its own frame.
    let frame = ClauseFrame {
        arguments: vec![
            ClauseArguments::Life(ClauseAmountPolicy::Expression { antecedent: None }),
            ClauseArguments::Unread(ArgumentFrame::Control),
        ],
        ..frame(
            effect_player(),
            ClauseModality::Indicative,
            ClauseTense::Present,
            ClauseAmountPolicy::Expression { antecedent: None },
        )
    };
    assert!(matches!(
        read("gain 3 life", &frame).0,
        ClauseRecognition::Recognized(Clause {
            action: ClauseAction::LifeTotalChange { .. },
            ..
        })
    ));
    // The control frame is claimed (not taken for life) and fails in its
    // own unread state.
    let (ClauseRecognition::Failed(failure), _) = read("gain control of target creature", &frame)
    else {
        panic!("control frame claimed");
    };
    assert_eq!(failure.state, "clause.arguments.unread");
    // A frame the parent does not accept is no claim at all.
    assert!(matches!(
        read("gains flying", &frame),
        (ClauseRecognition::NotClaimed, 0)
    ));
}
