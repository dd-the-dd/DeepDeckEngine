use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ExpressedTargetCardinality, NumericAtom, NumericExpression, ObjectCardType, ObjectKind,
    ObjectPredicate, ObjectQueryContext, PlayerPredicate, StackItemPredicate, TargetCardinality,
    TargetListNode, TargetSelector, TargetSlot, recognize_numeric_atom,
    recognize_numeric_expression, recognize_object_noun_query, recognize_target_slot_selector,
};

/// One bound of a declared target count (CR 601.2c, 115.1): an amount the
/// controller announces on declaration, read by the shared numeric grammar
/// (a literal, `X` of the same face, `that many`), or no upper bound at all
/// (`any number of target ...`). The X producer is validated after card
/// composition in the face context; it is never defaulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetCountBound {
    Amount(NumericExpression),
    Unbounded,
}

/// The one cardinality node shared by target lists, card targets and
/// non-target selections: `{minimum, maximum}` where the minimum is an
/// amount and the maximum an amount or unbounded. Literal positive bounds
/// stay representable as the historical [`TargetCardinality`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct TargetCount {
    pub minimum: NumericExpression,
    pub maximum: TargetCountBound,
}

impl TargetCount {
    pub(in crate::oracle::canonical) fn from_literal(cardinality: TargetCardinality) -> Self {
        Self {
            minimum: NumericExpression::from(i64::from(cardinality.minimum)),
            maximum: TargetCountBound::Amount(NumericExpression::from(i64::from(
                cardinality.maximum,
            ))),
        }
    }

    pub(in crate::oracle::canonical) fn from_atoms(
        cardinality: ExpressedTargetCardinality,
    ) -> Self {
        Self {
            minimum: NumericExpression::Atom(cardinality.minimum),
            maximum: TargetCountBound::Amount(NumericExpression::Atom(cardinality.maximum)),
        }
    }

    /// The literal projection, when both bounds are integer literals.
    pub(in crate::oracle::canonical) fn literal(&self) -> Option<TargetCardinality> {
        fn literal(expression: &NumericExpression) -> Option<u32> {
            match expression {
                NumericExpression::Atom(NumericAtom::Integer(value)) => u32::try_from(*value).ok(),
                _ => None,
            }
        }
        let TargetCountBound::Amount(maximum) = &self.maximum else {
            return None;
        };
        Some(TargetCardinality {
            minimum: literal(&self.minimum)?,
            maximum: literal(maximum)?,
        })
    }

    /// The atom projection (`X` or a literal on both sides), when no bound
    /// is unbounded or composite.
    pub(in crate::oracle::canonical) fn atoms(&self) -> Option<ExpressedTargetCardinality> {
        fn atom(expression: &NumericExpression) -> Option<NumericAtom> {
            match expression {
                NumericExpression::Atom(atom) => Some(*atom),
                _ => None,
            }
        }
        let TargetCountBound::Amount(maximum) = &self.maximum else {
            return None;
        };
        Some(ExpressedTargetCardinality {
            minimum: atom(&self.minimum)?,
            maximum: atom(maximum)?,
        })
    }

    pub(in crate::oracle::canonical) fn is_unbounded(&self) -> bool {
        self.maximum == TargetCountBound::Unbounded
    }

    /// More than one object may be declared: an unbounded or non-literal
    /// maximum, or a literal maximum above one.
    pub(in crate::oracle::canonical) fn is_plural(&self) -> bool {
        match &self.maximum {
            TargetCountBound::Unbounded => true,
            TargetCountBound::Amount(NumericExpression::Atom(NumericAtom::Integer(value))) => {
                *value > 1
            }
            TargetCountBound::Amount(_) => true,
        }
    }
}

impl TargetCardinality {
    /// The literal envelope of a slot whose authoritative count is
    /// [`TargetSlot::count`]: no literal reader may treat it as scalar.
    /// Only parents that read `count` opt into such slots.
    pub(in crate::oracle::canonical) const EXPRESSED: Self = Self {
        minimum: 0,
        maximum: u32::MAX,
    };
}

/// Whether a parent admits declared target counts that are not two literal
/// integers. Lowerers that only read [`TargetCardinality`] keep `Literal`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum TargetCountSupport {
    Literal,
    Expressed,
}

