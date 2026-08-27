use regex::Regex;
use serde_json::{Value, json};

use super::{compare, controller, decision_result, integer, x_variable_expression};

pub(super) fn count_word_pattern() -> &'static str {
    r"a|an|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|(?:twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety)(?:[- ](?:one|two|three|four|five|six|seven|eight|nine))?|\d+"
}

pub(super) fn quantity_word_pattern() -> &'static str {
    r"x|a|an|one|two|three|four|five|six|seven|eight|nine|ten|eleven|twelve|thirteen|fourteen|fifteen|sixteen|seventeen|eighteen|nineteen|(?:twenty|thirty|forty|fifty|sixty|seventy|eighty|ninety)(?:[- ](?:one|two|three|four|five|six|seven|eight|nine))?|\d+"
}

pub(super) fn ordinal_word_pattern() -> &'static str {
    r"first|second|third|fourth|fifth|sixth|seventh|eighth|ninth|tenth|\d+(?:st|nd|rd|th)"
}

pub(super) fn numeric_expression_pattern() -> String {
    let atom = quantity_word_pattern();
    let prefixed =
        format!(r"(?:twice|double|half(?: of)?)\s+(?:{atom})(?:,?\s+rounded\s+(?:up|down))?");
    let operand = format!(r"(?:{prefixed}|{atom})");
    format!(
        r"{operand}(?:\s+(?:plus|minus|times|multiplied by|divided by)\s+{operand}(?:,?\s+rounded\s+(?:up|down))?)*"
    )
}

pub(super) fn parse_number_word(value: &str) -> Option<i64> {
    let normalized = value.trim().to_ascii_lowercase().replace('-', " ");
    let mut words = normalized.split_whitespace();
    let first = words.next()?;
    let first_value = match first {
        "a" | "an" | "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        "ten" => Some(10),
        "eleven" => Some(11),
        "twelve" => Some(12),
        "thirteen" => Some(13),
        "fourteen" => Some(14),
        "fifteen" => Some(15),
        "sixteen" => Some(16),
        "seventeen" => Some(17),
        "eighteen" => Some(18),
        "nineteen" => Some(19),
        "twenty" => Some(20),
        "thirty" => Some(30),
        "forty" => Some(40),
        "fifty" => Some(50),
        "sixty" => Some(60),
        "seventy" => Some(70),
        "eighty" => Some(80),
        "ninety" => Some(90),
        raw => raw.parse::<i64>().ok(),
    }?;
    let Some(second) = words.next() else {
        return Some(first_value);
    };
    if words.next().is_some() {
        return None;
    }
    let second_value = match second {
        "one" => Some(1),
        "two" => Some(2),
        "three" => Some(3),
        "four" => Some(4),
        "five" => Some(5),
        "six" => Some(6),
        "seven" => Some(7),
        "eight" => Some(8),
        "nine" => Some(9),
        _ => None,
    }?;
    (first_value >= 20 && first_value % 10 == 0).then_some(first_value + second_value)
}

pub(super) fn parse_ordinal_word(value: &str) -> Option<i64> {
    match value.trim().to_ascii_lowercase().as_str() {
        "first" => Some(1),
        "second" => Some(2),
        "third" => Some(3),
        "fourth" => Some(4),
        "fifth" => Some(5),
        "sixth" => Some(6),
        "seventh" => Some(7),
        "eighth" => Some(8),
        "ninth" => Some(9),
        "tenth" => Some(10),
        other => other
            .trim_end_matches("st")
            .trim_end_matches("nd")
            .trim_end_matches("rd")
            .trim_end_matches("th")
            .parse::<i64>()
            .ok(),
    }
}

pub(super) fn parse_quantity_expression(value: &str) -> Option<Value> {
    parse_numeric_expression_text(value)
}

pub(super) fn parse_signed_stat_expression(value: &str) -> Option<Value> {
    let value = value.trim();
    let (negative, magnitude) = value
        .strip_prefix('-')
        .map(|magnitude| (true, magnitude))
        .or_else(|| value.strip_prefix('+').map(|magnitude| (false, magnitude)))
        .unwrap_or((false, value));
    let expression = if magnitude.eq_ignore_ascii_case("x") {
        json!({ "kind": "sourceCastXValue" })
    } else {
        integer(parse_number_word(magnitude)?)
    };
    if negative {
        Some(json!({
            "kind": "multiply",
            "left": expression,
            "right": integer(-1),
        }))
    } else {
        Some(expression)
    }
}

