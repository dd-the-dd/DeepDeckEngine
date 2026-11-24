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
            _ => None,
        }
    }

    /// The fixed phrase of a verb without a lexicon lemma (`face(s) a
    /// dilemma`, a named mechanic action).
    fn fixed_phrase(self, third_person: bool) -> Option<&'static [&'static str]> {
        Some(match (self, third_person) {
            (Self::FaceDilemma, false) => &["face", "a", "dilemma"],
            (Self::FaceDilemma, true) => &["faces", "a", "dilemma"],
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
        )
    }

    /// Verbs that take the shared card object.
    pub(in crate::oracle::canonical) fn takes_card_object(self) -> bool {
        matches!(self, Self::DiscardCard | Self::CycleCard)
    }
}

const FIXED_PLAYER_EVENT_ACTIONS: [PlayerEventAction; 3] = [
    PlayerEventAction::DrawCard,
    PlayerEventAction::Attack,
    PlayerEventAction::FaceDilemma,
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
    Ok(None)
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

/// Whether an attacked recipient starts at the cursor after `attack(s)`.
pub(in crate::oracle::canonical) fn attacked_recipient_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        ["you", "a", "an"]
            .iter()
            .any(|word| raw.eq_ignore_ascii_case(word))
    })
}

/// Reads `RECIPIENT [or RECIPIENT]...` after `attack(s)` (CR 506.2, 508.1b):
/// a player relative to the ability controller (`you`, `an opponent`, `a
/// player`) or a planeswalker or battle (`a planeswalker you control`) read
/// by the shared object query. The alternatives are one attribute of the
/// attack pattern, not separate events.
pub(in crate::oracle::canonical) fn recognize_attacked_recipients(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<AttackedRecipientNode>> {
    let mut recipients = Vec::new();
    loop {
        cursor.enter("event.attacked-recipient");
        if let Some(actor) = try_recognize_event_actor_scope(cursor)? {
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
        if !cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
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