/// Lexical lookahead for a target count that is not a positive literal:
/// `any number of`, `X`, `that many`, or `up to` one of those. Dispatch
/// hint only; [`recognize_target_count`] reads and validates the count.
pub(in crate::oracle::canonical) fn expressed_target_count_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let at = |offset: usize, word: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
    };
    let variable =
        |offset: usize| at(offset, "x") || (at(offset, "that") && at(offset + 1, "many"));
    (at(0, "any") && at(1, "number") && at(2, "of"))
        || (at(0, "one") && at(1, "or") && at(2, "more"))
        || variable(0)
        || (at(0, "up") && at(1, "to") && variable(2))
}

/// The shared declared-count child: `N`, `N or M`, `up to N`, `X`, `up to
/// X`, `that many`, `one or more` and `any number of`. Amounts are the shared
/// numeric expression; a literal must be positive (a zero upper bound
/// declares no target). The caller owns the following `[other] target` head.
pub(in crate::oracle::canonical) fn recognize_target_count(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCount> {
    cursor.enter("target-list.count.start");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("any"))
    {
        cursor.enter("target-list.count.unbounded");
        cursor.consume_phrase(&["any", "number", "of"])?;
        return Ok(TargetCount {
            minimum: NumericExpression::from(0),
            maximum: TargetCountBound::Unbounded,
        });
    }
    // `one or more target ...` (CR 601.2c): at least one, no upper bound.
    if ["one", "or", "more"]
        .iter()
        .enumerate()
        .all(|(offset, word)| {
            cursor
                .peek_ahead(offset)
                .is_some_and(|raw| raw.eq_ignore_ascii_case(word))
        })
    {
        cursor.enter("target-list.count.at-least-one");
        cursor.consume_phrase(&["one", "or", "more"])?;
        return Ok(TargetCount {
            minimum: NumericExpression::from(1),
            maximum: TargetCountBound::Unbounded,
        });
    }
    let up_to = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("up"));
    if up_to {
        cursor.consume_phrase(&["up", "to"])?;
    }
    cursor.enter("target-list.count.amount");
    if is_literal_quantity(cursor) && !up_to {
        // An exact literal keeps the historical literal grammar, including
        // the `N or M` range and its positivity diagnostics.
        return recognize_target_cardinality(cursor).map(TargetCount::from_literal);
    }
    let amount = recognize_numeric_expression(cursor, None)?;
    cursor.enter("target-list.count.positive");
    cursor.require(
        !matches!(amount, NumericExpression::Atom(NumericAtom::Integer(value)) if value <= 0),
        "a positive target count",
    )?;
    Ok(TargetCount {
        minimum: if up_to {
            NumericExpression::from(0)
        } else {
            amount.clone()
        },
        maximum: TargetCountBound::Amount(amount),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetListState {
    Start,
    FirstSlot,
    Continuation,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TargetListAttemptFrame {
    slots: Vec<TargetSlot>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TargetSlotState {
    Start,
    Cardinality,
    OptionalSourceExclusion,
    ExpressedCount,
    TargetHead,
    DeterminedHead,
    Selector,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CardinalityKind {
    Exact,
    UpTo,
}

/// Recognizes one complete target-list grammar from the shared token stream.
///
/// The parent owns target multiplicity. A repeated `target`, a determined noun
/// head, or another cardinalized target phrase opens another slot. Object-type
/// intersection and union stay inside the selector child automaton.
pub(in crate::oracle::canonical) fn recognize_target_list(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetListNode> {
    recognize_target_list_with_counts(cursor, TargetCountSupport::Literal)
}

/// The same target list for a parent whose lowerer reads
/// [`TargetSlot::count`]: `X target`, `up to X target`, `that many target`
/// and `any number of target` slots are admitted with their typed count.
/// Every other parent keeps the literal contract and its diagnostics.
pub(in crate::oracle::canonical) fn recognize_target_list_with_counts(
    cursor: &mut OracleTokenCursor<'_>,
    counts: TargetCountSupport,
) -> MachineResult<TargetListNode> {
    let mut state = TargetListState::Start;
    let mut frame = TargetListAttemptFrame::default();

    loop {
        state = match state {
            TargetListState::Start => {
                cursor.enter("target-list.start");
                TargetListState::FirstSlot
            }
            TargetListState::FirstSlot => {
                cursor.enter("target-list.first-slot");
                frame
                    .slots
                    .push(recognize_target_slot(cursor, true, false, counts)?);
                TargetListState::Continuation
            }
            TargetListState::Continuation => {
                cursor.enter("target-list.continuation");
                if let Some(slot) = try_recognize_additional_slot(cursor, counts)? {
                    frame.slots.push(slot);
                    TargetListState::Continuation
                } else {
                    TargetListState::Complete
                }
            }
            TargetListState::Complete => {
                cursor.enter("target-list.complete");
                cursor.require(!frame.slots.is_empty(), "at least one target slot")?;
                return Ok(TargetListNode { slots: frame.slots });
            }
        };
    }
}

/// Recognizes the possessive target head used when a later noun is owned by
/// the selected player, for example `target player's library`. The target
/// decision remains the same typed target-list node used by ordinary effects;
/// only the possessive surface form differs.
pub(in crate::oracle::canonical) fn recognize_possessive_target_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetListNode> {
    cursor.enter("target-list.possessive-player.target-head");
    cursor.consume("target")?;
    cursor.enter("target-list.possessive-player.role");
    let role = cursor.consume_type("possessivePlayerRoleWord")?;
    let predicate = if role.raw.eq_ignore_ascii_case("player's")
        || role.raw.eq_ignore_ascii_case("player’s")
    {
        PlayerPredicate::Any
    } else if role.raw.eq_ignore_ascii_case("opponent's")
        || role.raw.eq_ignore_ascii_case("opponent’s")
    {
        PlayerPredicate::Opponent
    } else {
        return Err(cursor.fail("the possessive `player's` or `opponent's` target role"));
    };
    cursor.enter("target-list.possessive-player.complete");
    Ok(TargetListNode {
        slots: vec![TargetSlot {
            selector: TargetSelector::Player(predicate),
            cardinality: TargetCardinality::EXACTLY_ONE,
            exclude_source: false,
            count: None,
        }],
    })
}

fn recognize_target_slot(
    cursor: &mut OracleTokenCursor<'_>,
    allow_any_target: bool,
    allow_determined_head: bool,
    counts: TargetCountSupport,
) -> MachineResult<TargetSlot> {
    let mut state = TargetSlotState::Start;
    let mut cardinality = None;
    let mut count = None;
    let mut selector = None;
    let mut exclude_source = false;

    loop {
        state = match state {
            TargetSlotState::Start
                if counts == TargetCountSupport::Expressed
                    && expressed_target_count_starts(cursor) =>
            {
                cursor.enter("target-list.slot.start");
                TargetSlotState::ExpressedCount
            }
            TargetSlotState::ExpressedCount => {
                cursor.enter("target-list.slot.expressed-count");
                let read = recognize_target_count(cursor)?;
                match read.literal() {
                    Some(literal) => cardinality = Some(literal),
                    None => {
                        cardinality = Some(TargetCardinality::EXPRESSED);
                        count = Some(read);
                    }
                }
                TargetSlotState::OptionalSourceExclusion
            }
            TargetSlotState::Start => {
                cursor.enter("target-list.slot.start");
                if !starts_explicit_target_slot(cursor)
                    && !(allow_determined_head
                        && cursor.peek().is_some_and(|raw| {
                            raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an")
                        }))
                {
                    return Err(cursor.fail(
                        "`any target`, `target`, a cardinalized target, or a determined target noun",
                    ));
                }
                match cursor.peek() {
                    // `any other target` (CR 115.3): any target that is not
                    // an object or player an earlier slot of the same
                    // instruction targets (`... to any target and 1 damage to
                    // any other target`).
                    Some(raw)
                        if allow_any_target
                            && raw.eq_ignore_ascii_case("any")
                            && cursor
                                .peek_ahead(1)
                                .is_some_and(|raw| raw.eq_ignore_ascii_case("other")) =>
                    {
                        cursor.enter("target-list.slot.any-other");
                        cursor.consume_phrase(&["any", "other", "target"])?;
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        exclude_source = true;
                        selector = Some(TargetSelector::Any);
                        TargetSlotState::Complete
                    }
                    Some(raw) if allow_any_target && raw.eq_ignore_ascii_case("any") => {
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        TargetSlotState::Selector
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("target") => {
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        TargetSlotState::TargetHead
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("another") => {
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        cursor.consume("another")?;
                        exclude_source = true;
                        TargetSlotState::TargetHead
                    }
                    // `a second|third target ...` (CR 115.3): one more target,
                    // distinct from every earlier one, like `another target`.
                    Some(_) if ordinal_target_head_follows(cursor) => {
                        cursor.enter("target-list.slot.ordinal");
                        let article = cursor.peek().expect("checked article").to_string();
                        cursor.consume(&article)?;
                        let ordinal = cursor.peek().expect("checked ordinal").to_string();
                        cursor.consume(&ordinal)?;
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        exclude_source = true;
                        TargetSlotState::TargetHead
                    }
                    Some(raw) if raw.eq_ignore_ascii_case("up") || is_literal_quantity(cursor) => {
                        TargetSlotState::Cardinality
                    }
                    Some(raw)
                        if allow_determined_head
                            && (raw.eq_ignore_ascii_case("a")
                                || raw.eq_ignore_ascii_case("an")) =>
                    {
                        cardinality = Some(TargetCardinality::EXACTLY_ONE);
                        TargetSlotState::DeterminedHead
                    }
                    _ => {
                        return Err(cursor.fail(
                            "`any target`, `target`, a cardinalized target, or a determined target noun",
                        ));
                    }
                }
            }
            TargetSlotState::Cardinality => {
                cursor.enter("target-list.slot.cardinality");
                cardinality = Some(recognize_target_cardinality(cursor)?);
                TargetSlotState::OptionalSourceExclusion
            }
            TargetSlotState::OptionalSourceExclusion => {
                cursor.enter("target-list.slot.optional-source-exclusion");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("other"))
                {
                    cursor.consume("other")?;
                    exclude_source = true;
                }
                TargetSlotState::TargetHead
            }
            TargetSlotState::TargetHead => {
                cursor.enter("target-list.slot.target-head");
                // `two targets`, `up to three other targets`, `X targets`:
                // the plural of `any target` (CR 115.4) after a count of
                // more than one. The head is the whole selector.
                let plural_count = count.is_some()
                    || cardinality.is_some_and(|cardinality| cardinality.maximum > 1);
                if plural_count
                    && cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("targets"))
                {
                    cursor.consume("targets")?;
                    selector = Some(TargetSelector::Any);
                    TargetSlotState::Complete
                } else {
                    cursor.consume("target")?;
                    // `another target` / `a third target` without a noun:
                    // the plural-free form of `any other target`.
                    if exclude_source && target_slot_boundary(cursor) {
                        selector = Some(TargetSelector::Any);
                        TargetSlotState::Complete
                    } else {
                        TargetSlotState::Selector
                    }
                }
            }
            TargetSlotState::DeterminedHead => {
                cursor.enter("target-list.slot.determined-head");
                let determiner = cursor.peek().expect("checked determiner").to_string();
                cursor.consume(&determiner)?;
                TargetSlotState::Selector
            }
            TargetSlotState::Selector => {
                cursor.enter("target-list.slot.selector");
                let (first, excluded) =
                    super::recognize_target_slot_selector_excluding_source(cursor)?;
                exclude_source |= excluded;
                let read = recognize_heterogeneous_selector_list(cursor, first)?;
                selector = Some(
                    if read == TargetSelector::Any && any_target_exception_starts(cursor) {
                        recognize_any_target_exception(cursor)?
                    } else {
                        read
                    },
                );
                TargetSlotState::Complete
            }
            TargetSlotState::Complete => {
                cursor.enter("target-list.slot.complete");
                let selector = cursor.require_value(selector, "a populated target selector")?;
                let cardinality =
                    cursor.require_value(cardinality, "a populated target cardinality")?;
                // `two target creatures controlled by different players`: a
                // constraint over the declared set (CR 601.2c) needs a slot
                // that may declare more than one object.
                if selector_states_set_constraint(&selector) {
                    cursor.enter("target-list.slot.set-constraint");
                    let plural = count
                        .as_ref()
                        .map_or(cardinality.maximum > 1, TargetCount::is_plural);
                    cursor.require(
                        plural,
                        "a plural target slot for a constraint over the declared set",
                    )?;
                }
                return Ok(TargetSlot {
                    selector,
                    cardinality,
                    exclude_source,
                    count,
                });
            }
        };
    }
}

/// `that isn't` / `that is not` right after `any target`.
fn any_target_exception_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    let word = |offset: usize, expected: &str| {
        cursor
            .peek_ahead(offset)
            .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
    };
    word(0, "that")
        && (word(1, "isn't") || word(1, "isn\u{2019}t") || (word(1, "is") && word(2, "not")))
}

/// `any target that isn't a Dragon` (CR 115.4): `any target` is a player, a
/// creature, a planeswalker or a battle; the relative clause excludes the
/// objects of the shared characteristic query. A player has no creature
/// type, so every player stays a candidate. The slot is the same union of a
/// player branch and a permanent branch as `target player or planeswalker`.
fn recognize_any_target_exception(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetSelector> {
    cursor.enter("target-list.any-target.exception");
    cursor.consume("that")?;
    let copula = cursor.peek().unwrap_or_default().to_string();
    cursor.consume(&copula)?;
    if copula.eq_ignore_ascii_case("is") {
        cursor.consume("not")?;
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
    {
        let article = cursor.peek().unwrap_or_default().to_string();
        cursor.consume(&article)?;
    }
    cursor.enter("target-list.any-target.exception.characteristic");
    let excluded = recognize_object_noun_query(cursor, ObjectQueryContext::ORDINARY)?
        .into_object_query(cursor)?;
    cursor.require(
        excluded.controller.is_none()
            && !excluded.exclude_source
            && !excluded.predicate.contains_set_constraint(),
        "a characteristic of the excluded objects, without a controller or set constraint",
    )?;
    let damageable = ObjectPredicate::Union(vec![
        ObjectPredicate::CardType(ObjectCardType::Creature),
        ObjectPredicate::CardType(ObjectCardType::Planeswalker),
        ObjectPredicate::CardType(ObjectCardType::Battle),
    ]);
    Ok(TargetSelector::Union(vec![
        TargetSelector::Player(PlayerPredicate::Any),
        TargetSelector::Object(ObjectPredicate::Intersection(vec![
            damageable,
            ObjectPredicate::Not(Box::new(excluded.predicate)),
        ])),
    ]))
}

/// Whether an object branch of `selector` states a constraint over the
/// whole declared set (`with different names`, `controlled by different
/// players`) rather than over one candidate.
fn selector_states_set_constraint(selector: &TargetSelector) -> bool {
    match selector {
        TargetSelector::Object(predicate) | TargetSelector::ControlledObject { predicate, .. } => {
            predicate.contains_set_constraint()
        }
        TargetSelector::Union(branches) => branches.iter().any(selector_states_set_constraint),
        TargetSelector::Any | TargetSelector::Player(_) | TargetSelector::StackItem(_) => false,
    }
}

/// The kind of object or player one selector branch of a target slot
/// names. Branches of different kinds are heterogeneous (CR 115.1: one
/// target may be a player, a permanent or a stack object).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SelectorBranchKind {
    AnyTarget,
    Player,
    Object,
    StackItem,
}

fn selector_branch_kinds(selector: &TargetSelector, kinds: &mut Vec<SelectorBranchKind>) {
    let kind = match selector {
        TargetSelector::Any => SelectorBranchKind::AnyTarget,
        TargetSelector::Player(_) => SelectorBranchKind::Player,
        TargetSelector::Object(_) | TargetSelector::ControlledObject { .. } => {
            SelectorBranchKind::Object
        }
        TargetSelector::StackItem(_) => SelectorBranchKind::StackItem,
        TargetSelector::Union(branches) => {
            for branch in branches {
                selector_branch_kinds(branch, kinds);
            }
            return;
        }
    };
    if !kinds.contains(&kind) {
        kinds.push(kind);
    }
}

/// `spell` alone: the stack-object domain with no other characteristic,
/// which the stack-item predicate names exactly (CR 112.1).
fn bare_spell_selector(selector: &TargetSelector) -> bool {
    let TargetSelector::Object(predicate) = selector else {
        return false;
    };
    match predicate {
        ObjectPredicate::ObjectKind(ObjectKind::Spell) => true,
        ObjectPredicate::Intersection(operands) => {
            matches!(
                operands.as_slice(),
                [ObjectPredicate::ObjectKind(ObjectKind::Spell)]
            )
        }
        _ => false,
    }
}

fn flatten_selector_union(selector: TargetSelector, branches: &mut Vec<TargetSelector>) {
    match selector {
        TargetSelector::Union(inner) => {
            for branch in inner {
                flatten_selector_union(branch, branches);
            }
        }
        other => branches.push(other),
    }
}

fn flatten_stack_item_union(predicate: StackItemPredicate, operands: &mut Vec<StackItemPredicate>) {
    match predicate {
        StackItemPredicate::Union(inner) => {
            for operand in inner {
                flatten_stack_item_union(operand, operands);
            }
        }
        other => {
            if !operands.contains(&other) {
                operands.push(other);
            }
        }
    }
}

/// Reads `[, SELECTOR]* [,] or SELECTOR` after the first selector of one
/// target slot, when the selectors name targets of different kinds: `target
/// spell, activated ability, or triggered ability`, `target opponent,
/// creature an opponent controls, or planeswalker an opponent controls`,
/// `target creature token, player, or planeswalker`. Each branch is the
/// shared slot selector with its own qualifiers; the slot is one target
/// whose legal objects are the union of the branches (CR 115.1, 601.2c).
///
/// A coordination of object heads alone is the object query's own grammar
/// (`target artifact, creature, or enchantment`): this reader makes no claim
/// on it and consumes nothing, so the query child keeps its diagnostic.
/// Stack branches merge into one stack-item predicate (`spell` alone is the
/// stack spell domain). A trailing controller written on the last object
/// branch only is refused explicitly: whether it scopes the earlier
/// unqualified object branches is not decided by this reader.
fn recognize_heterogeneous_selector_list(
    cursor: &mut OracleTokenCursor<'_>,
    first: TargetSelector,
) -> MachineResult<TargetSelector> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw == "," || raw.eq_ignore_ascii_case("or"))
    {
        return Ok(first);
    }
    let mut trial = cursor.clone();
    trial.enter("target-list.selector-list.branch");
    let mut read = Vec::new();
    let complete = loop {
        let comma = trial.peek() == Some(",");
        if comma && trial.consume(",").is_err() {
            break false;
        }
        let last = trial
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"));
        if last && trial.consume("or").is_err() {
            break false;
        }
        if !comma && !last {
            break false;
        }
        let branch_start = trial.position();
        match trial.attempt(recognize_target_slot_selector) {
            Ok(branch) => read.push(branch),
            Err(_) => break false,
        }
        if last {
            break true;
        }
        // `target player, creature, or planeswalker`: the object query read
        // its own head coordination through the final `or`, so the list
        // ends with that branch.
        if trial
            .token_slice(branch_start, trial.position())
            .iter()
            .any(|token| token.raw.eq_ignore_ascii_case("or"))
        {
            break true;
        }
        // A middle branch is followed by the next comma.
        if trial.peek() != Some(",") {
            break false;
        }
    };
    if !complete {
        return Ok(first);
    }
    let mut kinds = Vec::new();
    for branch in std::iter::once(&first).chain(read.iter()) {
        selector_branch_kinds(branch, &mut kinds);
    }
    if kinds.len() < 2 {
        // Homogeneous object coordination: the query child's grammar.
        return Ok(first);
    }
    let mut branches = Vec::new();
    flatten_selector_union(first, &mut branches);
    for branch in read {
        flatten_selector_union(branch, &mut branches);
    }
    *cursor = trial;
    cursor.enter("target-list.selector-list.scope");
    let object_branches = branches
        .iter()
        .filter(|branch| {
            matches!(
                branch,
                TargetSelector::Object(_) | TargetSelector::ControlledObject { .. }
            ) && !bare_spell_selector(branch)
        })
        .collect::<Vec<_>>();
    let trailing_controller_only = object_branches.last().is_some_and(|last| {
        matches!(last, TargetSelector::ControlledObject { .. })
            && object_branches
                .iter()
                .any(|branch| matches!(branch, TargetSelector::Object(_)))
    });
    cursor.require(
        !trailing_controller_only,
        "separately qualified object branches; a controller on the last branch does not scope earlier unqualified branches",
    )?;
    cursor.enter("target-list.selector-list.complete");
    let stack_only = branches.iter().all(|branch| {
        matches!(branch, TargetSelector::StackItem(_)) || bare_spell_selector(branch)
    });
    if stack_only {
        let mut operands = Vec::new();
        for branch in branches {
            let predicate = match branch {
                TargetSelector::StackItem(predicate) => predicate,
                _ => StackItemPredicate::Spell,
            };
            flatten_stack_item_union(predicate, &mut operands);
        }
        return Ok(TargetSelector::StackItem(match operands.len() {
            1 => operands.pop().expect("one stack-item operand"),
            _ => StackItemPredicate::Union(operands),
        }));
    }
    Ok(TargetSelector::Union(branches))
}

/// Lowers `another target X` / `up to N other target X` onto a target slot's
/// candidate domain: the objects are not the ability's source (`excludeSource`)
/// and, through the typed `otherThanEarlierTargets` relation the card
/// composition resolves against the ability's earlier target slots, not
/// those slots' objects either (CR 115.3; Deadshot: `Tap target creature. It
/// deals damage ... to another target creature.`).
pub(in crate::oracle::canonical) fn mark_other_target_slot(candidates: &mut serde_json::Value) {
    candidates["excludeSource"] = serde_json::Value::Bool(true);
    candidates["otherThanEarlierTargets"] = serde_json::Value::Bool(true);
}

/// Recognizes the shared exact or `up to` positive-literal cardinality used by
/// target collections, independently from the selected object domain.
pub(in crate::oracle::canonical) fn recognize_target_cardinality(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<TargetCardinality> {
    cursor.enter("target-list.cardinality.start");
    let kind = if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("up"))
    {
        cursor.consume_phrase(&["up", "to"])?;
        CardinalityKind::UpTo
    } else {
        CardinalityKind::Exact
    };

    cursor.enter("target-list.cardinality.value");
    let (token_index, span_start, span_end, raw) = cursor
        .peek_token()
        .map(|token| {
            (
                cursor.position(),
                token.span_start,
                token.span_end,
                token.raw.clone(),
            )
        })
        .ok_or_else(|| cursor.fail("a positive literal target cardinality"))?;
    let atom = recognize_numeric_atom(cursor)?;
    let NumericAtom::Integer(value) = atom else {
        cursor.enter("target-list.cardinality.literal-required");
        return Err(cursor.fail_with_span(
            "a positive literal target cardinality",
            token_index,
            span_start,
            span_end,
            raw,
        ));
    };
    let maximum = u32::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            cursor.enter("target-list.cardinality.positive-required");
            cursor.fail_with_span(
                "a positive literal target cardinality",
                token_index,
                span_start,
                span_end,
                raw,
            )
        })?;
    // `N or M target` (CR 601.2c: a variable number of targets announced on
    // declaration) is a bounded range between two positive literals.
    if kind == CardinalityKind::Exact
        && cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("or"))
        && cursor.peek_ahead(1).is_some_and(|_| {
            let mut probe = cursor.clone();
            probe.consume("or").is_ok() && is_literal_quantity(&probe)
        })
    {
        cursor.enter("target-list.cardinality.range");
        cursor.consume("or")?;
        let atom = recognize_numeric_atom(cursor)?;
        let upper = match atom {
            NumericAtom::Integer(value) => u32::try_from(value).ok(),
            NumericAtom::VariableX => None,
        }
        .filter(|upper| *upper > maximum)
        .ok_or_else(|| cursor.fail("a literal upper target bound greater than the lower bound"))?;
        return Ok(TargetCardinality {
            minimum: maximum,
            maximum: upper,
        });
    }
    Ok(TargetCardinality {
        minimum: match kind {
            CardinalityKind::Exact => maximum,
            CardinalityKind::UpTo => 0,
        },
        maximum,
    })
}

/// Recognizes `N` and `up to N` target bounds where `N` is either a positive
/// literal or `X`. The value remains typed until lowering, allowing unrelated
/// target domains to share one casting-time variable decision.
pub(in crate::oracle::canonical) fn recognize_expressed_target_cardinality(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ExpressedTargetCardinality> {
    cursor.enter("target-list.expressed-cardinality.start");
    let up_to = cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("up"));
    if up_to {
        cursor.consume_phrase(&["up", "to"])?;
    }
    cursor.enter("target-list.expressed-cardinality.value");
    let maximum = recognize_numeric_atom(cursor)?;
    cursor.require(
        !matches!(maximum, NumericAtom::Integer(value) if value <= 0),
        "a positive literal target cardinality or X",
    )?;
    Ok(ExpressedTargetCardinality {
        minimum: if up_to {
            NumericAtom::Integer(0)
        } else {
            maximum
        },
        maximum,
    })
}

fn try_recognize_additional_slot(
    cursor: &mut OracleTokenCursor<'_>,
    counts: TargetCountSupport,
) -> MachineResult<Option<TargetSlot>> {
    if !cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
    {
        return Ok(None);
    }

    let checkpoint = cursor.checkpoint();
    cursor.enter("target-list.continuation.probe");
    cursor.consume("and")?;
    if !starts_target_slot(cursor)
        && !(counts == TargetCountSupport::Expressed && expressed_target_count_starts(cursor))
    {
        cursor.rollback(checkpoint);
        return Ok(None);
    }

    cursor.enter("target-list.continuation.committed");
    recognize_target_slot(cursor, true, true, counts).map(Some)
}

/// The first-slot target head is shared with targeted-effect parent admission.
/// Collection heads (`all`, `each`) and determined selection heads are separate
/// grammars. This is a dispatch hint; the target child validates its full head,
/// bounds and selector, including malformed members of a supported family.
pub(in crate::oracle::canonical) fn starts_explicit_target_slot(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    ordinal_target_head_follows(cursor)
        || cursor.peek().is_some_and(|raw| {
            raw.eq_ignore_ascii_case("target")
                || raw.eq_ignore_ascii_case("another")
                || raw.eq_ignore_ascii_case("up")
                || raw.eq_ignore_ascii_case("any")
                || is_literal_quantity(cursor)
        })
}

/// `a second|third|fourth|fifth target`: an ordinal target head.
pub(in crate::oracle::canonical) fn ordinal_target_head_follows(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("a"))
        && cursor.peek_ahead(1).is_some_and(|raw| {
            ["second", "third", "fourth", "fifth"]
                .iter()
                .any(|ordinal| raw.eq_ignore_ascii_case(ordinal))
        })
        && cursor
            .peek_ahead(2)
            .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
}

