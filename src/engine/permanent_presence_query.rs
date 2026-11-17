//! Checked entry-condition presence queries. Layer-four ordering is bounded,
//! not inferred from battlefield buckets, instance IDs, or zone markers.
use super::static_condition_capability::condition_is_closed_without_characteristic_reads;
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PresenceQueryError {
    pub state: &'static str,
    pub detail: String,
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "permanent_presence_query_tests.rs"]
mod tests;

impl PresenceQueryError {
    fn new(state: &'static str, detail: impl Into<String>) -> Self {
        Self {
            state,
            detail: detail.into(),
        }
    }
    pub(super) fn into_engine_error(self) -> EngineError {
        EngineError::new(format!(
            "unsupported permanent presence: state={} detail={}",
            self.state, self.detail
        ))
    }
}

pub(super) fn node_requires_checkpoint(node: &Value) -> bool {
    value_kind(node) == Some("controlsPermanent")
}

/// Fallible counterpart of Iterator::all/any. This only folds already-owned
/// Boolean results; domain leaves retain their existing evaluators. An error
/// from an evaluated prefix propagates before any later decisive operand.
fn try_boolean_operands<T, E>(
    operands: impl IntoIterator<Item = T>,
    conjunction: bool,
    mut evaluate: impl FnMut(T) -> Result<bool, E>,
) -> Result<bool, E> {
    for operand in operands {
        let truth = evaluate(operand)?;
        if truth != conjunction {
            return Ok(truth);
        }
    }
    Ok(conjunction)
}

fn land_type_modifier(value: &Value) -> bool {
    matches!(
        value_kind(value),
        Some(
            "setLandSubtype"
                | "setSubtypes"
                | "addSubtype"
                | "addAllBasicLandTypes"
                | "nonbasicLandsBecomeMountains"
                | "addCardType"
                | "removeCardTypeWhile"
                | "addSupertype"
                | "setName"
        )
    )
}

fn basic_land_subtype(value: &Value) -> bool {
    value_kind(value) == Some("subtypeContains")
        && value["value"].as_str().is_some_and(|value| {
            ["Plains", "Island", "Swamp", "Mountain", "Forest"]
                .into_iter()
                .any(|name| value.eq_ignore_ascii_case(name))
        })
}

fn contains_basic_land_subtype(value: &Value, depth: usize) -> bool {
    if depth >= 64 {
        return true;
    }
    basic_land_subtype(value)
        || match value_kind(value) {
            Some("not") => contains_basic_land_subtype(&value["operand"], depth + 1),
            Some("and" | "or") => value["operands"].as_array().is_some_and(|values| {
                values
                    .iter()
                    .any(|value| contains_basic_land_subtype(value, depth + 1))
            }),
            _ => false,
        }
}

fn land_presence_filter_closed(value: &Value, depth: usize) -> bool {
    if depth >= 64 {
        return false;
    }
    if basic_land_subtype(value) {
        return true;
    }
    match value_kind(value) {
        Some("not") => land_presence_filter_closed(&value["operand"], depth + 1),
        Some("and" | "or") => value["operands"].as_array().is_some_and(|values| {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| land_presence_filter_closed(value, depth + 1))
        }),
        _ => false,
    }
}

/// Filters used to decide which object receives a type-changing effect must
/// not ask for the characteristic that this projection is calculating.
fn stable_recipient_filter(filter: &Value, depth: usize) -> bool {
    if depth >= 64 {
        return false;
    }
    match value_kind(filter) {
        None => filter.is_null(),
        Some(
            "nameEquals" | "nameStartsWith" | "cardTypeContains" | "isToken" | "isCommander"
            | "isBasic" | "isLegendary",
        ) => card_filter_supported(filter),
        Some("not") => stable_recipient_filter(&filter["operand"], depth + 1),
        Some("and" | "or") => filter["operands"].as_array().is_some_and(|values| {
            !values.is_empty()
                && values
                    .iter()
                    .all(|value| stable_recipient_filter(value, depth + 1))
        }),
        _ => false,
    }
}

