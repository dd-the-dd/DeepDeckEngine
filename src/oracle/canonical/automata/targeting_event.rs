//! The targeting frame of `become` (CR 115.1, 603.2): `SUBJECT becomes the
//! target of STACK-OBJECT`. The subject is either a union of targeted-object
//! selectors (this module) or the ability source (`source_event_union.rs`);
//! both share the targeting stack-object selector read here. The enclosing
//! event clause (`event_clause.rs`) owns the trigger word and separator.
use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ObjectControllerScope, ObjectKind, ObjectPredicate, VerbAgreement, VerbLemma, consume_lemma,
    recognize_object_query,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetedObjectDomain {
    Permanent,
    Spell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetedObjectSelectorNode {
    pub domain: TargetedObjectDomain,
    pub predicate: ObjectPredicate,
    /// The selector's controller relation, an optional typed field of the
    /// shared object query: `None` is an object of any controller.
    pub controller: Option<ObjectControllerScope>,
    /// `another PERMANENT`: the ability source is excluded.
    pub exclude_source: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetingStackKind {
    Spell,
    ActivatedAbility,
    TriggeredAbility,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetingStackSelectorNode {
    pub kinds: Vec<TargetingStackKind>,
    /// `you control` / `an opponent controls`; `None` is any controller.
    pub controller: Option<ObjectControllerScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BecameTargetEventNode {
    pub objects: Vec<TargetedObjectSelectorNode>,
    pub targeting_stack: TargetingStackSelectorNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SubjectState {
    Article,
    Selector,
    ConnectorOrEvent,
    Complete,
}

fn recognize_targeted_object_selectors(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<Vec<TargetedObjectSelectorNode>> {
    let mut state = SubjectState::Article;
    let mut selectors = Vec::new();
    let mut exclude_source = false;
    loop {
        state = match state {
            SubjectState::Article => {
                cursor.enter("event.became-target.subject.article");
                let article = cursor
                    .peek()
                    .filter(|raw| {
                        raw.eq_ignore_ascii_case("a")
                            || raw.eq_ignore_ascii_case("an")
                            || raw.eq_ignore_ascii_case("another")
                    })
                    .ok_or_else(|| cursor.fail("`a` or `an` before a targeted-object selector"))?
                    .to_string();
                exclude_source = article.eq_ignore_ascii_case("another");
                cursor.consume(&article)?;
                SubjectState::Selector
            }
            SubjectState::Selector => {
                cursor.enter("event.became-target.subject.selector");
                let query = recognize_object_query(cursor)?;
                let domain = if query.predicate.has_domain(ObjectKind::Spell) {
                    cursor.require(
                        query.predicate.supports_domain(ObjectKind::Spell),
                        "a predicate valid for spells",
                    )?;
                    TargetedObjectDomain::Spell
                } else {
                    cursor.require(
                        query.denotes_battlefield_permanents(),
                        "a predicate establishing a permanent or spell domain",
                    )?;
                    TargetedObjectDomain::Permanent
                };
                cursor.require(
                    !(exclude_source && domain == TargetedObjectDomain::Spell),
                    "`another` before a permanent selector (a spell is never the source)",
                )?;
                selectors.push(TargetedObjectSelectorNode {
                    domain,
                    predicate: query.predicate,
                    controller: query.controller,
                    exclude_source,
                });
                SubjectState::ConnectorOrEvent
            }
            SubjectState::ConnectorOrEvent => {
                cursor.enter("event.became-target.subject.connector-or-event");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    SubjectState::Article
                } else {
                    SubjectState::Complete
                }
            }
            SubjectState::Complete => {
                cursor.enter("event.became-target.subject.complete");
                cursor.require(
                    !selectors.is_empty(),
                    "at least one targeted-object selector",
                )?;
                return Ok(selectors);
            }
        };
    }
}

fn recognize_controller_scope(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ObjectControllerScope> {
    cursor.enter("event.became-target.source-controller.subject");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("you"))
    {
        cursor.consume("you")?;
        cursor.enter("event.became-target.source-controller.relation");
        cursor.consume("control")?;
        return Ok(ObjectControllerScope::AbilityController);
    }
    cursor.consume("an")?;
    cursor.enter("event.became-target.source-controller.opponent");
    cursor.consume("opponent")?;
    cursor.enter("event.became-target.source-controller.relation");
    cursor.consume("controls")?;
    Ok(ObjectControllerScope::OpponentOfAbilityController)
}

/// A written controller relation of the targeting stack object starts here.
fn controller_scope_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    (word(0, "you") && word(1, "control")) || (word(0, "an") && word(1, "opponent"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StackSourceState {
    Article,
    Kind,
    ConnectorOrController,
    Controller,
    Complete,
}

/// Reads `a spell [or ability] [CONTROLLER]` after `becomes the target of`.
/// The controller relation is an optional typed field: absent, the spell or
/// ability of any player (CR 115.1).
pub(in crate::oracle::canonical) fn recognize_targeting_stack_selector(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetingStackSelectorNode> {
    let mut state = StackSourceState::Article;
    let mut kinds = Vec::new();
    let mut controller = None;
    loop {
        state = match state {
            StackSourceState::Article => {
                cursor.enter("event.became-target.stack-source.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .ok_or_else(|| cursor.fail("`a` or `an` before a stack-source kind"))?
                    .to_string();
                cursor.consume(&article)?;
                StackSourceState::Kind
            }
            StackSourceState::Kind => {
                cursor.enter("event.became-target.stack-source.kind");
                match cursor.peek().map(str::to_ascii_lowercase).as_deref() {
                    Some("spell") => {
                        cursor.consume("spell")?;
                        kinds.push(TargetingStackKind::Spell);
                    }
                    Some("ability") => {
                        cursor.consume("ability")?;
                        kinds.push(TargetingStackKind::ActivatedAbility);
                        kinds.push(TargetingStackKind::TriggeredAbility);
                    }
                    _ => return Err(cursor.fail("a spell or ability stack-source kind")),
                }
                StackSourceState::ConnectorOrController
            }
            StackSourceState::ConnectorOrController => {
                cursor.enter("event.became-target.stack-source.connector-or-controller");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
                {
                    cursor.consume("or")?;
                    StackSourceState::Kind
                } else {
                    StackSourceState::Controller
                }
            }
            StackSourceState::Controller => {
                if controller_scope_starts(cursor) {
                    controller = Some(recognize_controller_scope(cursor)?);
                }
                StackSourceState::Complete
            }
            StackSourceState::Complete => {
                cursor.enter("event.became-target.stack-source.complete");
                kinds.sort_by_key(|kind| match kind {
                    TargetingStackKind::Spell => 0,
                    TargetingStackKind::ActivatedAbility => 1,
                    TargetingStackKind::TriggeredAbility => 2,
                });
                kinds.dedup();
                cursor.require(
                    !kinds.is_empty(),
                    "at least one targeting stack-source kind",
                )?;
                return Ok(TargetingStackSelectorNode { kinds, controller });
            }
        };
    }
}

/// `the target of` at `offset`: the targeting frame of the `become` lemma.
/// A lexical probe that commits nothing.
pub(in crate::oracle::canonical) fn targeting_frame_starts(
    cursor: &OracleTokenCursor<'_>,
    offset: usize,
) -> bool {
    ["the", "target", "of"]
        .iter()
        .enumerate()
        .all(|(index, word)| {
            cursor
                .peek_ahead(offset + index)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
}

/// Reads `the target of` after a consumed `become` verb.
pub(in crate::oracle::canonical) fn consume_targeting_relation(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<()> {
    cursor.enter("event.became-target.relation");
    cursor.consume_phrase(&["the", "target", "of"])
}

/// Reads `a|an SELECTOR (or a|an SELECTOR)* becomes the target of STACK`
/// after the trigger word; the enclosing clause owns the separator.
pub(in crate::oracle::canonical) fn recognize_became_target_event(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<BecameTargetEventNode> {
    let objects = recognize_targeted_object_selectors(cursor)?;
    cursor.enter("event.became-target.action");
    // Only `becomes the target of` is this frame; another complement of
    // `become` (`becomes tapped`) belongs to a later event reader, whose
    // further-reaching diagnostic must not be shadowed by this one.
    cursor.require(
        targeting_frame_starts(cursor, 1),
        "`becomes the target of` after the targeted-object subject",
    )?;
    consume_lemma(
        cursor,
        VerbLemma::Become,
        VerbAgreement::ThirdPersonSingular,
    )?;
    consume_targeting_relation(cursor)?;
    // The targeting object's controller is an optional typed relation, as
    // for the source observer: absent, a spell or ability of any player.
    let targeting_stack = recognize_targeting_stack_selector(cursor)?;
    Ok(BecameTargetEventNode {
        objects,
        targeting_stack,
    })
}

#[cfg(test)]
#[path = "targeting_event_tests.rs"]
mod tests;