pub(super) fn parse_numeric_expression_text(value: &str) -> Option<Value> {
    let value = value.trim().trim_end_matches('.').trim();
    if value.eq_ignore_ascii_case("x") {
        return Some(decision_result("xValue"));
    }
    if let Some(number) = parse_number_word(value) {
        return Some(integer(number));
    }
    let cast_spell_mana_value_sum_re = Regex::new(
        r"(?i)^the total mana value of (other )?spells (?:you've|you have) cast this turn$",
    )
    .expect("cast-spell mana-value sum regex compiles");
    if let Some(captures) = cast_spell_mana_value_sum_re.captures(value) {
        return Some(json!({
            "kind": "sumEventValuesThisTurn",
            "eventKind": "spellCast",
            "detailField": "manaValue",
            "player": controller(),
            "excludeSource": captures.get(1).is_some(),
        }));
    }

    let binary = |kind: &str, left: &str, right: &str| {
        Some(json!({
            "kind": kind,
            "left": parse_numeric_expression_text(left)?,
            "right": parse_numeric_expression_text(right)?,
        }))
    };
    for (pattern, kind) in [
        (r"(?i)^(.+?)\s+plus\s+(.+)$", "add"),
        (r"(?i)^(.+?)\s+minus\s+(.+)$", "subtract"),
        (r"(?i)^(.+?)\s+(?:times|multiplied by)\s+(.+)$", "multiply"),
    ] {
        let captures = Regex::new(pattern)
            .expect("numeric binary operation regex compiles")
            .captures(value);
        if let Some(captures) = captures {
            return binary(kind, captures.get(1)?.as_str(), captures.get(2)?.as_str());
        }
    }

    let divide_re = Regex::new(r"(?i)^(.+?)\s+divided by\s+(.+?)(?:,?\s+rounded\s+(up|down))?$")
        .expect("numeric division regex compiles");
    if let Some(captures) = divide_re.captures(value) {
        return Some(json!({
            "kind": "divide",
            "left": parse_numeric_expression_text(captures.get(1)?.as_str())?,
            "right": parse_numeric_expression_text(captures.get(2)?.as_str())?,
            "round": captures.get(3).map(|round| round.as_str().to_ascii_lowercase()).unwrap_or_else(|| "down".to_string()),
        }));
    }

    let prefix_rounded_re =
        Regex::new(r"(?i)^(twice|double|half(?: of)?)\s+(.+?),?\s+rounded\s+(up|down)$")
            .expect("rounded numeric prefix operation regex compiles");
    if let Some(captures) = prefix_rounded_re.captures(value) {
        let operand = parse_numeric_expression_text(captures.get(2)?.as_str())?;
        if captures[1].to_ascii_lowercase().starts_with("half") {
            return Some(json!({
                "kind": "divide",
                "left": operand,
                "right": integer(2),
                "round": captures.get(3)?.as_str().to_ascii_lowercase(),
            }));
        }
        return Some(json!({
            "kind": "multiply",
            "left": operand,
            "right": integer(2),
        }));
    }

    let prefix_re = Regex::new(r"(?i)^(twice|double|half(?: of)?)\s+(.+)$")
        .expect("numeric prefix operation regex compiles");
    if let Some(captures) = prefix_re.captures(value) {
        let operand = parse_numeric_expression_text(captures.get(2)?.as_str())?;
        if captures[1].to_ascii_lowercase().starts_with("half") {
            return Some(json!({
                "kind": "divide",
                "left": operand,
                "right": integer(2),
                "round": "down",
            }));
        }
        return Some(json!({
            "kind": "multiply",
            "left": operand,
            "right": integer(2),
        }));
    }

    x_variable_expression(value)
}

pub(super) fn parse_numeric_comparison_text(value: &str) -> Option<Value> {
    let comparison_re =
        Regex::new(r"(?i)^(.+?)\s+is\s+(.+?)\s+(or less|or fewer|or greater|or more)$")
            .expect("numeric comparison regex compiles");
    let captures = comparison_re.captures(value.trim())?;
    let operator = match captures.get(3)?.as_str().to_ascii_lowercase().as_str() {
        "or less" | "or fewer" => "<=",
        "or greater" | "or more" => ">=",
        _ => return None,
    };
    Some(compare(
        operator,
        parse_numeric_expression_text(captures.get(1)?.as_str())?,
        parse_numeric_expression_text(captures.get(2)?.as_str())?,
    ))
}
