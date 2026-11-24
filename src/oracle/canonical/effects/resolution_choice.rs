//! Lowering of the resolution-time `choose` domains (CR 608.2d) and of the
//! bindings they install.
//!
//! A `choose DOMAIN` instruction that is not a target declaration is one
//! effect step that records its result under a decision id. Its consumers
//! never repeat the choice: a chosen color is read by the shared object
//! predicate (`chosenColor`), and a chosen player is the antecedent of a
//! later `that player`, lowered as the bound value the choice writes. Both
//! links are symbolic; the producer must precede every consumer in the same
//! program, which the sequence lowering and `contextual_binding` prove.
use super::super::*;
use crate::oracle::contextual_binding::RESOLUTION_COLOR_DECISION;

/// The decision id of the player chosen by the `choose` instruction at
/// `effect_index`. Each instruction has its own id, so two player choices of
/// one program never share a binding.
pub(in crate::oracle::canonical) fn resolution_player_choice_id(effect_index: usize) -> String {
    format!("resolutionPlayer{effect_index}")
}

/// `choose a color`: the controller chooses one of the five colors.
pub(in crate::oracle::canonical) fn lower_color_choice() -> Value {
    json!({
        "kind": "chooseColor",
        "id": RESOLUTION_COLOR_DECISION,
        "player": controller(),
    })
}

/// `choose an opponent [at random]` / `choose a player`: exactly one player
/// of the population, chosen by the controller or at random. The choice
/// binds the chosen player under `id` for later `that player` references.
pub(in crate::oracle::canonical) fn lower_player_choice(node: PlayerChoiceNode, id: &str) -> Value {
    let candidates = match node.population {
        PlayerChoicePopulation::Opponent => json!({"kind": "opponentsOf", "player": controller()}),
        PlayerChoicePopulation::Player => json!({"kind": "eachPlayer"}),
    };
    let mut choice = json!({
        "kind": "choosePlayers",
        "id": id,
        "player": controller(),
        "candidates": candidates,
        "minimum": integer(1),
        "maximum": integer(1),
        "bindAs": id,
    });
    if node.random {
        choice["selection"] = json!({"kind": "random"});
    }
    choice
}

/// A player chosen on resolution is not a target (CR 115.1): the shared
/// player-binding lowering names it by its declaration id, and this pass
/// turns those references into the bound value the choice writes.
pub(in crate::oracle::canonical) fn rebind_resolution_players(value: &mut Value, ids: &[String]) {
    if ids.is_empty() {
        return;
    }
    if value["kind"].as_str() == Some("chosenTarget")
        && value["id"]
            .as_str()
            .is_some_and(|id| ids.iter().any(|candidate| candidate == id))
    {
        let id = value["id"].clone();
        *value = json!({"kind": "boundValue", "id": id});
        return;
    }
    match value {
        Value::Array(values) => values
            .iter_mut()
            .for_each(|value| rebind_resolution_players(value, ids)),
        Value::Object(values) => values
            .values_mut()
            .for_each(|value| rebind_resolution_players(value, ids)),
        _ => {}
    }
}

/// Whether every player chosen on resolution is read by a later instruction
/// (`that player ...`). A choice nobody reads is refused, not executed as a
/// decision without effect.
pub(in crate::oracle::canonical) fn every_resolution_player_is_read(
    effects: &[Value],
    ids: &[String],
) -> bool {
    fn reads(value: &Value, id: &str) -> bool {
        if value["kind"].as_str() == Some("boundValue") && value["id"].as_str() == Some(id) {
            return true;
        }
        match value {
            Value::Array(values) => values.iter().any(|value| reads(value, id)),
            Value::Object(values) => values.values().any(|value| reads(value, id)),
            _ => false,
        }
    }
    ids.iter()
        .all(|id| effects.iter().any(|effect| reads(effect, id)))
}

/// Whether a lowered value reads the color chosen on resolution.
pub(in crate::oracle::canonical) fn references_resolution_color(value: &Value) -> bool {
    crate::oracle::contextual_binding::references_resolution_color(value)
}
