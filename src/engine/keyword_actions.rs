//! Keyword actions performed by a resolving keyword ability (event model,
//! docs/oracle-event-model.md).
//!
//! Some keywords name the action their ability performs: a creature with
//! exploit "exploits a creature" when the controller of its exploit ability
//! sacrifices a creature as that ability resolves (CR 702.110b). The keyword
//! row declares that policy (`namedAction` in `keyword-templates-v1.json`);
//! the flattened rule carries it in `keywordProvenance`. When such a rule
//! resolves, each qualifying action recorded during its resolution publishes
//! one record of the named lemma: the subject is the ability source (as it
//! existed, even if it sacrificed itself) and the object is the acted-on
//! permanent. Triggers read it with the generic `eventPattern` shape:
//!
//! ```json
//! {"kind": "eventPattern", "lemma": "exploit",
//!  "subject": {"object": {"kind": "self"}},
//!  "counterpart": {"where": FILTER}}
//! ```
use super::*;

/// The policy of one keyword's named action, read from the rule provenance.
struct NamedActionPolicy<'a> {
    lemma: &'a str,
    /// The engine event that performs the action (`permanentSacrificed`).
    performed_by: &'a str,
}

fn named_action_policy(rule: &Value) -> Option<NamedActionPolicy<'_>> {
    let action = &rule["keywordProvenance"]["namedAction"];
    Some(NamedActionPolicy {
        lemma: action["lemma"].as_str()?,
        performed_by: action["performedBy"].as_str()?,
    })
}

/// True when a rule's keyword names the action its ability performs.
pub(super) fn rule_names_keyword_action(rule: &Value) -> bool {
    named_action_policy(rule).is_some()
}

/// The engine events that can perform a named keyword action.
fn performing_event_kind(performed_by: &str) -> Option<&'static str> {
    match performed_by {
        "sacrifice" => Some("permanentSacrificed"),
        _ => None,
    }
}

/// A trigger event of the named-action family: `Some(supported)`.
pub(super) fn keyword_action_event_supported(event: &Value) -> Option<bool> {
    if value_kind(event) != Some("eventPattern")
        || !matches!(event["lemma"].as_str(), Some("exploit"))
    {
        return None;
    }
    let fields_known = event.as_object().is_some_and(|fields| {
        fields
            .keys()
            .all(|key| ["kind", "lemma", "subject", "counterpart"].contains(&key.as_str()))
    });
    let subject_supported = event["subject"]
        .as_object()
        .is_some_and(|subject| subject.len() == 1)
        && (value_kind(&event["subject"]["object"]) == Some("self")
            || acting_permanents_supported(&event["subject"]["object"]));
    let counterpart_supported = event.get("counterpart").is_none_or(|counterpart| {
        counterpart
            .as_object()
            .is_some_and(|fields| fields.keys().all(|key| key == "where"))
            && counterpart_filter_supported(&counterpart["where"])
    });
    Some(fields_known && subject_supported && counterpart_supported)
}

/// `a creature you control exploits ...`: the acting permanent of another
/// object's keyword action, with an optional controller relation to the
/// observer, a printed-characteristics predicate and the observer excluded.
fn acting_permanents_supported(object: &Value) -> bool {
    value_kind(object) == Some("permanents")
        && object.as_object().is_some_and(|fields| {
            fields
                .keys()
                .all(|key| ["kind", "controller", "where", "excludeSource"].contains(&key.as_str()))
        })
        && object
            .get("controller")
            .is_none_or(player_action_events::event_player_reference_supported)
        && object.get("where").is_none_or(card_filter_supported)
        && object.get("excludeSource").is_none_or(Value::is_boolean)
}

/// The acted-on object is read from the performing event's record, which
/// carries whether it was a creature and a token when it was sacrificed.
fn counterpart_filter_supported(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("cardTypeContains") => filter["value"].as_str() == Some("Creature"),
        Some("isToken") => true,
        Some("not") => counterpart_filter_supported(&filter["operand"]),
        Some("and") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().all(counterpart_filter_supported)),
        _ => false,
    }
}

fn counterpart_matches(filter: &Value, detail: &Value) -> bool {
    match value_kind(filter) {
        None if filter.is_null() => true,
        Some("cardTypeContains") => detail["wasCreature"].as_bool() == Some(true),
        Some("isToken") => detail["wasToken"].as_bool() == Some(true),
        Some("not") => !counterpart_matches(&filter["operand"], detail),
        Some("and") => filter["operands"].as_array().is_some_and(|operands| {
            operands
                .iter()
                .all(|operand| counterpart_matches(operand, detail))
        }),
        _ => false,
    }
}

