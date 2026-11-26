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
//!   grants (`grantTriggeredAbility`, `grantKeywordAbility`,
//!   `grantActivatedAbility` and the zone-linked activated grants). Each
//!   granted triggered or keyword rule carries its grantor
//!   (`grantedByInstanceId`): two grantors give two instances (CR 113.2c).
//!
//! A granted static ability (`grantStaticAbility`, CR 613.1f) contributes its
//! modifiers to the static-modifier collection with the grantee as their
//! source, so `this creature` inside it is the grantee.
//!
//! The static-modifier collector reads the printed layer filtered by
//! `rule_active_for_source`: grants are themselves static modifiers, so the
//! grant layer is computed from the printed layer and never from itself.

use super::*;

/// Modifier kinds whose `ability` field is one complete granted rule.
const RULE_GRANT_KINDS: [&str; 5] = [
    "grantTriggeredAbility",
    "grantKeywordAbility",
    "grantActivatedAbility",
    "grantActivatedAbilitiesFromLinkedExile",
    "grantActivatedAbilitiesFromZone",
];

/// Granted rule kinds merged into the effective layer as complete rules.
const MERGED_RULE_GRANT_KINDS: [&str; 2] = ["grantTriggeredAbility", "grantKeywordAbility"];

/// Modifiers a granted static ability may not carry: rule grants and ability
/// loss (layer 6 is computed from the printed layer, never from itself), and
/// type, subtype, control or mana-production changes, whose ordered
/// projections read the printed static layer only (`static_modifier_site.rs`).
const GRANTED_STATIC_EXCLUDED_MODIFIERS: [&str; 21] = [
    "grantTriggeredAbility",
    "grantKeywordAbility",
    "grantStaticAbility",
    "grantActivatedAbility",
    "grantActivatedAbilitiesFromLinkedExile",
    "grantActivatedAbilitiesFromZone",
    "grantAbility",
    "grantManaAbility",
    "loseAllAbilities",
    "addCardType",
    "removeCardTypeWhile",
    "addSubtype",
    "setSubtypes",
    "setLandSubtype",
    "setCardTypes",
    "modifySubtypeSet",
    "nonbasicLandsBecomeMountains",
    "addAllBasicLandTypes",
    "multiplyManaProduction",
    "dampingSphereMana",
    "controlAttachedPermanent",
];

/// Validation of the layer-6 rule grants this module owns. `None` for any
/// other modifier kind.
pub(super) fn granted_rule_modifier_supported(modifier: &Value) -> Option<bool> {
    let condition_supported_or_absent = modifier.get("condition").is_none_or(condition_supported);
    match value_kind(modifier) {
        Some("grantKeywordAbility") => Some(
            static_object_selector_supported(&modifier["objects"])
                && keyword_rule_supported(&modifier["ability"])
                && condition_supported_or_absent,
        ),
        Some("grantStaticAbility") => Some(
            static_object_selector_supported(&modifier["objects"])
                && granted_static_rule_supported(&modifier["ability"])
                && condition_supported_or_absent,
        ),
        _ => None,
    }
}

/// A static ability another permanent can have: a supported battlefield
/// static rule carrying none of the excluded modifiers.
fn granted_static_rule_supported(rule: &Value) -> bool {
    value_kind(rule) == Some("staticAbility")
        && battlefield_static_rule_supported(rule)
        && rule["modifiers"].as_array().is_some_and(|modifiers| {
            modifiers.iter().all(|modifier| {
                value_kind(modifier)
                    .is_none_or(|kind| !GRANTED_STATIC_EXCLUDED_MODIFIERS.contains(&kind))
            })
        })
}

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
    /// CR 305.7: an effect set this land's subtype to basic land types, so
    /// the abilities of its rules text are gone. What resolved effects
    /// recorded on the object (characteristic markers, until-end-of-turn
    /// grants) is not rules text and stays.
    rules_text_lost: bool,
    /// A permanent's printed triggered rules that function only from
    /// another zone (CR 113.6: `triggerZone` graveyard, hand, exile) do not
    /// function while it is on the battlefield.
    on_battlefield: bool,
    granted: Vec<Value>,
}

/// Whether a printed-layer entry survives the loss of the rules text: only
/// what a resolved effect recorded on the object.
fn survives_rules_text_loss(rules_text_lost: bool, rule: &Value) -> bool {
    !rules_text_lost || GameEngine::is_resolved_characteristic_marker(rule)
}

/// Whether a printed rule functions where its object is, as far as this
/// layer knows it: on the battlefield, a triggered rule whose zone of
/// function is another zone does not (CR 113.6). Off the battlefield the
/// observer that reads the rules checks the object's zone.
fn functions_in_known_zone(on_battlefield: bool, rule: &Value) -> bool {
    !on_battlefield || !zone_of_function::functions_only_off_battlefield(rule)
}

