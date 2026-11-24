//! One-shot event modifiers: `{capture, match, apply, consume, expires}`.
//!
//! The effect captures players when it resolves, installs a pending modifier
//! that matches the first later event of its kind, and applies its typed
//! operations to that event's object. "The next spell you cast this turn
//! can't be countered" is `match: spellCast(player: captured)`,
//! `apply: modifyStackObject(eventSpell, cantBeCountered)` and
//! `consume: firstMatchingEvent`; no operation is named after the phrase.

use super::*;

/// Pending one-shot modifier kind stored in `rule_modifiers`.
const PENDING_KIND: &str = "oneShotEventModifier";

/// Stack-object modifiers an `apply` operation may set.
fn stack_object_modifier_supported(modifier: &Value) -> bool {
    matches!(value_kind(modifier), Some("cantBeCountered"))
}

fn apply_operation_supported(operation: &Value) -> bool {
    (value_kind(operation) == Some("modifyStackObject")
        && value_kind(&operation["object"]) == Some("eventSpell")
        && stack_object_modifier_supported(&operation["modifier"]))
        // CR 601.3e: the matched spell may be cast as though it had flash;
        // the permission is read before the cast that consumes it.
        || value_kind(operation) == Some(CAST_AS_THOUGH_FLASH)
}

/// The pending operation that lets the matched spell be cast at instant
/// speed (CR 702.8a).
const CAST_AS_THOUGH_FLASH: &str = "castAsThoughItHadFlash";

fn match_player_supported(reference: &Value, captures: &serde_json::Map<String, Value>) -> bool {
    match value_kind(reference) {
        Some("capturedValue") => reference["name"]
            .as_str()
            .is_some_and(|name| captures.contains_key(name)),
        _ => player_reference_supported(reference),
    }
}

/// Typed shape check for `installOneShotModifier`.
pub(super) fn install_one_shot_modifier_supported(effect: &Value) -> bool {
    let empty = serde_json::Map::new();
    let captures = match effect.get("capture") {
        None => &empty,
        Some(capture) => match capture.as_object() {
            Some(captures) => captures,
            None => return false,
        },
    };
    captures.values().all(player_reference_supported)
        && value_kind(&effect["expires"]) == Some("endOfCurrentTurn")
        && effect
            .get("consume")
            .is_none_or(|consume| value_kind(consume) == Some("firstMatchingEvent"))
        && value_kind(&effect["match"]) == Some("spellCast")
        && match_player_supported(&effect["match"]["player"], captures)
        && effect["match"]
            .get("where")
            .is_none_or(card_filter_supported)
        && match &effect["apply"] {
            Value::Array(operations) => {
                !operations.is_empty() && operations.iter().all(apply_operation_supported)
            }
            operation => apply_operation_supported(operation),
        }
}

impl GameEngine {
    /// Resolve captures, then install the pending modifier.
    pub(super) fn install_one_shot_modifier(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
    ) {
        let mut captured = BTreeMap::new();
        for (name, reference) in effect["capture"].as_object().into_iter().flatten() {
            let Some(player_id) = self.resolve_player(reference, stack_object, bindings) else {
                return;
            };
            captured.insert(name.clone(), player_id);
        }
        let match_player = &effect["match"]["player"];
        let player_id = match value_kind(match_player) {
            Some("capturedValue") => match_player["name"]
                .as_str()
                .and_then(|name| captured.get(name))
                .cloned(),
            _ => self.resolve_player(match_player, stack_object, bindings),
        };
        let Some(player_id) = player_id else {
            return;
        };
        let operations = match &effect["apply"] {
            Value::Array(operations) => operations.clone(),
            operation => vec![operation.clone()],
        };
        let modifier = json!({
            "kind": PENDING_KIND,
            "event": value_kind(&effect["match"]).unwrap_or_default(),
            "playerId": player_id,
            "where": effect["match"].get("where").cloned().unwrap_or(Value::Null),
            "apply": operations,
            "expiresAfterTurn": self.state.turn_number,
            "sourceCardInstanceId": stack_object.card.instance_id,
        });
        self.state.rule_modifiers.push(modifier.clone());
        self.record_event(
            "oneShotModifierInstalled",
            Some(stack_object.controller.clone()),
            Some(stack_object.card.instance_id.clone()),
            modifier,
        );
    }

    /// Consume the first pending modifier matching this spell cast (CR 601.2i
    /// "becomes cast"), even if its operations change nothing, and return its
    /// operations for the stack object being created.
    pub(super) fn consume_one_shot_spell_cast_modifier(
        &mut self,
        player_id: &str,
        spell: &CardDefinition,
    ) -> Vec<Value> {
        let turn = u64::from(self.state.turn_number);
        let Some(index) = self.state.rule_modifiers.iter().position(|modifier| {
            value_kind(modifier) == Some(PENDING_KIND)
                && modifier["event"].as_str() == Some("spellCast")
                && modifier["playerId"].as_str() == Some(player_id)
                && modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_none_or(|expires| expires >= turn)
                && (modifier["where"].is_null() || matches_card_filter(spell, &modifier["where"]))
        }) else {
            return Vec::new();
        };
        let modifier = self.state.rule_modifiers.remove(index);
        modifier["apply"].as_array().cloned().unwrap_or_default()
    }
}

impl GameEngine {
    /// CR 601.3e: a pending one-shot modifier of `player_id` whose spell
    /// matches `spell` lets it be cast as though it had flash.
    pub(super) fn one_shot_flash_permission(
        &self,
        player_id: &str,
        spell: &CardDefinition,
    ) -> bool {
        let turn = u64::from(self.state.turn_number);
        self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some(PENDING_KIND)
                && modifier["event"].as_str() == Some("spellCast")
                && modifier["playerId"].as_str() == Some(player_id)
                && modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_none_or(|expires| expires >= turn)
                && (modifier["where"].is_null() || matches_card_filter(spell, &modifier["where"]))
                && modifier["apply"].as_array().is_some_and(|operations| {
                    operations
                        .iter()
                        .any(|operation| value_kind(operation) == Some(CAST_AS_THOUGH_FLASH))
                })
        })
    }
}

/// Whether consumed operations set the named stack-object modifier.
pub(super) fn operations_set_stack_modifier(operations: &[Value], modifier_kind: &str) -> bool {
    operations.iter().any(|operation| {
        value_kind(operation) == Some("modifyStackObject")
            && value_kind(&operation["modifier"]) == Some(modifier_kind)
    })
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "one_shot_modifier_tests.rs"]
mod tests;