impl GameEngine {
    /// Battlefield observers of a keyword action performed by another
    /// permanent (`a creature you control exploits a creature`): the acting
    /// permanent `actor` (as it existed when its ability resolved) satisfies
    /// the subject's controller relation and predicate, and the acted-on
    /// object its counterpart filter. Each triggers with the acted-on
    /// permanent bound. False when a quantity failure stops the publication.
    fn publish_observed_keyword_action(
        &mut self,
        lemma: &str,
        actor: &CardInstance,
        object_id: &str,
        detail: &Value,
    ) -> bool {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|observer| {
                self.effective_rules(observer)
                    .iter()
                    .filter(|rule| triggered_rule_supported(rule))
                    .filter(|rule| {
                        triggered_event_variants(&rule["event"])
                            .iter()
                            .any(|event| {
                                let subject = &event["subject"]["object"];
                                value_kind(event) == Some("eventPattern")
                                    && event["lemma"].as_str() == Some(lemma)
                                    && value_kind(subject) == Some("permanents")
                                    && !(subject["excludeSource"].as_bool() == Some(true)
                                        && observer.instance_id == actor.instance_id)
                                    && subject.get("controller").is_none_or(|controller| {
                                        self.event_player_reference_matches(
                                            controller,
                                            observer,
                                            &actor.controller,
                                        )
                                    })
                                    && subject.get("where").is_none_or(|filter| {
                                        matches_card_filter(&actor.definition, filter)
                                    })
                                    && counterpart_matches(&event["counterpart"]["where"], detail)
                            })
                    })
                    .cloned()
                    .map(|rule| (observer.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (observer, rule) in triggers {
            self.put_triggered_ability_on_stack_with_decisions(
                &observer,
                rule,
                BTreeMap::from([(
                    "triggeringPermanentId".to_string(),
                    Value::String(object_id.to_string()),
                )]),
            );
            if self.check_quantity_failure().is_err() {
                return false;
            }
        }
        true
    }

    /// After `resolved` resolved, publishes its keyword's named action for
    /// each qualifying action its controller performed during resolution
    /// (events recorded from `first_event`).
    pub(super) fn publish_keyword_named_actions(
        &mut self,
        resolved: &StackObject,
        first_event: usize,
    ) {
        let Some(rule) = resolved.ability_rule.as_ref() else {
            return;
        };
        let Some(policy) = named_action_policy(rule) else {
            return;
        };
        let Some(performing) = performing_event_kind(policy.performed_by) else {
            return;
        };
        let lemma = policy.lemma.to_string();
        let acted_on = self
            .state
            .events
            .iter()
            .skip(first_event)
            .filter(|event| {
                event.kind == performing
                    && event.player_id.as_deref() == Some(resolved.controller.as_str())
            })
            .filter_map(|event| Some((event.card_instance_id.clone()?, event.detail.clone())))
            .collect::<Vec<_>>();
        if acted_on.is_empty() {
            return;
        }
        // The subject's own abilities: the live permanent when it is still
        // the same object, else the object as it was when the ability resolved.
        let source = self
            .permanent_position(&resolved.card.instance_id)
            .map(|(player, index)| self.state.players[player].battlefield[index].clone())
            .filter(|live| {
                Self::tracked_zone_identity(live) == Self::tracked_zone_identity(&resolved.card)
            })
            .unwrap_or_else(|| resolved.card.clone());
        for (object_id, detail) in acted_on {
            let triggers = self
                .effective_rules(&source)
                .iter()
                .filter(|rule| triggered_rule_supported(rule))
                .filter(|rule| {
                    triggered_event_variants(&rule["event"])
                        .iter()
                        .any(|event| {
                            value_kind(event) == Some("eventPattern")
                                && event["lemma"].as_str() == Some(lemma.as_str())
                                && value_kind(&event["subject"]["object"]) == Some("self")
                                && counterpart_matches(&event["counterpart"]["where"], &detail)
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            for rule in triggers {
                self.put_triggered_ability_on_stack_with_decisions(
                    &source,
                    rule,
                    BTreeMap::from([(
                        "triggeringPermanentId".to_string(),
                        Value::String(object_id.clone()),
                    )]),
                );
                if self.check_quantity_failure().is_err() {
                    return;
                }
            }
            if !self.publish_observed_keyword_action(&lemma, &source, &object_id, &detail) {
                return;
            }
            self.record_event(
                "keywordActionPerformed",
                Some(resolved.controller.clone()),
                Some(source.instance_id.clone()),
                json!({ "lemma": lemma, "object": object_id }),
            );
        }
    }
}
