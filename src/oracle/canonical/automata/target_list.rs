use super::super::{MachineResult, OracleTokenCursor};
use super::{
    ExpressedTargetCardinality, NumericAtom, PlayerPredicate, TargetCardinality, TargetListNode,
    TargetSelector, TargetSlot, recognize_numeric_atom, recognize_target_slot_selector,
};

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
                    .push(recognize_target_slot(cursor, true, false)?);
                TargetListState::Continuation
            }
            TargetListState::Continuation => {
                cursor.enter("target-list.continuation");
                if let Some(slot) = try_recognize_additional_slot(cursor)? {
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
        }],
    })
}

fn recognize_target_slot(
    cursor: &mut OracleTokenCursor<'_>,
    allow_any_target: bool,
    allow_determined_head: bool,
) -> MachineResult<TargetSlot> {
    let mut state = TargetSlotState::Start;
    let mut cardinality = None;
    let mut selector = None;
    let mut exclude_source = false;

    loop {
        state = match state {
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
                cursor.consume("target")?;
                TargetSlotState::Selector
            }
            TargetSlotState::DeterminedHead => {
                cursor.enter("target-list.slot.determined-head");
                let determiner = cursor.peek().expect("checked determiner").to_string();
                cursor.consume(&determiner)?;
                TargetSlotState::Selector
            }
            TargetSlotState::Selector => {
                cursor.enter("target-list.slot.selector");
                selector = Some(recognize_target_slot_selector(cursor)?);
                TargetSlotState::Complete
            }
            TargetSlotState::Complete => {
                cursor.enter("target-list.slot.complete");
                return Ok(TargetSlot {
                    selector: cursor.require_value(selector, "a populated target selector")?,
                    cardinality: cursor
                        .require_value(cardinality, "a populated target cardinality")?,
                    exclude_source,
                });
            }
        };
    }
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
    if !starts_target_slot(cursor) {
        cursor.rollback(checkpoint);
        return Ok(None);
    }

    cursor.enter("target-list.continuation.committed");
    recognize_target_slot(cursor, true, true).map(Some)
}

/// The first-slot target head is shared with targeted-effect parent admission.
/// Collection heads (`all`, `each`) and determined selection heads are separate
/// grammars. This is a dispatch hint; the target child validates its full head,
/// bounds and selector, including malformed members of a supported family.
pub(in crate::oracle::canonical) fn starts_explicit_target_slot(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    cursor.peek().is_some_and(|raw| {
        raw.eq_ignore_ascii_case("target")
            || raw.eq_ignore_ascii_case("another")
            || raw.eq_ignore_ascii_case("up")
            || raw.eq_ignore_ascii_case("any")
            || is_literal_quantity(cursor)
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
