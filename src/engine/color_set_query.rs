//! Checked five-color query. Color identity and colorless are separate concepts.
//! This bounded layer-five reader never asks a P/T evaluator for its own input.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct FiveColorSet(u8);

impl FiveColorSet {
    const SYMBOLS: [&'static str; 5] = ["W", "U", "B", "R", "G"];

    fn insert(&mut self, color: &str) -> Result<(), ColorSetQueryError> {
        let index = match color.to_ascii_uppercase().as_str() {
            "W" | "WHITE" => 0,
            "U" | "BLUE" => 1,
            "B" | "BLACK" => 2,
            "R" | "RED" => 3,
            "G" | "GREEN" => 4,
            "C" | "COLORLESS" => return Ok(()),
            _ => return Err(ColorSetQueryError::new("color.query.symbol", color)),
        };
        self.0 |= 1 << index;
        Ok(())
    }

    pub(super) fn cardinality(self) -> i32 {
        self.0.count_ones() as i32
    }

    pub(super) fn symbols(self) -> Vec<String> {
        Self::SYMBOLS
            .into_iter()
            .enumerate()
            .filter(|(index, _)| self.0 & (1 << index) != 0)
            .map(|(_, symbol)| symbol.to_owned())
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ColorSetQueryError {
    pub state: &'static str,
    pub detail: String,
}

impl ColorSetQueryError {
    fn new(state: &'static str, detail: impl Into<String>) -> Self {
        Self {
            state,
            detail: detail.into(),
        }
    }
}

fn color_modifier(modifier: &Value) -> bool {
    matches!(
        value_kind(modifier),
        Some("setAllColors" | "setColor" | "setStoredColor")
    )
}

use super::static_condition_capability::condition_is_closed_without_characteristic_reads;

impl GameEngine {
    /// Initial public query domain is permanents controlled by the ability actor.
    /// The actor is supplied by the evaluation frame, never guessed from a new
    /// incarnation of the source after a controller change.
    pub(super) fn checked_color_set_query(
        &self,
        query: &Value,
        controller_id: &str,
    ) -> Result<FiveColorSet, ColorSetQueryError> {
        if value_kind(query) != Some("eachColorAmongPermanents") {
            return Err(ColorSetQueryError::new(
                "color.query.kind",
                query.to_string(),
            ));
        }
        let player = &query["player"];
        if !(value_kind(player) == Some("abilityController")
            || (value_kind(player) == Some("controllerOf")
                && matches!(
                    value_kind(&player["object"]),
                    Some("self" | "abilitySource")
                )))
        {
            return Err(ColorSetQueryError::new(
                "color.query.player-context",
                player.to_string(),
            ));
        }
        if query.get("where").is_some_and(|filter| !filter.is_null()) {
            return Err(ColorSetQueryError::new(
                "color.query.filter-domain",
                query.to_string(),
            ));
        }
        self.player_index(controller_id)
            .map_err(|_| ColorSetQueryError::new("color.query.player-context", controller_id))?;
        let mut result = FiveColorSet::default();
        for permanent in self
            .state
            .players
            .iter()
            .flat_map(|player| &player.battlefield)
            .filter(|permanent| permanent.controller == controller_id)
        {
            if permanent.flags.get("phasedOut").copied().unwrap_or(false) {
                continue;
            }
            result.0 |= self.checked_permanent_five_colors(permanent)?.0;
        }
        Ok(result)
    }

    fn checked_color_selector_matches(
        source: &CardInstance,
        selector: &Value,
        permanent: &CardInstance,
    ) -> Result<bool, ColorSetQueryError> {
        let source_controller = |reference: &Value| {
            value_kind(reference) == Some("controllerOf")
                && value_kind(&reference["object"]) == Some("self")
        };
        let controller_closed = selector.get("controller").is_none_or(|reference| {
            source_controller(reference)
                || (value_kind(reference) == Some("opponentsOf")
                    && source_controller(&reference["player"]))
        });
        match value_kind(selector) {
            Some("self" | "attachedPermanent") if selector.get("controller").is_none() => {
                Ok(Self::static_selector_matches(source, selector, permanent))
            }
            Some("permanents")
                if controller_closed
                    && selector.get("where").is_none_or(Value::is_null)
                    && selector.get("history").is_none()
                    && static_object_selector_supported(selector) =>
            {
                Ok(Self::static_selector_matches(source, selector, permanent))
            }
            _ => Err(ColorSetQueryError::new(
                "color.query.selector-domain",
                selector.to_string(),
            )),
        }
    }

    pub(super) fn checked_permanent_five_colors(
        &self,
        permanent: &CardInstance,
    ) -> Result<FiveColorSet, ColorSetQueryError> {
        let mut replacements = Vec::new();
        // A resolved color-setting effect recorded on the object (CR 105.3).
        if let Some(colors) = effect_domains::marker_colors(&permanent.definition) {
            replacements.push(json!(colors));
        }
        for modifier in &self.state.rule_modifiers {
            if value_kind(modifier) == Some("temporaryColors")
                && modifier["cardInstanceId"].as_str() == Some(permanent.instance_id.as_str())
                && modifier["expiresAfterTurn"].as_u64() == Some(u64::from(self.state.turn_number))
            {
                replacements.push(modifier["colors"].clone());
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
            for rule in source.definition.printed_rules() {
                // Select the color domain before ANY condition. Unrelated P/T
                // modifiers must not call back through current_power here.
                let Some(modifiers) = rule["modifiers"].as_array() else {
                    continue;
                };
                if !modifiers.iter().any(color_modifier)
                    || !Self::rule_metadata_active_for_source(source, rule)
                {
                    continue;
                }
                let active = &rule["activeWhile"];
                let known_zone = value_kind(active) == Some("inZone")
                    && value_kind(&active["object"]) == Some("self");
                if known_zone
                    && active["zone"]["kind"].as_str().is_some_and(|zone| {
                        matches!(
                            zone,
                            "hand" | "library" | "graveyard" | "exile" | "command" | "stack"
                        )
                    })
                {
                    continue;
                }
                let mut relevant = Vec::new();
                for modifier in modifiers.iter().filter(|modifier| color_modifier(modifier)) {
                    if Self::checked_color_selector_matches(
                        source,
                        &modifier["objects"],
                        permanent,
                    )? {
                        relevant.push(modifier);
                    }
                }
                if relevant.is_empty() {
                    continue;
                }
                if value_kind(rule) != Some("staticAbility")
                    || !battlefield_static_rule_supported(rule)
                {
                    return Err(ColorSetQueryError::new(
                        "color.query.rule-domain",
                        rule.to_string(),
                    ));
                }
                'modifier: for modifier in relevant {
                    for condition in [rule.get("condition"), modifier.get("condition")]
                        .into_iter()
                        .flatten()
                        .filter(|condition| !condition.is_null())
                    {
                        if !condition_is_closed_without_characteristic_reads(condition, 0) {
                            return Err(ColorSetQueryError::new(
                                "color.query.characteristic-dependency",
                                condition.to_string(),
                            ));
                        }
                        if !self.condition_value_with_source(
                            condition,
                            &source.controller,
                            &BTreeMap::new(),
                            Some(source),
                        ) {
                            continue 'modifier;
                        }
                    }
                    // Color effects start in layer 5. Layer-6 ability loss does
                    // not erase a color effect that has already started (613.6).
                    let colors = match value_kind(modifier) {
                        Some("setAllColors") => json!(["W", "U", "B", "R", "G"]),
                        Some("setColor") => modifier["colors"].clone(),
                        Some("setStoredColor") => {
                            let decision_id = modifier["decisionId"].as_str().ok_or_else(|| {
                                ColorSetQueryError::new(
                                    "color.query.stored-color",
                                    modifier.to_string(),
                                )
                            })?;
                            let color = stored_decision_value(&source.definition, decision_id)
                                .ok_or_else(|| {
                                    ColorSetQueryError::new("color.query.stored-color", decision_id)
                                })?;
                            json!([color])
                        }
                        _ => unreachable!("color modifier filtered above"),
                    };
                    replacements.push(colors);
                }
            }
        }
        // Existing color modifiers lack one shared timestamp/dependency order.
        // Reject an unclosed ordering rather than prefer all-colors arbitrarily.
        if replacements.len() > 1 {
            return Err(ColorSetQueryError::new(
                "color.query.layer-order",
                &permanent.instance_id,
            ));
        }
        let mut colors = FiveColorSet::default();
        if let Some(replacement) = replacements.first() {
            let values = replacement.as_array().ok_or_else(|| {
                ColorSetQueryError::new("color.query.symbols", replacement.to_string())
            })?;
            for value in values {
                colors.insert(value.as_str().ok_or_else(|| {
                    ColorSetQueryError::new("color.query.symbol", value.to_string())
                })?)?;
            }
            return Ok(colors);
        }
        if has_keyword(&permanent.definition, "devoid") {
            return Ok(colors);
        }
        // Intentionally excludes rulesMarker.colorIdentity. Activated symbols,
        // reminder text and a commander's identity do not define its colors.
        for symbol in parse_mana_symbols(&permanent.definition.mana_cost) {
            for part in symbol.split('/') {
                if FiveColorSet::SYMBOLS.contains(&part) {
                    colors.insert(part)?;
                }
            }
        }
        for indicator in permanent
            .definition
            .printed_rules()
            .iter()
            .flat_map(|rule| rule["colorIndicator"].as_array().into_iter().flatten())
        {
            colors.insert(indicator.as_str().ok_or_else(|| {
                ColorSetQueryError::new("color.query.indicator", indicator.to_string())
            })?)?;
        }
        Ok(colors)
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "color_set_query_tests.rs"]
mod tests;
