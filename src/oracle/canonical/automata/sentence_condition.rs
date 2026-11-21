//! Sentence-level conditional instruction `If CONDITION, EFFECTS.` with an
//! optional `Otherwise, EFFECTS.` sentence (CR 608.2c). The effect-sequence
//! parent owns the introducer position, the sentence boundaries and the
//! branch ranges; this child owns only the Boolean program between `if` and
//! its comma. Every atom reuses an existing reader: controller permanent
//! presence/count, controller state and history, turn ownership, cast facts of
//! the source, and a referenced object tested with the shared object query.
//! A referenced object is a symbolic binding resolved from the sequence's
//! proven antecedents; it is never guessed.
use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::syntax::{NameResolutionFailure, ResolvedNameLink};

/// The object an object-state condition tests. Bindings stay symbolic until
/// lowering resolves them to the declared target or the bound card result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ConditionObjectReference {
    /// A battlefield permanent: the source, a declared target, or the object
    /// observed by the triggering event.
    Permanent(PermanentEffectReference),
    /// A card declared as the target of an earlier card zone change.
    CardTarget(TargetBinding),
    /// The singular card result of the current card-collection scope (the
    /// looked-at, revealed or chosen card).
    CardFocus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct ReferencedObjectConditionNode {
    pub object: ConditionObjectReference,
    pub predicate: ObjectPredicate,
}

/// The zone a spell was cast from (CR 601.2a).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SourceCastZone {
    Hand,
    Graveyard,
    Exile,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum SentenceConditionAtom {
    /// Controller permanent presence/count and kicker atoms.
    Local(LocalConditionNode),
    /// Controller state and this-turn history atoms of static conditions.
    State(StaticConditionAtom),
    ControllerTurn(ControllerTurnPropositionNode),
    /// `this spell was cast from ZONE`; `negated` reads `anywhere other than`.
    CastFrom {
        spell: LocalObjectReference,
        zone: SourceCastZone,
        negated: bool,
    },
    ReferencedObject(ReferencedObjectConditionNode),
}

pub(in crate::oracle::canonical) type SentenceConditionNode =
    BooleanConditionNode<SentenceConditionAtom>;

/// A conditional instruction over the flat action list of its sequence.
/// `then_first..then_end` are the actions of the `If` sentence and
/// `then_end..else_end` those of its `Otherwise` sentence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct SentenceConditionalNode {
    pub condition: SentenceConditionNode,
    pub then_first: usize,
    pub then_end: usize,
    pub else_end: Option<usize>,
}

impl SentenceConditionalNode {
    pub(in crate::oracle::canonical) fn end(&self) -> usize {
        self.else_end.unwrap_or(self.then_end)
    }

    pub(in crate::oracle::canonical) fn shifted(&self, offset: usize) -> Self {
        Self {
            condition: self.condition.clone(),
            then_first: self.then_first + offset,
            then_end: self.then_end + offset,
            else_end: self.else_end.map(|end| end + offset),
        }
    }
}

/// Antecedents proven by the enclosing sequence at the condition's position.
pub(in crate::oracle::canonical) struct SentenceConditionContext<'a> {
    pub name_links: &'a [ResolvedNameLink],
    pub name_failure: Option<&'a NameResolutionFailure>,
    pub effects: &'a EffectRecognitionContext,
    /// The singular card declared by the latest card-target zone change.
    pub card_target: Option<TargetBinding>,
    /// Whether the latest object-introducing action is a card-collection
    /// action whose singular result `it` may denote (checked at lowering).
    pub card_focus: bool,
    /// Permanent targets that an earlier action of this sequence moved out of
    /// the battlefield. Testing them needs last-known information.
    pub departed_targets: &'a [TargetBinding],
}

fn word_is(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

fn contraction_is(cursor: &OracleTokenCursor<'_>, offset: usize, base: &str, tail: &str) -> bool {
    cursor.peek_ahead(offset).is_some_and(|raw| {
        let raw = raw.replace('\u{2019}', "'").to_ascii_lowercase();
        raw == format!("{base}'{tail}")
    })
}

fn consume_current(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<()> {
    let raw = cursor
        .peek()
        .ok_or_else(|| cursor.fail("a token"))?
        .to_string();
    cursor.consume(&raw)
}

/// Atoms of the shared static reader that open with a quantity or player
/// noun: `no spells were cast last turn`, `an opponent lost life this turn`,
/// `a creature died this turn`, `two or more Elves died this turn`.
fn static_condition_event_count_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_is(cursor, 0, "no")
        || word_is(cursor, 0, "a")
        || word_is(cursor, 0, "an")
        || cursor.peek().is_some_and(|raw| {
            raw.parse::<i64>().is_ok() || super::super::parse_number_word(raw).is_some()
        })
}

/// True at a sentence-initial `if` whose condition this child may read.
pub(in crate::oracle::canonical) fn sentence_condition_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_is(cursor, 0, "if")
}

