//! Player events (CR 603.2): one shared actor, then one or more verbs the
//! actor performs.
//!
//! ```text
//! PlayerEvent  := Actor FixedVerb
//!               | Actor ObjectVerb ("or" ObjectVerb)* CardObject
//!               | Actor NamedVerb ("or" NamedVerb)*
//!               | Actor LifeClause    (the shared clause, `clause.rs`)
//!               | Actor "activate(s)" AbilityQuery
//!               | Actor "tap(s)" PermanentObject "for mana"
//!               | Actor "commit(s) a crime"
//!               | Actor OutcomeVerb               -- random and contested outcomes
//!               | Actor "proliferate(s)" | Actor "complete(s) a dungeon"
//!               | Actor "exert(s)" PermanentObject
//! OutcomeVerb  := "roll(s)" DieRoll | ("win(s)" | "lose(s)") "a coin flip"
//!               | "clash(es)" ["and win(s)"] | "win(s) a clash"
//! DieRoll      := "a die" | "one or more dice"
//!               | "a" N ["or higher" | "or lower"] ["on a die"]
//! Actor        := "you" | "a player" | "an opponent" | "one or more" PLAYERS
//! FixedVerb    := "draw(s) a card" | "attack(s)" | ...
//! ObjectVerb   := "discard(s)" | "cycle(s)"
//! CardObject   := ("a" | "an" | "another") CardQuery
//! NamedVerb    := a keyword action whose catalog entry publishes a completed
//!                 named action (CR 701.22d scry, 701.25 surveil, ...)
//! AbilityQuery := ("a" | "an") ["loyalty"] "ability"
//!                 ["that isn't a mana ability"]
//!
//! AttackDeclaration := Actor "attack(s) with" QuantifiedAttackers
//! AttackedRecipient := Attacked ("or" Attacked)*    -- after `attack(s)`
//! Attacked          := Actor | ("a" | "an") PermanentQuery
//! ```
//!
//! Actor, verbs and the card object are independent attributes. A union
//! selects one event occurrence; it shares the actor and the object. Every
//! verb is a lexicon lemma read in the actor's agreement.
use crate::oracle::ability_catalog::{AbilitySignalPolicy, lookup_exact_lexeme, lookup_registry};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ArgumentFrame, ClauseAction, ClauseAmountPolicy, ClauseArguments, ClauseFrame, ClauseModality,
    ClauseSubjectPolicy, ClauseTense, DamageArguments, DamageClauseArguments, DamageKind,
    EventActorScope, EventMultiplicity, LIFE_LEMMAS, LocalObjectReference, LocalTriggerEventNode,
    ObjectKind, ObjectQueryNode, QuantifiedEventSubjectNode, TargetPlayerLifeAction, VerbAgreement,
    VerbLemma, Voice, consume_lemma, is_verb_form, recognize_clause, recognize_object_query,
    recognize_operand_verb, recognize_quantified_event_subject, recognize_supplied_damage_clause,
    supplied_damage_frame, try_recognize_event_actor_scope, try_recognize_plural_event_actor_scope,
};

/// The kind of activated ability an activation event names. Each is a
/// property the runtime records for the activation (CR 602.2, 605.1a, 606.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ActivatedAbilityKind {
    /// `an ability that isn't a mana ability` (CR 605.1a).
    NonMana,
    /// `a loyalty ability` (CR 606.3): never a mana ability.
    Loyalty,
}

/// One declaration of attackers (CR 508.1) as a trigger event: the attacking
/// player, the attackers it must contain (shared numeric bound and object
/// query) and, for a coordinated subject (`SOURCE and at least N other
/// creatures attack`), the source among them. It is one event per
/// declaration, never one per attacker (CR 603.2c).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct AttackDeclarationNode {
    pub attacker: EventActorScope,
    /// The attackers beyond the source; their bound is `PerEvent`.
    pub attackers: QuantifiedEventSubjectNode,
    pub source: Option<LocalObjectReference>,
}

/// One alternative of the object an attacking creature attacks (CR 506.2,
/// 508.1b): a player relative to the ability controller, or a planeswalker
/// or battle read by the shared object query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AttackedRecipientNode {
    Player(EventActorScope),
    Permanent(ObjectQueryNode),
}

