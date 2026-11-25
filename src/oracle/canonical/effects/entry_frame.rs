//! Lowering of the shared entry-replacement frame (CR 614.1c-e, 614.12,
//! 707.9) to one `wouldEnterBattlefield` replacement of the current source.
//!
//! Each element of the frame lowers to its own replacement primitive and the
//! primitives compose in one ordered program: a copy (`copyEnteringPermanent`)
//! precedes the entering state (`setEnteringState`), counters
//! (`putEnteringCounters`) and the stored choice (`storeDecision`). A choice
//! stores its value on the permanent under the decision id of its binding,
//! which later abilities of the same face read (`entry_choice_binding.rs`).
//! An element the runtime cannot apply fails here with its own state; no
//! element is dropped.
use super::super::*;
use crate::oracle::characteristic_vocabulary::{
    CardColor, ORACLE_CARD_TYPES, type_line_has_permanent_card_type,
};
use crate::oracle::syntax::AbilityInput;

/// The outcome of a shared replacement frame for one ability.
pub(in crate::oracle::canonical) enum ReplacementFrameAttempt {
    Parsed(CanonicalRuleDraft),
    /// The frame read its source and entry verb, then failed.
    Committed(MachineFailure),
    NotClaimed,
}

const OPTIONAL_ENTRY_DECISION: &str = "optionalEntryReplacement";
const ENTRY_COPY_DECISION: &str = "entryCopy";
const ENTRY_REVEAL_DECISION: &str = "entryReveal";
const BASIC_LAND_TYPES: [&str; 5] = ["Plains", "Island", "Swamp", "Mountain", "Forest"];

pub(in crate::oracle::canonical) fn try_parse_entry_frame(
    input: &AbilityInput<'_>,
) -> ReplacementFrameAttempt {
    let mut cursor = OracleTokenCursor::new("entry-frame", input.tokens);
    let node = match try_recognize_entry_frame(
        &mut cursor,
        input.name_links,
        input.name_resolution_failure,
    ) {
        ChildOutcome::Matched(node) => node,
        ChildOutcome::Committed(failure) => return ReplacementFrameAttempt::Committed(failure),
        ChildOutcome::NoMatch(_) => return ReplacementFrameAttempt::NotClaimed,
    };
    match lower_entry_frame(&mut cursor, input, &node) {
        Ok(draft) => ReplacementFrameAttempt::Parsed(draft),
        Err(failure) => ReplacementFrameAttempt::Committed(failure),
    }
}

/// The decision producing the value a choice stores (CR 614.12).
fn lower_entry_choice_decision(choice: &EntryChoiceNode) -> Value {
    let binding = choice.domain.binding();
    let mut decision = json!({
        "id": binding.decision_id(),
        "kind": binding.decision_kind(),
        // The entering permanent's controller chooses (CR 614.12); the
        // entry choice reader produces no other chooser.
        "player": controller(),
    });
    match &choice.domain {
        EntryChoiceDomain::Color { excluded } => {
            decision["options"] = json!(
                [
                    CardColor::White,
                    CardColor::Blue,
                    CardColor::Black,
                    CardColor::Red,
                    CardColor::Green,
                ]
                .into_iter()
                .filter(|color| !excluded.contains(color))
                .map(CardColor::ir_symbol)
                .collect::<Vec<_>>()
            );
        }
        EntryChoiceDomain::CreatureType => {}
        EntryChoiceDomain::BasicLandType => decision["options"] = json!(BASIC_LAND_TYPES),
        EntryChoiceDomain::CardType => {
            decision["options"] = json!(
                ORACLE_CARD_TYPES
                    .iter()
                    .map(|card_type| card_type.ir_name())
                    .collect::<Vec<_>>()
            );
        }
    }
    decision
}

