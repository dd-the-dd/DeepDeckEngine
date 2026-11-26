//! A postposed `where X is EXPR` read by a target declaration (`counter
//! target spell with mana value X or less, where X is the number of Faeries
//! you control`, `put a +1/+1 counter on each of up to X target creatures,
//! where X is ...`).
//!
//! Targets are chosen when the ability is put on the stack (CR 601.2c,
//! 603.3d), before anything of its program resolves, and a candidate is
//! rechecked against the same characteristic when the ability resolves (CR
//! 608.2b). A definition of X that a declaration reads is therefore an amount
//! of the game read at those moments, not a value the program binds while it
//! resolves: the declaration carries the amount itself in place of the X
//! reference, and the engine reads it each time it enumerates candidates.
//!
//! Only amounts the engine reads without a resolving program are admitted:
//! counts and player facts relative to the declaring player, and the
//! ability source's own characteristics in a candidate comparison. An amount
//! that reads an action result, a triggering event, a declared target or a
//! decision has no value yet when targets are chosen; it fails here instead
//! of being read as zero or as an empty candidate set.

use super::*;

/// Where an X reference of one declaration decision sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DeclarationSite {
    /// The candidate domain (`with mana value X or less`).
    Candidates,
    /// The declared number of targets (`up to X target creatures`).
    Cardinality,
}

fn kind(value: &Value) -> Option<&str> {
    value.get("kind").and_then(Value::as_str)
}

fn is_x_reference(value: &Value) -> bool {
    kind(value) == Some("decisionResult")
        && value.get("decisionId").and_then(Value::as_str) == Some("xValue")
}

fn contains_x_reference(value: &Value) -> bool {
    is_x_reference(value)
        || match value {
            Value::Object(object) => object.values().any(contains_x_reference),
            Value::Array(items) => items.iter().any(contains_x_reference),
            _ => false,
        }
}

fn replace_x_reference(value: &mut Value, amount: &Value) -> usize {
    if is_x_reference(value) {
        *value = amount.clone();
        return 1;
    }
    match value {
        Value::Object(object) => object
            .values_mut()
            .map(|child| replace_x_reference(child, amount))
            .sum(),
        Value::Array(items) => items
            .iter_mut()
            .map(|child| replace_x_reference(child, amount))
            .sum(),
        _ => 0,
    }
}

/// A player reference that denotes the player declaring the targets: absent
/// (the amount's own default), the controller, or the source's controller.
fn declaring_player(reference: Option<&Value>) -> bool {
    match reference {
        None => true,
        Some(reference) => match kind(reference) {
            Some("controller" | "abilityController") => true,
            Some("controllerOf") => matches!(
                reference.get("object").and_then(kind),
                Some("self" | "abilitySource")
            ),
            _ => false,
        },
    }
}

/// A count or player fact relative to the declaring player, read from the
/// game state alone.
fn declaring_player_leaf(expression: &Value) -> bool {
    if expression.get("excludeSource").and_then(Value::as_bool) == Some(true) {
        return false;
    }
    let all_players = expression.get("allPlayers").and_then(Value::as_bool) == Some(true);
    let player = expression.get("player");
    match kind(expression) {
        Some("countPermanents") => all_players || declaring_player(player),
        Some("countCards") => {
            matches!(
                expression.get("zone").and_then(kind),
                Some("hand" | "graveyard" | "library")
            ) && (all_players
                || declaring_player(expression.get("zone").and_then(|zone| zone.get("player"))))
        }
        Some(
            "countPermanentsControlledByOpponents"
            | "countDistinctPermanentNames"
            | "domainCount"
            | "greatestPower"
            | "greatestToughness"
            | "sumPowers"
            | "countOpponents"
            | "countOpponentsAttackedThisCombat"
            | "countOpponentsAttackedThisTurn"
            | "devotion"
            | "lifeTotal"
            | "lifeGainedThisTurn"
            | "countPlayerCounters",
        ) => declaring_player(player),
        _ => false,
    }
}

