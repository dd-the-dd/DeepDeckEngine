//! Mechanical domain adapter. Existing mutation/validation bodies are preserved.
//! `None`/`Ok(false)` means not handled; `Some(false)` means recognized but invalid.
//! A handled early no-op still returns `Ok(true)`. The root owns transactions.
use super::super::*;

pub(super) fn supports_effect(effect: &Value) -> Option<bool> {
    Some(supports_known_effect(effect))
}

fn supports_known_effect(effect: &Value) -> bool {
    match value_kind(effect) {
        Some("transformPermanent") => {
            matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || object_expression_supported(&effect["object"])
        }
        Some("becomeCreature") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && effect["addTypes"]
                    .as_array()
                    .is_some_and(|types| types.iter().all(Value::is_string))
                && effect["addSubtypes"]
                    .as_array()
                    .is_some_and(|types| types.iter().all(Value::is_string))
                && effect["addColors"]
                    .as_array()
                    .is_none_or(|colors| colors.iter().all(Value::is_string))
                && integer_value(&effect["basePower"]).is_some()
                && integer_value(&effect["baseToughness"]).is_some()
                && effect["retainExistingTypes"].as_bool() == Some(true)
                && effect.get("replaceSubtypes").is_none_or(Value::is_boolean)
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("permanent" | "untilEndOfCurrentTurn")
                )
        }
        Some("addSubtypeToPermanent") => {
            (matches!(
                value_kind(&effect["object"]),
                Some("self" | "abilitySource")
            ) || target_reference_supported(&effect["object"]))
                && effect["subtype"].is_string()
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("permanent" | "untilEndOfCurrentTurn")
                )
        }
        Some("setPermanentCardTypes") => {
            object_expression_supported(&effect["object"])
                && effect["cardTypes"]
                    .as_array()
                    .is_some_and(|types| !types.is_empty() && types.iter().all(Value::is_string))
                && effect.get("removeSubtypes").is_none_or(Value::is_boolean)
        }
        Some("addCardType") => {
            super::characteristic_object_supported(&effect["object"])
                && matches!(
                    effect["cardType"].as_str(),
                    Some(
                        "Artifact"
                            | "Battle"
                            | "Creature"
                            | "Enchantment"
                            | "Land"
                            | "Planeswalker"
                    )
                )
                && effect["retainExistingTypes"].as_bool() == Some(true)
                && matches!(
                    value_kind(&effect["duration"]),
                    Some("untilEndOfCurrentTurn" | "permanent")
                )
        }
        Some("setAllColorsUntilEndOfTurn") => {
            target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                )
        }
        Some("setColorlessUntilEndOfTurn") => {
            target_reference_supported(&effect["object"])
                || matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                )
        }
        Some("addPermanentCharacteristics") => {
            target_reference_supported(&effect["object"])
                && effect.get("supertypes").is_none_or(|values| {
                    values
                        .as_array()
                        .is_some_and(|values| values.iter().all(Value::is_string))
                })
                && effect.get("subtypes").is_none_or(|values| {
                    values
                        .as_array()
                        .is_some_and(|values| values.iter().all(Value::is_string))
                })
        }
        Some("removeAllSubtypesUntilEndOfTurn") => target_reference_supported(&effect["object"]),
        _ => false,
    }
}

impl GameEngine {
    pub(super) fn execute_type_modification_domain<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        _next_effect: Option<&Value>,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<bool, EngineError> {
        self.execute_type_modification_domain_known(
            effect,
            stack_object,
            bindings,
            decisions,
            provider,
        )
        .map(|()| true)
    }