/// The copy decision and its replacement operation (CR 707.9). The copied
/// permanent is chosen among battlefield permanents matching the shared
/// query; the stated exceptions modify the copiable values it gets.
fn lower_entry_copy(
    cursor: &mut OracleTokenCursor<'_>,
    copy: &EntryCopyNode,
    optional: bool,
    tapped: bool,
) -> MachineResult<(Value, Value)> {
    cursor.enter("replacement.entry-frame.copy.lowering");
    cursor.require(
        !copy.object.exclude_source,
        "a copied permanent without a source exclusion (the source is not yet on the battlefield)",
    )?;
    cursor.require(
        domain_filter_is_representable(&copy.object.predicate),
        "a copied-permanent predicate representable without erasing a Boolean branch",
    )?;
    let filter = lower_permanent_object_predicate(&copy.object.predicate);
    cursor.require(
        !filter.is_null(),
        "a copied-permanent predicate that constrains the battlefield permanent",
    )?;
    let controlled = match copy.object.controller {
        None => false,
        Some(ObjectControllerScope::AbilityController) => true,
        Some(_) => {
            return Err(cursor.fail(
                "a copied permanent on the battlefield or one you control (the entry choice reads no other controller)",
            ));
        }
    };
    let mut decision = json!({
        "id": ENTRY_COPY_DECISION,
        "kind": "chooseBattlefieldPermanent",
        "optional": optional,
        "where": filter,
    });
    if controlled {
        decision["controlledByEnteringController"] = json!(true);
    }
    let mut operation = json!({
        "kind": "copyEnteringPermanent",
        "decisionId": ENTRY_COPY_DECISION,
        "tapped": tapped,
    });
    if copy.exceptions.not_legendary {
        operation["removeLegendary"] = json!(true);
    }
    cursor.enter("replacement.entry-frame.copy-exception.runtime");
    cursor.require(
        !copy.exceptions.no_mana_cost,
        "a copy exception the entry copy applies: granted keywords or `isn't legendary`",
    )?;
    if let Some(changes) = &copy.exceptions.changes {
        cursor.enter("replacement.entry-frame.copy-exception.runtime");
        cursor.require(
            changes.subtypes.is_empty()
                && changes.supertypes.is_empty()
                && changes.copy_name.is_none()
                && changes.card_types.is_empty()
                && changes.colors.is_none()
                && changes.base_power_toughness().is_none()
                && changes.characteristics.abilities.is_empty()
                && !changes.characteristics.keywords.is_empty(),
            "a copy exception the entry copy applies: granted keywords or `isn't legendary`",
        )?;
        operation["grantKeywords"] = json!(
            changes
                .characteristics
                .keywords
                .iter()
                .map(|keyword| keyword.canonical_name())
                .collect::<Vec<_>>()
        );
    }
    Ok((decision, operation))
}

/// The entering state's own operations: `setEnteringState` for `tapped`
/// and one `putEnteringCounters` per counter kind. A copy carries its own
/// tapped state, so the caller passes `tapped` only without one.
fn lower_entering_state_operations(
    cursor: &mut OracleTokenCursor<'_>,
    tapped: bool,
    counters: &[EntryCounterNode],
) -> MachineResult<Vec<Value>> {
    let mut operations = Vec::new();
    if tapped {
        operations.push(json!({
            "kind": "setEnteringState",
            "object": self_ref(),
            "tapped": true,
        }));
    }
    for counter in counters {
        cursor.enter("replacement.entry-frame.counters.lowering");
        let count = cursor.require_value(
            lower_numeric_expression(&counter.count),
            "a counter quantity the entry replacement can evaluate",
        )?;
        operations.push(json!({
            "kind": "putEnteringCounters",
            "counter": counter.counter.as_str(),
            "count": count,
        }));
    }
    Ok(operations)
}

/// The `if` / `unless` condition checked as the permanent enters, read by
/// the shared sentence-condition lowering with no bound objects.
fn lower_entry_condition(
    cursor: &mut OracleTokenCursor<'_>,
    clause: &SourceEntryConditionNode,
) -> MachineResult<Value> {
    cursor.enter("replacement.entry-frame.condition-lowering");
    let condition = lower_sentence_condition(
        cursor,
        &clause.condition,
        &ConditionObjectBindings {
            permanent_targets: &[],
            card_targets: &[],
            card_focus: None,
            previous_action_result: None,
        },
    )?;
    Ok(if clause.unless {
        negation(condition)
    } else {
        condition
    })
}

/// The one optional entry cost the entry replacement runtime applies: one
/// card the controller chooses to reveal from their hand (a
/// `chooseRevealCard` decision). Every other payment action stays an
/// explicit engine gap; none is approximated by a reveal.
fn entry_revealed_card_filter<'a>(
    cursor: &mut OracleTokenCursor<'_>,
    cost: &'a PaymentActionNode,
) -> MachineResult<&'a ObjectPredicate> {
    cursor.enter("replacement.entry-cost.runtime");
    match cost {
        PaymentActionNode::Reveal {
            objects:
                PaymentObjectsNode::Chosen(ChosenPaymentObjectsNode {
                    count: CostQuantity::Exactly(NumericAtom::Integer(1)),
                    exclude_source: false,
                    filter: PaymentObjectFilter::Cards(filter),
                    method: CollectionSelectionMethod::Choice,
                }),
            origin: Some(PaymentOriginZone::Hand),
        } => Ok(filter),
        _ => Err(cursor.engine_gap(
            crate::oracle::canonical::engine_gap::EngineGap::ResolutionCostAction,
            "an optional entry cost the entry replacement applies: revealing one chosen card from your hand",
        )),
    }
}

