//! The one accessor for the rules an object has.
//!
//! Two layers are distinguished, and every engine read of a definition's
//! rules goes through exactly one of them:
//!
//! * the printed layer (`CardDefinition::printed_rules`) is the copiable rule
//!   text a definition carries before any continuous effect applies (CR 707.2).
//!   Face switching, copies, tokens, declarations of a card's own spell
//!   abilities, and cards that are not on the battlefield read this layer;
//! * the effective layer (`GameEngine::effective_rules`) is what a permanent
//!   currently has after layer 6 (CR 613.1f): printed rules whose metadata
//!   gate is open (class level CR 716, Room doors CR 709.5), removed when the
//!   permanent loses all abilities, plus complete rules granted by static
//!   grants (`grantTriggeredAbility`, `grantActivatedAbility` and the
//!   zone-linked activated grants).
//!
//! The static-modifier collector reads the printed layer filtered by
//! `rule_active_for_source`: grants are themselves static modifiers, so the
//! grant layer is computed from the printed layer and never from itself.

use super::*;

/// Modifier kinds whose `ability` field is one complete granted rule.
const RULE_GRANT_KINDS: [&str; 4] = [
    "grantTriggeredAbility",
    "grantActivatedAbility",
    "grantActivatedAbilitiesFromLinkedExile",
    "grantActivatedAbilitiesFromZone",
];

impl CardDefinition {
    /// Printed (copiable) rules of this definition, before continuous effects.
    pub(super) fn printed_rules(&self) -> &[Value] {
        &self.rules
    }

    /// Mutable printed layer. Only definition construction (faces, copies,
    /// splice, entering replacements recorded on the object) writes it.
    pub(super) fn printed_rules_mut(&mut self) -> &mut Vec<Value> {
        &mut self.rules
    }

    /// Replace the printed layer, as a face switch or declaration probe does.
    pub(super) fn set_printed_rules(&mut self, rules: Vec<Value>) {
        self.rules = rules;
    }

    /// Move the printed layer out of an owned definition.
    pub(super) fn into_printed_rules(self) -> Vec<Value> {
        self.rules
    }
}

/// Rules an object currently has. It borrows only the object, never the
/// engine, so a caller may mutate the engine while iterating it.
pub(super) struct EffectiveRules<'a> {
    object: &'a CardInstance,
    functioning: bool,
    granted: Vec<Value>,
}

impl<'a> EffectiveRules<'a> {
    /// Printed rules that currently function, with their printed index. The
    /// index addresses `printed_rules()`, so activation and trigger records
    /// that store a rule index stay valid for the printed layer.
    pub(super) fn printed(&self) -> impl Iterator<Item = (usize, &'a Value)> + use<'a> {
        let object = self.object;
        let functioning = self.functioning;
        object
            .definition
            .rules
            .iter()
            .enumerate()
            .filter(move |(_, rule)| {
                functioning && GameEngine::rule_metadata_active_for_source(object, rule)
            })
    }

    /// Every rule the object currently has: functioning printed rules, then
    /// granted rules in static-modifier order.
    pub(super) fn iter(&self) -> impl Iterator<Item = &Value> + '_ {
        let object: &CardInstance = self.object;
        let printed: &[Value] = &object.definition.rules;
        let functioning = self.functioning;
        printed
            .iter()
            .filter(move |rule| {
                functioning && GameEngine::rule_metadata_active_for_source(object, rule)
            })
            .chain(self.granted.iter())
    }
}

fn modifier_grants_rule(modifier: &Value) -> bool {
    value_kind(modifier).is_some_and(|kind| RULE_GRANT_KINDS.iter().any(|grant| *grant == kind))
}

fn rule_declares_ability_loss(rule: &Value) -> bool {
    rule["modifiers"].as_array().is_some_and(|modifiers| {
        modifiers
            .iter()
            .any(|modifier| value_kind(modifier) == Some("loseAllAbilities"))
    })
}

