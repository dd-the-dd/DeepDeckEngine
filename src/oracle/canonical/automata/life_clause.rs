//! Interpretations of the shared life clause (`gain` / `lose` with the
//! `LifeTotalChange` frame) for effect parents. Triggers, conditions,
//! replacements and numeric facts interpret the same clause in their own
//! modules; none of them tests a life verb word.
//!
//! Effect lowering keeps the established IR per subject kind:
//!
//! | Clause subject                                  | Effect node            |
//! |-------------------------------------------------|------------------------|
//! | imperative, `you` (no inherited subject), `its controller` reference | `GainLife` / `LoseLife` |
//! | `target player` (declared), `that player` (bound), finite verb | `TargetPlayerLife` |
//! | any other player subject, inherited subject, causative performer | `PlayerAction(Life)` |
//! | a player set or coordinated subjects (supplied by their parent) | the parent's list item |
use super::super::{MachineResult, OracleTokenCursor};
use super::clause::{
    ClauseAction, ClauseAmount, ClauseAmountPolicy, ClauseFrame, ClauseModality, ClauseSubject,
    ClauseSubjectPolicy, ClauseTense, EffectSubjectContext, clause_starts, recognize_clause,
};
use super::source_sacrifice::atomic_action::player_subject::ObjectSubjectAntecedent;
use super::verb_lexicon::{VerbAgreement, VerbLemma};
use super::{
    EffectRecognitionContext, GainLifeEffectNode, LocalEffectNode, LocalPlayerReference,
    LoseLifeEffectNode, NumericExpression, PlayerActionNode, PlayerBinding, PlayerEffectSubject,
    PlayerPredicate, PlayerTargetBinding, SingularPlayerAction, SubjectVerbAgreement,
    TargetBinding, TargetCardinality, TargetPlayerLifeAction, TargetPlayerLifeEffectNode,
    TargetSelector,
};

/// The lemmas whose argument frame is a life-total change.
pub(in crate::oracle::canonical) const LIFE_LEMMAS: &[VerbLemma] =
    &[VerbLemma::Gain, VerbLemma::Lose];

/// The life action an effect performs: direction and amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct LifeActionNode {
    pub action: TargetPlayerLifeAction,
    pub amount: NumericExpression,
}

fn life_action(clause_action: ClauseAction) -> Option<(TargetPlayerLifeAction, ClauseAmount)> {
    match clause_action {
        ClauseAction::LifeTotalChange { direction, amount } => Some((direction, amount)),
    }
}

fn supplied_frame(
    agreement: VerbAgreement,
    antecedent: Option<TargetBinding>,
) -> ClauseFrame<'static> {
    ClauseFrame {
        subject: ClauseSubjectPolicy::Supplied(agreement),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        amount: ClauseAmountPolicy::Expression { antecedent },
        lemmas: LIFE_LEMMAS,
    }
}

/// True when a life clause of the subject the parent supplied starts here.
pub(in crate::oracle::canonical) fn supplied_life_action_starts(
    cursor: &OracleTokenCursor<'_>,
    agreement: VerbAgreement,
) -> bool {
    clause_starts(cursor, &supplied_frame(agreement, None))
}

/// The life action of a subject the parent already read (player sets,
/// coordinated subjects). `Ok(None)` makes no claim.
pub(in crate::oracle::canonical) fn recognize_supplied_life_action(
    cursor: &mut OracleTokenCursor<'_>,
    agreement: VerbAgreement,
    antecedent: Option<TargetBinding>,
) -> MachineResult<Option<LifeActionNode>> {
    let Some(clause) =
        recognize_clause(cursor, &supplied_frame(agreement, antecedent)).into_result()?
    else {
        return Ok(None);
    };
    let Some((action, ClauseAmount::Expression(amount))) = life_action(clause.action) else {
        return Err(cursor.fail("a life amount"));
    };
    Ok(Some(LifeActionNode { action, amount }))
}

/// Bindings an effect parent proved before the clause.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct LifeEffectBindings<'a> {
    /// The binding a new `target player` declaration takes.
    pub declared_target: Option<PlayerBinding>,
    /// The earlier player binding `that player` names.
    pub player_antecedent: Option<PlayerBinding>,
    /// The previous clause's player subject.
    pub inherited: Option<&'a PlayerEffectSubject>,
    /// The singular object a possessive subject relates to.
    pub object_subject: Option<ObjectSubjectAntecedent>,
    /// The object binding for `its controller` and the amount's anaphora.
    pub object_antecedent: Option<TargetBinding>,
}

fn is_declared_any_player(target: &super::TargetListNode) -> bool {
    matches!(
        target.slots.as_slice(),
        [slot] if slot.selector == TargetSelector::Player(PlayerPredicate::Any)
            && slot.cardinality == TargetCardinality::EXACTLY_ONE
            && !slot.exclude_source
    )
}

