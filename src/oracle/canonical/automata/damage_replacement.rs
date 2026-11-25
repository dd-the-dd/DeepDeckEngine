//! Damage-event replacement and prevention frames (CR 614.1a, 615.1a, 616.1).
//!
//! ```text
//! Frame      := "if" Event "," Modification "."
//! Event      := Source DealClause[would, active]:
//!                 [Threshold] [Kind] "damage" ["to" Recipients]
//!             | DealClause[would, passive, damage subject]:
//!                 [Kind] "damage" "would" "be" "dealt" "to" Recipients
//!               (both are the shared clause, `clause.rs` / `damage_clause.rs`)
//! Threshold  := N "or" ("more" | "less")
//! Kind       := "combat" | "noncombat"
//! Source     := this source | Det ["other"] (Query | Adjectives "source") [Relation]
//! Recipients := Recipient ("or" Recipient)*
//! Modification := ["instead"] Anaphor DealClause[event quantity] ["to" Anaphor] ["instead"]
//!               | "prevent" ("that damage" | Amount "of that damage"
//!                            | "all" "but" N "of that damage")
//! ```
//!
//! The event pattern (source, recipients, damage kind and amount guard) and
//! its modification are independent typed children. Sources and recipients
//! reuse the shared object query, controller relations, source references and
//! source exclusion; amounts reuse the shared numeric atoms and arithmetic
//! nodes bound to the replaced event's quantity. Prevention is a separate
//! modification kind because damage that can't be prevented ignores it
//! (CR 615.12) while an ordinary replacement still applies.
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

use super::super::{MachineFailure, MachineResult, OracleTokenCursor};
use super::DamageKind;
use super::{
    Clause, ClauseAction, ClauseArguments, ClauseFrame, ClauseModality, ClauseRecognition,
    ClauseSubject, ClauseSubjectPolicy, ClauseTense, DEAL, DamageArguments, DamageClauseArguments,
    EventNumericBinding, NumericExpression, ObjectControllerScope, ObjectKind, ObjectPredicate,
    VerbAgreement, Voice, event_quantity_scale_starts, recognize_clause,
    recognize_event_quantity_expression, recognize_literal_quantity, recognize_object_query,
    recognize_optional_source_exclusion, recognize_source_reference,
    recognize_supplied_damage_clause, source_reference_starts_at, supplied_damage_frame,
};

/// The source of the damage event. A query keeps its shared predicate (absent
/// for a bare `source`, which admits every object) and its controller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageSourceNode {
    SourceSelf,
    /// The permanent the source Equipment or Aura is attached to.
    AttachedPermanent,
    Objects {
        predicate: Option<ObjectPredicate>,
        controller: Option<ObjectControllerScope>,
        exclude_source: bool,
        /// CR 109.2: `a Goblin source` is any object, not a permanent; the
        /// noun cancels the subtype-head inference.
        source_noun: bool,
    },
}

/// A player recipient relative to the ability controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamagePlayerRecipient {
    You,
    Opponent,
    AnyPlayer,
}