    fn execute_type_modification_domain_known<P: DecisionProvider>(
        &mut self,
        effect: &Value,
        stack_object: &StackObject,
        bindings: &mut BTreeMap<String, RuntimeBinding>,
        decisions: &mut BTreeMap<String, Value>,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        match value_kind(effect) {
            Some("transformPermanent") => {
                let instance_ids = match value_kind(&effect["object"]) {
                    Some("self" | "abilitySource") => {
                        vec![stack_object.card.instance_id.clone()]
                    }
                    _ => self
                        .resolve_runtime_object_ids(
                            &effect["object"],
                            stack_object,
                            bindings,
                            decisions,
                        )
                        .unwrap_or_default(),
                };
                for instance_id in instance_ids {
                    if let Some(source) = self.transform_permanent(&instance_id, None) {
                        self.record_event(
                            "permanentTransformed",
                            Some(stack_object.controller.clone()),
                            Some(source.instance_id.clone()),
                            json!({}),
                        );
                        self.enqueue_permanent_transformed_triggers(&source);
                    }
                }
            }
            Some("modifySubtypeSet") => {
                self.execute_modify_subtype_set(effect, stack_object, bindings, decisions);
            }
            Some("becomeCreature") => {
                let instance_id = match value_kind(&effect["object"]) {
                    Some("self" | "abilitySource") => Some(stack_object.card.instance_id.clone()),
                    _ => self
                        .resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        }),
                };
                if let Some(instance_id) = instance_id
                    && let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                {
                    if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn")
                        && !self.state.rule_modifiers.iter().any(|modifier| {
                            value_kind(modifier) == Some("temporaryCreatureUntilEndOfTurn")
                                && modifier["cardInstanceId"].as_str() == Some(instance_id.as_str())
                                && modifier["expiresAfterTurn"].as_u64()
                                    == Some(u64::from(self.state.turn_number))
                        })
                    {
                        let original_definition = self.state.players[player_index].battlefield
                            [card_index]
                            .definition
                            .clone();
                        self.state.rule_modifiers.push(json!({
                            "kind": "temporaryCreatureUntilEndOfTurn",
                            "cardInstanceId": instance_id.clone(),
                            "originalDefinition": original_definition,
                            "expiresAfterTurn": self.state.turn_number,
                        }));
                    }
                    let type_line = {
                        let permanent =
                            &mut self.state.players[player_index].battlefield[card_index];
                        if effect["replaceSubtypes"].as_bool() == Some(true) {
                            permanent.definition.type_line = permanent
                                .definition
                                .type_line
                                .split_once(" - ")
                                .map(|(types, _)| types.to_string())
                                .unwrap_or_else(|| permanent.definition.type_line.clone());
                        }
                        let added_types = effect["addTypes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .filter(|card_type| {
                                !type_line_contains(&permanent.definition.type_line, card_type)
                            })
                            .collect::<Vec<_>>();
                        if !added_types.is_empty() {
                            permanent.definition.type_line = format!(
                                "{} {}",
                                permanent.definition.type_line,
                                added_types.join(" ")
                            );
                        }
                        let added_subtypes = effect["addSubtypes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .filter(|subtype| {
                                !type_line_contains(&permanent.definition.type_line, subtype)
                            })
                            .collect::<Vec<_>>();
                        if !added_subtypes.is_empty() {
                            permanent.definition.type_line = format!(
                                "{} - {}",
                                permanent.definition.type_line,
                                added_subtypes.join(" ")
                            );
                        }
                        let colors = effect["addColors"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(Value::as_str)
                            .collect::<Vec<_>>();
                        if !colors.is_empty() {
                            permanent.definition.printed_rules_mut().push(json!({
                                "kind": "rulesMarker",
                                "colorIndicator": colors,
                            }));
                        }
                        permanent.definition.power =
                            integer_value(&effect["basePower"]).map(|value| value.to_string());
                        permanent.definition.toughness =
                            integer_value(&effect["baseToughness"]).map(|value| value.to_string());
                        permanent.definition.type_line.clone()
                    };
                    self.record_event(
                        "permanentBecameCreature",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({
                            "power": effect["basePower"],
                            "toughness": effect["baseToughness"],
                            "typeLine": type_line,
                        }),
                    );
                }
            }
            Some("addSubtypeToPermanent") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    stack_object.card.instance_id.clone()
                } else if let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                {
                    instance_id
                } else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let subtype = effect["subtype"].as_str().unwrap_or_default();
                let permanent = &mut self.state.players[player_index].battlefield[card_index];
                if value_kind(&effect["duration"]) == Some("untilEndOfCurrentTurn") {
                    permanent.definition.printed_rules_mut().push(json!({
                        "kind": "staticAbility",
                        "source": { "kind": "self" },
                        "activeWhile": {
                            "kind": "inZone",
                            "object": { "kind": "self" },
                            "zone": { "kind": "battlefield" },
                        },
                        "modifiers": [{
                            "kind": "addSubtype",
                            "objects": { "kind": "self" },
                            "subtype": subtype,
                        }],
                        "temporaryUntilTurn": self.state.turn_number,
                    }));
                    return Ok(());
                }
                if !type_line_contains(&permanent.definition.type_line, subtype) {
                    permanent.definition.type_line =
                        if permanent.definition.type_line.contains(" - ") {
                            format!("{} {subtype}", permanent.definition.type_line)
                        } else {
                            format!("{} - {subtype}", permanent.definition.type_line)
                        };
                }
            }
            Some("setPermanentCardTypes") => {
                let Some(instance_id) = self
                    .resolve_runtime_object_ids(
                        &effect["object"],
                        stack_object,
                        bindings,
                        decisions,
                    )
                    .and_then(|objects| objects.into_iter().next())
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let card_types = effect["cardTypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>();
                let permanent = &mut self.state.players[player_index].battlefield[card_index];
                let (main_types, subtypes) = split_type_line(&permanent.definition.type_line)
                    .map_or(
                        (permanent.definition.type_line.as_str(), None),
                        |(main, subtypes)| (main, Some(subtypes)),
                    );
                let supertypes = main_types
                    .split_whitespace()
                    .filter(|word| {
                        matches!(*word, "Basic" | "Legendary" | "Ongoing" | "Snow" | "World")
                    })
                    .collect::<Vec<_>>();
                let mut type_line = supertypes
                    .into_iter()
                    .chain(card_types.iter().copied())
                    .collect::<Vec<_>>()
                    .join(" ");
                if effect["removeSubtypes"].as_bool() != Some(true)
                    && let Some(subtypes) = subtypes
                {
                    type_line.push_str(" - ");
                    type_line.push_str(subtypes);
                }
                permanent.definition.type_line = type_line;
                if !card_types.iter().any(|card_type| *card_type == "Creature") {
                    permanent.definition.power = None;
                    permanent.definition.toughness = None;
                }
            }
            Some("setAllColorsUntilEndOfTurn") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    Some(stack_object.card.instance_id.clone())
                } else {
                    self.resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        })
                };
                if let Some(instance_id) = instance_id
                    && let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                {
                    let rule = json!({
                        "kind": "staticAbility",
                        "source": { "kind": "self" },
                        "activeWhile": {
                            "kind": "inZone",
                            "object": { "kind": "self" },
                            "zone": { "kind": "battlefield" },
                        },
                        "modifiers": [{
                            "kind": "setAllColors",
                            "objects": { "kind": "self" },
                        }],
                        "temporaryUntilTurn": self.state.turn_number,
                    });
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .printed_rules_mut()
                        .push(rule);
                }
            }
            Some("setColorlessUntilEndOfTurn") => {
                let instance_id = if matches!(
                    value_kind(&effect["object"]),
                    Some("self" | "abilitySource")
                ) {
                    Some(stack_object.card.instance_id.clone())
                } else {
                    self.resolve_target(&effect["object"], stack_object, bindings)
                        .and_then(|target| match target {
                            TargetRef::Permanent { instance_id } => Some(instance_id),
                            _ => None,
                        })
                };
                if let Some(instance_id) = instance_id
                    && let Some((player_index, card_index)) = self.permanent_position(&instance_id)
                {
                    self.state.players[player_index].battlefield[card_index]
                        .definition
                        .printed_rules_mut()
                        .push(json!({
                            "kind": "staticAbility",
                            "source": { "kind": "self" },
                            "activeWhile": {
                                "kind": "inZone",
                                "object": { "kind": "self" },
                                "zone": { "kind": "battlefield" },
                            },
                            "modifiers": [{
                                "kind": "setColor",
                                "objects": { "kind": "self" },
                                "colors": [],
                            }],
                            "temporaryUntilTurn": self.state.turn_number,
                        }));
                }
            }
            Some("addPermanentCharacteristics") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let definition =
                    &mut self.state.players[player_index].battlefield[card_index].definition;
                let mut type_line = definition.type_line.clone();
                for supertype in effect["supertypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if !type_line
                        .split_whitespace()
                        .any(|word| word.eq_ignore_ascii_case(supertype))
                    {
                        type_line = format!("{supertype} {type_line}");
                    }
                }
                let mut parts = type_line.splitn(2, " — ");
                let main_types = parts.next().unwrap_or_default().to_string();
                let mut subtypes = parts
                    .next()
                    .map(|part| {
                        part.split_whitespace()
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                for subtype in effect["subtypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                {
                    if !subtypes
                        .iter()
                        .any(|value| value.eq_ignore_ascii_case(subtype))
                    {
                        subtypes.push(subtype.to_string());
                    }
                }
                definition.type_line = if subtypes.is_empty() {
                    main_types
                } else {
                    format!("{main_types} — {}", subtypes.join(" "))
                };
            }
            Some("removeAllSubtypesUntilEndOfTurn") => {
                let Some(TargetRef::Permanent { instance_id }) =
                    self.resolve_target(&effect["object"], stack_object, bindings)
                else {
                    return Ok(());
                };
                let Some((player_index, card_index)) = self.permanent_position(&instance_id) else {
                    return Ok(());
                };
                let original_type_line = self.state.players[player_index].battlefield[card_index]
                    .definition
                    .type_line
                    .clone();
                let main_types = original_type_line
                    .split_once(" - ")
                    .map_or(original_type_line.as_str(), |(main, _)| main)
                    .to_string();
                self.state.players[player_index].battlefield[card_index]
                    .definition
                    .type_line = main_types;
                self.state.rule_modifiers.push(json!({
                    "kind": "restoreTypeLineAtCleanup",
                    "cardInstanceId": instance_id,
                    "originalTypeLine": original_type_line,
                    "expiresAfterTurn": self.state.turn_number,
                }));
            }
            Some("addCardType") => {
                for instance_id in self.characteristic_object_ids(
                    &effect["object"],
                    stack_object,
                    bindings,
                    decisions,
                ) {
                    if value_kind(&effect["duration"]) == Some("permanent") {
                        if let Some((player_index, card_index)) =
                            self.permanent_position(&instance_id)
                        {
                            let card_type = effect["cardType"].as_str().unwrap_or_default();
                            let permanent =
                                &mut self.state.players[player_index].battlefield[card_index];
                            if !type_line_contains(&permanent.definition.type_line, card_type) {
                                permanent.definition.type_line = if let Some((types, subtypes)) =
                                    split_type_line(&permanent.definition.type_line)
                                {
                                    format!("{types} {card_type} - {subtypes}")
                                } else {
                                    format!("{} {card_type}", permanent.definition.type_line)
                                };
                            }
                        }
                    } else {
                        self.state.rule_modifiers.push(json!({
                            "kind": "temporaryCardType",
                            "objectId": instance_id.clone(),
                            "cardType": effect["cardType"],
                            "expiresAfterTurn": self.state.turn_number,
                        }));
                    }
                    self.record_event(
                        "cardTypeAdded",
                        Some(stack_object.controller.clone()),
                        Some(instance_id),
                        json!({
                            "cardType": effect["cardType"],
                            "duration": effect["duration"],
                        }),
                    );
                }
            }
            _ => {
                return Err(EngineError::new(format!(
                    "unsupported effect reached execution: {}",
                    value_kind(effect).unwrap_or("missingKind")
                )));
            }
        }
        Ok(())
    }
}