/// Reads `if CONDITION,`. The comma is consumed here because the sequence
/// parent has already proved that this `if` opens its sentence.
pub(in crate::oracle::canonical) fn recognize_sentence_condition(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionNode> {
    cursor.enter("condition.sentence.if");
    cursor.consume("if")?;
    cursor.enter("condition.sentence.boolean-program");
    let condition = recognize_boolean_condition_list_before_separator(cursor, |cursor| {
        recognize_sentence_condition_atom(cursor, context)
    })?;
    cursor.enter("condition.sentence.separator");
    cursor.consume(",")?;
    cursor.enter("condition.sentence.instead");
    cursor.require(
        !word_is(cursor, 0, "instead"),
        "a conditional sentence that does not replace an earlier instruction with `instead`",
    )?;
    Ok(condition)
}

pub(in crate::oracle::canonical) fn recognize_sentence_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionAtom> {
    cursor.enter("condition.sentence.atom-kind");
    if word_is(cursor, 0, "you")
        && word_is(cursor, 1, "control")
        && !super::opponent_count_comparison_starts(cursor)
    {
        return recognize_trigger_guard_atom(cursor).map(SentenceConditionAtom::Local);
    }
    if contraction_is(cursor, 0, "you", "ve")
        || (word_is(cursor, 0, "you")
            && (word_is(cursor, 1, "have")
                || contraction_is(cursor, 1, "haven", "t")
                || word_is(cursor, 1, "gained")
                || word_is(cursor, 1, "attacked")
                || word_is(cursor, 1, "lost")))
        || word_is(cursor, 0, "there")
        || contraction_is(cursor, 0, "there", "s")
        || static_condition_event_count_starts(cursor)
        || super::opponent_count_comparison_starts(cursor)
    {
        return recognize_static_condition_atom(
            cursor,
            StaticConditionContext {
                source_subject: false,
                name_links: context.name_links,
                name_failure: context.name_failure,
            },
        )
        .map(SentenceConditionAtom::State);
    }
    if contraction_is(cursor, 0, "it", "s")
        && (word_is(cursor, 1, "your") || (word_is(cursor, 1, "not") && word_is(cursor, 2, "your")))
    {
        return recognize_controller_turn_proposition(cursor)
            .map(SentenceConditionAtom::ControllerTurn);
    }
    if source_reference_starts_at(
        cursor.position(),
        cursor.peek(),
        context.name_links,
        context.name_failure,
    ) {
        return recognize_source_condition_atom(cursor, context);
    }
    if contraction_is(cursor, 0, "it", "s")
        || word_is(cursor, 0, "it")
        || word_is(cursor, 0, "its")
        || word_is(cursor, 0, "that")
    {
        return recognize_referenced_object_atom(cursor, context)
            .map(SentenceConditionAtom::ReferencedObject);
    }
    Err(cursor.fail(
        "a supported sentence condition: controller permanent presence or count, controller state or history, turn ownership, a cast fact of this spell, or a referenced object's characteristic",
    ))
}

/// `this spell was kicked`, `this spell was cast from ZONE`, and
/// `this creature is CHARACTERISTIC` over the ability's own source.
fn recognize_source_condition_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<SentenceConditionAtom> {
    cursor.enter("condition.sentence.source.subject");
    let spell = recognize_source_reference(cursor, context.name_links, context.name_failure)?;
    cursor.enter("condition.sentence.source.predicate");
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "kicked") {
        cursor.consume_phrase(&["was", "kicked"])?;
        return Ok(SentenceConditionAtom::Local(
            LocalConditionNode::WasKicked { spell },
        ));
    }
    if word_is(cursor, 0, "was") && word_is(cursor, 1, "cast") {
        cursor.consume_phrase(&["was", "cast", "from"])?;
        cursor.enter("condition.sentence.source.cast-zone");
        let (zone, negated) = if word_is(cursor, 0, "anywhere") {
            cursor.consume_phrase(&["anywhere", "other", "than", "your", "hand"])?;
            (SourceCastZone::Hand, true)
        } else if word_is(cursor, 0, "your") && word_is(cursor, 1, "hand") {
            cursor.consume_phrase(&["your", "hand"])?;
            (SourceCastZone::Hand, false)
        } else if word_is(cursor, 0, "a") && word_is(cursor, 1, "graveyard") {
            cursor.consume_phrase(&["a", "graveyard"])?;
            (SourceCastZone::Graveyard, false)
        } else if word_is(cursor, 0, "exile") {
            cursor.consume("exile")?;
            (SourceCastZone::Exile, false)
        } else {
            return Err(cursor
                .fail("`your hand`, `a graveyard`, `exile` or `anywhere other than your hand`"));
        };
        return Ok(SentenceConditionAtom::CastFrom {
            spell,
            zone,
            negated,
        });
    }
    let object = ConditionObjectReference::Permanent(PermanentEffectReference::Source(spell));
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, object, predicate)
        .map(SentenceConditionAtom::ReferencedObject)
}

