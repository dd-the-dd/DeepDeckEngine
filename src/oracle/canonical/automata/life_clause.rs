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
use super::{ClauseArguments, Voice};
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
        ClauseAction::DealDamage(_) | ClauseAction::Operand(_) => None,
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
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Expression {
            antecedent,
        })],
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

/// The possessor of a life total (CR 119.1): one player, or each player of
/// a population (`each player's life total`).
enum LifeTotalPossessor {
    Player(PlayerEffectSubject),
    Population(super::PlayerSetReferenceNode),
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The written player noun of a genitive (`player's`, `opponent’s`).
fn genitive_player_noun(raw: &str) -> Option<&'static str> {
    match super::performer::genitive_base(raw)?.as_str() {
        "player" => Some("player"),
        "opponent" => Some("opponent"),
        _ => None,
    }
}

/// The token length of a life-total possessor followed by `life total
/// becomes` (`your`, `target player's`, `that opponent's`, `each player's`).
/// Lexical lookahead only; the possessor readers prove the reference.
fn life_total_possessor_length(cursor: &OracleTokenCursor<'_>) -> Option<usize> {
    let length = if word_is(cursor, 0, "your") {
        1
    } else if ["target", "that", "each"]
        .iter()
        .any(|determiner| word_is(cursor, 0, determiner))
        && cursor
            .peek_ahead(1)
            .and_then(genitive_player_noun)
            .is_some()
    {
        2
    } else {
        return None;
    };
    (word_is(cursor, length, "life")
        && word_is(cursor, length + 1, "total")
        && word_is(cursor, length + 2, "becomes"))
    .then_some(length)
}