/// Player-event verbs. Each is a vocabulary atom; the actor, the object and
/// the lowering to a runtime event are independent concepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerEventAction {
    GainLife,
    LoseLife,
    DrawCard,
    Attack,
    FaceDilemma,
    BeDealtDamage(DamageKind),
    /// `discard CARD` (CR 701.9).
    DiscardCard,
    /// `cycle CARD` (CR 702.29): activating a cycling ability.
    CycleCard,
    /// A keyword action that publishes one completed named action, by its
    /// catalog identity (`scry`, `surveil`, ...).
    NamedAction(&'static str),
    /// `activate ABILITY` (CR 602.2), with the kind of ability activated.
    ActivateAbility(ActivatedAbilityKind),
    /// `tap PERMANENT for mana` (CR 106.11a, 605.1a): activating a mana
    /// ability of that permanent with {T} in its cost. The permanent is
    /// the event's `permanent` object.
    TapForMana,
    /// `commit a crime` (CR 700.13).
    CommitCrime,
    /// `play(s) a land` (CR 305.1): the special action of playing a land.
    PlayLand,
    /// `unlock this door` (CR 709.5): the special action unlocking the door
    /// of the Room face that owns the ability.
    UnlockSourceDoor,
    /// `fully unlock a Room` (CR 709.5): unlocking a door leaves that Room
    /// with every door unlocked.
    FullyUnlockRoom,
    /// `expend N` (CR 701.58): the player spends their Nth total mana to
    /// cast spells this turn.
    Expend(i64),
    /// `roll a die`, `roll a 5 or higher`, `roll one or more dice` (CR 706):
    /// one roll of the acting player, with the result filter it names.
    RollDie(DieRollFilter),
    /// `win a coin flip` / `lose a coin flip` (CR 705.2): one flip the
    /// acting player won or lost.
    FlipCoin {
        won: bool,
    },
    /// `clash` / `clash and win` / `win a clash` (CR 701.30): every
    /// clashing player clashes; `won` restricts to the clash winner.
    Clash {
        won: bool,
    },
    /// `proliferate` (CR 701.34): one proliferation of the acting player.
    Proliferate,
    /// `exert PERMANENT` (CR 701.43): the exerted permanent is the event's
    /// `permanent` object.
    Exert,
    /// `complete a dungeon` (CR 309.7): the acting player completed one.
    CompleteDungeon,
}

/// The die results a roll event observes (CR 706.2). Both bounds inclusive;
/// `batch` reads `one or more dice`: one occurrence per roll instruction
/// (CR 603.2c) rather than one per die.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DieRollFilter {
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    pub batch: bool,
}

impl PlayerEventAction {
    /// Base and third-person verb forms followed by the fixed complement.
    /// The lexicon lemma of a verb with a fixed complement, then the
    /// complement (`draw(s) a card`, `attack(s)`). The verb surface agreeing
    /// with the actor is lexicon data, never a phrase of this table.
    fn lemma_frame(self) -> Option<(VerbLemma, &'static [&'static str])> {
        match self {
            Self::DrawCard => Some((VerbLemma::Draw, &["a", "card"])),
            Self::Attack => Some((VerbLemma::Attack, &[])),
            Self::PlayLand => Some((VerbLemma::Play, &["a", "land"])),
            _ => None,
        }
    }

    /// The fixed phrase of a verb without a lexicon lemma (`face(s) a
    /// dilemma`, a named mechanic action). `unlock` names a Room's special
    /// action (CR 709.5) only; it has no general effect lemma.
    fn fixed_phrase(self, third_person: bool) -> Option<&'static [&'static str]> {
        Some(match (self, third_person) {
            (Self::FaceDilemma, false) => &["face", "a", "dilemma"],
            (Self::FaceDilemma, true) => &["faces", "a", "dilemma"],
            (Self::UnlockSourceDoor, false) => &["unlock", "this", "door"],
            (Self::UnlockSourceDoor, true) => &["unlocks", "this", "door"],
            (Self::FullyUnlockRoom, false) => &["fully", "unlock", "a", "room"],
            (Self::FullyUnlockRoom, true) => &["fully", "unlocks", "a", "room"],
            _ => return None,
        })
    }

    /// The runtime records the acting player of each occurrence as the
    /// triggering player, so `that player` can name them in the body. The
    /// mana-ability observer records the controller of the tapped permanent,
    /// the player who activated its mana ability (CR 602.2).
    pub(in crate::oracle::canonical) fn records_acting_player(self) -> bool {
        matches!(
            self,
            Self::LoseLife
                | Self::DrawCard
                | Self::DiscardCard
                | Self::CycleCard
                | Self::NamedAction(_)
                | Self::TapForMana
                | Self::PlayLand
                | Self::CommitCrime
                | Self::RollDie(_)
                | Self::FlipCoin { .. }
                | Self::Clash { .. }
                | Self::Proliferate
                | Self::Exert
        )
    }

    /// Verbs that take the shared card object.
    pub(in crate::oracle::canonical) fn takes_card_object(self) -> bool {
        matches!(self, Self::DiscardCard | Self::CycleCard)
    }
}