/// `it's a CHARACTERISTIC`, `it isn't …`, `that NOUN is …`, `it has mana
/// value N or less`, `its PROPERTY is N or greater`.
fn recognize_referenced_object_atom(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.sentence.object.subject");
    if word_is(cursor, 0, "its") {
        cursor.consume("its")?;
        let object = resolve_object_anaphor(cursor, context)?;
        cursor.enter("condition.sentence.object.property");
        let property = recognize_selector_numeric_property(cursor)?;
        cursor.enter("condition.sentence.object.copula");
        cursor.require(
            word_is(cursor, 0, "is") || word_is(cursor, 0, "was"),
            "`is` or `was` after the possessed property",
        )?;
        consume_current(cursor)?;
        cursor.enter("condition.sentence.object.comparison");
        let predicate = recognize_numeric_property_comparison_tail(cursor, property)?;
        return finish_referenced_object(cursor, context, object, predicate);
    }
    if contraction_is(cursor, 0, "it", "s") {
        consume_current(cursor)?;
        let object = resolve_object_anaphor(cursor, context)?;
        let predicate = recognize_object_state_predicate(cursor, true)?;
        return finish_referenced_object(cursor, context, object, predicate);
    }
    let object = if word_is(cursor, 0, "it") {
        cursor.consume("it")?;
        resolve_object_anaphor(cursor, context)?
    } else if word_is(cursor, 0, "that") && word_is(cursor, 1, "card") {
        cursor.consume_phrase(&["that", "card"])?;
        resolve_card_anaphor(cursor, context)?
    } else {
        match recognize_contextual_permanent_operand(
            cursor,
            context.name_links,
            context.name_failure,
            context.effects,
        )? {
            PermanentOperandNode::BoundReference(reference) => {
                ConditionObjectReference::Permanent(reference)
            }
            _ => {
                cursor.enter("condition.sentence.object.nominal-anaphor");
                return Err(cursor.fail("a nominal anaphor of one proven permanent antecedent"));
            }
        }
    };
    let predicate = recognize_object_state_predicate(cursor, false)?;
    finish_referenced_object(cursor, context, object, predicate)
}

/// The verb phrase after a referenced object: a copula with a shared object
/// query, or `has` with a numeric property comparison or a keyword.
/// `copula_read` is true after the contraction `it's`.
fn recognize_object_state_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    copula_read: bool,
) -> MachineResult<ObjectPredicate> {
    cursor.enter("condition.sentence.object.verb");
    let mut negated = false;
    if !copula_read {
        let verb = cursor
            .peek()
            .map(|raw| raw.replace('\u{2019}', "'").to_ascii_lowercase())
            .ok_or_else(|| cursor.fail("`is`, `was`, `isn't`, `wasn't`, `has` or `had`"))?;
        match verb.as_str() {
            "is" | "was" => consume_current(cursor)?,
            "isn't" | "wasn't" => {
                consume_current(cursor)?;
                negated = true;
            }
            "has" | "had" => {
                consume_current(cursor)?;
                cursor.enter("condition.sentence.object.possession");
                if let Some(keyword) = cursor
                    .peek()
                    .and_then(parameterless_keyword_from_oracle_word)
                    && !word_is(cursor, 0, "mana")
                    && !word_is(cursor, 0, "power")
                    && !word_is(cursor, 0, "toughness")
                {
                    consume_current(cursor)?;
                    return Ok(ObjectPredicate::HasKeyword(keyword));
                }
                let property = recognize_selector_numeric_property(cursor)?;
                cursor.enter("condition.sentence.object.comparison");
                return recognize_numeric_property_comparison_tail(cursor, property);
            }
            _ => return Err(cursor.fail("`is`, `was`, `isn't`, `wasn't`, `has` or `had`")),
        }
    }
    if word_is(cursor, 0, "not") {
        cursor.consume("not")?;
        negated = !negated;
    }
    cursor.enter("condition.sentence.object.characteristic");
    if word_is(cursor, 0, "a") || word_is(cursor, 0, "an") {
        consume_current(cursor)?;
    }
    let query = recognize_object_query_before_condition_connector(cursor)?;
    cursor.enter("condition.sentence.object.relation");
    cursor.require(
        query.controller.is_none(),
        "an object characteristic without a controller relation",
    )?;
    Ok(if negated {
        ObjectPredicate::Not(Box::new(query.predicate))
    } else {
        query.predicate
    })
}