/// The possessor of `length` tokens. A genitive player reference is read by
/// the shared player-subject reader over a view of the same tokens whose
/// genitive is its noun (`target player's` is the declared `target player`,
/// `that player's` the bound `that player`): it declares or binds exactly as
/// the subject would. A declaration needs the binding the effect sequence
/// supplies; without it the possessor fails explicitly.
fn recognize_life_total_possessor(
    cursor: &mut OracleTokenCursor<'_>,
    length: usize,
    bindings: &LifeEffectBindings<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<LifeTotalPossessor> {
    cursor.enter("effect.life-total.possessor");
    if length == 1 {
        cursor.consume("your")?;
        return Ok(LifeTotalPossessor::Player(PlayerEffectSubject::Controller));
    }
    let noun = cursor
        .peek_ahead(1)
        .and_then(genitive_player_noun)
        .ok_or_else(|| cursor.fail("`player's` or `opponent's`"))?;
    if word_is(cursor, 0, "each") {
        cursor.enter("effect.life-total.possessor.population");
        cursor.advance_to(cursor.position() + 2)?;
        return Ok(LifeTotalPossessor::Population(if noun == "player" {
            super::PlayerSetReferenceNode::EachPlayer
        } else {
            super::PlayerSetReferenceNode::OpponentsOfAbilityController
        }));
    }
    let declaration = if word_is(cursor, 0, "target") {
        cursor.enter("effect.life-total.possessor.declaration");
        cursor.require_value(
            bindings.declared_target,
            "a player-target binding supplied by the effect sequence",
        )?
    } else {
        PlayerBinding(usize::MAX)
    };
    let genitive = cursor.position() + 1;
    let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
    if let Some(class) = crate::oracle::syntax::recognize_entities(noun)
        .into_iter()
        .next()
    {
        tokens[genitive].category = class.category;
        tokens[genitive].token_type = class.token_type;
    }
    tokens[genitive].raw = noun.to_string();
    let mut view = cursor.with_tokens(&tokens);
    let player = view.bounded(genitive + 1, |subject| {
        super::source_sacrifice::atomic_action::player_subject::recognize_player_effect_subject(
            subject,
            declaration,
            bindings.player_antecedent,
            context.frame.event_player,
        )
    })?;
    cursor.advance_to(view.position())?;
    Ok(LifeTotalPossessor::Player(player))
}

/// `becomes AMOUNT` / `becomes equal to AMOUNT` after `life total`: the
/// shared definition grammar of a number.
fn recognize_life_total_amount(
    cursor: &mut OracleTokenCursor<'_>,
    verb: &str,
) -> MachineResult<NumericExpression> {
    cursor.enter("effect.life-total.verb");
    cursor.consume(verb)?;
    cursor.enter("effect.life-total.amount");
    if word_is(cursor, 0, "equal") && word_is(cursor, 1, "to") {
        cursor.consume_phrase(&["equal", "to"])?;
        return super::recognize_equal_to_amount_expression(cursor, None);
    }
    super::recognize_variable_definition_expression(cursor)
}

/// `POSSESSOR life total becomes AMOUNT` (CR 119.5): the possessor gains or
/// loses the difference. One player is the player of the ordinary player
/// action; each player of a population performs the change in turn order
/// (CR 101.4), so the amount is read in the performer's person: `they` and
/// `their` name the member whose total changes (`each player's life total
/// becomes the number of creatures they control`). A written `you` there
/// would name the member too and is refused as ambiguous. `Ok(None)` makes
/// no claim.
fn try_recognize_life_total_clause(
    cursor: &mut OracleTokenCursor<'_>,
    bindings: &LifeEffectBindings<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<LocalEffectNode>> {
    let Some(length) = life_total_possessor_length(cursor) else {
        return Ok(None);
    };
    cursor.enter("effect.life-total");
    cursor.require(
        context.subject_verb_agreement == SubjectVerbAgreement::Finite,
        "a finite `life total becomes` clause (no causative `have`)",
    )?;
    let possessor = recognize_life_total_possessor(cursor, length, bindings, context)?;
    cursor.consume_phrase(&["life", "total"])?;
    Ok(Some(match possessor {
        LifeTotalPossessor::Player(player) => {
            let total = recognize_life_total_amount(cursor, "becomes")?;
            LocalEffectNode::PlayerAction(PlayerActionNode {
                player,
                action: SingularPlayerAction::SetLifeTotal(total),
            })
        }
        LifeTotalPossessor::Population(players) => {
            cursor.enter("effect.life-total.population");
            let verb = cursor.position();
            let tokens = super::person_shifted_tokens(
                cursor,
                VerbAgreement::ThirdPersonSingular,
                "a player population (`each player's life total`)",
            )?;
            let mut shifted = cursor.with_tokens(&tokens);
            let total = recognize_life_total_amount(&mut shifted, "become")?;
            let end = shifted.position();
            if let Some(ambiguous) = super::controller_reference_failure(cursor, verb, end) {
                return Err(ambiguous);
            }
            cursor.advance_to(end)?;
            LocalEffectNode::Performed(super::PerformedEffectNode {
                performer: super::PerformerNode::Population {
                    players,
                    excluding_controller: false,
                },
                effect: Box::new(LocalEffectNode::PlayerAction(PlayerActionNode {
                    player: PlayerEffectSubject::Controller,
                    action: SingularPlayerAction::SetLifeTotal(total),
                })),
            })
        }
    }))
}

/// `[SUBJECT] gain(s)|lose(s) AMOUNT life` as one effect, or `POSSESSOR life
/// total becomes AMOUNT`. `Ok(None)` makes no claim and restores the cursor.
pub(in crate::oracle::canonical) fn try_recognize_life_effect_clause(
    cursor: &mut OracleTokenCursor<'_>,
    bindings: LifeEffectBindings<'_>,
    context: &EffectRecognitionContext,
) -> MachineResult<Option<LocalEffectNode>> {
    if let Some(change) = try_recognize_life_total_clause(cursor, &bindings, context)? {
        return Ok(Some(change));
    }
    let causative = context.subject_verb_agreement == SubjectVerbAgreement::BareInfinitive;
    let has_inherited = bindings.inherited.is_some();
    let frame = ClauseFrame {
        subject: ClauseSubjectPolicy::EffectPlayer(EffectSubjectContext {
            declared_target: bindings.declared_target,
            player_antecedent: bindings.player_antecedent,
            inherited: bindings.inherited,
            object_subject: bindings.object_subject,
            object_antecedent: bindings.object_antecedent,
            frame: context.frame.clone(),
            causative,
        }),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Expression {
            antecedent: bindings.object_antecedent,
        })],
        lemmas: LIFE_LEMMAS,
    };
    // A written player subject other than `you` is the acting player of the
    // amount (`target opponent loses life equal to the number of creatures
    // they control`); the imperative and `you` are the controller's.
    let third_person = !cursor.peek().is_some_and(|raw| {
        ["you", "gain", "gains", "lose", "loses"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    });
    let read = |clause: &mut OracleTokenCursor<'_>| recognize_clause(clause, &frame).into_result();
    let clause = if third_person {
        super::read_with_acting_player(cursor, read)?
    } else {
        read(&mut *cursor)?
    };
    let Some(clause) = clause else {
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
        ClauseSubject::Supplied | ClauseSubject::Scope { .. } | ClauseSubject::DamageTheme(_) => {
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