impl GameEngine {
    fn presence_selector_matches(
        source: &CardInstance,
        selector: &Value,
        permanent: &CardInstance,
    ) -> Result<bool, PresenceQueryError> {
        let source_controller = |value: &Value| {
            value_kind(value) == Some("controllerOf")
                && value_kind(&value["object"]) == Some("self")
        };
        let controller_closed = selector.get("controller").is_none_or(|value| {
            source_controller(value)
                || (value_kind(value) == Some("opponentsOf") && source_controller(&value["player"]))
        });
        match value_kind(selector) {
            Some("self" | "attachedPermanent") if selector.get("controller").is_none() => {
                Ok(Self::static_selector_matches(source, selector, permanent))
            }
            Some("permanents")
                if controller_closed
                    && selector.get("history").is_none()
                    && stable_recipient_filter(&selector["where"], 0)
                    && static_object_selector_supported(selector) =>
            {
                Ok(Self::static_selector_matches(source, selector, permanent))
            }
            _ => Err(PresenceQueryError::new(
                "presence.query.selector-dependency",
                selector.to_string(),
            )),
        }
    }

    fn presence_static_condition(
        &self,
        value: &Value,
        source: &CardInstance,
    ) -> Result<bool, PresenceQueryError> {
        if value.is_null() {
            return Ok(true);
        }
        if !condition_is_closed_without_characteristic_reads(value, 0) {
            return Err(PresenceQueryError::new(
                "presence.query.characteristic-dependency",
                value.to_string(),
            ));
        }
        Ok(self.condition_value_with_source(
            value,
            &source.controller,
            &BTreeMap::new(),
            Some(source),
        ))
    }

