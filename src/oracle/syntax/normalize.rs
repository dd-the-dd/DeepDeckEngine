pub(crate) fn strip_short_oracle_label(instruction: &str) -> &str {
    if let Some((label, effect)) = instruction
        .split_once(" \u{2014} ")
        .or_else(|| instruction.split_once(" \u{2013} "))
        .or_else(|| instruction.split_once(" - "))
        && !label.contains('.')
        && label.split_whitespace().count() <= 5
    {
        return effect.trim();
    }
    for separator in [" â€” ", " — ", " Ã¢â‚¬â€ "] {
        if let Some((label, effect)) = instruction.split_once(separator)
            && !label.contains('.')
            && label.split_whitespace().count() <= 5
        {
            return effect.trim();
        }
    }
    instruction
}

pub(super) fn normalize_oracle_encoding(text: &str) -> String {
    text.replace("â€”", "—")
        .replace("â€¢", "•")
        .replace("âˆ’", "−")
        .replace("Ã¢â\u{0082}¬â\u{0080}\u{009d}", "—")
        .replace("Ã¢â\u{0082}¬Â¢", "•")
        .replace("Ã¢Ë\u{0086}â\u{0080}\u{0099}", "−")
        .replace("â\u{0080}\u{0094}", "—")
        .replace("â\u{0080}¢", "•")
        .replace("â\u{0088}\u{0092}", "−")
}
