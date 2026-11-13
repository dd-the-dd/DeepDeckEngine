//! Pure mana-cost parsing and transformation primitives.
//!
//! This module deliberately has no dependency on [`GameState`](super::GameState)
//! or on runtime decisions. It is the shared boundary between schema validation,
//! cost declaration, and payment search.

use regex::Regex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Clone, Debug, Default)]
pub(super) struct ManaRequirement {
    pub(super) colored: BTreeMap<String, i32>,
    pub(super) generic: i32,
    pub(super) hybrid: Vec<Vec<String>>,
    pub(super) phyrexian: BTreeMap<String, i32>,
    pub(super) snow: i32,
}

impl ManaRequirement {
    pub(super) fn add(&mut self, other: &Self) {
        self.generic += other.generic;
        self.snow += other.snow;
        for (symbol, amount) in &other.colored {
            *self.colored.entry(symbol.clone()).or_default() += amount;
        }
        for (symbol, amount) in &other.phyrexian {
            *self.phyrexian.entry(symbol.clone()).or_default() += amount;
        }
        self.hybrid.extend(other.hybrid.iter().cloned());
    }

    pub(super) fn is_empty(&self) -> bool {
        self.generic == 0
            && self.snow == 0
            && self.colored.values().all(|count| *count == 0)
            && self.hybrid.is_empty()
            && self.phyrexian.values().all(|count| *count == 0)
    }

    pub(super) fn symbol_count(&self) -> usize {
        self.generic.max(0) as usize
            + self.snow.max(0) as usize
            + self
                .colored
                .values()
                .map(|count| count.max(&0))
                .sum::<i32>() as usize
            + self.hybrid.len()
            + self
                .phyrexian
                .values()
                .map(|count| count.max(&0))
                .sum::<i32>() as usize
    }

    pub(super) fn as_mana_cost(&self) -> String {
        let mut cost = String::new();
        if self.generic > 0 {
            cost.push_str(&format!("{{{}}}", self.generic));
        }
        for _ in 0..self.snow.max(0) {
            cost.push_str("{S}");
        }
        for symbol in ["W", "U", "B", "R", "G", "C"] {
            for _ in 0..self.colored.get(symbol).copied().unwrap_or(0).max(0) {
                cost.push_str(&format!("{{{symbol}}}"));
            }
        }
        for options in &self.hybrid {
            cost.push_str(&format!("{{{}}}", options.join("/")));
        }
        for symbol in ["W", "U", "B", "R", "G"] {
            for _ in 0..self.phyrexian.get(symbol).copied().unwrap_or(0).max(0) {
                cost.push_str(&format!("{{{symbol}/P}}"));
            }
        }
        cost
    }

    pub(super) fn convoke_reductions(&self, colors: &BTreeSet<String>) -> Vec<Self> {
        let mut reductions = Vec::new();
        if self.generic > 0 {
            let mut reduced = self.clone();
            reduced.generic -= 1;
            reductions.push(reduced);
        }
        for color in colors {
            if self.colored.get(color).copied().unwrap_or(0) > 0 {
                let mut reduced = self.clone();
                *reduced.colored.entry(color.clone()).or_default() -= 1;
                reductions.push(reduced);
            }
            if self.phyrexian.get(color).copied().unwrap_or(0) > 0 {
                let mut reduced = self.clone();
                *reduced.phyrexian.entry(color.clone()).or_default() -= 1;
                reductions.push(reduced);
            }
            for (index, options) in self.hybrid.iter().enumerate() {
                if options.contains(color) {
                    let mut reduced = self.clone();
                    reduced.hybrid.remove(index);
                    reductions.push(reduced);
                }
            }
        }
        let mut seen = BTreeSet::new();
        reductions
            .into_iter()
            .filter(|requirement| seen.insert(requirement.as_mana_cost()))
            .collect()
    }
}

pub(super) fn parse_mana_symbols(value: &str) -> Vec<String> {
    static MANA_SYMBOL_RE: OnceLock<Regex> = OnceLock::new();
    MANA_SYMBOL_RE
        .get_or_init(|| Regex::new(r"\{([^}]+)\}").expect("mana symbol regex compiles"))
        .captures_iter(value)
        .map(|captures| captures[1].to_ascii_uppercase())
        .collect()
}

