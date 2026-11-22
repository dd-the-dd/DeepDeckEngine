//! Shared clause reader: `Subject? Modality? Verb[agreement] Arguments
//! Time?` for every lemma of the verb lexicon (`verb_lexicon.rs`).
//!
//! ```text
//! Clause    := Subject Modality? Verb Arguments TimeFrame?
//! Subject   := policy of the parent frame:
//!                imperative | explicit player | inherited | causative `have X`
//!                (EffectPlayer), `you`/`a player`/`an opponent`/`one or more
//!                PLAYERS` (PlayerScope), or a subject the parent already
//!                read with its agreement (Supplied: player sets, coordinated
//!                subjects, event actors)
//!            | `[KIND] damage` (DamageTheme, the passive's damage subject)
//! Modality  := "would" (replacement)                -> bare verb
//!            | "have" | "has" | `you've` (past history) -> past participle
//! Voice     := active | passive: ("is" | "are" | "be") + past participle
//! Verb      := a lemma of the frame in the form the subject and tense require
//! Arguments := one of the lemma's argument frames that the parent accepts,
//!              chosen from the tokens after the verb
//! TimeFrame := "this" "turn" (past history)
//! ```
//!
//! The parent states what it expects in a `ClauseFrame`; it never tests a
//! verb word. The reader is transactional: a subject (even one whose binding
//! failed) commits nothing until a verb of the frame and the start of its
//! argument frame are recognized. After that claim every failure is the
//! frame's own explicit diagnostic. Subjects and amounts come from the
//! existing shared children; this module adds no subject or numeric grammar.
use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::damage_clause::{
    DamageArguments, DamageClauseArguments, damage_arguments_claim, read_damage_arguments,
    recognize_damage_theme,
};
use super::source_sacrifice::atomic_action::player_subject::{
    ObjectSubjectAntecedent, defending_player_starts, object_related_player_head_length,
    recognize_defending_player_subject, recognize_object_related_player_subject,
    recognize_player_effect_subject,
};
use super::verb_lexicon::{
    ArgumentFrame, VerbAgreement, VerbEntry, VerbForm, VerbLemma, Voice, lookup_verb,
};
use super::{
    ControlledCountDirection, DamageKind, EventActorScope, LocalPlayerReference, NumericExpression,
    PermanentEffectReference, PlayerBinding, PlayerEffectSubject, TargetBinding,
    TargetPlayerLifeAction, recognize_count_comparison, recognize_event_quantity_expression,
    recognize_numeric_expression, recognize_player_reference, try_recognize_event_actor_scope,
    try_recognize_plural_event_actor_scope,
};

/// Who performs the clause and how the reader finds them.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) enum ClauseSubjectPolicy<'a> {
    /// The parent already read the subject (a player set, coordinated
    /// subjects, an event actor) and states the agreement it requires.
    Supplied(VerbAgreement),
    /// A player population read by the shared event-actor readers: `you`,
    /// `a player`, `an opponent`, `one or more PLAYERS`.
    PlayerScope,
    /// The acting player of an effect: imperative, an explicit player
    /// subject, the subject inherited from the previous clause, or the
    /// performer named by a causative `have`.
    EffectPlayer(EffectSubjectContext<'a>),
    /// The passive whose subject is the damage itself (`combat damage would
    /// be dealt to ...`).
    DamageTheme,
}

