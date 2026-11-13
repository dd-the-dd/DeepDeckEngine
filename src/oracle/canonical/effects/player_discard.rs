use super::super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerDiscardLoweringIds<'a> {
    pub target_player: &'a str,
    pub random_selection: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct BoundPlayerLowering<'a> {
    pub binding: PlayerBinding,
    pub target_id: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::oracle::canonical) struct LoweredPlayerDiscardFragment {
    pub decisions: Vec<Value>,
    pub effects: Vec<Value>,
    pub declared_player: Option<(PlayerBinding, String)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerDiscardLoweringError {
    InvalidDeclaredPlayerTarget,
    MissingBoundPlayer,
    BoundPlayerMismatch,
    UnsupportedNumericExpression,
}

/// Lowers the typed player-discard node exclusively to existing engine
/// primitives. Random discard is a generic random `chooseCards` over the
/// player's hand followed by a linked `discardCards` consumer.
pub(in crate::oracle::canonical) fn lower_player_discard_effect(
    node: &PlayerDiscardEffectNode,
    ids: PlayerDiscardLoweringIds<'_>,
    antecedent: Option<BoundPlayerLowering<'_>>,
) -> Result<LoweredPlayerDiscardFragment, PlayerDiscardLoweringError> {
    let mut decisions = Vec::new();
    let mut declared_player = None;
    let player = match &node.player {
        PlayerEffectSubject::Controller => controller(),
        PlayerEffectSubject::DeclaredTarget { target, binding } => {
            let [slot] = target.slots.as_slice() else {
                return Err(PlayerDiscardLoweringError::InvalidDeclaredPlayerTarget);
            };
            let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                .ok_or(PlayerDiscardLoweringError::InvalidDeclaredPlayerTarget)?;
            decisions.push(target_decision(ids.target_player, candidates, 1, 1));
            declared_player = Some((*binding, ids.target_player.to_string()));
            chosen_target(ids.target_player)
        }
        PlayerEffectSubject::Bound(binding) => {
            let antecedent = antecedent.ok_or(PlayerDiscardLoweringError::MissingBoundPlayer)?;
            if antecedent.binding != *binding {
                return Err(PlayerDiscardLoweringError::BoundPlayerMismatch);
            }
            chosen_target(antecedent.target_id)
        }
    };
    let count = lower_numeric_expression(&node.count)
        .ok_or(PlayerDiscardLoweringError::UnsupportedNumericExpression)?;

    let effects = match node.selection {
        CollectionSelectionMethod::Choice => vec![json!({
            "kind": "discardCards",
            "player": player,
            "count": count,
        })],
        CollectionSelectionMethod::Random => {
            let selection_id = ids.random_selection;
            vec![
                json!({
                    "kind": "chooseCards",
                    "id": selection_id,
                    "player": player.clone(),
                    "minimum": count.clone(),
                    "maximum": count,
                    "candidates": {
                        "kind": "cards",
                        "zone": hand(player.clone()),
                        "where": Value::Null,
                    },
                    "selection": { "kind": "random" },
                }),
                json!({
                    "kind": "discardCards",
                    "player": player,
                    "cards": decision_result(selection_id),
                    "selectionProvenance": {
                        "kind": "random",
                        "decisionId": selection_id,
                    },
                }),
            ]
        }
    };

    Ok(LoweredPlayerDiscardFragment {
        decisions,
        effects,
        declared_player,
    })
}

#[cfg(test)]
#[path = "player_discard_tests.rs"]
mod tests;