impl GameEngine {
    /// Rules the object currently has (see module documentation). Ability
    /// loss and grants apply to permanents only (CR 613.1f); an object in
    /// another zone keeps its printed rules, whose own zone of function is
    /// checked by the observer that reads them (CR 113.6).
    pub(super) fn effective_rules<'a>(&self, object: &'a CardInstance) -> EffectiveRules<'a> {
        let on_battlefield = self.permanent_position(&object.instance_id).is_some();
        let functioning = !self.object_loses_all_abilities(object, on_battlefield);
        let granted = if functioning
            && on_battlefield
            && !self.collecting_rule_grants.get()
            && self.any_rule_grant_printed()
        {
            self.with_rule_grant_collection(|engine| {
                engine.granted_rules_from(object, &engine.battlefield_static_modifiers())
            })
        } else {
            Vec::new()
        };
        EffectiveRules {
            object,
            functioning,
            granted,
        }
    }

    /// The same layer for an observer pass that already collected the active
    /// static modifiers once (departure and death snapshots, step triggers).
    pub(super) fn effective_rules_with_modifiers<'a>(
        &self,
        object: &'a CardInstance,
        modifiers: &[(CardInstance, Value)],
    ) -> EffectiveRules<'a> {
        let on_battlefield = self.permanent_position(&object.instance_id).is_some();
        let functioning = !self.object_loses_all_abilities(object, on_battlefield);
        let granted = if functioning && on_battlefield && !self.collecting_rule_grants.get() {
            self.with_rule_grant_collection(|engine| engine.granted_rules_from(object, modifiers))
        } else {
            Vec::new()
        };
        EffectiveRules {
            object,
            functioning,
            granted,
        }
    }

    /// Layer 6 is computed from the printed layer: a static condition or
    /// selector evaluated while grants are collected sees no grants, so the
    /// grant layer never depends on itself.
    fn with_rule_grant_collection<T>(&self, work: impl FnOnce(&Self) -> T) -> T {
        self.collecting_rule_grants.set(true);
        let result = work(self);
        self.collecting_rule_grants.set(false);
        result
    }

    /// Instance-specific ability loss (an effect that named this object)
    /// follows the object anywhere; static ability loss applies only to a
    /// permanent.
    fn object_loses_all_abilities(&self, object: &CardInstance, on_battlefield: bool) -> bool {
        if on_battlefield {
            return self.permanent_loses_all_abilities(object);
        }
        self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("loseAllAbilities")
                && modifier["cardInstanceId"].as_str() == Some(object.instance_id.as_str())
                && modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_none_or(|turn| turn >= u64::from(self.state.turn_number))
        })
    }

    /// Cheap printed scan: whether any permanent prints a rule grant. It
    /// avoids evaluating every static condition when no grant exists.
    fn any_rule_grant_printed(&self) -> bool {
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| source.definition.rules.iter())
            .filter_map(|rule| rule["modifiers"].as_array())
            .flatten()
            .any(modifier_grants_rule)
    }

    /// Complete rules granted by the given active static modifiers.
    fn granted_rules_from(
        &self,
        object: &CardInstance,
        modifiers: &[(CardInstance, Value)],
    ) -> Vec<Value> {
        let mut granted = modifiers
            .iter()
            .filter(|(source, modifier)| {
                value_kind(modifier) == Some("grantTriggeredAbility")
                    && Self::static_selector_matches(source, &modifier["objects"], object)
            })
            .map(|(_, modifier)| modifier["ability"].clone())
            .collect::<Vec<_>>();
        for rule in self.granted_activated_abilities_from(object, modifiers) {
            if !granted.contains(&rule) {
                granted.push(rule);
            }
        }
        granted
    }

    pub(super) fn granted_activated_abilities(&self, permanent: &CardInstance) -> Vec<Value> {
        self.granted_activated_abilities_from(permanent, &self.battlefield_static_modifiers())
    }

    /// Activated abilities granted to `permanent` by the given active static
    /// modifiers, each tagged with its grantor.
    pub(super) fn granted_activated_abilities_from(
        &self,
        permanent: &CardInstance,
        modifiers: &[(CardInstance, Value)],
    ) -> Vec<Value> {
        let mut abilities = Vec::new();
        for (source, modifier) in modifiers.iter().filter(|(source, modifier)| {
            value_kind(modifier) == Some("grantActivatedAbility")
                && Self::static_selector_matches(source, &modifier["objects"], permanent)
        }) {
            let mut ability = modifier["ability"].clone();
            ability["grantedByInstanceId"] = Value::String(source.instance_id.clone());
            if !abilities.contains(&ability) {
                abilities.push(ability);
            }
        }
        for (source, modifier) in modifiers
            .iter()
            .filter(|(_, modifier)| {
                matches!(
                    value_kind(modifier),
                    Some(
                        "grantActivatedAbilitiesFromLinkedExile"
                            | "grantActivatedAbilitiesFromZone"
                    )
                )
            })
            .filter(|(source, modifier)| {
                Self::static_selector_matches(source, &modifier["objects"], permanent)
            })
        {
            let cards = if value_kind(modifier) == Some("grantActivatedAbilitiesFromLinkedExile") {
                let linked_card_ids = self
                    .state
                    .rule_modifiers
                    .iter()
                    .filter(|link| {
                        value_kind(link) == Some("exiledWithSource")
                            && link["sourceCardInstanceId"].as_str()
                                == Some(source.instance_id.as_str())
                    })
                    .filter_map(|link| link["cardInstanceId"].as_str())
                    .collect::<BTreeSet<_>>();
                self.state
                    .players
                    .iter()
                    .flat_map(|player| player.exile.iter())
                    .filter(|card| linked_card_ids.contains(card.instance_id.as_str()))
                    .collect::<Vec<_>>()
            } else {
                let player_id = match value_kind(&modifier["cards"]["zone"]["player"]) {
                    Some("controllerOf") => source.controller.as_str(),
                    _ => continue,
                };
                let Ok(player_index) = self.player_index(player_id) else {
                    continue;
                };
                match value_kind(&modifier["cards"]["zone"]) {
                    Some("graveyard") => {
                        self.state.players[player_index].graveyard.iter().collect()
                    }
                    Some("exile") => self.state.players[player_index].exile.iter().collect(),
                    _ => Vec::new(),
                }
            };
            for card in cards
                .into_iter()
                .filter(|card| matches_card_filter(&card.definition, &modifier["cards"]["where"]))
            {
                for rule in card
                    .definition
                    .printed_rules()
                    .iter()
                    .filter(|rule| activated_rule_supported(rule))
                {
                    if !abilities.contains(rule) {
                        abilities.push(rule.clone());
                    }
                }
            }
        }
        abilities
    }

    pub(super) fn rule_metadata_active_for_source(source: &CardInstance, rule: &Value) -> bool {
        if rule["minimumClassLevel"].as_i64().is_some_and(|minimum| {
            i64::from(source.counters.get("classLevel").copied().unwrap_or(1)) < minimum
        }) {
            return false;
        }
        let Some(door_index) = rule["roomDoorIndex"].as_u64() else {
            return true;
        };
        source
            .flags
            .get(&format!("roomDoor{door_index}Unlocked"))
            .copied()
            .unwrap_or(false)
    }

    /// Ability loss is computed from the printed layer only: a granted rule
    /// can never remove abilities, and reading the effective layer here would
    /// make the layer depend on itself.
    pub(super) fn permanent_loses_all_abilities(&self, permanent: &CardInstance) -> bool {
        if self.state.rule_modifiers.iter().any(|modifier| {
            (value_kind(modifier) == Some("loseAllAbilities")
                && modifier["cardInstanceId"].as_str() == Some(permanent.instance_id.as_str())
                && modifier["expiresAfterTurn"]
                    .as_u64()
                    .is_none_or(|turn| turn >= u64::from(self.state.turn_number)))
                || (value_kind(modifier) == Some("linkedAbilityLoss")
                    && modifier["targetCardInstanceId"].as_str()
                        == Some(permanent.instance_id.as_str())
                    && modifier["sourceCardInstanceId"]
                        .as_str()
                        .and_then(|source_id| self.permanent_position(source_id))
                        .is_some())
        }) {
            return true;
        }

        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .filter(|source| !source.flags.get("phasedOut").copied().unwrap_or(false))
            .any(|source| {
                source
                    .definition
                    .rules
                    .iter()
                    .filter(|rule| {
                        rule_declares_ability_loss(rule)
                            && battlefield_static_rule_supported(rule)
                            && Self::rule_metadata_active_for_source(source, rule)
                    })
                    .flat_map(|rule| rule["modifiers"].as_array().into_iter().flatten())
                    .any(|modifier| {
                        value_kind(modifier) == Some("loseAllAbilities")
                            && Self::static_selector_matches(
                                source,
                                &modifier["objects"],
                                permanent,
                            )
                    })
            })
    }

    pub(super) fn rule_active_for_source(&self, source: &CardInstance, rule: &Value) -> bool {
        Self::rule_metadata_active_for_source(source, rule)
            && !self.permanent_loses_all_abilities(source)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "effective_rules_tests.rs"]
mod tests;