const FIXED_PLAYER_EVENT_ACTIONS: [PlayerEventAction; 6] = [
    PlayerEventAction::DrawCard,
    PlayerEventAction::Attack,
    PlayerEventAction::FaceDilemma,
    PlayerEventAction::PlayLand,
    PlayerEventAction::UnlockSourceDoor,
    PlayerEventAction::FullyUnlockRoom,
];

/// The card a player discards or cycles. `another` excludes the ability source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerEventCardObject {
    pub query: ObjectQueryNode,
    pub exclude_source: bool,
}

/// Third-person singular of a base verb (`scry` -> `scries`).
fn third_person_form(base: &str) -> String {
    match base.strip_suffix('y') {
        Some(stem) if !stem.ends_with(['a', 'e', 'i', 'o', 'u']) => format!("{stem}ies"),
        _ => format!("{base}s"),
    }
}

/// A verb whose catalog entry is a keyword action publishing one completed
/// named action. The form must agree with the actor.
fn named_action_at(cursor: &OracleTokenCursor<'_>, third_person: bool) -> Option<&'static str> {
    let raw = cursor.peek()?.to_ascii_lowercase();
    let entry = lookup_exact_lexeme(&raw)?;
    let registration = lookup_registry(&entry.canonical_id)?;
    if registration.semantics.signal.policy != AbilitySignalPolicy::AfterActionCompletes {
        return None;
    }
    let action_id = registration.semantics.signal.action_id.as_deref()?;
    let base = entry.canonical_id.as_str();
    let expected = if third_person {
        third_person_form(base)
    } else {
        base.to_string()
    };
    (raw == expected).then_some(action_id)
}

/// One verb of a union: an object verb of the lexicon (`discard`, `cycle`,
/// in the actor's agreement through the shared clause) or a named keyword
/// action, never a fixed phrase (whose complement is part of the verb).
fn recognize_union_verb(
    cursor: &mut OracleTokenCursor<'_>,
    third_person: bool,
) -> Option<PlayerEventAction> {
    let agreement = if third_person {
        VerbAgreement::ThirdPersonSingular
    } else {
        VerbAgreement::Base
    };
    let mut trial = cursor.clone();
    if let Ok(Some((lemma, _))) = recognize_operand_verb(
        &mut trial,
        agreement,
        &[VerbLemma::Discard, VerbLemma::Cycle],
        &[ArgumentFrame::ObjectSelection],
    ) {
        *cursor = trial;
        return Some(match lemma {
            VerbLemma::Cycle => PlayerEventAction::CycleCard,
            _ => PlayerEventAction::DiscardCard,
        });
    }
    let action = named_action_at(cursor, third_person).map(PlayerEventAction::NamedAction)?;
    let raw = cursor.peek()?.to_string();
    cursor.consume(&raw).ok()?;
    Some(action)
}

fn recognize_card_object(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerEventCardObject> {
    cursor.enter("event.player.card-determiner");
    let head = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| ["a", "an", "another"].contains(&raw.as_str()))
        .ok_or_else(|| cursor.fail("`a`, `an` or `another` before the card"))?;
    cursor.consume(&head)?;
    cursor.enter("event.player.card-query");
    let query = recognize_object_query(cursor)?;
    cursor.enter("event.player.card-domain");
    cursor.require(
        query.predicate.supports_domain(ObjectKind::Card)
            && query.predicate.has_nominal_head()
            && query.controller.is_none(),
        "a card query with a nominal head and no controller relation",
    )?;
    Ok(PlayerEventCardObject {
        query,
        exclude_source: head == "another",
    })
}

/// Recognizes `ACTOR VERB…` after the trigger word. The actor comes from the
/// shared event-actor reader; every verb must agree with it (`you discard`,
/// `an opponent discards`). The enclosing envelope owns the comma and body.
pub(in crate::oracle::canonical) fn recognize_player_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<LocalTriggerEventNode> {
    cursor.enter("event.player.actor");
    let plural = try_recognize_plural_event_actor_scope(cursor)?;
    let one_or_more = plural.is_some();
    let actor = match plural {
        Some(actor) => actor,
        None => try_recognize_event_actor_scope(cursor)?
            .ok_or_else(|| cursor.fail("a player event actor"))?,
    };
    let third_person = actor != EventActorScope::Controller && !one_or_more;
    cursor.enter("event.player.action");
    // `gain(s) life` / `lose(s) life`: the shared life clause with the actor
    // as its supplied subject; the pattern has no amount.
    let life_agreement = if third_person {
        VerbAgreement::ThirdPersonSingular
    } else {
        VerbAgreement::Base
    };
    let life_frame = ClauseFrame {
        subject: ClauseSubjectPolicy::Supplied(life_agreement),
        modality: ClauseModality::Indicative,
        tense: ClauseTense::Present,
        voice: Voice::Active,
        arguments: vec![ClauseArguments::Life(ClauseAmountPolicy::Absent)],
        lemmas: LIFE_LEMMAS,
    };
    if let Some(clause) = recognize_clause(cursor, &life_frame).into_result()? {
        let ClauseAction::LifeTotalChange { direction, .. } = clause.action else {
            return Err(cursor.fail("a life-total change"));
        };
        return Ok(LocalTriggerEventNode::PlayerEvent {
            actor,
            actions: vec![match direction {
                TargetPlayerLifeAction::Gain => PlayerEventAction::GainLife,
                TargetPlayerLifeAction::Lose => PlayerEventAction::LoseLife,
            }],
            one_or_more,
            card: None,
            permanent: None,
            first_time_each_turn: false,
        });
    }
    // `is|are dealt [combat|noncombat] damage`: the passive damage clause
    // with the actor as its supplied subject (the recipient).
    let damage_frame = supplied_damage_frame(
        life_agreement,
        Voice::Passive,
        ClauseModality::Indicative,
        DamageArguments::PassiveKind,
    );
    if let Some(arguments) = recognize_supplied_damage_clause(cursor, &damage_frame)? {
        let DamageClauseArguments::Kind(kind) = arguments else {
            return Err(cursor.fail("the kind of damage a player is dealt"));
        };
        return Ok(LocalTriggerEventNode::PlayerEvent {
            actor,
            actions: vec![PlayerEventAction::BeDealtDamage(kind)],
            one_or_more,
            card: None,
            permanent: None,
            first_time_each_turn: false,
        });
    }
    // Action verbs with their own operand frames: an activated ability, a
    // permanent tapped for mana, a crime. Each is a lexicon lemma in the
    // actor's agreement.
    if let Some(event) = recognize_player_action_frame(cursor, actor, life_agreement, one_or_more)?
    {
        return Ok(event);
    }
    for action in FIXED_PLAYER_EVENT_ACTIONS {
        let matched = if let Some((lemma, complement)) = action.lemma_frame() {
            let complete = lemma_form_at(cursor, lemma, life_agreement)
                && complement.iter().enumerate().all(|(offset, word)| {
                    cursor
                        .peek_ahead(offset + 1)
                        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
                });
            if complete {
                consume_verb_word(cursor)?;
                cursor.consume_phrase(complement)?;
            }
            complete
        } else {
            let phrase = action.fixed_phrase(third_person).expect("fixed phrase");
            let complete = (0..phrase.len()).all(|offset| {
                cursor
                    .peek_ahead(offset)
                    .is_some_and(|raw| raw.eq_ignore_ascii_case(phrase[offset]))
            });
            if complete {
                cursor.consume_phrase(phrase)?;
            }
            complete
        };
        if matched {
            return Ok(LocalTriggerEventNode::PlayerEvent {
                actor,
                actions: vec![action],
                one_or_more,
                card: None,
                permanent: None,
                first_time_each_turn: false,
            });
        }
    }
    let first = recognize_union_verb(cursor, third_person)
        .ok_or_else(|| cursor.fail("a player-event verb agreeing with its actor"))?;
    let mut actions = vec![first];
    while cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
    {
        cursor.enter("event.player.union");
        cursor.consume("or")?;
        let next = recognize_union_verb(cursor, third_person).ok_or_else(|| {
            cursor.fail("another player-event verb sharing the actor and its object")
        })?;
        cursor.enter("event.player.union-distinct");
        cursor.require(!actions.contains(&next), "a distinct event in the union")?;
        cursor.require(
            next.takes_card_object() == first.takes_card_object(),
            "union verbs that agree on taking the shared card object",
        )?;
        actions.push(next);
    }
    let card = if first.takes_card_object() {
        Some(recognize_card_object(cursor)?)
    } else {
        None
    };
    cursor.enter("event.player.complete");
    Ok(LocalTriggerEventNode::PlayerEvent {
        actor,
        actions,
        one_or_more,
        card,
        permanent: None,
        first_time_each_turn: false,
    })
}

/// The surface at the cursor is `lemma` in `agreement` (lexicon data).
fn lemma_form_at(
    cursor: &OracleTokenCursor<'_>,
    lemma: VerbLemma,
    agreement: VerbAgreement,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| is_verb_form(raw, lemma, agreement.form()))
}