/// Bindings the effect parent proved for the player subject readers.
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct EffectSubjectContext<'a> {
    /// The binding a new `target player` declaration takes, when the parent
    /// can declare one.
    pub declared_target: Option<PlayerBinding>,
    /// The earlier player binding `that player` names.
    pub player_antecedent: Option<PlayerBinding>,
    /// The subject of the previous clause, which a subjectless verb continues.
    pub inherited: Option<&'a PlayerEffectSubject>,
    /// The singular object `its controller` / `that NOUN's owner` relates to.
    pub object_subject: Option<ObjectSubjectAntecedent>,
    /// The object binding of the legacy `its controller` player reference
    /// and the default antecedent of the amount's anaphora.
    pub object_antecedent: Option<TargetBinding>,
    /// What the triggering event binds (event player, defending player).
    pub frame: super::FrameBindings,
    /// After a causative `have`: an explicit subject and the bare verb.
    pub causative: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseModality {
    Indicative,
    /// `PLAYER would VERB` (CR 614.1a replacement events).
    Would,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseTense {
    /// A present one-shot action or an event pattern.
    Present,
    /// `PLAYER VERBed ... this turn`: a history query over the same pattern.
    PastThisTurn,
}

/// How the parent expects the amount slot of a quantity frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseAmountPolicy {
    /// `N life` | `life equal to N`: the shared numeric expression. The
    /// antecedent is the default one for anaphora in the amount; an
    /// object-related subject (`its controller`) supplies its own object.
    Expression { antecedent: Option<TargetBinding> },
    /// The replacement's modified quantity (`twice that much life`).
    EventQuantity,
    /// An event pattern without an amount (`gains life`).
    Absent,
    /// History: bare `life` or `N or more life`.
    Threshold,
    /// The parent extracted the resource noun (`the amount of life you gained`).
    Gapped,
}

/// One argument frame the parent interprets, with its slot policy.
#[derive(Clone, Copy, Debug)]
pub(in crate::oracle::canonical) enum ClauseArguments<'a> {
    Life(ClauseAmountPolicy),
    Damage(DamageArguments<'a>),
    /// A declared frame with no interpretation yet: claiming it fails
    /// explicitly instead of being taken for another frame.
    Unread(ArgumentFrame),
    /// The parent reads the frame's operand with its own role child (a cost,
    /// an event object, an effect operand): the clause owns the subject, the
    /// lemma, its form and the frame choice, and stops after the verb.
    Operand(ArgumentFrame),
}

impl ClauseArguments<'_> {
    pub(in crate::oracle::canonical) fn frame(&self) -> ArgumentFrame {
        match self {
            Self::Life(_) => ArgumentFrame::LifeTotalChange,
            Self::Damage(_) => ArgumentFrame::DamageDealt,
            Self::Unread(frame) | Self::Operand(frame) => *frame,
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct ClauseFrame<'a> {
    pub subject: ClauseSubjectPolicy<'a>,
    pub modality: ClauseModality,
    pub tense: ClauseTense,
    pub voice: Voice,
    /// The argument frames this parent interprets.
    pub arguments: Vec<ClauseArguments<'a>>,
    /// The lemmas this parent can interpret.
    pub lemmas: &'static [VerbLemma],
}

/// The subject the reader found, in the vocabulary of the existing children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseSubject {
    /// No subject: the ability controller performs the instruction.
    Imperative,
    /// The parent read the subject.
    Supplied,
    /// A subjectless verb continuing the previous clause's player.
    Inherited(PlayerEffectSubject),
    /// An explicit player subject of the shared player-subject readers.
    Player(PlayerEffectSubject),
    /// `its controller` read by the shared player-reference child when no
    /// singular object subject was proven.
    PlayerReference(LocalPlayerReference),
    /// A player population of the shared event-actor readers.
    Scope {
        actor: EventActorScope,
        one_or_more: bool,
    },
    /// The damage itself, subject of a passive (`combat damage would be dealt`).
    DamageTheme(DamageKind),
}

/// The amount slot of a quantity frame as written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseAmount {
    /// No amount in a pattern (`gains life`).
    Unspecified,
    Expression(NumericExpression),
    /// `N or more` in a history threshold.
    Count {
        value: i64,
        direction: ControlledCountDirection,
    },
    /// The noun was extracted by the parent.
    Gapped,
}

/// One action node per lemma frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ClauseAction {
    /// CR 119.3: the acting player's life total changes by the amount.
    LifeTotalChange {
        direction: TargetPlayerLifeAction,
        amount: ClauseAmount,
    },
    /// CR 120.1: an object deals damage; the arguments follow the policy.
    DealDamage(DamageClauseArguments),
    /// The verb of the lemma in this frame; the parent reads the operand.
    Operand(ArgumentFrame),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct Clause {
    pub subject: ClauseSubject,
    pub lemma: VerbLemma,
    pub action: ClauseAction,
    pub modality: ClauseModality,
    pub tense: ClauseTense,
    pub voice: Voice,
}

pub(in crate::oracle::canonical) enum ClauseRecognition {
    Recognized(Clause),
    /// Nothing was claimed; the cursor is restored.
    NotClaimed,
    /// A verb of the frame claimed the clause; the failure is the frame's.
    Failed(MachineFailure),
}

