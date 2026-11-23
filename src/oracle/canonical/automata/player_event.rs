//! Player events (CR 603.2): one shared actor, then one or more verbs the
//! actor performs.
//!
//! ```text
//! PlayerEvent  := Actor FixedVerb
//!               | Actor ObjectVerb ("or" ObjectVerb)* CardObject
//!               | Actor NamedVerb ("or" NamedVerb)*
//!               | Actor LifeClause    (the shared clause, `clause.rs`)
//! Actor        := "you" | "a player" | "an opponent" | "one or more" PLAYERS
//! FixedVerb    := "draw(s) a card" | "attack(s)" | ...
//! ObjectVerb   := "discard(s)" | "cycle(s)"
//! CardObject   := ("a" | "an" | "another") CardQuery
//! NamedVerb    := a keyword action whose catalog entry publishes a completed
//!                 named action (CR 701.22d scry, 701.25 surveil, ...)
//! ```
//!
//! Actor, verbs and the card object are independent attributes. A union
//! selects one event occurrence; it shares the actor and the object.
use crate::oracle::ability_catalog::{AbilitySignalPolicy, lookup_exact_lexeme, lookup_registry};

use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ArgumentFrame, ClauseAction, ClauseAmountPolicy, ClauseArguments, ClauseFrame, ClauseModality,
    ClauseSubjectPolicy, ClauseTense, DamageArguments, DamageClauseArguments, DamageKind,
    EventActorScope, LIFE_LEMMAS, LocalTriggerEventNode, ObjectKind, ObjectQueryNode,
    TargetPlayerLifeAction, VerbAgreement, VerbLemma, Voice, recognize_clause,
    recognize_object_query, recognize_operand_verb, recognize_supplied_damage_clause,
    supplied_damage_frame, try_recognize_event_actor_scope, try_recognize_plural_event_actor_scope,
};

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
}

impl PlayerEventAction {
    /// Base and third-person verb forms followed by the fixed complement.
    fn fixed_phrase(self, third_person: bool) -> Option<&'static [&'static str]> {
        Some(match (self, third_person) {
            (Self::DrawCard, false) => &["draw", "a", "card"],
            (Self::DrawCard, true) => &["draws", "a", "card"],
            (Self::Attack, false) => &["attack"],
            (Self::Attack, true) => &["attacks"],
            (Self::FaceDilemma, false) => &["face", "a", "dilemma"],
            (Self::FaceDilemma, true) => &["faces", "a", "dilemma"],
            // Life verbs are the shared life clause's; object and named
            // verbs are union verbs.
            (
                Self::GainLife
                | Self::LoseLife
                | Self::BeDealtDamage(_)
                | Self::DiscardCard
                | Self::CycleCard
                | Self::NamedAction(_),
                _,
            ) => return None,
        })
    }

    /// The runtime records the acting player of each occurrence as the
    /// triggering player, so `that player` can name them in the body.
    pub(in crate::oracle::canonical) fn records_acting_player(self) -> bool {
        matches!(
            self,
            Self::LoseLife
                | Self::DrawCard
                | Self::DiscardCard
                | Self::CycleCard
                | Self::NamedAction(_)
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
        });
    }
    for action in FIXED_PLAYER_EVENT_ACTIONS {
        let phrase = action.fixed_phrase(third_person).expect("fixed phrase");
        if (0..phrase.len()).all(|offset| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(phrase[offset]))
        }) {
            cursor.consume_phrase(phrase)?;
            return Ok(LocalTriggerEventNode::PlayerEvent {
                actor,
                actions: vec![action],
                one_or_more,
                card: None,
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
    })
}

#[cfg(test)]
#[path = "player_event_tests.rs"]
mod tests;