fn consume_verb_word(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("an event verb"))?;
    cursor.consume(&raw)
}

/// `activate(s) ABILITY`, `tap(s) PERMANENT for mana` and `commit(s) a
/// crime`: player-action verbs whose operand frame is not a card object.
/// `None` when no such lemma is at the cursor (nothing consumed).
fn recognize_player_action_frame(
    cursor: &mut OracleTokenCursor<'_>,
    actor: EventActorScope,
    agreement: VerbAgreement,
    one_or_more: bool,
) -> MachineResult<Option<LocalTriggerEventNode>> {
    let event = |actions: Vec<PlayerEventAction>, permanent: Option<PlayerEventCardObject>| {
        LocalTriggerEventNode::PlayerEvent {
            actor,
            actions,
            one_or_more,
            card: None,
            permanent,
            first_time_each_turn: false,
        }
    };
    if lemma_form_at(cursor, VerbLemma::Activate, agreement) {
        cursor.enter("event.player.activate");
        consume_verb_word(cursor)?;
        let kind = recognize_activated_ability_query(cursor)?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(
            vec![PlayerEventAction::ActivateAbility(kind)],
            None,
        )));
    }
    if lemma_form_at(cursor, VerbLemma::Tap, agreement) {
        cursor.enter("event.player.tap-for-mana");
        consume_verb_word(cursor)?;
        let (query, exclude_source) = super::recognize_permanent_event_object(
            cursor,
            "event.player.tap-for-mana.object-head",
            "event.player.tap-for-mana.object-domain",
            "event.player.tap-for-mana.object-controller",
        )?;
        cursor.enter("event.player.tap-for-mana.purpose");
        cursor.consume_phrase(&["for", "mana"])?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(
            vec![PlayerEventAction::TapForMana],
            Some(PlayerEventCardObject {
                query,
                exclude_source,
            }),
        )));
    }
    if lemma_form_at(cursor, VerbLemma::Commit, agreement) {
        cursor.enter("event.player.commit");
        consume_verb_word(cursor)?;
        cursor.enter("event.player.commit.object");
        cursor.consume_phrase(&["a", "crime"])?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(vec![PlayerEventAction::CommitCrime], None)));
    }
    // Random and contested outcomes of the acting player (CR 701.30, 705,
    // 706): each verb is a lexicon lemma with its own complement.
    if let Some(action) = recognize_outcome_verb(cursor, agreement)? {
        cursor.enter("event.player.complete");
        return Ok(Some(event(vec![action], None)));
    }
    // `exert(s) PERMANENT` (CR 701.43): the exerted permanent is read by the
    // shared permanent-event object, as for `tap(s) PERMANENT for mana`.
    if lemma_form_at(cursor, VerbLemma::Exert, agreement) {
        cursor.enter("event.player.exert");
        consume_verb_word(cursor)?;
        let (query, exclude_source) = super::recognize_permanent_event_object(
            cursor,
            "event.player.exert.object-head",
            "event.player.exert.object-domain",
            "event.player.exert.object-controller",
        )?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(
            vec![PlayerEventAction::Exert],
            Some(PlayerEventCardObject {
                query,
                exclude_source,
            }),
        )));
    }
    // Keyword actions without a general effect lemma (CR 701.34 proliferate,
    // CR 309.7 completing a dungeon): the surface agrees with the actor.
    if keyword_surface_at(cursor, "proliferate", agreement) {
        cursor.enter("event.player.proliferate");
        consume_verb_word(cursor)?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(vec![PlayerEventAction::Proliferate], None)));
    }
    if keyword_surface_at(cursor, "complete", agreement) {
        cursor.enter("event.player.complete-dungeon");
        consume_verb_word(cursor)?;
        cursor.consume_phrase(&["a", "dungeon"])?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(vec![PlayerEventAction::CompleteDungeon], None)));
    }
    // `expend N` (CR 701.58) is a keyword action without a general effect
    // lemma: its surface agrees with the actor and takes a fixed ordinal.
    let expend = match agreement {
        VerbAgreement::Base => "expend",
        VerbAgreement::ThirdPersonSingular => "expends",
    };
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expend))
    {
        cursor.enter("event.player.expend");
        consume_verb_word(cursor)?;
        cursor.enter("event.player.expend.amount");
        let super::NumericAtom::Integer(amount) = super::recognize_numeric_atom(cursor)? else {
            return Err(cursor.fail("a fixed amount of total mana spent on spells"));
        };
        cursor.require(amount >= 1, "a positive expend amount")?;
        cursor.enter("event.player.complete");
        return Ok(Some(event(vec![PlayerEventAction::Expend(amount)], None)));
    }
    Ok(None)
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// The surface of a keyword action without a lexicon lemma, in `agreement`.
fn keyword_surface_at(
    cursor: &OracleTokenCursor<'_>,
    base: &str,
    agreement: VerbAgreement,
) -> bool {
    let expected = match agreement {
        VerbAgreement::Base => base.to_string(),
        VerbAgreement::ThirdPersonSingular => third_person_form(base),
    };
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(&expected))
}

