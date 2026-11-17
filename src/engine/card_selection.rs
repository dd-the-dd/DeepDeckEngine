//! Ordinary collection selection: query, predicate and cardinality are parameters.
//! Cross-selection historical flags/candidate-zone variants retain their caller.
use super::*;

pub(super) fn ordinary_bound_selection(effect: &Value) -> bool {
    value_kind(&effect["candidates"]) != Some("cards")
        && !effect["sameLandType"].as_bool().unwrap_or(false)
        && !effect["onePerControlledColor"].as_bool().unwrap_or(false)
        && value_kind(&effect["selection"]) != Some("random")
}

impl GameEngine {
    pub(super) fn execute_bound_card_selection<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        next_effect: Option<&Value>,
        source: &StackObject,
        bindings: &BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let id = effect["id"]
            .as_str()
            .ok_or_else(|| EngineError::new("chooseCards requires a decision identifier"))?;
        let player = self
            .resolve_player(&effect["player"], source, bindings)
            .ok_or_else(|| EngineError::new("unresolved chooseCards player reference"))?;
        self.player_index(&player)?;
        // Pure independent operands are prepared even when the source is empty.
        let predicate = effect
            .get("where")
            .map(|predicate| {
                self.prepare_runtime_object_filter(predicate, source, bindings, decisions)
            })
            .transpose()?;
        let mut objects =
            self.resolve_runtime_object_ids_checked(&effect["from"], source, bindings, decisions)?;
        if let Some(predicate) = predicate {
            objects.retain(|id| {
                self.runtime_collection_object_matches(id, &predicate, source, bindings, decisions)
            });
        }
        if objects.iter().collect::<BTreeSet<_>>().len() != objects.len() {
            return Err(EngineError::new(
                "chooseCards collection contains duplicate object identities",
            ));
        }
        let quantity = |expression: &Value, field: &'static str| {
            self.checked_runtime_numeric_expression(expression, source, bindings, decisions, field)
                .map(|n| n.max(0) as usize)
        };
        let (minimum, maximum) =
            if effect.get("minimum").is_some() || effect.get("maximum").is_some() {
                (
                    quantity(&effect["minimum"], "chooseCards.minimum")?,
                    quantity(&effect["maximum"], "chooseCards.maximum")?,
                )
            } else {
                let count = quantity(&effect["count"], "chooseCards.count")?;
                (count, count)
            };
        if minimum > maximum {
            return Err(EngineError::new("chooseCards minimum exceeds maximum"));
        }
        let maximum = maximum.min(objects.len());
        let minimum = minimum.min(maximum);
        // These are bounded fallback strategy previews, not the legal domain.
        // With no cross-selection constraints their length is1 iff the full
        // domain is singleton. Direct structured responses may select ANY
        // subset satisfying the full candidate IDs/cardinality contract.
        let mut previews = Vec::new();
        for count in minimum..=maximum {
            previews.extend(Self::resolution_selections(
                &objects,
                count,
                128 - previews.len(),
            ));
            if previews.len() >= 128 {
                break;
            }
        }
        self.make_resolution_decision(
            id,
            &player,
            source,
            DecisionChoice::CardSelection {
                decision_id: id.to_owned(),
                candidate_card_instance_ids: objects,
                minimum,
                maximum,
                prompt: Self::resolution_choice_prompt(id, next_effect, false),
            },
            previews,
            decisions,
            provider,
        )
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "card_selection_tests.rs"]
mod tests;