/// The static layer-4 modifiers of a battlefield source (CR 205.1, 613.1d):
///
/// - `setCardTypes {objects, cardTypes}` (CR 205.1a): the objects' card types
///   are exactly these while the ability applies;
/// - `modifySubtypeSet {objects, subtypeClass: "creature", mode, subtypes}`
///   (CR 205.1b, 205.3m): `replace` makes the listed creature types the
///   objects' only creature types, `all` gives every creature type.
///
/// A creature subtype applies only to a creature (CR 205.3d), and after a
/// card-type set a registered subtype whose class no longer corresponds to a
/// card type is gone. Additive `addSubtype` modifiers still add their own.
const PERMANENT_CARD_TYPES: [&str; 6] = [
    "Artifact",
    "Battle",
    "Creature",
    "Enchantment",
    "Land",
    "Planeswalker",
];

impl GameEngine {
    /// Validation of the static layer-4 modifiers this domain evaluates.
    pub(in crate::engine) fn static_type_modifier_supported(modifier: &Value) -> bool {
        let condition = modifier.get("condition").is_none_or(condition_supported);
        match value_kind(modifier) {
            Some("setCardTypes") => {
                static_object_selector_supported(&modifier["objects"])
                    && condition
                    && modifier["cardTypes"].as_array().is_some_and(|types| {
                        !types.is_empty()
                            && types.iter().all(|card_type| {
                                card_type
                                    .as_str()
                                    .is_some_and(|value| PERMANENT_CARD_TYPES.contains(&value))
                            })
                    })
            }
            Some("modifySubtypeSet") => {
                static_object_selector_supported(&modifier["objects"])
                    && condition
                    && modifier["subtypeClass"].as_str() == Some("creature")
                    && match modifier["mode"].as_str() {
                        Some("replace") => modifier["subtypes"].as_array().is_some_and(|values| {
                            !values.is_empty()
                                && values.iter().all(|value| {
                                    value
                                        .as_str()
                                        .is_some_and(object_predicate::is_creature_subtype)
                                })
                        }),
                        Some("all") => modifier.get("subtypes").is_none(),
                        _ => false,
                    }
            }
            _ => false,
        }
    }

