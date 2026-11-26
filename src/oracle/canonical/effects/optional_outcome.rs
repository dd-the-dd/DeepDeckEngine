//! `If you do` / `If you don't` after an optional instruction the sequence
//! itself states (CR 608.2c): `If it shares a creature type with this
//! creature, you may reveal it. If you do, create ...` (Kinship), `You may
//! reveal a Dinosaur card from your hand. If you do or if you control
//! another Dinosaur, you gain 3 life.`
//!
//! The condition language reads the outcome as a symbolic atom
//! (`ActionOutcomeNode::OptionalInstruction`), lowered as a pending
//! condition. Once the sequence's conditionals are wrapped, this module
//! rewrites each outcome conditional into the measured `optionalAction` of
//! the instruction just before it: its instructions become the measured
//! group (`actions`), the outcome's branches become `onPerformed` and
//! `onDeclined`. When a sentence conditional guards the instruction (it is
//! the last action of the guard's branch), a failed guard offers nothing,
//! so `you do` is false there: the outcome's declined branch also runs
//! where the guard fails. `you do or CONDITION` performs the branch, or
//! tests the condition where the instruction was declined or never offered.
//!
//! Anything else (no optional instruction just before the outcome, a
//! guarded `If you don't`, an instruction whose performance the runtime
//! cannot measure) is an explicit failure, never an always-true outcome.
use super::*;

/// The pending outcome atom the condition lowering emits.
pub(in crate::oracle::canonical) const OPTIONAL_OUTCOME_CONDITION: &str = "pendingOptionalOutcome";

/// Runtime kinds a measured instruction group may contain (the engine's
/// `optional_action` roles): selections, neutral bookkeeping and measured
/// primitives. The engine still validates every member.
const GROUP_MEMBER_KINDS: &[&str] = &[
    "chooseCards",
    "choosePermanents",
    "bind",
    "shuffleZone",
    "shuffleLibrariesHolding",
    "recordLibrarySearch",
    "revealCards",
    "drawCards",
    "mill",
    "discardCards",
    "discardHand",
    "gainLife",
    "loseLife",
    "sacrificePermanents",
    "sacrificePermanent",
    "exileCards",
    "removeCounters",
    "createTokens",
    "putCounters",
    "tapPermanent",
    "untapPermanent",
    "destroyPermanent",
    "exilePermanent",
    "returnToOwnersHand",
    "moveCards",
    "exileTargetCards",
    "dealDamage",
    "gainControlPermanent",
    "attachPermanent",
    "modifyPowerToughness",
    "grantKeyword",
];

/// Runtime kinds whose performance is measured (a group needs one).
const MEASURED_KINDS: &[&str] = &[
    "revealCards",
    "drawCards",
    "mill",
    "discardCards",
    "discardHand",
    "gainLife",
    "loseLife",
    "sacrificePermanents",
    "sacrificePermanent",
    "exileCards",
    "removeCounters",
    "createTokens",
    "putCounters",
    "tapPermanent",
    "untapPermanent",
    "destroyPermanent",
    "exilePermanent",
    "returnToOwnersHand",
    "moveCards",
    "exileTargetCards",
    "dealDamage",
    "gainControlPermanent",
    "attachPermanent",
    "modifyPowerToughness",
    "grantKeyword",
];

fn is_pending_outcome(value: &Value) -> bool {
    value["kind"].as_str() == Some(OPTIONAL_OUTCOME_CONDITION)
}

fn mentions_pending_outcome(value: &Value) -> bool {
    match value {
        Value::Object(map) => {
            is_pending_outcome(value) || map.values().any(mentions_pending_outcome)
        }
        Value::Array(items) => items.iter().any(mentions_pending_outcome),
        _ => false,
    }
}

/// The shape of an outcome condition this module rewrites.
enum OutcomeForm {
    /// `you do` (true) / `you don't` (false).
    Outcome { performed: bool },
    /// `you do or CONDITION`: the condition is tested where the
    /// instruction was not performed.
    PerformedOr(Value),
}

fn outcome_form(condition: &Value) -> Option<OutcomeForm> {
    if is_pending_outcome(condition) {
        return condition["performed"]
            .as_bool()
            .map(|performed| OutcomeForm::Outcome { performed });
    }
    if condition["kind"].as_str() != Some("or") {
        return None;
    }
    let operands = condition["operands"].as_array()?;
    let (outcomes, others): (Vec<&Value>, Vec<&Value>) = operands
        .iter()
        .partition(|operand| is_pending_outcome(operand));
    let [outcome] = outcomes.as_slice() else {
        return None;
    };
    if outcome["performed"].as_bool() != Some(true)
        || others.is_empty()
        || others
            .iter()
            .any(|operand| mentions_pending_outcome(operand))
    {
        return None;
    }
    let rest = match others.as_slice() {
        [single] => (*single).clone(),
        _ => json!({ "kind": "or", "operands": others }),
    };
    Some(OutcomeForm::PerformedOr(rest))
}