impl<'a> EffectiveRules<'a> {
    /// Printed rules that currently function, with their printed index. The
    /// index addresses `printed_rules()`, so activation and trigger records
    /// that store a rule index stay valid for the printed layer.
    pub(super) fn printed(&self) -> impl Iterator<Item = (usize, &'a Value)> + use<'a> {
        let object = self.object;
        let functioning = self.functioning;
        let rules_text_lost = self.rules_text_lost;
        let on_battlefield = self.on_battlefield;
        object
            .definition
            .rules
            .iter()
            .enumerate()
            .filter(move |(_, rule)| {
                functioning
                    && survives_rules_text_loss(rules_text_lost, rule)
                    && functions_in_known_zone(on_battlefield, rule)
                    && GameEngine::rule_metadata_active_for_source(object, rule)
            })
    }

    /// Every rule the object currently has: functioning printed rules, then
    /// granted rules in static-modifier order.
    pub(super) fn iter(&self) -> impl Iterator<Item = &Value> + '_ {
        let object: &CardInstance = self.object;
        let printed: &[Value] = &object.definition.rules;
        let functioning = self.functioning;
        let rules_text_lost = self.rules_text_lost;
        let on_battlefield = self.on_battlefield;
        printed
            .iter()
            .filter(move |rule| {
                functioning
                    && survives_rules_text_loss(rules_text_lost, rule)
                    && functions_in_known_zone(on_battlefield, rule)
                    && GameEngine::rule_metadata_active_for_source(object, rule)
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
        // An object that prints no rule and receives no grant has no rules,
        // whether or not it functions: skip the battlefield-wide queries.
        if object.definition.rules.is_empty()
            && (self.collecting_rule_grants.get() || !self.any_rule_grant_printed())
        {
            return EffectiveRules {
                object,
                functioning: true,
                rules_text_lost: false,
                on_battlefield: false,
                granted: Vec::new(),
            };
        }
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
        let rules_text_lost = on_battlefield
            && (subtypes::land_rules_text_replaced(&object.definition)
                || self.static_land_subtype_set_applies(object, None));
        EffectiveRules {
            object,
            functioning,
            rules_text_lost,
            on_battlefield,
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
        let rules_text_lost = on_battlefield
            && (subtypes::land_rules_text_replaced(&object.definition)
                || self.static_land_subtype_set_applies(object, Some(modifiers)));
        EffectiveRules {
            object,
            functioning,
            rules_text_lost,
            on_battlefield,
            granted,
        }
    }

    /// CR 305.7: a static ability that sets a land's subtype to a basic land
    /// type (`setLandSubtype`) removes the abilities of its rules text, as a
    /// recorded subtype-set marker does; the intrinsic mana ability of the
    /// new type is read by mana production. A cheap printed scan avoids the
    /// static-modifier collection when no source prints such a change.
    fn static_land_subtype_set_applies(
        &self,
        object: &CardInstance,
        modifiers: Option<&[(CardInstance, Value)]>,
    ) -> bool {
        let printed = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| source.definition.rules.iter())
            .filter_map(|rule| rule["modifiers"].as_array())
            .flatten()
            .any(|modifier| value_kind(modifier) == Some("setLandSubtype"));
        // Like the grant layer, this read is computed from the printed
        // layer: a nested read while it is collected sees no such loss.
        if !printed || self.collecting_rule_grants.get() {
            return false;
        }
        let applies = |source: &CardInstance, modifier: &Value| {
            value_kind(modifier) == Some("setLandSubtype")
                && Self::static_selector_matches(source, &modifier["objects"], object)
        };
        match modifiers {
            Some(modifiers) => modifiers
                .iter()
                .any(|(source, modifier)| applies(source, modifier)),
            None => self.with_rule_grant_collection(|engine| {
                engine
                    .battlefield_static_modifiers()
                    .iter()
                    .any(|(source, modifier)| applies(source, modifier))
            }),
        }
    }

    /// Layer 6 is computed from the printed layer: a static condition or
    /// selector evaluated while grants are collected sees no grants, so the
    /// grant layer never depends on itself.
    pub(super) fn with_rule_grant_collection<T>(&self, work: impl FnOnce(&Self) -> T) -> T {
        self.collecting_rule_grants.set(true);
        let result = work(self);
        self.collecting_rule_grants.set(false);
        result
    }

    /// Ability loss is a continuous effect on a permanent (CR 613.1f). An
    /// object in another zone is a new object (CR 400.7) whose printed
    /// abilities function from their own zone of function; departure
    /// observers capture the permanent's effective rules before it leaves.
    fn object_loses_all_abilities(&self, object: &CardInstance, on_battlefield: bool) -> bool {
        on_battlefield && self.permanent_loses_all_abilities(object)
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
        let mut granted = Vec::new();
        for (source, modifier) in modifiers.iter().filter(|(source, modifier)| {
            value_kind(modifier).is_some_and(|kind| MERGED_RULE_GRANT_KINDS.contains(&kind))
                && Self::static_selector_matches(source, &modifier["objects"], object)
                && modifier.get("condition").is_none_or(|condition| {
                    self.condition_value_with_source(
                        condition,
                        &source.controller,
                        &BTreeMap::new(),
                        Some(source),
                    )
                })
        }) {
            // Each grantor gives its own instance of the rule (CR 113.2c).
            let mut rule = modifier["ability"].clone();
            rule["grantedByInstanceId"] = Value::String(source.instance_id.clone());
            if !granted.contains(&rule) {
                granted.push(rule);
            }
        }
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
                self.cards_exiled_with(source)
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
        // A level, station or Case band (S6) gates its rule on the source.
        if !band_gate::band_gate_open(source, rule) {
            return false;
        }
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

    /// The same metadata read for an object that is not a permanent. It has
    /// no counters (CR 122.2), so level and station bands with a positive
    /// threshold are closed, and a Class is at its first level (CR 716.2a).
    /// Room doors are battlefield designations (CR 709.5): off the
    /// battlefield the card has the characteristics of both halves.
    pub(super) fn rule_metadata_active_off_battlefield(rule: &Value) -> bool {
        band_gate::band_gate_open_for_counters(&BTreeMap::new(), rule)
            && rule["minimumClassLevel"]
                .as_i64()
                .is_none_or(|minimum| minimum <= 1)
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

    /// The active static modifiers plus those of static abilities granted by
    /// active `grantStaticAbility` modifiers (CR 613.1f). A granted
    /// modifier's source is the grantee, so its selectors and conditions read
    /// `self` as the permanent that has the ability. Grants are computed from
    /// the printed layer: while they are collected, a nested read sees no
    /// granted static ability, so the layer never depends on itself.
    pub(super) fn with_granted_static_modifiers(
        &self,
        mut modifiers: Vec<(CardInstance, Value)>,
    ) -> Vec<(CardInstance, Value)> {
        if self.collecting_rule_grants.get()
            || !modifiers
                .iter()
                .any(|(_, modifier)| value_kind(modifier) == Some("grantStaticAbility"))
        {
            return modifiers;
        }
        let granted = self.with_rule_grant_collection(|engine| {
            let mut granted = Vec::new();
            for (grantor, modifier) in modifiers
                .iter()
                .filter(|(_, modifier)| value_kind(modifier) == Some("grantStaticAbility"))
            {
                let rule = &modifier["ability"];
                if !granted_static_rule_supported(rule)
                    || modifier.get("condition").is_some_and(|condition| {
                        !engine.condition_value_with_source(
                            condition,
                            &grantor.controller,
                            &BTreeMap::new(),
                            Some(grantor),
                        )
                    })
                {
                    continue;
                }
                for grantee in engine
                    .state
                    .players
                    .iter()
                    .flat_map(|player| player.battlefield.iter())
                    .filter(|grantee| !grantee.flags.get("phasedOut").copied().unwrap_or(false))
                    .filter(|grantee| {
                        Self::static_selector_matches(grantor, &modifier["objects"], grantee)
                    })
                {
                    if engine.permanent_loses_all_abilities(grantee)
                        || rule.get("condition").is_some_and(|condition| {
                            !engine.condition_value_with_source(
                                condition,
                                &grantee.controller,
                                &BTreeMap::new(),
                                Some(grantee),
                            )
                        })
                    {
                        continue;
                    }
                    for inner in rule["modifiers"].as_array().into_iter().flatten() {
                        granted.push((grantee.clone(), inner.clone()));
                    }
                }
            }
            granted
        });
        modifiers.extend(granted);
        modifiers
    }

    /// Keyword abilities of the rules granted to a permanent by static
    /// `grantKeywordAbility` modifiers (`has toxic 2`, `has annihilator 1`).
    fn granted_keyword_abilities(&self, permanent: &CardInstance) -> Vec<Value> {
        // Cheap printed scan first: keyword reads are frequent.
        let any_keyword_grant = self
            .state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| source.definition.rules.iter())
            .filter_map(|rule| rule["modifiers"].as_array())
            .flatten()
            .any(|modifier| value_kind(modifier) == Some("grantKeywordAbility"));
        if !any_keyword_grant {
            return Vec::new();
        }
        self.effective_rules(permanent)
            .granted
            .iter()
            .flat_map(|rule| match value_kind(rule) {
                Some("keywordAbility") => vec![rule["ability"].clone()],
                Some("keywordAbilityGroup") => {
                    rule["abilities"].as_array().cloned().unwrap_or_default()
                }
                _ => Vec::new(),
            })
            .collect()
    }

    /// The summed parameter (`count`, `quantity`) of a keyword granted by a
    /// static rule grant; printed and resolved grants are counted by the
    /// printed reader.
    pub(super) fn granted_keyword_parameter_total(
        &self,
        permanent: &CardInstance,
        keyword: &str,
        field: &str,
    ) -> i32 {
        self.granted_keyword_abilities(permanent)
            .iter()
            .filter(|ability| value_kind(ability) == Some(keyword))
            .filter_map(|ability| integer_value(&ability[field]))
            .fold(0i32, i32::saturating_add)
    }

    /// Whether a static rule grant gives the permanent this keyword.
    pub(super) fn has_granted_keyword_rule(&self, permanent: &CardInstance, keyword: &str) -> bool {
        self.granted_keyword_abilities(permanent)
            .iter()
            .any(|ability| value_kind(ability) == Some(keyword))
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "effective_rules_tests.rs"]
mod tests;