/// Substitutes the activation's declared X before payment search. An absent,
/// negative, or out-of-range producer is not an implicit zero payment.
/// This consumes canonical mana-cost symbols, never Oracle sentence text.
pub(super) fn bind_mana_cost_x(value: &str, x_value: Option<i64>) -> Option<String> {
    parse_mana_symbols(value)
        .into_iter()
        .map(|symbol| {
            if symbol == "X" {
                let amount = i32::try_from(x_value?).ok().filter(|amount| *amount >= 0)?;
                Some(format!("{{{amount}}}"))
            } else {
                Some(format!("{{{symbol}}}"))
            }
        })
        .collect()
}

pub(super) fn color_name_to_symbol(color: &str) -> Option<&'static str> {
    match color.to_ascii_lowercase().as_str() {
        "white" => Some("W"),
        "blue" => Some("U"),
        "black" => Some("B"),
        "red" => Some("R"),
        "green" => Some("G"),
        _ => None,
    }
}

pub(super) fn parse_mana_requirement(value: &str) -> Option<ManaRequirement> {
    let mut requirement = ManaRequirement::default();
    for symbol in parse_mana_symbols(value) {
        if let Ok(amount) = symbol.parse::<i32>() {
            requirement.generic += amount;
        } else if ["W", "U", "B", "R", "G", "C"].contains(&symbol.as_str()) {
            *requirement.colored.entry(symbol).or_default() += 1;
        } else if symbol == "S" {
            requirement.snow += 1;
        } else if let Some(color) = symbol.strip_suffix("/P")
            && ["W", "U", "B", "R", "G"].contains(&color)
        {
            *requirement.phyrexian.entry(color.to_string()).or_default() += 1;
        } else if symbol.contains('/') {
            let options = symbol
                .split('/')
                .filter(|option| ["W", "U", "B", "R", "G"].contains(option))
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if options.len() < 2 || options.len() != symbol.split('/').count() {
                return None;
            }
            requirement.hybrid.push(options);
        } else if symbol == "X" {
            continue;
        } else {
            return None;
        }
    }
    Some(requirement)
}

pub(super) fn mana_cost_as_generic(value: &str) -> String {
    format!("{{{}}}", mana_cost_mana_amount(value))
}

pub(super) fn mana_cost_mana_amount(value: &str) -> i32 {
    parse_mana_symbols(value)
        .into_iter()
        .map(|symbol| symbol.parse::<i32>().unwrap_or(1))
        .sum::<i32>()
        .max(0)
}

pub(super) fn mana_cost_generic_amount(value: &str) -> i32 {
    parse_mana_symbols(value)
        .into_iter()
        .filter_map(|symbol| symbol.parse::<i32>().ok())
        .sum::<i32>()
        .max(0)
}

pub(super) fn reduce_generic_mana_cost(value: &str, reduction: i32) -> String {
    let mut generic = 0;
    let mut non_generic = Vec::new();
    for symbol in parse_mana_symbols(value) {
        if let Ok(amount) = symbol.parse::<i32>() {
            generic += amount;
        } else {
            non_generic.push(symbol);
        }
    }
    let mut reduced = String::new();
    let remaining = (generic - reduction.max(0)).max(0);
    if remaining > 0 {
        reduced.push_str(&format!("{{{remaining}}}"));
    }
    for symbol in non_generic {
        reduced.push_str(&format!("{{{symbol}}}"));
    }
    reduced
}

pub(super) fn reduce_mana_cost_by_mana_cost(value: &str, reduction: &str) -> String {
    let Some(mut remaining) = parse_mana_requirement(value) else {
        return value.to_string();
    };
    let Some(reduction) = parse_mana_requirement(reduction) else {
        return value.to_string();
    };
    remaining.generic = (remaining.generic - reduction.generic).max(0);
    remaining.snow = (remaining.snow - reduction.snow).max(0);
    for (symbol, amount) in reduction.colored {
        let remaining_amount = remaining.colored.entry(symbol).or_default();
        *remaining_amount = (*remaining_amount - amount).max(0);
    }
    for (symbol, amount) in reduction.phyrexian {
        let remaining_amount = remaining.phyrexian.entry(symbol).or_default();
        *remaining_amount = (*remaining_amount - amount).max(0);
    }
    for hybrid in reduction.hybrid {
        if let Some(index) = remaining
            .hybrid
            .iter()
            .position(|current| current == &hybrid)
        {
            remaining.hybrid.remove(index);
        }
    }
    remaining.as_mana_cost()
}