/// `you may COST. If you do|don't, SOURCE enters STATE [CONDITION].`: the
/// cost is one decision; the stated outcome of that decision, and the
/// state's own condition, gate the shared entering-state operations.
fn lower_entry_optional_cost(
    cursor: &mut OracleTokenCursor<'_>,
    node: &EntryOptionalCostNode,
) -> MachineResult<(Value, Value)> {
    let card = entry_revealed_card_filter(cursor, &node.cost)?;
    cursor.enter("replacement.entry-cost.lowering");
    cursor.require(
        domain_filter_is_representable(card),
        "a revealed-card predicate representable without erasing a Boolean branch",
    )?;
    let filter = lower_card_object_predicate(card);
    cursor.require(
        !filter.is_null(),
        "a revealed-card predicate that constrains the card",
    )?;
    cursor.enter("replacement.entry-cost.outcome-state");
    cursor.require(
        node.state.copy.is_none(),
        "an outcome entering state without a copy (the copy choice is made before the cost)",
    )?;
    let operations =
        lower_entering_state_operations(cursor, node.state.tapped, &node.state.counters)?;
    let decision = json!({
        "id": ENTRY_REVEAL_DECISION,
        "kind": "chooseRevealCard",
        "where": filter.clone(),
        "minimum": integer(0),
        "maximum": integer(1),
    });
    let revealed = json!({
        "kind": "countDecisionCardsMatching",
        "decisionId": ENTRY_REVEAL_DECISION,
        "where": filter,
    });
    let outcome = json!({
        "kind": "compare",
        "operator": match node.outcome {
            PerformanceOutcome::Declined => "==",
            _ => ">",
        },
        "left": revealed,
        "right": integer(0),
    });
    let condition = match &node.state.condition {
        Some(clause) => and(vec![outcome, lower_entry_condition(cursor, clause)?]),
        None => outcome,
    };
    let operation = json!({
        "kind": "conditional",
        "condition": condition,
        "then": operations,
        "else": [],
    });
    Ok((decision, operation))
}

fn lower_entry_frame(
    cursor: &mut OracleTokenCursor<'_>,
    input: &AbilityInput<'_>,
    node: &EntryFrameNode,
) -> MachineResult<CanonicalRuleDraft> {
    cursor.enter("replacement.entry-frame.validation");
    cursor.require(
        type_line_has_permanent_card_type(input.face_type_line),
        "a permanent card face for an entering source",
    )?;
    let mut decisions = Vec::new();
    let mut replacement = Vec::new();
    let mut conditions = Vec::new();
    if let Some(copy) = &node.copy {
        let (decision, operation) = lower_entry_copy(cursor, copy, node.optional, node.tapped)?;
        decisions.push(decision);
        replacement.push(operation);
    } else if node.optional {
        // CR 614.1c: the controller chooses, as it enters, whether the
        // optional replacement applies.
        decisions.push(json!({ "id": OPTIONAL_ENTRY_DECISION, "kind": "chooseWhether" }));
        conditions.push(json!({
            "kind": "decisionWas",
            "decisionId": OPTIONAL_ENTRY_DECISION,
            "value": true,
        }));
    }
    replacement.extend(lower_entering_state_operations(
        cursor,
        node.tapped && node.copy.is_none(),
        &node.counters,
    )?);
    if let Some(optional_cost) = &node.optional_cost {
        let (decision, operation) = lower_entry_optional_cost(cursor, optional_cost)?;
        decisions.push(decision);
        replacement.push(operation);
    }
    if let Some(choice) = &node.choice {
        cursor.enter("replacement.entry-choice.lowering");
        cursor.require(
            choice.player == LocalPlayerReference::Controller,
            "the entering permanent's controller as the chooser (CR 614.12)",
        )?;
        decisions.push(lower_entry_choice_decision(choice));
        replacement.push(json!({
            "kind": "storeDecision",
            "decisionId": choice.domain.binding().decision_id(),
        }));
    }
    if let Some(clause) = &node.condition {
        conditions.push(lower_entry_condition(cursor, clause)?);
    }
    cursor.enter("replacement.entry-frame.program");
    cursor.require(
        !replacement.is_empty(),
        "at least one entry replacement operation",
    )?;
    let mut rule = json!({
        "kind": "replacementEffect",
        "source": self_ref(),
        "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
        "replacement": replacement,
    });
    if !decisions.is_empty() {
        rule["decisions"] = Value::Array(decisions);
    }
    match conditions.len() {
        0 => {}
        1 => rule["condition"] = conditions.remove(0),
        _ => rule["condition"] = and(conditions),
    }
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(draft(
        rule,
        &[
            "Recognize the entering source and each entry element independently",
            "Compose copy, entering state, counters, optional cost outcome and stored choice primitives",
            "Apply one ordered entering replacement program",
            &trace,
        ],
    ))
}

/// The shared replacement frames (an entry replacement, then a replaced
/// event), tried for an ability no historical family completed.
pub(in crate::oracle::canonical) fn try_parse_replacement_frame(
    input: &AbilityInput<'_>,
) -> ReplacementFrameAttempt {
    match try_parse_entry_frame(input) {
        ReplacementFrameAttempt::NotClaimed => try_parse_event_replacement(input),
        outcome => outcome,
    }
}

#[cfg(test)]
#[path = "entry_frame_tests.rs"]
mod tests;