fn controller_node(
    player: LocalPlayerReference,
    action: TargetPlayerLifeAction,
    amount: NumericExpression,
) -> LocalEffectNode {
    match action {
        TargetPlayerLifeAction::Gain => {
            LocalEffectNode::GainLife(GainLifeEffectNode { player, amount })
        }
        TargetPlayerLifeAction::Lose => {
            LocalEffectNode::LoseLife(LoseLifeEffectNode { player, amount })
        }
    }
}

/// `[SUBJECT] gain(s)|lose(s) AMOUNT life` as one effect. `Ok(None)` makes
/// no claim and restores the cursor.
pub(in crate::oracle::canonical) fn try_recognize_life_effect_clause(
    cursor: &mut OracleTokenCursor<'_>,
    bindings: LifeEffectBindings<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<LocalEffectNode>> {
    let causative = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
    let has_inherited = bindings.inherited.is_some();
    let frame = ClauseFrame {
        subject: ClauseSubjectPolicy::EffectPlayer(EffectSubjectContext {
            declared_target: bindings.declared_target,
            player_antecedent: bindings.player_antecedent,
            inherited: bindings.inherited,
            object_subject: bindings.object_subject,
            object_antecedent: bindings.object_antecedent,
            event_player: context.event_player,
            defending_player: context.defending_player,
            causative,
        }),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        amount: ClauseAmountPolicy::Expression {
            antecedent: bindings.object_antecedent,
        },
        lemmas: LIFE_LEMMAS,
    };
    let Some(clause) = recognize_clause(cursor, &frame).into_result()? else {
        return Ok(None);
    };
    cursor.enter("effect.life.interpretation");
    let Some((action, ClauseAmount::Expression(amount))) = life_action(clause.action) else {
        return Err(cursor.fail("a life amount"));
    };
    let player_action = |player: PlayerEffectSubject, amount: NumericExpression| {
        LocalEffectNode::PlayerAction(PlayerActionNode {
            player,
            action: SingularPlayerAction::Life(LifeActionNode { action, amount }),
        })
    };
    Ok(Some(match clause.subject {
        ClauseSubject::Imperative => {
            controller_node(LocalPlayerReference::Controller, action, amount)
        }
        ClauseSubject::PlayerReference(player) => controller_node(player, action, amount),
        ClauseSubject::Inherited(player) => player_action(player, amount),
        ClauseSubject::Player(PlayerEffectSubject::Controller) if !causative && !has_inherited => {
            controller_node(LocalPlayerReference::Controller, action, amount)
        }
        ClauseSubject::Player(PlayerEffectSubject::DeclaredTarget { target, binding }) => {
            cursor.require(
                bindings.declared_target.is_some(),
                "a player-target binding supplied by the effect sequence",
            )?;
            if !causative && is_declared_any_player(&target) {
                LocalEffectNode::TargetPlayerLife(TargetPlayerLifeEffectNode {
                    player: PlayerTargetBinding {
                        effect_index: binding.0,
                    },
                    declares_target: true,
                    action,
                    amount,
                })
            } else {
                player_action(
                    PlayerEffectSubject::DeclaredTarget { target, binding },
                    amount,
                )
            }
        }
        ClauseSubject::Player(PlayerEffectSubject::Bound(binding)) if !causative => {
            LocalEffectNode::TargetPlayerLife(TargetPlayerLifeEffectNode {
                player: PlayerTargetBinding {
                    effect_index: binding.0,
                },
                declares_target: false,
                action,
                amount,
            })
        }
        ClauseSubject::Player(player) => player_action(player, amount),
        ClauseSubject::Supplied | ClauseSubject::Scope { .. } => {
            return Err(cursor.fail("an effect player subject"));
        }
    }))
}

/// A life clause required at the cursor, for parents outside an effect
/// sequence (no player declarations, no inherited subject).
pub(in crate::oracle::canonical) fn recognize_life_effect(
    cursor: &mut OracleTokenCursor<'_>,
    object_antecedent: Option<TargetBinding>,
) -> MachineResult<LocalEffectNode> {
    cursor.enter("effect.life.required");
    let bindings = LifeEffectBindings {
        object_antecedent,
        ..LifeEffectBindings::default()
    };
    try_recognize_life_effect_clause(cursor, bindings, &EffectRecognitionContext::default())?
        .ok_or_else(|| {
            cursor.fail("a life clause (`gain`, `gains`, `lose`, or `loses` AMOUNT `life`)")
        })
}

#[cfg(test)]
#[path = "life_clause_tests.rs"]
mod tests;