/// `roll(s) DIE-ROLL`, `win(s)|lose(s) a coin flip`, `clash(es) [and
/// win(s)]`, `win(s) a clash`: the outcome events of CR 701.30, 705 and 706.
/// `None` when no outcome lemma is at the cursor (nothing consumed); once
/// the lemma is read, an unsupported complement fails explicitly.
fn recognize_outcome_verb(
    cursor: &mut OracleTokenCursor<'_>,
    agreement: VerbAgreement,
) -> MachineResult<Option<PlayerEventAction>> {
    if lemma_form_at(cursor, VerbLemma::Roll, agreement) {
        cursor.enter("event.player.roll");
        consume_verb_word(cursor)?;
        return recognize_die_roll(cursor).map(|filter| Some(PlayerEventAction::RollDie(filter)));
    }
    if lemma_form_at(cursor, VerbLemma::Clash, agreement) {
        cursor.enter("event.player.clash");
        consume_verb_word(cursor)?;
        // `clash and win` (CR 701.30d): the clash this player won.
        let won = word_at(cursor, 0, "and");
        if won {
            cursor.enter("event.player.clash.outcome");
            cursor.consume("and")?;
            cursor.require(
                lemma_form_at(cursor, VerbLemma::Win, agreement),
                "`win` agreeing with the clashing player",
            )?;
            consume_verb_word(cursor)?;
        }
        return Ok(Some(PlayerEventAction::Clash { won }));
    }
    let won = if lemma_form_at(cursor, VerbLemma::Win, agreement) {
        true
    } else if lemma_form_at(cursor, VerbLemma::Lose, agreement) && word_at(cursor, 1, "a") {
        false
    } else {
        return Ok(None);
    };
    cursor.enter("event.player.outcome");
    consume_verb_word(cursor)?;
    if word_at(cursor, 0, "a") && word_at(cursor, 1, "clash") {
        cursor.enter("event.player.outcome.clash");
        cursor.require(won, "a won clash (no lost-clash event is read)")?;
        cursor.consume_phrase(&["a", "clash"])?;
        return Ok(Some(PlayerEventAction::Clash { won: true }));
    }
    cursor.enter("event.player.outcome.coin-flip");
    cursor.consume_phrase(&["a", "coin", "flip"])?;
    Ok(Some(PlayerEventAction::FlipCoin { won }))
}