impl ClauseRecognition {
    /// `Ok(None)` for an unclaimed clause, the failure once claimed.
    pub(in crate::oracle::canonical) fn into_result(self) -> MachineResult<Option<Clause>> {
        match self {
            Self::Recognized(clause) => Ok(Some(clause)),
            Self::NotClaimed => Ok(None),
            Self::Failed(failure) => Err(failure),
        }
    }
}

enum SubjectRead {
    /// The policy finds no subject here.
    Absent,
    Read(ClauseSubject, VerbAgreement),
    /// A subject was read but its binding failed: it becomes the clause's
    /// failure only if a verb of the frame follows.
    Tentative(MachineFailure),
}

fn agreement_of_player(player: &PlayerEffectSubject) -> VerbAgreement {
    match player {
        PlayerEffectSubject::Controller => VerbAgreement::Base,
        _ => VerbAgreement::ThirdPersonSingular,
    }
}

fn verb_at(cursor: &OracleTokenCursor<'_>, lemmas: &[VerbLemma]) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| lookup_verb(raw, lemmas).is_some())
}

fn read_subject(cursor: &mut OracleTokenCursor<'_>, frame: &ClauseFrame<'_>) -> SubjectRead {
    match &frame.subject {
        ClauseSubjectPolicy::Supplied(agreement) => {
            SubjectRead::Read(ClauseSubject::Supplied, *agreement)
        }
        ClauseSubjectPolicy::PlayerScope => read_player_scope(cursor),
        ClauseSubjectPolicy::EffectPlayer(context) => read_effect_player(cursor, frame, context),
        ClauseSubjectPolicy::DamageTheme => {
            cursor.enter("clause.subject.damage-theme");
            let start = cursor.checkpoint();
            let theme = cursor
                .peek_ahead(usize::from(cursor.peek().is_some_and(|raw| {
                    raw.eq_ignore_ascii_case("combat") || raw.eq_ignore_ascii_case("noncombat")
                })))
                .is_some_and(|raw| raw.eq_ignore_ascii_case("damage"));
            if !theme {
                return SubjectRead::Absent;
            }
            match recognize_damage_theme(cursor) {
                Ok(kind) => SubjectRead::Read(
                    ClauseSubject::DamageTheme(kind),
                    VerbAgreement::ThirdPersonSingular,
                ),
                Err(_) => {
                    cursor.rollback(start);
                    SubjectRead::Absent
                }
            }
        }
    }
}

fn read_player_scope(cursor: &mut OracleTokenCursor<'_>) -> SubjectRead {
    cursor.enter("clause.subject.player-scope");
    match try_recognize_plural_event_actor_scope(cursor) {
        Ok(Some(actor)) => {
            return SubjectRead::Read(
                ClauseSubject::Scope {
                    actor,
                    one_or_more: true,
                },
                VerbAgreement::Base,
            );
        }
        Ok(None) => {}
        Err(failure) => return SubjectRead::Tentative(failure),
    }
    match try_recognize_event_actor_scope(cursor) {
        Ok(Some(actor)) => SubjectRead::Read(
            ClauseSubject::Scope {
                actor,
                one_or_more: false,
            },
            if actor == EventActorScope::Controller {
                VerbAgreement::Base
            } else {
                VerbAgreement::ThirdPersonSingular
            },
        ),
        Ok(None) => SubjectRead::Absent,
        Err(failure) => SubjectRead::Tentative(failure),
    }
}

