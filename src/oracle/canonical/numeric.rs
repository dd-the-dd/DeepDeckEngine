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