/// `a die` | `one or more dice` | `a N [or higher|or lower] [on a die]`
/// after `roll(s)` (CR 706.2): the result filter of the observed roll. Any
/// other die (`the planar die`, an ordinal die) or comparator fails.
fn recognize_die_roll(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<DieRollFilter> {
    let any = DieRollFilter {
        minimum: None,
        maximum: None,
        batch: false,
    };
    cursor.enter("event.player.roll.dice");
    if word_at(cursor, 0, "one") {
        cursor.consume_phrase(&["one", "or", "more", "dice"])?;
        return Ok(DieRollFilter { batch: true, ..any });
    }
    cursor.consume("a")?;
    if word_at(cursor, 0, "die") {
        cursor.consume("die")?;
        return Ok(any);
    }
    cursor.enter("event.player.roll.result");
    let value = cursor
        .peek()
        .and_then(|raw| raw.parse::<i64>().ok())
        .filter(|value| *value >= 1)
        .ok_or_else(|| cursor.fail("`die`, `one or more dice` or a positive die result"))?;
    consume_verb_word(cursor)?;
    let (minimum, maximum) = if word_at(cursor, 0, "or") {
        cursor.enter("event.player.roll.comparator");
        if word_at(cursor, 1, "higher") {
            cursor.consume_phrase(&["or", "higher"])?;
            (Some(value), None)
        } else if word_at(cursor, 1, "lower") {
            cursor.consume_phrase(&["or", "lower"])?;
            (None, Some(value))
        } else {
            return Err(cursor.fail("a die-result comparator (`or higher` or `or lower`)"));
        }
    } else {
        (Some(value), Some(value))
    };
    if word_at(cursor, 0, "on") {
        cursor.enter("event.player.roll.die");
        cursor.consume_phrase(&["on", "a", "die"])?;
    }
    Ok(DieRollFilter {
        minimum,
        maximum,
        batch: false,
    })
}

/// `a loyalty ability` | `an ability that isn't a mana ability`: the kinds of
/// activated abilities the activation record distinguishes. Any other
/// ability noun phrase fails explicitly.
fn recognize_activated_ability_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ActivatedAbilityKind> {
    cursor.enter("event.player.ability-query.article");
    let article = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .filter(|raw| raw == "a" || raw == "an")
        .ok_or_else(|| cursor.fail("`a` or `an` before the activated ability"))?;
    cursor.consume(&article)?;
    cursor.enter("event.player.ability-query.noun");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("loyalty"))
    {
        cursor.consume_phrase(&["loyalty", "ability"])?;
        return Ok(ActivatedAbilityKind::Loyalty);
    }
    cursor.consume("ability")?;
    cursor.enter("event.player.ability-query.relative");
    cursor.consume("that")?;
    let negation = cursor
        .peek()
        .filter(|raw| raw.eq_ignore_ascii_case("isn't") || raw.eq_ignore_ascii_case("isn’t"))
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("`that isn't a mana ability`"))?;
    cursor.consume(&negation)?;
    cursor.consume_phrase(&["a", "mana", "ability"])?;
    Ok(ActivatedAbilityKind::NonMana)
}