/// One alternative recipient of the damage event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageEventRecipient {
    Player(DamagePlayerRecipient),
    Permanents {
        predicate: Option<ObjectPredicate>,
        controller: Option<ObjectControllerScope>,
        exclude_source: bool,
    },
    SourceSelf,
    /// The permanent the source Equipment or Aura is attached to.
    AttachedPermanent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DamageEventNode {
    pub source: Option<DamageSourceNode>,
    pub kind: DamageKind,
    pub minimum: Option<i64>,
    pub maximum: Option<i64>,
    /// Empty when the event names no recipient (`would deal damage`).
    pub recipients: Vec<DamageEventRecipient>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum DamageModificationNode {
    /// The event's damage becomes this amount (CR 614.1a).
    Amount(NumericExpression),
    /// This much of the event's damage is prevented (CR 615.1a).
    Prevent(NumericExpression),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct DamageReplacementNode {
    pub event: DamageEventNode,
    pub modification: DamageModificationNode,
}

pub(in crate::oracle::canonical) enum DamageReplacementRecognition {
    Parsed(DamageReplacementNode),
    NotClaimed,
    Failed(MachineFailure),
}

pub(in crate::oracle::canonical) struct DamageReplacementContext<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    /// Whether the source face is an Equipment or Aura, the only faces whose
    /// `equipped`/`enchanted` creature is an attached permanent (CR 301.5).
    pub attachment_word: Option<&'static str>,
}

fn peek_is(cursor: &OracleTokenCursor<'_>, word: &str) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

fn ahead_is(cursor: &OracleTokenCursor<'_>, offset: usize, word: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
}

/// The damage event, not the leading `if`, commits this frame: a `would deal`
/// or `damage would be dealt` event read after the source. Other conditional
/// envelopes are restored to the caller unchanged.
pub(in crate::oracle::canonical) fn try_recognize_damage_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> DamageReplacementRecognition {
    let checkpoint = cursor.checkpoint();
    cursor.enter("replacement.damage-event.envelope");
    if cursor.consume("if").is_err() {
        cursor.rollback(checkpoint);
        return DamageReplacementRecognition::NotClaimed;
    }
    let mut committed = false;
    match recognize_damage_replacement_inner(cursor, context, &mut committed) {
        Ok(node) => DamageReplacementRecognition::Parsed(node),
        Err(failure) if committed => DamageReplacementRecognition::Failed(failure),
        Err(_) => {
            cursor.rollback(checkpoint);
            DamageReplacementRecognition::NotClaimed
        }
    }
}

fn recognize_damage_replacement_inner(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
    committed: &mut bool,
) -> MachineResult<DamageReplacementNode> {
    let event = recognize_damage_event(cursor, context, committed)?;
    cursor.enter("replacement.damage-event.separator");
    cursor.consume(",")?;
    let modification = recognize_damage_modification(cursor, &event, context)?;
    cursor.enter("replacement.damage-event.terminator");
    cursor.consume(".")?;
    cursor.enter("replacement.damage-event.complete");
    cursor.finish()?;
    Ok(DamageReplacementNode {
        event,
        modification,
    })
}

fn recognize_damage_event(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
    committed: &mut bool,
) -> MachineResult<DamageEventNode> {
    // `[Kind] damage would be dealt to RECIPIENTS`: the passive whose subject
    // is the damage. A claimed clause commits this frame.
    cursor.enter("replacement.damage-event.passive");
    let passive = ClauseFrame {
        subject: ClauseSubjectPolicy::DamageTheme,
        modality: ClauseModality::Would,
        tense: ClauseTense::Present,
        voice: Voice::Passive,
        arguments: vec![ClauseArguments::Damage(
            DamageArguments::ReplacementRecipients(context),
        )],
        lemmas: DEAL,
    };
    match recognize_clause(cursor, &passive) {
        ClauseRecognition::Recognized(Clause {
            subject: ClauseSubject::DamageTheme(kind),
            action: ClauseAction::DealDamage(DamageClauseArguments::Recipients(recipients)),
            ..
        }) => {
            *committed = true;
            return Ok(DamageEventNode {
                source: None,
                kind,
                minimum: None,
                maximum: None,
                recipients,
            });
        }
        ClauseRecognition::Recognized(_) => {
            *committed = true;
            return Err(cursor.fail("a damage subject and its recipients"));
        }
        ClauseRecognition::Failed(failure) => {
            *committed = true;
            return Err(failure);
        }
        ClauseRecognition::NotClaimed => {}
    }
    let source = recognize_damage_source(cursor, context)?;
    recognize_would_deal_event(cursor, context, source, committed)
}

/// `SOURCE of your choice would deal [KIND] damage [to RECIPIENTS]`: the
/// event of a created next-time shield (CR 615.7), whose source the
/// controller chooses on resolution. The shared source and damage-clause
/// children read it; only the choice phrase is added.
pub(in crate::oracle::canonical) fn recognize_chosen_source_damage_event(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<DamageEventNode> {
    let source = recognize_damage_source(cursor, context)?;
    cursor.enter("replacement.damage-source.choice");
    cursor.consume_phrase(&["of", "your", "choice"])?;
    let mut committed = false;
    recognize_would_deal_event(cursor, context, source, &mut committed)
}

/// `would deal [KIND] damage [to RECIPIENTS]` after a read source.
fn recognize_would_deal_event(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
    source: DamageSourceNode,
    committed: &mut bool,
) -> MachineResult<DamageEventNode> {
    // `SOURCE would deal ...`: the clause's `would` modality and the
    // replacement policy of the damage frame. A claimed clause commits.
    cursor.enter("replacement.damage-event.verb");
    let active = supplied_damage_frame(
        VerbAgreement::Base,
        Voice::Active,
        ClauseModality::Would,
        DamageArguments::ReplacementEvent(context),
    );
    let claim = recognize_clause(cursor, &active);
    *committed = !matches!(claim, ClauseRecognition::NotClaimed);
    let Some(clause) = claim.into_result()? else {
        return Err(cursor.fail("`would deal` and the damage of the replaced event"));
    };
    let ClauseAction::DealDamage(DamageClauseArguments::ReplacementEvent {
        kind,
        minimum,
        maximum,
        recipients,
    }) = clause.action
    else {
        return Err(cursor.fail("the replaced damage event"));
    };
    Ok(DamageEventNode {
        source: Some(source),
        kind,
        minimum,
        maximum,
        recipients,
    })
}

/// `N or more` / `N or less` damage: a guard on the event's amount, checked
/// each time the replacement is considered (CR 616.1, after other effects).
pub(in crate::oracle::canonical) fn recognize_damage_threshold(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<(Option<i64>, Option<i64>)> {
    cursor.enter("replacement.damage-event.threshold");
    if peek_is(cursor, "damage") || peek_is(cursor, "combat") || peek_is(cursor, "noncombat") {
        return Ok((None, None));
    }
    let value = recognize_literal_quantity(
        cursor,
        "replacement.damage-event.threshold-literal",
        "a literal damage threshold",
    )?;
    cursor.consume("or")?;
    cursor.enter("replacement.damage-event.threshold-direction");
    if peek_is(cursor, "more") {
        cursor.consume("more")?;
        Ok((Some(value), None))
    } else {
        cursor.consume("less")?;
        Ok((None, Some(value)))
    }
}

/// The source of a damage event: the ability's own object, or a query whose
/// head is either a shared object noun (`a creature you control`, `a red
/// spell`) or the event-supplied noun `source` after shared characteristics
/// (`a red or artifact source you control`). A source is any object, so the
/// event, not the query, supplies that head (CR 120.1, 609.7).
pub(in crate::oracle::canonical) fn recognize_damage_source(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<DamageSourceNode> {
    cursor.enter("replacement.damage-source.start");
    // The attachment noun is read first: an unknown `enchanted` must not be
    // taken for an unresolved card name.
    if recognize_attachment_reference(cursor, context)? {
        return Ok(DamageSourceNode::AttachedPermanent);
    }
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        context.name_links,
        context.name_failure,
    ) {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        return Ok(DamageSourceNode::SourceSelf);
    }
    cursor.enter("replacement.damage-source.determiner");
    let exclude_source = if peek_is(cursor, "another") {
        cursor.consume("another")?;
        true
    } else {
        if peek_is(cursor, "a") || peek_is(cursor, "an") || peek_is(cursor, "any") {
            let article = cursor.peek().expect("checked article").to_string();
            cursor.consume(&article)?;
        } else {
            return Err(cursor.fail("a damage source determiner"));
        }
        recognize_optional_source_exclusion(cursor)?
    };
    cursor.enter("replacement.damage-source.head");
    if peek_is(cursor, "source") {
        cursor.consume("source")?;
        let controller = recognize_optional_controller_relation(cursor)?;
        return Ok(DamageSourceNode::Objects {
            predicate: None,
            controller,
            exclude_source,
            source_noun: true,
        });
    }
    let query = recognize_object_query(cursor)?;
    cursor.enter("replacement.damage-source.source-noun");
    if peek_is(cursor, "source") {
        cursor.consume("source")?;
        cursor.require(
            query.controller.is_none(),
            "a controller relation after the `source` noun",
        )?;
        let controller = recognize_optional_controller_relation(cursor)?;
        return Ok(DamageSourceNode::Objects {
            predicate: Some(query.predicate),
            controller,
            exclude_source,
            source_noun: true,
        });
    }
    cursor.enter("replacement.damage-source.query-domain");
    cursor.require(
        query.predicate.has_nominal_head(),
        "a damage source noun (an object type or `source`)",
    )?;
    Ok(DamageSourceNode::Objects {
        predicate: Some(query.predicate),
        controller: query.controller,
        exclude_source,
        source_noun: false,
    })
}

/// A controller relation after the event-supplied `source` head. It reads the
/// same subjects as the shared query relation (`you control`, `an opponent
/// controls`, `your opponents control`, `you don't control`).
fn recognize_optional_controller_relation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Option<ObjectControllerScope>> {
    cursor.enter("replacement.damage-source.controller");
    if peek_is(cursor, "you") {
        cursor.consume("you")?;
        if peek_is(cursor, "don't") {
            cursor.consume("don't")?;
            cursor.consume("control")?;
            return Ok(Some(ObjectControllerScope::NotAbilityController));
        }
        cursor.consume("control")?;
        return Ok(Some(ObjectControllerScope::AbilityController));
    }
    if peek_is(cursor, "an") && ahead_is(cursor, 1, "opponent") {
        cursor.consume_phrase(&["an", "opponent", "controls"])?;
        return Ok(Some(ObjectControllerScope::OpponentOfAbilityController));
    }
    if peek_is(cursor, "your") && ahead_is(cursor, 1, "opponents") {
        cursor.consume_phrase(&["your", "opponents", "control"])?;
        return Ok(Some(ObjectControllerScope::OpponentOfAbilityController));
    }
    Ok(None)
}

/// `equipped creature` / `enchanted creature` on the matching attachment face.
fn recognize_attachment_reference(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<bool> {
    let Some(word) = context.attachment_word else {
        return Ok(false);
    };
    if !(peek_is(cursor, word) && ahead_is(cursor, 1, "creature")) {
        return Ok(false);
    }
    cursor.enter("reference.attached-permanent");
    cursor.consume_phrase(&[word, "creature"])?;
    Ok(true)
}

pub(in crate::oracle::canonical) fn recognize_damage_replacement_recipients(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<Vec<DamageEventRecipient>> {
    let mut recipients = vec![recognize_damage_recipient(cursor, context, false)?];
    loop {
        cursor.enter("replacement.damage-recipient.connector");
        if !peek_is(cursor, "or") {
            return Ok(recipients);
        }
        cursor.consume("or")?;
        let recipient = recognize_damage_recipient(cursor, context, true)?;
        cursor.enter("replacement.damage-recipient.distinct");
        cursor.require(
            !recipients.contains(&recipient),
            "distinct alternative damage recipients",
        )?;
        recipients.push(recipient);
    }
}

/// One alternative recipient. After `or`, the shared article may be elided
/// (`a permanent or player`).
fn recognize_damage_recipient(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
    coordinated: bool,
) -> MachineResult<DamageEventRecipient> {
    cursor.enter("replacement.damage-recipient.start");
    if peek_is(cursor, "you") {
        cursor.consume("you")?;
        return Ok(DamageEventRecipient::Player(DamagePlayerRecipient::You));
    }
    if recognize_attachment_reference(cursor, context)? {
        return Ok(DamageEventRecipient::AttachedPermanent);
    }
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        context.name_links,
        context.name_failure,
    ) {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        return Ok(DamageEventRecipient::SourceSelf);
    }
    cursor.enter("replacement.damage-recipient.determiner");
    let exclude_source = if peek_is(cursor, "another") {
        cursor.consume("another")?;
        true
    } else {
        if peek_is(cursor, "a") || peek_is(cursor, "an") {
            let article = cursor.peek().expect("checked article").to_string();
            cursor.consume(&article)?;
        } else {
            cursor.require(coordinated, "a damage recipient determiner")?;
        }
        recognize_optional_source_exclusion(cursor)?
    };
    cursor.enter("replacement.damage-recipient.player");
    if !exclude_source && (peek_is(cursor, "player") || peek_is(cursor, "opponent")) {
        let player = if peek_is(cursor, "player") {
            cursor.consume("player")?;
            DamagePlayerRecipient::AnyPlayer
        } else {
            cursor.consume("opponent")?;
            DamagePlayerRecipient::Opponent
        };
        return Ok(DamageEventRecipient::Player(player));
    }
    cursor.enter("replacement.damage-recipient.permanent");
    let query = recognize_object_query(cursor)?;
    // Only permanents and players are dealt damage (CR 120.1), so the event
    // supplies the permanent domain to a nominal head such as `a Hero`.
    cursor.require(
        query.predicate.has_nominal_head()
            && query.predicate.supports_domain(ObjectKind::Permanent),
        "a damage recipient in the battlefield-permanent domain",
    )?;
    Ok(DamageEventRecipient::Permanents {
        predicate: Some(query.predicate),
        controller: query.controller,
        exclude_source,
    })
}

fn recipient_is_player(recipient: &DamageEventRecipient) -> bool {
    matches!(recipient, DamageEventRecipient::Player(_))
}

fn recognize_damage_modification(
    cursor: &mut OracleTokenCursor<'_>,
    event: &DamageEventNode,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<DamageModificationNode> {
    cursor.enter("replacement.damage-modification.start");
    if peek_is(cursor, "prevent") {
        return recognize_damage_prevention(cursor);
    }
    let leading_instead = peek_is(cursor, "instead");
    if leading_instead {
        cursor.consume("instead")?;
    }
    cursor.enter("replacement.damage-modification.subject");
    cursor.require(
        event.source.is_some(),
        "a damage source named by the event for the replacing action's subject",
    )?;
    if peek_is(cursor, "it") {
        cursor.consume("it")?;
    } else if peek_is(cursor, "that") && ahead_is(cursor, 1, "source") {
        cursor.consume_phrase(&["that", "source"])?;
    } else {
        recognize_source_reference(cursor, context.name_links, context.name_failure)?;
        cursor.require(
            event.source == Some(DamageSourceNode::SourceSelf),
            "the event's own source as the replacing action's subject",
        )?;
    }
    cursor.enter("replacement.damage-modification.verb");
    let frame = supplied_damage_frame(
        VerbAgreement::ThirdPersonSingular,
        Voice::Active,
        ClauseModality::Indicative,
        DamageArguments::EventQuantity,
    );
    let Some(DamageClauseArguments::Amount(amount)) =
        recognize_supplied_damage_clause(cursor, &frame)?
    else {
        return Err(cursor.fail("`deals` and the modified damage"));
    };
    if peek_is(cursor, "to") {
        cursor.consume("to")?;
        recognize_recipient_anaphor(cursor, event, context)?;
    }
    cursor.enter("replacement.damage-modification.instead");
    if !leading_instead {
        cursor.consume("instead")?;
    }
    Ok(DamageModificationNode::Amount(amount))
}

/// The modified event keeps its recipient (CR 614.6): a trailing `to that
/// player` must name a recipient the event already admits.
fn recognize_recipient_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    event: &DamageEventNode,
    context: &DamageReplacementContext<'_>,
) -> MachineResult<()> {
    cursor.enter("replacement.damage-modification.recipient-anaphor");
    cursor.require(
        !event.recipients.is_empty(),
        "a recipient named by the replaced event",
    )?;
    if !peek_is(cursor, "that") {
        // A definite recipient (`you`, `this creature`, `equipped creature`)
        // restates the event's single recipient with the same child.
        let restated = recognize_damage_recipient(cursor, context, false)?;
        cursor.enter("replacement.damage-modification.recipient-restated");
        return cursor.require(
            event.recipients == [restated.clone()]
                && matches!(
                    restated,
                    DamageEventRecipient::Player(DamagePlayerRecipient::You)
                        | DamageEventRecipient::SourceSelf
                        | DamageEventRecipient::AttachedPermanent
                ),
            "the event's single definite recipient",
        );
    }
    cursor.consume("that")?;
    let mut nouns = Vec::new();
    loop {
        let noun = cursor
            .peek()
            .map(str::to_ascii_lowercase)
            .filter(|noun| matches!(noun.as_str(), "player" | "permanent" | "creature"))
            .ok_or_else(|| cursor.fail("`player`, `permanent` or `creature`"))?;
        cursor.consume(&noun)?;
        nouns.push(noun);
        if !peek_is(cursor, "or") {
            break;
        }
        cursor.consume("or")?;
    }
    let has_player = event.recipients.iter().any(recipient_is_player);
    let has_permanent = event.recipients.iter().any(|r| !recipient_is_player(r));
    cursor.require(
        nouns.iter().any(|noun| noun == "player") == has_player
            && nouns.iter().any(|noun| noun != "player") == has_permanent,
        "an anaphor naming exactly the replaced event's recipient kinds",
    )
}

fn recognize_damage_prevention(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<DamageModificationNode> {
    cursor.enter("replacement.damage-prevention.action");
    cursor.consume("prevent")?;
    let event_value = NumericExpression::EventValue(EventNumericBinding::ReplacementQuantity);
    if peek_is(cursor, "that") {
        cursor.enter("replacement.damage-prevention.all");
        cursor.consume_phrase(&["that", "damage"])?;
        return Ok(DamageModificationNode::Prevent(event_value));
    }
    if event_quantity_scale_starts(cursor) {
        cursor.enter("replacement.damage-prevention.scaled");
        return Ok(DamageModificationNode::Prevent(
            recognize_event_quantity_expression(cursor, "damage")?,
        ));
    }
    let amount = if peek_is(cursor, "all") {
        cursor.enter("replacement.damage-prevention.all-but");
        cursor.consume_phrase(&["all", "but"])?;
        let kept = recognize_literal_quantity(
            cursor,
            "replacement.damage-prevention.literal",
            "a literal amount of damage left unprevented",
        )?;
        NumericExpression::subtract(event_value, NumericExpression::from(kept))
    } else {
        cursor.enter("replacement.damage-prevention.amount");
        let amount = recognize_literal_quantity(
            cursor,
            "replacement.damage-prevention.literal",
            "a literal amount of prevented damage",
        )?;
        NumericExpression::from(amount)
    };
    cursor.enter("replacement.damage-prevention.partitive");
    cursor.consume_phrase(&["of", "that", "damage"])?;
    Ok(DamageModificationNode::Prevent(amount))
}

/// `[Until end of turn,] if EVENT [this turn], MODIFICATION.` read as one
/// sentence of a resolving effect: a damage replacement the effect creates
/// through this turn (CR 611.2a, 614.1a). Lexical lookahead only.
pub(in crate::oracle::canonical) fn created_damage_replacement_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let offset = if ["until", "end", "of", "turn", ","]
        .iter()
        .enumerate()
        .all(|(offset, word)| ahead_is(cursor, offset, word))
    {
        5
    } else {
        0
    };
    if !ahead_is(cursor, offset, "if") {
        return false;
    }
    (offset + 1..)
        .take_while(|index| {
            cursor
                .peek_ahead(*index)
                .is_some_and(|raw| raw != "," && raw != ".")
        })
        .any(|index| {
            (ahead_is(cursor, index, "would") && ahead_is(cursor, index + 1, "deal"))
                || (ahead_is(cursor, index, "damage")
                    && ahead_is(cursor, index + 1, "would")
                    && ahead_is(cursor, index + 2, "be"))
        })
}

/// Reads one created damage replacement sentence through its period. The
/// lifetime is a preposed `Until end of turn,` or a `this turn` closing the
/// event before its comma; the event and the modification are the static
/// frame's children. `NoMatch` until the damage event commits.
pub(in crate::oracle::canonical) fn try_recognize_created_damage_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
) -> super::super::ChildOutcome<DamageReplacementNode> {
    use super::super::ChildOutcome;
    let checkpoint = cursor.checkpoint();
    let mut committed = false;
    match recognize_created_damage_replacement(cursor, context, &mut committed) {
        Ok(node) => ChildOutcome::Matched(node),
        Err(failure) => {
            cursor.rollback(checkpoint);
            if committed {
                ChildOutcome::Committed(failure)
            } else {
                ChildOutcome::NoMatch(failure)
            }
        }
    }
}

fn recognize_created_damage_replacement(
    cursor: &mut OracleTokenCursor<'_>,
    context: &DamageReplacementContext<'_>,
    committed: &mut bool,
) -> MachineResult<DamageReplacementNode> {
    cursor.enter("replacement.created-damage.lifetime");
    let preposed = peek_is(cursor, "until");
    if preposed {
        cursor.consume_phrase(&["until", "end", "of", "turn", ","])?;
    }
    cursor.enter("replacement.created-damage.envelope");
    cursor.consume("if")?;
    let comma = (0..)
        .take_while(|offset| cursor.peek_ahead(*offset).is_some())
        .find(|offset| cursor.peek_ahead(*offset) == Some(","))
        .map(|offset| cursor.position() + offset)
        .ok_or_else(|| cursor.fail("the comma closing the damage event"))?;
    let postposed = comma >= cursor.position() + 2 && {
        let mut probe = cursor.clone();
        probe.advance_to(comma - 2).is_ok()
            && peek_is(&probe, "this")
            && ahead_is(&probe, 1, "turn")
    };
    let event_end = if postposed { comma - 2 } else { comma };
    let event = cursor.bounded(event_end, |cursor| {
        recognize_damage_event(cursor, context, committed)
    })?;
    *committed = true;
    cursor.enter("replacement.created-damage.lifetime.postposed");
    if postposed {
        cursor.consume_phrase(&["this", "turn"])?;
    }
    cursor.require(
        preposed != postposed,
        "exactly one lifetime: `Until end of turn,` or `this turn`",
    )?;
    cursor.enter("replacement.damage-event.separator");
    cursor.consume(",")?;
    let modification = recognize_damage_modification(cursor, &event, context)?;
    cursor.enter("replacement.damage-event.terminator");
    cursor.consume(".")?;
    Ok(DamageReplacementNode {
        event,
        modification,
    })
}