    /// Additions commute. A single replacement is closed. Replacements mixed
    /// with additions or with other replacements need timestamp/dependency
    /// ordering that the existing modifier contract does not provide here.
    fn checked_land_subtype_membership(
        &self,
        permanent: &CardInstance,
        subtype: &str,
    ) -> Result<bool, PresenceQueryError> {
        let mut replacements: Vec<Vec<String>> = Vec::new();
        let mut additions = BTreeSet::new();
        for modifier in &self.state.rule_modifiers {
            match value_kind(modifier) {
                Some("allBasicLandTypesUntilEndOfTurn")
                    if modifier["playerId"].as_str() == Some(permanent.controller.as_str()) =>
                {
                    additions.extend(
                        ["Plains", "Island", "Swamp", "Mountain", "Forest"].map(ToOwned::to_owned),
                    );
                }
                Some("temporarySubtype")
                    if modifier["cardInstanceId"].as_str()
                        == Some(permanent.instance_id.as_str())
                        && modifier["expiresAfterTurn"].as_u64()
                            == Some(u64::from(self.state.turn_number)) =>
                {
                    additions.insert(
                        modifier["subtype"]
                            .as_str()
                            .ok_or_else(|| {
                                PresenceQueryError::new(
                                    "presence.query.modifier",
                                    modifier.to_string(),
                                )
                            })?
                            .to_owned(),
                    );
                }
                Some("counterLinkedCharacteristics")
                    if modifier["cardInstanceId"].as_str()
                        == Some(permanent.instance_id.as_str())
                        && modifier["counter"].as_str().is_some_and(|name| {
                            permanent.counters.get(name).copied().unwrap_or(0) > 0
                        }) =>
                {
                    if modifier["addTypes"]
                        .as_array()
                        .is_some_and(|types| !types.is_empty())
                    {
                        return Err(PresenceQueryError::new(
                            "presence.query.card-type-layer",
                            modifier.to_string(),
                        ));
                    }
                    for value in modifier["addSubtypes"].as_array().into_iter().flatten() {
                        additions.insert(
                            value
                                .as_str()
                                .ok_or_else(|| {
                                    PresenceQueryError::new(
                                        "presence.query.modifier",
                                        modifier.to_string(),
                                    )
                                })?
                                .to_owned(),
                        );
                    }
                }
                _ => {}
            }
        }
        for source in self
            .state
            .players
            .iter()
            .flat_map(|player| &player.battlefield)
        {
            if source.flags.get("phasedOut").copied().unwrap_or(false) {
                continue;
            }
            for rule in &source.definition.rules {
                if !Self::rule_metadata_active_for_source(source, rule) {
                    continue;
                }
                let Some(modifiers) = rule["modifiers"].as_array() else {
                    continue;
                };
                if !modifiers.iter().any(land_type_modifier) {
                    continue;
                }
                if value_kind(&rule["activeWhile"]) == Some("inZone")
                    && value_kind(&rule["activeWhile"]["object"]) == Some("self")
                    && matches!(
                        value_kind(&rule["activeWhile"]["zone"]),
                        Some("hand" | "library" | "graveyard" | "exile" | "command" | "stack")
                    )
                {
                    continue;
                }
                for modifier in modifiers.iter().filter(|value| land_type_modifier(value)) {
                    // Card-type changes invalidate the printed recipient-domain
                    // proof; reject before consulting a printed type filter.
                    if matches!(
                        value_kind(modifier),
                        Some("addCardType" | "removeCardTypeWhile" | "addSupertype" | "setName")
                    ) {
                        return Err(PresenceQueryError::new(
                            "presence.query.card-type-layer",
                            modifier.to_string(),
                        ));
                    }
                    let matches = if value_kind(modifier) == Some("nonbasicLandsBecomeMountains") {
                        is_land(&permanent.definition)
                            && !matches_card_filter(
                                &permanent.definition,
                                &json!({"kind":"isBasic"}),
                            )
                    } else {
                        Self::presence_selector_matches(source, &modifier["objects"], permanent)?
                    };
                    if !matches {
                        continue;
                    }
                    if value_kind(rule) != Some("staticAbility")
                        || !battlefield_static_rule_supported(rule)
                    {
                        return Err(PresenceQueryError::new(
                            "presence.query.rule-domain",
                            rule.to_string(),
                        ));
                    }
                    if !self.presence_static_condition(&rule["condition"], source)?
                        || !self.presence_static_condition(&modifier["condition"], source)?
                    {
                        continue;
                    }
                    match value_kind(modifier) {
                        Some("setLandSubtype") => replacements.push(vec![
                            modifier["subtype"]
                                .as_str()
                                .ok_or_else(|| {
                                    PresenceQueryError::new(
                                        "presence.query.modifier",
                                        modifier.to_string(),
                                    )
                                })?
                                .to_owned(),
                        ]),
                        Some("nonbasicLandsBecomeMountains") => {
                            replacements.push(vec!["Mountain".to_owned()])
                        }
                        Some("setSubtypes") => replacements.push(
                            modifier["subtypes"]
                                .as_array()
                                .ok_or_else(|| {
                                    PresenceQueryError::new(
                                        "presence.query.modifier",
                                        modifier.to_string(),
                                    )
                                })?
                                .iter()
                                .map(|value| {
                                    value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
                                        PresenceQueryError::new(
                                            "presence.query.modifier",
                                            modifier.to_string(),
                                        )
                                    })
                                })
                                .collect::<Result<Vec<_>, _>>()?,
                        ),
                        Some("addSubtype") => {
                            additions.insert(
                                modifier["subtype"]
                                    .as_str()
                                    .ok_or_else(|| {
                                        PresenceQueryError::new(
                                            "presence.query.modifier",
                                            modifier.to_string(),
                                        )
                                    })?
                                    .to_owned(),
                            );
                        }
                        Some("addAllBasicLandTypes") => additions.extend(
                            ["Plains", "Island", "Swamp", "Mountain", "Forest"]
                                .map(ToOwned::to_owned),
                        ),
                        _ => unreachable!("type modifier checked above"),
                    }
                }
            }
        }
        if replacements.len() > 1 || (!replacements.is_empty() && !additions.is_empty()) {
            return Err(PresenceQueryError::new(
                "presence.query.unclosed-layer-order",
                permanent.instance_id.clone(),
            ));
        }
        let base_matches = replacements.first().map_or_else(
            || has_card_subtype(&permanent.definition, subtype),
            |values| {
                values
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(subtype))
            },
        );
        Ok(is_land(&permanent.definition)
            && (base_matches
                || additions
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(subtype))))
    }

    fn checked_presence_filter(
        &self,
        permanent: &CardInstance,
        filter: &Value,
        depth: usize,
    ) -> Result<bool, PresenceQueryError> {
        if depth >= 64 {
            return Err(PresenceQueryError::new("presence.query.depth", "filter"));
        }
        match value_kind(filter) {
            Some("subtypeContains") if basic_land_subtype(filter) => self
                .checked_land_subtype_membership(
                    permanent,
                    filter["value"].as_str().expect("guarded"),
                ),
            Some("not") => {
                Ok(!self.checked_presence_filter(permanent, &filter["operand"], depth + 1)?)
            }
            Some("and" | "or") => {
                let operands = filter["operands"]
                    .as_array()
                    .filter(|values| !values.is_empty())
                    .ok_or_else(|| {
                        PresenceQueryError::new("presence.query.filter-domain", filter.to_string())
                    })?;
                try_boolean_operands(operands, value_kind(filter) == Some("and"), |operand| {
                    self.checked_presence_filter(permanent, operand, depth + 1)
                })
            }
            _ => Err(PresenceQueryError::new(
                "presence.query.filter-domain",
                filter.to_string(),
            )),
        }
    }

    pub(super) fn checked_entry_condition_with_source(
        &self,
        condition: &Value,
        controller: &str,
        decisions: &BTreeMap<String, Value>,
        source: &CardInstance,
    ) -> Result<bool, PresenceQueryError> {
        self.checked_entry_condition_inner(condition, controller, decisions, source, 0)
    }

    fn checked_entry_condition_inner(
        &self,
        condition: &Value,
        controller: &str,
        decisions: &BTreeMap<String, Value>,
        source: &CardInstance,
        depth: usize,
    ) -> Result<bool, PresenceQueryError> {
        if depth >= 64 {
            return Err(PresenceQueryError::new("presence.query.depth", "condition"));
        }
        match value_kind(condition) {
            Some("controlsPermanent") => {
                // Existing non-land-presence consumers keep their established
                // semantics. This delta closes basic-land-type membership;
                // mixed characteristic queries are not silently approximated.
                if !contains_basic_land_subtype(&condition["where"], 0) {
                    return Ok(self.condition_value_with_source(
                        condition,
                        controller,
                        decisions,
                        Some(source),
                    ));
                }
                if !land_presence_filter_closed(&condition["where"], 0) {
                    return Err(PresenceQueryError::new(
                        "presence.query.filter-domain",
                        condition["where"].to_string(),
                    ));
                }
                let player = &condition["player"];
                if !(value_kind(player) == Some("abilityController")
                    || (value_kind(player) == Some("controllerOf")
                        && value_kind(&player["object"]) == Some("self")))
                {
                    return Err(PresenceQueryError::new(
                        "presence.query.player",
                        player.to_string(),
                    ));
                }
                self.player_index(controller).map_err(|error| {
                    PresenceQueryError::new("presence.query.player", error.message)
                })?;
                // Battlefield storage order is not a semantic ordering. A
                // fully proved witness establishes existence even if a prior
                // object had an unclosed projection. No witness means that
                // unknown membership must remain an error, never false.
                let mut first_error = None;
                for permanent in self
                    .state
                    .players
                    .iter()
                    .flat_map(|player| &player.battlefield)
                {
                    if permanent.controller != controller
                        || permanent.flags.get("phasedOut").copied().unwrap_or(false)
                        || (condition["excludeSource"].as_bool() == Some(true)
                            && permanent.instance_id == source.instance_id)
                        || condition["excludeName"].as_str().is_some_and(|name| {
                            permanent.definition.name.eq_ignore_ascii_case(name)
                        })
                    {
                        continue;
                    }
                    match self.checked_presence_filter(permanent, &condition["where"], 0) {
                        Ok(true) => return Ok(true),
                        Ok(false) => {}
                        Err(error) => {
                            if first_error.is_none() {
                                first_error = Some(error);
                            }
                        }
                    }
                }
                match first_error {
                    Some(error) => Err(error),
                    None => Ok(false),
                }
            }
            Some("not") => Ok(!self.checked_entry_condition_inner(
                &condition["operand"],
                controller,
                decisions,
                source,
                depth + 1,
            )?),
            Some("and" | "or") => {
                let operands = condition["operands"]
                    .as_array()
                    .filter(|values| !values.is_empty())
                    .ok_or_else(|| {
                        PresenceQueryError::new(
                            "presence.query.condition-domain",
                            condition.to_string(),
                        )
                    })?;
                try_boolean_operands(operands, value_kind(condition) == Some("and"), |operand| {
                    self.checked_entry_condition_inner(
                        operand,
                        controller,
                        decisions,
                        source,
                        depth + 1,
                    )
                })
            }
            _ => Ok(self.condition_value_with_source(
                condition,
                controller,
                decisions,
                Some(source),
            )),
        }
    }
}