/// Reads `ACTOR attack(s) with N or more ATTACKERS` (CR 508.1): one attack
/// declaration of the actor containing at least N attackers matching the
/// shared object query. The attacking player controls the attackers, so the
/// query carries no second controller relation.
pub(in crate::oracle::canonical) fn recognize_player_attack_declaration(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<AttackDeclarationNode> {
    cursor.enter("event.attack-declaration.actor");
    let attacker = try_recognize_event_actor_scope(cursor)?
        .ok_or_else(|| cursor.fail("an attacking player"))?;
    let agreement = if attacker == EventActorScope::Controller {
        VerbAgreement::Base
    } else {
        VerbAgreement::ThirdPersonSingular
    };
    cursor.enter("event.attack-declaration.verb");
    consume_lemma(cursor, VerbLemma::Attack, agreement)?;
    cursor.enter("event.attack-declaration.with");
    cursor.consume("with")?;
    let attackers = recognize_quantified_event_subject(cursor)?;
    cursor.enter("event.attack-declaration.attackers");
    cursor.require(
        matches!(attackers.multiplicity, EventMultiplicity::PerEvent { .. }),
        "`N or more` attackers of one declaration",
    )?;
    cursor.require(
        attackers.query.controller.is_none() && !attackers.exclude_source,
        "attackers without a second controller relation or source exclusion",
    )?;
    Ok(AttackDeclarationNode {
        attacker,
        attackers,
        source: None,
    })
}

/// Whether an attacked recipient starts `offset` tokens after the cursor: a
/// player actor (`you`, `a player`, `an opponent`), the partitive `one of
/// your opponents`, a determiner of a permanent query (`a planeswalker`) or
/// a definite player relation (`the player with the most life`), which the
/// recipient reader owns and rejects at its predicate when unsupported.
fn attacked_recipient_starts_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let word = |extra: usize, expected: &str| {
        cursor
            .peek_ahead(offset + extra)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    ["you", "a", "an"].iter().any(|start| word(0, start))
        || (word(0, "one") && word(1, "of"))
        || (word(0, "the") && word(1, "player"))
}

/// Whether an attacked recipient starts at the cursor after `attack(s)`.
pub(in crate::oracle::canonical) fn attacked_recipient_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    attacked_recipient_starts_at(cursor, 0)
}