fn read_effect_player(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &ClauseFrame<'_>,
    context: &EffectSubjectContext<'_>,
) -> SubjectRead {
    cursor.enter("clause.subject.effect-player");
    let bare = |agreement| {
        if context.causative {
            VerbAgreement::Base
        } else {
            agreement
        }
    };
    if verb_at(cursor, frame.lemmas) {
        // A causative names its performer; it never inherits one.
        if context.causative {
            return SubjectRead::Absent;
        }
        return match context.inherited {
            Some(player) => SubjectRead::Read(
                ClauseSubject::Inherited(player.clone()),
                agreement_of_player(player),
            ),
            None => SubjectRead::Read(ClauseSubject::Imperative, VerbAgreement::Base),
        };
    }
    let start = cursor.checkpoint();
    if object_related_player_head_length(cursor).is_some() {
        // `its controller` / `that NOUN's owner`: the proven singular object,
        // else the object binding of the player-reference child.
        match recognize_object_related_player_subject(cursor, context.object_subject.clone()) {
            Ok(player) => {
                return SubjectRead::Read(
                    ClauseSubject::Player(player),
                    bare(VerbAgreement::ThirdPersonSingular),
                );
            }
            Err(relation_failure) => {
                // The verb claim is tested where the relation reader stopped.
                let failed_at = cursor.checkpoint();
                cursor.rollback(start);
                return match recognize_player_reference(cursor, context.object_antecedent) {
                    Ok(reference) => {
                        let agreement = match reference {
                            LocalPlayerReference::Controller => VerbAgreement::Base,
                            _ => VerbAgreement::ThirdPersonSingular,
                        };
                        SubjectRead::Read(
                            ClauseSubject::PlayerReference(reference),
                            bare(agreement),
                        )
                    }
                    Err(_) => {
                        cursor.rollback(failed_at);
                        SubjectRead::Tentative(relation_failure)
                    }
                };
            }
        }
    }
    if defending_player_starts(cursor) {
        return match recognize_defending_player_subject(cursor, context.frame.defending_player) {
            Ok(player) => SubjectRead::Read(
                ClauseSubject::Player(player),
                bare(VerbAgreement::ThirdPersonSingular),
            ),
            Err(failure) => SubjectRead::Tentative(failure),
        };
    }
    let head_is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    if !(head_is(0, "you") || head_is(0, "target") || (head_is(0, "that") && head_is(1, "player")))
    {
        return SubjectRead::Absent;
    }
    match recognize_player_effect_subject(
        cursor,
        context.declared_target.unwrap_or(PlayerBinding(usize::MAX)),
        context.player_antecedent,
        context.frame.event_player,
    ) {
        Ok(player) => {
            let agreement = bare(agreement_of_player(&player));
            SubjectRead::Read(ClauseSubject::Player(player), agreement)
        }
        Err(failure) => SubjectRead::Tentative(failure),
    }
}

fn is_clause_boundary(raw: &str) -> bool {
    matches!(raw, "." | "," | ";" | ":")
        || ["and", "then", "or"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
}

/// True when the argument frame starts after the verb (cursor on the
/// token after the verb). Children are tried on a copy only.
fn arguments_claim(cursor: &OracleTokenCursor<'_>, arguments: ClauseArguments<'_>) -> bool {
    match arguments {
        ClauseArguments::Life(amount) => life_frame_claims(cursor, amount),
        ClauseArguments::Damage(policy) => damage_arguments_claim(cursor, policy),
        // A declared frame claims at its marker noun (`control`, `all
        // abilities`); a keyword list has no marker until its reader exists.
        ClauseArguments::Unread(frame) => frame.marker_noun().is_some_and(|noun| {
            (0..2)
                .filter_map(|offset| cursor.peek_ahead(offset))
                .any(|raw| raw.eq_ignore_ascii_case(noun))
        }),
        ClauseArguments::Operand(frame) => operand_frame_claims(cursor, frame),
    }
}

/// A counter frame claims when its counter noun precedes its relation word
/// (`on` / `from`) in the same clause; other operand frames are the lemma's
/// only reading in an operand position and claim on the verb.
fn operand_frame_claims(cursor: &OracleTokenCursor<'_>, frame: ArgumentFrame) -> bool {
    let relation = match frame {
        ArgumentFrame::CountersOn => "on",
        ArgumentFrame::CountersFrom => "from",
        _ => return true,
    };
    let mut counter_seen = false;
    for raw in (0..).map_while(|offset| cursor.peek_ahead(offset)) {
        if matches!(raw, "." | "," | ";" | ":") {
            return false;
        }
        if raw.eq_ignore_ascii_case(relation) {
            return counter_seen;
        }
        counter_seen |= raw.eq_ignore_ascii_case("counter") || raw.eq_ignore_ascii_case("counters");
    }
    false
}

fn life_frame_claims(cursor: &OracleTokenCursor<'_>, amount: ClauseAmountPolicy) -> bool {
    let noun = "life";
    let noun_at = |cursor: &OracleTokenCursor<'_>| {
        cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case(noun))
    };
    if amount == ClauseAmountPolicy::Gapped || noun_at(cursor) {
        return true;
    }
    let mut trial = cursor.clone();
    let amount_read = match amount {
        ClauseAmountPolicy::Expression { antecedent } => {
            recognize_numeric_expression(&mut trial, antecedent).is_ok() && noun_at(&trial)
        }
        ClauseAmountPolicy::Threshold => {
            recognize_count_comparison(&mut trial).is_ok() && noun_at(&trial)
        }
        ClauseAmountPolicy::EventQuantity => {
            recognize_event_quantity_expression(&mut trial, noun).is_ok()
        }
        ClauseAmountPolicy::Absent | ClauseAmountPolicy::Gapped => false,
    };
    // An amount the numeric children cannot read still belongs to this
    // frame when its resource noun closes the same clause; the amount's
    // own diagnostic then surfaces instead of another parent's.
    amount_read
        || (0..)
            .map_while(|offset| cursor.peek_ahead(offset))
            .take_while(|raw| !is_clause_boundary(raw))
            .any(|raw| raw.eq_ignore_ascii_case(noun))
}