/// `this creature's power`, `the number of verse counters on this
/// enchantment`: a characteristic of the ability source.
fn source_characteristic(expression: &Value) -> bool {
    let on_source = matches!(
        expression.get("object").and_then(kind),
        Some("self" | "abilitySource")
    );
    on_source
        && match kind(expression) {
            Some("powerOf" | "toughnessOf") => true,
            Some("countCounters") => expression.get("counter").is_some_and(Value::is_string),
            _ => false,
        }
}

/// Integer arithmetic over leaves `leaf` admits, with at least one leaf.
fn arithmetic_over(expression: &Value, leaf: &dyn Fn(&Value) -> bool) -> bool {
    fn visit(
        expression: &Value,
        depth: usize,
        leaf: &dyn Fn(&Value) -> bool,
        leaves: &mut usize,
    ) -> bool {
        if depth >= 32 {
            return false;
        }
        if expression.as_i64().is_some() {
            return true;
        }
        match kind(expression) {
            Some("add" | "subtract" | "multiply") => {
                visit(&expression["left"], depth + 1, leaf, leaves)
                    && visit(&expression["right"], depth + 1, leaf, leaves)
            }
            Some("divide") => {
                matches!(
                    expression.get("round").and_then(Value::as_str),
                    Some("up" | "down")
                ) && visit(&expression["left"], depth + 1, leaf, leaves)
                    && visit(&expression["right"], depth + 1, leaf, leaves)
            }
            _ if leaf(expression) => {
                *leaves += 1;
                true
            }
            _ => false,
        }
    }
    let mut leaves = 0;
    visit(expression, 0, leaf, &mut leaves) && leaves > 0
}

/// The right side of a candidate comparison after X is replaced: a source
/// characteristic alone, or arithmetic over declaring-player facts.
fn candidate_amount(expression: &Value) -> bool {
    source_characteristic(expression) || arithmetic_over(expression, &declaring_player_leaf)
}

/// A declared number of targets: arithmetic over declaring-player facts,
/// the colors among that player's permanents, or the counters on the
/// source.
fn cardinality_amount(expression: &Value) -> bool {
    arithmetic_over(expression, &|leaf| {
        declaring_player_leaf(leaf)
            || (kind(leaf) == Some("countDistinctColors") && declaring_player(leaf.get("player")))
            || (kind(leaf) == Some("countCounters") && source_characteristic(leaf))
    })
}

/// Replaces each X reference of `value` at `site` by `amount`. A reference
/// in a candidate domain must be the threshold of a comparison of the
/// candidate's own property; any other position fails.
fn bind_at(
    value: &mut Value,
    site: Option<DeclarationSite>,
    amount: &Value,
) -> Result<usize, &'static str> {
    if is_x_reference(value) {
        return match site {
            Some(DeclarationSite::Cardinality) if cardinality_amount(amount) => {
                *value = amount.clone();
                Ok(1)
            }
            Some(DeclarationSite::Cardinality) => Err(
                "a number of targets read when they are chosen: a count or player fact of the declaring player, or counters on the source",
            ),
            _ => Err("X as a target count or as the threshold of a candidate comparison"),
        };
    }
    if site == Some(DeclarationSite::Candidates)
        && kind(value) == Some("compare")
        && value
            .get("left")
            .and_then(|left| left.get("object"))
            .and_then(kind)
            == Some("candidate")
        && value.get("right").is_some_and(contains_x_reference)
    {
        let right = &mut value["right"];
        let replaced = replace_x_reference(right, amount);
        if !candidate_amount(right) {
            return Err(
                "a candidate threshold read when targets are chosen: a count or player fact of the declaring player, or a characteristic of the source",
            );
        }
        return Ok(replaced);
    }
    match value {
        Value::Object(object) => {
            let mut replaced = 0;
            for (key, child) in object.iter_mut() {
                let child_site = match key.as_str() {
                    "candidates" if site.is_none() => Some(DeclarationSite::Candidates),
                    "minimum" | "maximum" if site.is_none() => Some(DeclarationSite::Cardinality),
                    _ => site,
                };
                replaced += bind_at(child, child_site, amount)?;
            }
            Ok(replaced)
        }
        Value::Array(items) => {
            let mut replaced = 0;
            for item in items {
                replaced += bind_at(item, site, amount)?;
            }
            Ok(replaced)
        }
        _ => Ok(0),
    }
}

