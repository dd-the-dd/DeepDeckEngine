//! Life-gain replacement frame (CR 614.1a, 119.10 a player gains life).
//!
//! ```text
//! Frame   := "if" LifeClause[would, player scope, no amount] ","
//!            LifeClause[effect player, event quantity] "instead" "."
//! ```
//!
//! Both halves are the shared life clause (`clause.rs`): the event is its
//! `would` modality over a player scope (`you`, `an opponent`, `a player`),
//! the replacing action the same lemma with the modified quantity. The
//! replacing subject must restate the event's player: `you` for `you`,
//! `that player` (the event player) otherwise.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::{
    ClauseAction, ClauseAmount, ClauseAmountPolicy, ClauseFrame, ClauseModality, ClauseSubject,
    ClauseSubjectPolicy, ClauseTense, EffectSubjectContext, EventActorScope, NumericExpression,
    PlayerEffectSubject, TargetPlayerLifeAction, VerbLemma, recognize_clause,
};
use super::{ClauseArguments, Voice};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum LifeGainPlayer {
    You,
    Opponent,
    AnyPlayer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LifeGainReplacementNode {
    pub player: LifeGainPlayer,
    pub amount: NumericExpression,
}

pub(in crate::oracle::canonical) enum LifeGainReplacementRecognition {
    Parsed(LifeGainReplacementNode),
    NotClaimed,
    Failed(MachineFailure),
}

/// The only lemma this replacement interprets.
const GAIN: &[VerbLemma] = &[VerbLemma::Gain];

/// `would gain life` after a player subject commits this frame.
pub(in crate::oracle::canonical) fn try_recognize_life_gain_replacement(
    cursor: &mut OracleTokenCursor<'_>,
) -> LifeGainReplacementRecognition {
    let checkpoint = cursor.checkpoint();
    let mut committed = false;
    match recognize_life_gain_replacement(cursor, &mut committed) {
        Ok(node) => LifeGainReplacementRecognition::Parsed(node),
        Err(failure) if committed => LifeGainReplacementRecognition::Failed(failure),
        Err(_) => {
            cursor.rollback(checkpoint);
            LifeGainReplacementRecognition::NotClaimed
        }
    }
}

fn recognize_life_gain_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    committed: &mut bool,
) -> MachineResult<LifeGainReplacementNode> {
    cursor.enter("replacement.life-gain.envelope");
    cursor.consume("if")?;
    cursor.enter("replacement.life-gain.event");
    let event_frame = ClauseFrame {
        subject: ClauseSubjectPolicy::PlayerScope,
        modality: ClauseModality::Would,
        tense: ClauseTense::Present,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Absent)],
        lemmas: GAIN,
    };
    let event = recognize_clause(cursor, &event_frame);
    // A claimed `would gain` commits the frame even when the clause failed.
    *committed = !matches!(event, super::ClauseRecognition::NotClaimed);
    let event = event
        .into_result()?
        .ok_or_else(|| cursor.fail("`PLAYER would gain life`"))?;
    let player = match event.subject {
        ClauseSubject::Scope {
            actor,
            one_or_more: false,
        } => match actor {
            EventActorScope::Controller => LifeGainPlayer::You,
            EventActorScope::Opponent => LifeGainPlayer::Opponent,
            EventActorScope::AnyPlayer => LifeGainPlayer::AnyPlayer,
        },
        _ => return Err(cursor.fail("`you`, `an opponent`, or `a player` as the life-gain player")),
    };
    cursor.enter("replacement.life-gain.separator");
    cursor.consume(",")?;
    cursor.enter("replacement.life-gain.subject");
    let replacing_frame = ClauseFrame {
        subject: ClauseSubjectPolicy::EffectPlayer(EffectSubjectContext {
            event_player: true,
            ..EffectSubjectContext::default()
        }),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::EventQuantity)],
        lemmas: GAIN,
    };
    // The clause head (subject and verb, amount slot not read) is checked
    // against the event's player before the modified quantity.
    let head = recognize_clause(
        &mut cursor.clone(),
        &ClauseFrame {
            voice: Voice::Active,
            arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Gapped)],
            ..replacing_frame.clone()
        },
    );
    let expected = match player {
        LifeGainPlayer::You => PlayerEffectSubject::Controller,
        LifeGainPlayer::Opponent | LifeGainPlayer::AnyPlayer => PlayerEffectSubject::EventPlayer,
    };
    cursor.require(
        matches!(&head, super::ClauseRecognition::Recognized(clause)
            if clause.subject == ClauseSubject::Player(expected.clone())),
        "`you gain` after `if you`, otherwise `that player gains`",
    )?;
    let replacing = recognize_clause(cursor, &replacing_frame)
        .into_result()?
        .ok_or_else(|| cursor.fail("the replacing life gain of the same player"))?;
    let ClauseAction::LifeTotalChange {
        direction: TargetPlayerLifeAction::Gain,
        amount: ClauseAmount::Expression(amount),
    } = replacing.action
    else {
        return Err(cursor.fail("a replacing life-gain amount"));
    };
    cursor.enter("replacement.life-gain.instead");
    cursor.consume("instead")?;
    cursor.consume(".")?;
    cursor.enter("replacement.life-gain.complete");
    cursor.finish()?;
    Ok(LifeGainReplacementNode { player, amount })
}