/// `it` denotes exactly one proven singular antecedent: a permanent subject,
/// the latest declared card target, or the current card-collection result.
fn resolve_object_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.antecedent");
    let mut candidates = Vec::new();
    if let Some(reference) = super::permanent_subject::proven_permanent_antecedent(context.effects)
    {
        candidates.push(ConditionObjectReference::Permanent(reference));
    }
    if let Some(binding) = context.card_target {
        candidates.push(ConditionObjectReference::CardTarget(binding));
    }
    if context.card_focus {
        candidates.push(ConditionObjectReference::CardFocus);
    }
    match candidates.as_slice() {
        [reference] => Ok(*reference),
        [] => Err(cursor.fail("one proven singular antecedent for the pronoun")),
        _ => Err(cursor.fail("exactly one proven antecedent for the pronoun, not several")),
    }
}

fn resolve_card_anaphor(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
) -> MachineResult<ConditionObjectReference> {
    cursor.enter("condition.sentence.object.card-antecedent");
    match (context.card_target, context.card_focus) {
        (Some(binding), false) => Ok(ConditionObjectReference::CardTarget(binding)),
        (None, true) => Ok(ConditionObjectReference::CardFocus),
        (None, false) => Err(cursor.fail("one proven singular card antecedent for `that card`")),
        (Some(_), true) => {
            Err(cursor.fail("exactly one proven card antecedent for `that card`, not several"))
        }
    }
}

fn finish_referenced_object(
    cursor: &mut OracleTokenCursor<'_>,
    context: &SentenceConditionContext<'_>,
    object: ConditionObjectReference,
    predicate: ObjectPredicate,
) -> MachineResult<ReferencedObjectConditionNode> {
    cursor.enter("condition.sentence.object.last-known-information");
    if let ConditionObjectReference::Permanent(PermanentEffectReference::Target(binding)) = object {
        // CR 608.2h: an object moved out of the battlefield by an earlier
        // instruction is tested with its last known information, which this
        // condition does not capture. Refuse instead of reading the new object.
        cursor.require(
            !context.departed_targets.contains(&binding),
            "a referenced permanent still on the battlefield; last-known information of a moved object is not captured",
        )?;
    }
    cursor.enter("condition.sentence.object.domain");
    let domain = match object {
        ConditionObjectReference::Permanent(_) => ObjectKind::Permanent,
        ConditionObjectReference::CardTarget(_) | ConditionObjectReference::CardFocus => {
            ObjectKind::Card
        }
    };
    cursor.require(
        predicate.supports_domain(domain),
        "a characteristic evaluable in the referenced object's domain",
    )?;
    Ok(ReferencedObjectConditionNode { object, predicate })
}

impl SentenceConditionAtom {
    /// Kicker status is chosen as the spell is cast (CR 702.33d); targets in
    /// a branch it governs are required only if the spell was kicked.
    pub(in crate::oracle::canonical) fn depends_on_cast_choice(&self) -> bool {
        matches!(self, Self::Local(LocalConditionNode::WasKicked { .. }))
    }
}

pub(in crate::oracle::canonical) fn sentence_condition_depends_on_cast_choice(
    condition: &SentenceConditionNode,
) -> bool {
    match condition {
        BooleanConditionNode::Atom(atom) => atom.depends_on_cast_choice(),
        BooleanConditionNode::And(operands) | BooleanConditionNode::Or(operands) => operands
            .iter()
            .any(sentence_condition_depends_on_cast_choice),
        BooleanConditionNode::Not(operand) => sentence_condition_depends_on_cast_choice(operand),
    }
}

#[cfg(test)]
#[path = "sentence_condition_tests.rs"]
mod tests;