/// `one of your opponents` (CR 102.3): the partitive names one member of the
/// ability controller's opponents, the same player relation as `an opponent`.
fn consume_partitive_opponent(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<bool> {
    let partitive = ["one", "of", "your", "opponents"];
    if !partitive.iter().enumerate().all(|(offset, word)| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    }) {
        return Ok(false);
    }
    cursor.enter("event.attacked-recipient.partitive-opponent");
    cursor.consume_phrase(&partitive)?;
    Ok(true)
}

/// Reads `RECIPIENT [or RECIPIENT]...` after `attack(s)` (CR 506.2, 508.1b):
/// a player relative to the ability controller (`you`, `an opponent`, `one
/// of your opponents`, `a player`) or a planeswalker or battle (`a
/// planeswalker you control`) read by the shared object query. The
/// alternatives are one attribute of the attack pattern, not separate
/// events. `or` continues the list only when another recipient follows; an
/// `or` before another verb (`attacks a planeswalker or blocks`) belongs to
/// the enclosing occurrence union.
pub(in crate::oracle::canonical) fn recognize_attacked_recipients(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<AttackedRecipientNode>> {
    let mut recipients = Vec::new();
    loop {
        cursor.enter("event.attacked-recipient");
        if consume_partitive_opponent(cursor)? {
            recipients.push(AttackedRecipientNode::Player(EventActorScope::Opponent));
        } else if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("the"))
        {
            // `the player with the most life [or tied for most life]`: a
            // superlative player relation without a typed player predicate.
            cursor.consume_phrase(&["the", "player"])?;
            cursor.enter("event.attacked-recipient.player-predicate");
            return Err(cursor.fail(
                "a supported attacked-player relation (no superlative player predicate is typed)",
            ));
        } else if let Some(actor) = try_recognize_event_actor_scope(cursor)? {
            recipients.push(AttackedRecipientNode::Player(actor));
        } else {
            let article = cursor
                .peek()
                .map(str::to_ascii_lowercase)
                .filter(|raw| raw == "a" || raw == "an")
                .ok_or_else(|| cursor.fail("a player or `a`/`an` attacked permanent"))?;
            cursor.consume(&article)?;
            cursor.enter("event.attacked-recipient.query");
            let query = recognize_object_query(cursor)?;
            cursor.enter("event.attacked-recipient.domain");
            cursor.require(
                query.predicate.supports_domain(ObjectKind::Permanent)
                    && query.predicate.has_nominal_head()
                    && !query.exclude_source,
                "an attacked permanent with a nominal head",
            )?;
            recipients.push(AttackedRecipientNode::Permanent(query));
        }
        if !(cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
            && attacked_recipient_starts_at(cursor, 1))
        {
            return Ok(recipients);
        }
        cursor.enter("event.attacked-recipient.union");
        cursor.consume("or")?;
    }
}

#[cfg(test)]
#[path = "player_event_tests.rs"]
mod tests;