    /// The active static layer-4 modifiers of one kind that apply to the
    /// permanent, in battlefield order.
    fn static_type_modifiers_for(&self, permanent: &CardInstance, kind: &str) -> Vec<Value> {
        self.battlefield_static_modifiers()
            .into_iter()
            .filter(|(source, modifier)| {
                value_kind(modifier) == Some(kind)
                    && Self::static_selector_matches(source, &modifier["objects"], permanent)
                    && modifier.get("condition").is_none_or(|condition| {
                        self.condition_value_with_source(
                            condition,
                            &source.controller,
                            &BTreeMap::new(),
                            Some(source),
                        )
                    })
            })
            .map(|(_, modifier)| modifier)
            .collect()
    }

    /// CR 205.1a: the card types a static `setCardTypes` gives the
    /// permanent, replacing its printed or recorded card types. The latest
    /// applicable modifier in battlefield order wins.
    pub(in crate::engine) fn static_card_type_set(
        &self,
        permanent: &CardInstance,
    ) -> Option<Vec<String>> {
        self.static_type_modifiers_for(permanent, "setCardTypes")
            .last()
            .map(|modifier| {
                modifier["cardTypes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
    }

    /// Whether the static layer-4 modifiers decide the permanent's membership
    /// in `subtype`: `Some(true)` when a creature-type set gives it,
    /// `Some(false)` when a creature-type replacement or a card-type set
    /// removes it and no additive modifier gives it back, `None` when they
    /// say nothing (the printed and recorded reading decides).
    pub(in crate::engine) fn static_subtype_membership(
        &self,
        permanent: &CardInstance,
        subtype: &str,
    ) -> Option<bool> {
        let class = super::super::subtypes::registered_subtype_class(subtype)?;
        let set_types = self.static_card_type_set(permanent);
        let corresponds = |card_type: &str| {
            set_types
                .as_ref()
                .is_none_or(|types| types.iter().any(|value| value == card_type))
        };
        let removed_by_card_types = match class {
            "creature" => !corresponds("Creature"),
            "land" => !corresponds("Land"),
            _ => false,
        };
        let mut decided = removed_by_card_types.then_some(false);
        if class == "creature" && !removed_by_card_types && self.permanent_is_creature(permanent) {
            for modifier in self.static_type_modifiers_for(permanent, "modifySubtypeSet") {
                if modifier["subtypeClass"].as_str() != Some("creature") {
                    continue;
                }
                decided = Some(match modifier["mode"].as_str() {
                    Some("all") => true,
                    _ => modifier["subtypes"].as_array().is_some_and(|values| {
                        values.iter().any(|value| {
                            value
                                .as_str()
                                .is_some_and(|value| value.eq_ignore_ascii_case(subtype))
                        })
                    }),
                });
            }
        }
        if decided == Some(false)
            && self
                .static_type_modifiers_for(permanent, "addSubtype")
                .iter()
                .any(|modifier| {
                    modifier["subtype"]
                        .as_str()
                        .is_some_and(|value| value.eq_ignore_ascii_case(subtype))
                })
        {
            // A later additive change gives the subtype back (CR 205.1b).
            return None;
        }
        decided
    }
}