/// Reads one clause of the frame. See the module grammar.
pub(in crate::oracle::canonical) fn recognize_clause(
    cursor: &mut OracleTokenCursor<'_>,
    frame: &ClauseFrame<'_>,
) -> ClauseRecognition {
    let checkpoint = cursor.checkpoint();
    let not_claimed = |cursor: &mut OracleTokenCursor<'_>| {
        cursor.rollback(checkpoint);
        ClauseRecognition::NotClaimed
    };
    cursor.enter("clause.subject");
    // `you've VERBed`: the contracted perfect auxiliary of the controller.
    let contracted_perfect = matches!(frame.subject, ClauseSubjectPolicy::PlayerScope)
        && cursor
            .peek()
            .is_some_and(|raw| raw.replace('\u{2019}', "'").eq_ignore_ascii_case("you've"));
    let read = if contracted_perfect {
        let raw = cursor.peek().expect("checked contraction").to_string();
        if cursor.consume(&raw).is_err() {
            return not_claimed(cursor);
        }
        SubjectRead::Read(
            ClauseSubject::Scope {
                actor: EventActorScope::Controller,
                one_or_more: false,
            },
            VerbAgreement::Base,
        )
    } else {
        read_subject(cursor, frame)
    };
    let (subject, agreement, tentative) = match read {
        SubjectRead::Absent => return not_claimed(cursor),
        SubjectRead::Read(subject, agreement) => (Some(subject), agreement, None),
        SubjectRead::Tentative(failure) => (None, VerbAgreement::Base, Some(failure)),
    };
    // A past history reads the simple past or the present perfect
    // (`you have gained`, `you've lost`).
    let mut perfect = contracted_perfect;
    if frame.tense == ClauseTense::PastThisTurn
        && !perfect
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("have") || raw.eq_ignore_ascii_case("has"))
    {
        let raw = cursor.peek().expect("checked auxiliary").to_string();
        if cursor.consume(&raw).is_err() {
            return not_claimed(cursor);
        }
        perfect = true;
    }
    // The amount's anaphora follow a possessive subject's object.
    let possessive_antecedent = match &subject {
        Some(ClauseSubject::Player(PlayerEffectSubject::ObjectRelation { object, .. })) => {
            Some(match object {
                PermanentEffectReference::Target(binding) => Some(*binding),
                _ => None,
            })
        }
        _ => None,
    };
    let arguments = frame
        .arguments
        .iter()
        .map(|arguments| match (arguments, possessive_antecedent) {
            (ClauseArguments::Life(ClauseAmountPolicy::Expression { .. }), Some(antecedent)) => {
                ClauseArguments::Life(ClauseAmountPolicy::Expression { antecedent })
            }
            (arguments, _) => *arguments,
        })
        .collect::<Vec<_>>();
    cursor.enter("clause.modality");
    if frame.modality == ClauseModality::Would && cursor.consume("would").is_err() {
        return not_claimed(cursor);
    }
    // The passive auxiliary: bare `be` after a modal, else `is` / `are`.
    let auxiliary = if frame.voice == Voice::Passive {
        cursor.enter("clause.passive-auxiliary");
        let Some(raw) = cursor
            .peek()
            .map(str::to_ascii_lowercase)
            .filter(|raw| matches!(raw.as_str(), "is" | "are" | "be"))
        else {
            return not_claimed(cursor);
        };
        let position = cursor.checkpoint();
        if cursor.advance_to(cursor.position() + 1).is_err() {
            return not_claimed(cursor);
        }
        Some((raw, position))
    } else {
        None
    };
    cursor.enter("clause.verb");
    let Some((entry, forms)) = cursor
        .peek()
        .and_then(|raw| lookup_verb(raw, frame.lemmas))
        .filter(|(entry, _)| entry.voices.contains(&frame.voice))
    else {
        return not_claimed(cursor);
    };
    let verb_position = cursor.checkpoint();
    let mut after_verb = cursor.clone();
    // The lemma's frames, in lexicon order, that the parent interprets; the
    // first one whose arguments start after the verb is the clause's.
    let chosen = if after_verb.advance_to(after_verb.position() + 1).is_ok() {
        entry.frames.iter().find_map(|lemma_frame| {
            arguments
                .iter()
                .find(|arguments| {
                    arguments.frame() == *lemma_frame && arguments_claim(&after_verb, **arguments)
                })
                .copied()
        })
    } else {
        None
    };
    let Some(chosen) = chosen else {
        return not_claimed(cursor);
    };
    // Claimed: the verb and its argument frame belong to this reader.
    if let Some(failure) = tentative {
        return ClauseRecognition::Failed(failure);
    }
    let subject = subject.expect("a read subject");
    if let Some((raw, position)) = auxiliary {
        let expected = match (frame.modality, agreement) {
            (ClauseModality::Would, _) => "be",
            (_, VerbAgreement::Base) => "are",
            (_, VerbAgreement::ThirdPersonSingular) => "is",
        };
        if raw != expected {
            cursor.rollback(position);
            return ClauseRecognition::Failed(cursor.fail(match expected {
                "be" => "`be`, the passive auxiliary after a modal",
                "are" => "`are`, the passive auxiliary its subject requires",
                _ => "`is`, the passive auxiliary its subject requires",
            }));
        }
    }
    let expected = match (frame.voice, frame.modality, frame.tense) {
        (Voice::Passive, _, _) => VerbForm::PastParticiple,
        (Voice::Active, ClauseModality::Would, _) => VerbForm::Base,
        (Voice::Active, _, ClauseTense::PastThisTurn) if perfect => VerbForm::PastParticiple,
        (Voice::Active, _, ClauseTense::PastThisTurn) => VerbForm::Past,
        (Voice::Active, ClauseModality::Indicative, ClauseTense::Present) => agreement.form(),
    };
    cursor.rollback(verb_position);
    if !forms.contains(&expected) {
        return ClauseRecognition::Failed(cursor.fail(&agreement_expectation(entry, expected)));
    }
    match read_predicate(cursor, entry, chosen, frame.tense) {
        Ok(action) => ClauseRecognition::Recognized(Clause {
            subject,
            lemma: entry.lemma,
            action,
            modality: frame.modality,
            tense: frame.tense,
            voice: frame.voice,
        }),
        Err(failure) => ClauseRecognition::Failed(failure),
    }
}