/// A conditional node over `then` and `otherwise`, or the branch alone when
/// the condition is absent.
fn conditional(condition: Value, then: Vec<Value>, otherwise: Vec<Value>) -> Value {
    json!({
        "kind": "conditional",
        "condition": condition,
        "then": then,
        "else": otherwise,
    })
}

/// Whether the instructions of an optional form a measured group.
fn measured_group(effects: &[Value]) -> bool {
    !effects.is_empty()
        && effects.iter().all(|effect| {
            effect["kind"]
                .as_str()
                .is_some_and(|kind| GROUP_MEMBER_KINDS.contains(&kind))
        })
        && effects.iter().any(|effect| {
            effect["kind"]
                .as_str()
                .is_some_and(|kind| MEASURED_KINDS.contains(&kind))
        })
}

/// Rewrites `optionalEffects{player, effects}` into the measured
/// `optionalAction` whose outcomes are `performed` and `declined`.
fn measured_optional(
    cursor: &mut OracleTokenCursor<'_>,
    optional: &Value,
    performed: Vec<Value>,
    declined: Vec<Value>,
) -> Result<Value, EffectSequenceLoweringFailure> {
    cursor.enter("lower.sentence-condition.optional-outcome.instruction");
    let effects = optional["effects"].as_array().cloned().unwrap_or_default();
    let extra_fields = optional.as_object().is_some_and(|map| {
        map.keys()
            .any(|key| !matches!(key.as_str(), "kind" | "player" | "effects" | "oncePerTurn"))
    });
    if extra_fields {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.fail("an optional instruction without a revealed source"),
        ));
    }
    if !measured_group(&effects) {
        return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
            cursor.engine_gap(
                crate::oracle::canonical::engine_gap::EngineGap::MeasuredPerformance,
                "an optional instruction whose performance the runtime measures (CR 608.2c)",
            ),
        ));
    }
    let mut action = json!({
        "kind": "optionalAction",
        "player": optional["player"].clone(),
        "actions": effects,
        "onPerformed": performed,
    });
    if !declined.is_empty() {
        action["onDeclined"] = Value::Array(declined);
    }
    if let Some(once) = optional.get("oncePerTurn") {
        action["oncePerTurn"] = once.clone();
    }
    Ok(action)
}

/// Rewrites every outcome conditional of `effects` (one lowered sequence
/// level) into the measured optional instruction just before it.
pub(super) fn resolve_optional_outcomes(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut Vec<Value>,
) -> Result<(), EffectSequenceLoweringFailure> {
    let mut index = 0;
    while index < effects.len() {
        let outcome = &effects[index];
        if outcome["kind"].as_str() != Some("conditional")
            || !mentions_pending_outcome(&outcome["condition"])
        {
            index += 1;
            continue;
        }
        cursor.enter("lower.sentence-condition.optional-outcome");
        let form = outcome_form(&outcome["condition"]).ok_or_else(|| {
            EffectSequenceLoweringFailure::UnsupportedSupportLimit(cursor.fail(
                "`you do`, `you don't` or `you do or CONDITION` as the whole outcome condition",
            ))
        })?;
        let then = outcome["then"].as_array().cloned().unwrap_or_default();
        let otherwise = outcome["else"].as_array().cloned().unwrap_or_default();
        // What happens when the instruction was performed, declined, or
        // never offered because a guard failed.
        let (performed, declined, unoffered) = match form {
            OutcomeForm::Outcome { performed: true } => (then, otherwise.clone(), otherwise),
            OutcomeForm::Outcome { performed: false } => (otherwise, then, Vec::new()),
            OutcomeForm::PerformedOr(rest) => {
                let tested = conditional(rest, then.clone(), otherwise);
                (then, vec![tested.clone()], vec![tested])
            }
        };
        cursor.enter("lower.sentence-condition.optional-outcome.producer");
        // The first instruction of this sequence: its producer is the last
        // instruction of the program step before it (`You may reveal a
        // Dinosaur card from your hand. If you do or ...`), which the
        // program lowering resolves (`resolve_program_optional_outcomes`).
        let Some(producer_index) = index.checked_sub(1) else {
            index += 1;
            continue;
        };
        let producer = effects[producer_index].clone();
        let rewritten = match producer["kind"].as_str() {
            Some("optionalEffects") => measured_optional(cursor, &producer, performed, declined)?,
            // A guarded instruction: the last action of a sentence
            // conditional without another branch.
            Some("conditional")
                if producer["then"]
                    .as_array()
                    .and_then(|then| then.last())
                    .is_some_and(|last| last["kind"].as_str() == Some("optionalEffects")) =>
            {
                let guard_else = producer["else"].as_array().cloned().unwrap_or_default();
                cursor
                    .require(
                        guard_else.is_empty(),
                        "a guarded optional instruction without an `Otherwise` branch",
                    )
                    .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
                // `If you don't` after a guard: whether an instruction never
                // offered was declined is not stated (CR 608.2c).
                cursor
                    .require(
                        matches!(
                            outcome_form(&effects[index]["condition"]),
                            Some(
                                OutcomeForm::Outcome { performed: true }
                                    | OutcomeForm::PerformedOr(_)
                            )
                        ),
                        "`you do` after a guarded optional instruction, not `you don't`",
                    )
                    .map_err(EffectSequenceLoweringFailure::UnsupportedSupportLimit)?;
                let mut guard_then = producer["then"].as_array().cloned().unwrap_or_default();
                let optional = guard_then
                    .pop()
                    .expect("the guarded branch ends with its optional instruction");
                guard_then.push(measured_optional(cursor, &optional, performed, declined)?);
                let mut guarded = producer.clone();
                guarded["then"] = Value::Array(guard_then);
                guarded["else"] = Value::Array(unoffered);
                guarded
            }
            _ => {
                return Err(EffectSequenceLoweringFailure::UnsupportedSupportLimit(
                    cursor.fail("an optional instruction (`you may ...`) just before the outcome"),
                ));
            }
        };
        effects[producer_index] = rewritten;
        effects.remove(index);
    }
    Ok(())
}

