use super::super::super::OracleTokenCursor;
use super::super::{
    EffectRecognitionContext, EffectSequenceNode, LocalEffectNode, PlayerEffectSubject,
    PlayerProhibition, SingularPlayerAction, SkipTurnPartNode, SkippedTurnPart,
    recognize_effect_sequence_with_context,
};
use super::{AcquiredDesignation, PlayerDesignationAction};
use crate::oracle::syntax::recognize_entities;

fn parse(text: &str) -> Result<EffectSequenceNode, super::super::super::MachineFailure> {
    let tokens = recognize_entities(text);
    let mut cursor = OracleTokenCursor::new("player-designation-unit", &tokens);
    recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default(),
    )
}

fn player_action(effect: &LocalEffectNode) -> (&PlayerEffectSubject, &SingularPlayerAction) {
    let LocalEffectNode::PlayerAction(action) = effect else {
        panic!("a player action, got {effect:?}")
    };
    (&action.player, &action.action)
}

#[test]
fn the_ring_tempts_the_controller_after_another_sentence() {
    // Claim the Precious: the Ring is the subject, `you` the tempted object.
    let sequence = parse("Destroy target creature. The Ring tempts you.").unwrap();
    let (player, action) = player_action(&sequence.effects[1]);
    assert_eq!(*player, PlayerEffectSubject::Controller);
    assert_eq!(
        *action,
        SingularPlayerAction::Designation(PlayerDesignationAction::RingTemptation)
    );
    // An unused combination: a draw then the temptation joined by `then`.
    let sequence = parse("Draw a card, then the Ring tempts you.").unwrap();
    assert_eq!(sequence.effects.len(), 2);
    // Only `you` is a tempted player; another object fails explicitly.
    let failure = parse("The Ring tempts target player.").unwrap_err();
    assert_eq!(failure.state, "player-designation.ring.object");
}

#[test]
fn designations_are_registry_nouns_of_their_own_lemma() {
    let sequence = parse("You become the monarch.").unwrap();
    let (player, action) = player_action(&sequence.effects[0]);
    assert_eq!(*player, PlayerEffectSubject::Controller);
    assert_eq!(
        *action,
        SingularPlayerAction::Designation(PlayerDesignationAction::Acquire(
            AcquiredDesignation::Monarch
        ))
    );
    let sequence = parse("Target player becomes the monarch.").unwrap();
    assert!(matches!(
        player_action(&sequence.effects[0]).0,
        PlayerEffectSubject::DeclaredTarget { .. }
    ));
    let sequence = parse("You take the initiative and gain 3 life.").unwrap();
    assert_eq!(
        *player_action(&sequence.effects[0]).1,
        SingularPlayerAction::Designation(PlayerDesignationAction::Acquire(
            AcquiredDesignation::Initiative
        ))
    );
    // Near misses: the registry pairs each noun with one lemma.
    assert!(parse("You take the monarch.").is_err());
    assert!(parse("You become the initiative.").is_err());
}

#[test]
fn skip_reads_the_acting_players_next_turn_part() {
    for (text, part, count, this_turn) in [
        ("You skip your next turn.", SkippedTurnPart::Turn, 1, false),
        (
            "You skip your next two turns.",
            SkippedTurnPart::Turn,
            2,
            false,
        ),
        (
            "Skip your next untap step.",
            SkippedTurnPart::UntapStep,
            1,
            false,
        ),
        (
            "Target player skips their next draw step.",
            SkippedTurnPart::DrawStep,
            1,
            false,
        ),
        (
            "Target player skips their next combat phase this turn.",
            SkippedTurnPart::CombatPhase,
            1,
            true,
        ),
    ] {
        let sequence = parse(text).unwrap_or_else(|failure| panic!("{text}: {failure:?}"));
        assert_eq!(
            *player_action(&sequence.effects[0]).1,
            SingularPlayerAction::Skip(SkipTurnPartNode {
                part,
                count,
                this_turn,
            }),
            "{text}"
        );
    }
    // A skip without a turn part, or a static `skip your draw step`
    // without `next`, is not a one-shot skip.
    assert!(parse("You skip your next.").is_err());
    assert!(parse("Skip your draw step.").is_err());
    assert!(parse("You skip your next two untap steps.").is_err());
    // The possessive agrees with the acting player.
    assert!(parse("Target player skips your next turn.").is_err());
}

#[test]
fn a_player_prohibition_is_the_acting_players_until_end_of_turn() {
    let sequence = parse("Target player can't cast spells this turn.").unwrap();
    let (player, action) = player_action(&sequence.effects[0]);
    assert!(matches!(player, PlayerEffectSubject::DeclaredTarget { .. }));
    assert_eq!(
        *action,
        SingularPlayerAction::Prohibition(PlayerProhibition::CastSpells)
    );
    // Prohibitions without a runtime rule fail explicitly.
    assert!(parse("Target player can't play lands this turn.").is_err());
    assert!(parse("Target player can't cast creature spells this turn.").is_err());
    assert!(parse("Target player can't cast spells.").is_err());
}