fn agreement_expectation(entry: &VerbEntry, expected: VerbForm) -> String {
    format!(
        "`{}`, the verb form its {} requires",
        entry.surface(expected),
        match expected {
            VerbForm::Base | VerbForm::ThirdPersonSingular => "subject",
            VerbForm::Past => "tense",
            VerbForm::PastParticiple | VerbForm::PresentParticiple => "voice or aspect",
        }
    )
}

/// The verb of an operand frame after a subject the parent read: the lemma,
/// in the subject's agreement, and the frame the tokens select. `Ok(None)`
/// makes no claim; the cursor is left after the verb for the operand child.
pub(in crate::oracle::canonical) fn recognize_operand_verb(
    cursor: &mut OracleTokenCursor<'_>,
    agreement: VerbAgreement,
    lemmas: &'static [VerbLemma],
    frames: &[ArgumentFrame],
) -> MachineResult<Option<(VerbLemma, ArgumentFrame)>> {
    let frame = ClauseFrame {
        subject: ClauseSubjectPolicy::Supplied(agreement),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        voice: Voice::Active,
        arguments: frames
            .iter()
            .map(|frame| ClauseArguments::Operand(*frame))
            .collect(),
        lemmas,
    };
    let Some(clause) = recognize_clause(cursor, &frame).into_result()? else {
        return Ok(None);
    };
    match clause.action {
        ClauseAction::Operand(frame) => Ok(Some((clause.lemma, frame))),
        _ => Err(cursor.fail("an operand frame")),
    }
}