/// Resolves the outcomes a program step left for the step before it, then
/// refuses any outcome still without its producer: an explicit failure,
/// never an always-true outcome.
pub(in crate::oracle::canonical) fn resolve_program_optional_outcomes(
    cursor: &mut OracleTokenCursor<'_>,
    effects: &mut Vec<Value>,
) -> MachineResult<()> {
    let unwrap = |failure| match failure {
        EffectSequenceLoweringFailure::UnsupportedSupportLimit(failure)
        | EffectSequenceLoweringFailure::FailedSupport(failure)
        | EffectSequenceLoweringFailure::FailedInvariant(failure) => failure,
    };
    nest_outcomes_of_guarded_options(effects);
    for effect in effects.iter_mut() {
        if effect["kind"].as_str() == Some("optionalEffects")
            && let Some(inner) = effect["effects"].as_array_mut()
            && inner.iter().any(|inner| {
                inner["kind"].as_str() == Some("conditional")
                    && mentions_pending_outcome(&inner["condition"])
            })
        {
            resolve_optional_outcomes(cursor, inner).map_err(unwrap)?;
        }
    }
    resolve_optional_outcomes(cursor, effects).map_err(unwrap)?;
    cursor.enter("lower.sentence-condition.optional-outcome.producer");
    cursor.require(
        !effects.iter().any(|effect| {
            effect["kind"].as_str() == Some("conditional")
                && mentions_pending_outcome(&effect["condition"])
        }),
        "an optional instruction just before the outcome",
    )
}

/// `You may look at the top card of your library. If it shares a creature
/// type with this creature, you may reveal it. If you do, ...` (Kinship):
/// the outcome's producer is the guarded option at the end of the previous
/// step's permission, which offers it only when that permission was used.
/// A `you do` outcome without a declined branch is false wherever the inner
/// option was not offered, so it is read inside the outer permission, after
/// the guarded option it measures (CR 608.2c).
fn nest_outcomes_of_guarded_options(effects: &mut Vec<Value>) {
    fn ends_with_guarded_option(effects: &[Value]) -> bool {
        effects.last().is_some_and(|last| {
            last["kind"].as_str() == Some("conditional")
                && last["else"].as_array().is_none_or(Vec::is_empty)
                && last["then"]
                    .as_array()
                    .and_then(|then| then.last())
                    .is_some_and(|option| option["kind"].as_str() == Some("optionalEffects"))
        })
    }
    let mut index = 1;
    while index < effects.len() {
        let outcome = &effects[index];
        let nestable = outcome["kind"].as_str() == Some("conditional")
            && outcome["else"].as_array().is_none_or(Vec::is_empty)
            && matches!(
                outcome_form(&outcome["condition"]),
                Some(OutcomeForm::Outcome { performed: true })
            )
            && effects[index - 1]["kind"].as_str() == Some("optionalEffects")
            && effects[index - 1]["effects"]
                .as_array()
                .is_some_and(|inner| ends_with_guarded_option(inner));
        if nestable {
            let outcome = effects.remove(index);
            if let Some(inner) = effects[index - 1]["effects"].as_array_mut() {
                inner.push(outcome);
            }
        } else {
            index += 1;
        }
    }
}
