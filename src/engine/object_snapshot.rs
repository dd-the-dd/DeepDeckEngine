//! Effective characteristics of a permanent captured at the moment it
//! leaves the battlefield (last-known information, CR 608.2h, 603.10a).
//!
//! The central battlefield exits call `capture_pending_source_lki` while the
//! permanent is still on the battlefield; that capture also records the
//! permanent's effective characteristics, every continuous effect applied.
//! Event patterns of the zone-change family read an object predicate on this
//! snapshot: the snapshot answers the characteristics it captured (card
//! types, subtypes, colors, supertypes, token status, keywords) and leaves
//! every other atom to the pattern's established view.
use super::*;

/// The effective characteristics of one object incarnation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct EffectiveCharacteristics {
    card_types: BTreeSet<String>,
    subtypes: BTreeSet<String>,
    /// Every creature type (changeling, CR 702.73a).
    all_creature_types: bool,
    colors: BTreeSet<String>,
    legendary: bool,
    basic: bool,
    snow: bool,
    token: bool,
    keywords: BTreeSet<String>,
}

const CARD_TYPES: [&str; 9] = [
    "Artifact",
    "Battle",
    "Creature",
    "Enchantment",
    "Kindred",
    "Land",
    "Planeswalker",
    "Instant",
    "Sorcery",
];

fn snapshot_key(card: &CardInstance) -> String {
    format!(
        "{}@{}",
        card.instance_id,
        GameEngine::tracked_zone_identity(card).unwrap_or_default()
    )
}

/// Every string named `field` (or each string of an array) in a JSON value.
fn collect_strings(value: &Value, fields: &[&str], out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if fields.contains(&key.as_str()) {
                    match child {
                        Value::String(text) => {
                            out.insert(text.clone());
                        }
                        Value::Array(items) => {
                            out.extend(items.iter().filter_map(Value::as_str).map(str::to_string));
                        }
                        _ => {}
                    }
                }
                collect_strings(child, fields, out);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_strings(item, fields, out);
            }
        }
        _ => {}
    }
}

impl EffectiveCharacteristics {
    /// The snapshot's answer for one predicate atom, or `None` when the atom
    /// reads something the snapshot does not capture.
    fn atom(&self, atom: &Value) -> Option<bool> {
        let value = atom["value"].as_str();
        match value_kind(atom)? {
            "cardTypeContains" => Some(
                self.card_types
                    .iter()
                    .any(|card_type| value.is_some_and(|v| card_type.eq_ignore_ascii_case(v))),
            ),
            "subtypeContains" => {
                let value = value?;
                Some(
                    self.subtypes
                        .iter()
                        .any(|subtype| subtype.eq_ignore_ascii_case(value))
                        || (self.all_creature_types
                            && object_predicate::is_creature_subtype(value)),
                )
            }
            "colorContains" => Some(
                color_name_to_symbol(value?).is_some_and(|symbol| self.colors.contains(symbol)),
            ),
            "colorDoesNotContain" => Some(
                color_name_to_symbol(value?).is_some_and(|symbol| !self.colors.contains(symbol)),
            ),
            "isToken" => Some(self.token),
            "isLegendary" => Some(self.legendary),
            "typeLineContains" if value?.eq_ignore_ascii_case("Basic") => Some(self.basic),
            "typeLineContains" if value?.eq_ignore_ascii_case("Snow") => Some(self.snow),
            "hasKeyword" => Some(
                self.keywords
                    .iter()
                    .any(|keyword| value.is_some_and(|v| keyword.eq_ignore_ascii_case(v))),
            ),
            _ => None,
        }
    }

    /// Evaluates a predicate on the snapshot; atoms it does not capture are
    /// delegated to `fallback` (the pattern's established view).
    pub(super) fn matches(&self, filter: &Value, fallback: &dyn Fn(&Value) -> bool) -> bool {
        match value_kind(filter) {
            None if filter.is_null() => true,
            Some("and") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .all(|operand| self.matches(operand, fallback))
            }),
            Some("or") => filter["operands"].as_array().is_some_and(|operands| {
                operands
                    .iter()
                    .any(|operand| self.matches(operand, fallback))
            }),
            Some("not") => !self.matches(&filter["operand"], fallback),
            _ => self.atom(filter).unwrap_or_else(|| fallback(filter)),
        }
    }
}

impl GameEngine {
    /// The effective characteristics of a battlefield permanent now.
    pub(super) fn effective_characteristics(
        &self,
        permanent: &CardInstance,
    ) -> EffectiveCharacteristics {
        let atom = |kind: &str, value: &str| {
            self.permanent_matches_effective_filter(
                permanent,
                &json!({"kind": kind, "value": value}),
            )
        };
        let mut subtype_candidates = BTreeSet::new();
        if let Some((_, subtypes)) = permanent
            .definition
            .type_line
            .split_once('—')
            .or_else(|| permanent.definition.type_line.split_once(" - "))
        {
            subtype_candidates.extend(subtypes.split_whitespace().map(str::to_string));
        }
        subtype_candidates
            .extend(["Plains", "Island", "Swamp", "Mountain", "Forest"].map(str::to_string));
        let mut keyword_candidates = BTreeSet::new();
        for rule in permanent.definition.printed_rules() {
            if let Some(kind) = value_kind(&rule["ability"]) {
                keyword_candidates.insert(kind.to_string());
            }
            for ability in rule["abilities"].as_array().into_iter().flatten() {
                if let Some(kind) = value_kind(ability) {
                    keyword_candidates.insert(kind.to_string());
                }
            }
        }
        for (_, modifier) in self.battlefield_static_modifiers() {
            collect_strings(&modifier, &["subtype", "subtypes"], &mut subtype_candidates);
            collect_strings(&modifier, &["keyword", "keywords"], &mut keyword_candidates);
        }
        for modifier in &self.state.rule_modifiers {
            collect_strings(modifier, &["subtype", "subtypes"], &mut subtype_candidates);
            collect_strings(modifier, &["keyword", "keywords"], &mut keyword_candidates);
        }
        EffectiveCharacteristics {
            card_types: CARD_TYPES
                .iter()
                .filter(|card_type| self.permanent_has_card_type(permanent, card_type))
                .map(|card_type| card_type.to_string())
                .collect(),
            subtypes: subtype_candidates
                .into_iter()
                .filter(|subtype| atom("subtypeContains", subtype))
                .collect(),
            all_creature_types: self.permanent_has_keyword(permanent, "changeling"),
            colors: self.permanent_color_symbols(permanent),
            legendary: self
                .permanent_matches_effective_filter(permanent, &json!({"kind": "isLegendary"})),
            basic: atom("typeLineContains", "Basic"),
            snow: atom("typeLineContains", "Snow"),
            token: permanent.definition.is_token,
            keywords: keyword_candidates
                .into_iter()
                .filter(|keyword| self.permanent_has_keyword(permanent, keyword))
                .collect(),
        }
    }

    /// Records the effective characteristics of a permanent about to leave
    /// the battlefield, keyed by its incarnation.
    pub(super) fn capture_departure_characteristics(&mut self, permanent: &CardInstance) {
        let characteristics = self.effective_characteristics(permanent);
        self.departure_characteristics
            .insert(snapshot_key(permanent), characteristics);
    }

    /// The captured characteristics of a departed object incarnation.
    pub(super) fn departed_characteristics(
        &self,
        object: &CardInstance,
    ) -> Option<&EffectiveCharacteristics> {
        self.departure_characteristics.get(&snapshot_key(object))
    }
}
