use regex::Regex;
use serde_json::Value;

pub(crate) fn is_class_type_line(type_line: &str) -> bool {
    type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("Class"))
}

pub(crate) fn class_level_header(text: &str) -> Option<i64> {
    let class_level_re = Regex::new(r"^((?:\{[^}]+\})+): Level ([2-9][0-9]*)$")
        .expect("class level header regex compiles");
    class_level_re
        .captures(text.trim())?
        .get(2)?
        .as_str()
        .parse()
        .ok()
}

pub(crate) fn apply_class_level_requirement(rule: &mut Value, minimum_level: i64) {
    if minimum_level <= 1 {
        return;
    }
    let existing = rule["minimumClassLevel"].as_i64().unwrap_or(1);
    rule["minimumClassLevel"] = Value::from(existing.max(minimum_level));
}