/// True when `lemma` starts a clause at the cursor in `agreement`: the
/// shared clause reads the verb with every frame of the lemma, the parent
/// owning the operand. Effect dispatch uses this instead of a verb word.
pub(in crate::oracle::canonical) fn lemma_at(
    cursor: &OracleTokenCursor<'_>,
    lemma: VerbLemma,
    agreement: VerbAgreement,
) -> bool {
    let entry = super::verb_entry(lemma);
    let mut trial = cursor.clone();
    matches!(
        recognize_operand_verb(
            &mut trial,
            agreement,
            std::slice::from_ref(&entry.lemma),
            entry.frames,
        ),
        Ok(Some(_))
    )
}

/// Consumes `lemma` at the cursor in `agreement` as the shared clause reads
/// it (any frame of the lemma, the parent reading the operand). A reader
/// that owns a fixed verb position uses this instead of a verb word; the
/// failure names the expected surface.
pub(in crate::oracle::canonical) fn consume_lemma(
    cursor: &mut OracleTokenCursor<'_>,
    lemma: VerbLemma,
    agreement: VerbAgreement,
) -> MachineResult<()> {
    let entry = super::verb_entry(lemma);
    match recognize_operand_verb(
        cursor,
        agreement,
        std::slice::from_ref(&entry.lemma),
        entry.frames,
    ) {
        Ok(Some(_)) => Ok(()),
        _ => Err(cursor.fail(entry.surface(agreement.form()))),
    }
}

/// The diagnostic of a subject whose reader does not take the verb at
/// `verb` (S2): state `clause.verb.unsupported`, naming the lexicon lemma
/// (or the word when it has no entry) and the subject kind, instead of a
/// verb another reader expected.
pub(in crate::oracle::canonical) fn clause_verb_unsupported(
    verb: &OracleTokenCursor<'_>,
    subject: &str,
) -> MachineFailure {
    let mut at = verb.clone();
    at.enter("clause.verb.unsupported");
    let word = verb.peek().unwrap_or("").to_string();
    let lemma = super::ALL_LEMMAS
        .iter()
        .find_map(|lemma| {
            let entry = super::verb_entry(*lemma);
            (!entry.forms_of(&word).is_empty()).then_some(entry.surface(VerbForm::Base))
        })
        .map_or_else(
            || format!("`{word}` (no lexicon lemma)"),
            |base| format!("lemma `{base}`"),
        );
    at.fail(&format!(
        "a verb this subject reads: {lemma} is not read for {subject}"
    ))
}

/// True when `you` is the explicit subject of a finite verb at the cursor
/// (`you mill two cards`): the controller's clause, read like the
/// imperative. Modal, negated and relative continuations (`you may`, `you
/// don't`, `you control`) belong to their own frames.
pub(in crate::oracle::canonical) fn controller_subject_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    const NOT_A_VERB: &[&str] = &[
        "may",
        "can",
        "can't",
        "cannot",
        "could",
        "do",
        "don't",
        "does",
        "did",
        "didn't",
        "would",
        "will",
        "won't",
        "control",
        "controlled",
        "own",
        "owned",
        "have",
        "had",
        "and",
        "or",
        "also",
        "each",
    ];
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            raw.chars().all(|character| character.is_ascii_alphabetic())
                && !NOT_A_VERB.iter().any(|word| raw.eq_ignore_ascii_case(word))
        })
}

/// The token length of a player-set subject (`each player`, `each
/// opponent`, `each other player`) at the cursor.
pub(in crate::oracle::canonical) fn player_set_subject_length(
    cursor: &OracleTokenCursor<'_>,
) -> Option<usize> {
    let is = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    if !is(0, "each") {
        return None;
    }
    if is(1, "player") || is(1, "opponent") {
        Some(2)
    } else if is(1, "other") && (is(2, "player") || is(2, "opponent")) {
        Some(3)
    } else {
        None
    }
}