/// Replaces the X references of the declaration `decisions` by the
/// definition's `amount`, read again each time candidates are enumerated.
/// Returns how many references were replaced, or the expectation a reference
/// or the amount does not meet.
pub(super) fn bind_declaration_x(
    decisions: &mut [Value],
    amount: &Value,
) -> Result<usize, &'static str> {
    let mut replaced = 0;
    for decision in decisions {
        replaced += bind_at(decision, None, amount)?;
    }
    Ok(replaced)
}

/// The single declared spell of each `counter target spell` action, with
/// the index of the action that declares and counters it.
pub(super) fn countered_spell_bindings(
    sequence: &EffectSequenceNode,
    target_bindings: &[(TargetBinding, String)],
) -> Vec<(TargetBinding, usize)> {
    target_bindings
        .iter()
        .filter(|(binding, _)| {
            matches!(
                sequence.effects.get(binding.effect_index),
                Some(LocalEffectNode::CounterSpell(counter))
                    if !counter.event_spell
                        && counter.target.slots.get(binding.slot_index).is_some_and(|slot| {
                            slot.cardinality == TargetCardinality::EXACTLY_ONE
                        })
            )
        })
        .map(|(binding, _)| (*binding, binding.effect_index))
        .collect()
}

/// The numeric target slots of the countered spells, read while each spell
/// is still on the stack: the read precedes the counter action
/// (`target_read_position`), so no zone change separates it from the
/// declaration (CR 608.2h is not needed).
pub(super) fn countered_spell_slots(
    sequence: &EffectSequenceNode,
    target_bindings: &[(TargetBinding, String)],
) -> Vec<NumericTargetSlot> {
    countered_spell_bindings(sequence, target_bindings)
        .into_iter()
        .filter_map(|(binding, effect_index)| {
            let LocalEffectNode::CounterSpell(counter) = sequence.effects.get(effect_index)? else {
                return None;
            };
            let slot = counter.target.slots.get(binding.slot_index)?;
            let target_id = target_bindings
                .iter()
                .find_map(|(actual, id)| (*actual == binding).then(|| id.clone()))?;
            Some(NumericTargetSlot {
                binding,
                target_id,
                selector: slot.selector.clone(),
                requires_post_zone_change_lki: false,
            })
        })
        .collect()
}

/// The target bindings `expression` reads when it reads nothing but
/// declared target properties and literals; `None` when it reads anything
/// else, whose value may change before the counter action.
fn target_reads_only(expression: &NumericExpression, reads: &mut Vec<TargetBinding>) -> bool {
    match expression {
        NumericExpression::Atom(NumericAtom::Integer(_)) => true,
        NumericExpression::PowerOf(binding) | NumericExpression::ManaValueOf(binding) => {
            reads.push(*binding);
            true
        }
        NumericExpression::QualifiedTargetProperty { object, .. } => {
            reads.push(*object);
            true
        }
        NumericExpression::Add { left, right }
        | NumericExpression::Subtract { left, right }
        | NumericExpression::Multiply { left, right }
        | NumericExpression::Divide { left, right, .. } => {
            target_reads_only(left, reads) && target_reads_only(right, reads)
        }
        _ => false,
    }
}

/// Where a definition reading a countered spell is computed: before the
/// first counter action whose spell it reads. `None` when the definition
/// reads no countered spell, or reads anything besides target properties.
pub(super) fn target_read_position(
    expression: &NumericExpression,
    counter_slots: &[(TargetBinding, usize)],
    action_starts: &[(usize, usize)],
) -> Option<usize> {
    let mut reads = Vec::new();
    if !target_reads_only(expression, &mut reads) {
        return None;
    }
    counter_slots
        .iter()
        .filter(|(binding, _)| reads.contains(binding))
        .map(|(_, effect_index)| action_starts[*effect_index].0)
        .min()
}

#[cfg(test)]
#[path = "where_x_declaration_tests.rs"]
mod tests;