/// The end of a target phrase without a selector noun.
fn target_slot_boundary(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_none_or(|raw| {
        matches!(raw, "." | "," | ";" | ":")
            || raw.eq_ignore_ascii_case("and")
            || raw.eq_ignore_ascii_case("instead")
    })
}

/// True when `target` introduces a possessive player (`target player's`,
/// `target opponent's`) that owns a following noun such as a zone. That head
/// selects a player, never an object target slot, so object-target families
/// must leave it to the shared zone-collection operand.
pub(in crate::oracle::canonical) fn starts_possessive_target_player(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    let mut trial = cursor.clone();
    trial
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("target"))
        && trial.consume("target").is_ok()
        && trial
            .peek_token()
            .is_some_and(|token| token.token_type == "possessivePlayerRoleWord")
}

fn starts_target_slot(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("up")
            || raw.eq_ignore_ascii_case("any")
            || raw.eq_ignore_ascii_case("a")
            || raw.eq_ignore_ascii_case("an")
            || (is_literal_quantity(cursor) && quantified_target_head_follows(cursor))
    })
}

/// A cardinal continues a target list only as a target quantifier (`two
/// target ...`, `one or two target ...`), never as the amount of a gapped
/// verb phrase (`and 1 damage to you`).
fn quantified_target_head_follows(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor.peek_ahead(1).is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("other")
            || raw.eq_ignore_ascii_case("or")
    })
}

fn is_literal_quantity(cursor: &OracleTokenCursor<'_>) -> bool {
    cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "quantity" && !token.raw.eq_ignore_ascii_case("x"))
}

#[cfg(test)]
#[path = "target_list_scope_tests.rs"]
mod scope_tests;