/// True when a clause of the frame is claimed at the cursor (recognized or
/// failed after the claim). Reads a copy; the cursor is unchanged.
pub(in crate::oracle::canonical) fn clause_starts(
    cursor: &OracleTokenCursor<'_>,
    frame: &ClauseFrame<'_>,
) -> bool {
    let mut trial = cursor.clone();
    !matches!(
        recognize_clause(&mut trial, frame),
        ClauseRecognition::NotClaimed
    )
}

/// The verb and the lemma's argument frame, the verb in the checked form.
fn read_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    entry: &VerbEntry,
    arguments: ClauseArguments<'_>,
    tense: ClauseTense,
) -> MachineResult<ClauseAction> {
    cursor.enter("clause.verb.claimed");
    cursor.advance_to(cursor.position() + 1)?;
    let action = match arguments {
        ClauseArguments::Life(amount) => ClauseAction::LifeTotalChange {
            direction: match entry.lemma {
                VerbLemma::Gain => TargetPlayerLifeAction::Gain,
                VerbLemma::Lose => TargetPlayerLifeAction::Lose,
                _ => return Err(cursor.fail("a life-total lemma")),
            },
            amount: read_life_amount(cursor, amount)?,
        },
        ClauseArguments::Damage(policy) => {
            ClauseAction::DealDamage(read_damage_arguments(cursor, policy)?)
        }
        ClauseArguments::Unread(_) => {
            cursor.enter("clause.arguments.unread");
            return Err(cursor.fail("an argument frame this parent interprets"));
        }
        ClauseArguments::Operand(frame) => ClauseAction::Operand(frame),
    };
    if tense == ClauseTense::PastThisTurn {
        cursor.enter("clause.time-frame");
        cursor.consume_phrase(&["this", "turn"])?;
    }
    Ok(action)
}

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn read_life_amount(
    cursor: &mut OracleTokenCursor<'_>,
    policy: ClauseAmountPolicy,
) -> MachineResult<ClauseAmount> {
    match policy {
        ClauseAmountPolicy::Expression { antecedent } => {
            cursor.enter("effect.life.amount-or-resource");
            if peek_is(cursor, "life") {
                cursor.consume("life")?;
                cursor.enter("effect.life.amount-after-resource");
                cursor.require(
                    peek_is(cursor, "equal"),
                    "`equal to` followed by a numeric expression",
                )?;
                return recognize_numeric_expression(cursor, antecedent)
                    .map(ClauseAmount::Expression);
            }
            cursor.enter("effect.life.amount-before-resource");
            let amount = recognize_numeric_expression(cursor, antecedent)?;
            cursor.enter("effect.life.resource-after-amount");
            cursor.consume("life")?;
            Ok(ClauseAmount::Expression(amount))
        }
        ClauseAmountPolicy::EventQuantity => {
            cursor.enter("clause.life.event-quantity");
            recognize_event_quantity_expression(cursor, "life").map(ClauseAmount::Expression)
        }
        ClauseAmountPolicy::Absent => {
            cursor.enter("clause.life.pattern-resource");
            cursor.require(
                peek_is(cursor, "life"),
                "`life` without an amount in this pattern",
            )?;
            cursor.consume("life")?;
            Ok(ClauseAmount::Unspecified)
        }
        ClauseAmountPolicy::Threshold => {
            cursor.enter("clause.life.threshold");
            if peek_is(cursor, "life") {
                cursor.consume("life")?;
                return Ok(ClauseAmount::Unspecified);
            }
            let (value, direction, _) = recognize_count_comparison(cursor)?;
            cursor.enter("clause.life.threshold-resource");
            cursor.consume("life")?;
            Ok(ClauseAmount::Count { value, direction })
        }
        ClauseAmountPolicy::Gapped => Ok(ClauseAmount::Gapped),
    }
}

#[cfg(test)]
#[path = "clause_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "effect_clause_tests.rs"]
mod effect_tests;

#[cfg(test)]
#[path = "effect_subject_tests.rs"]
mod subject_tests;
