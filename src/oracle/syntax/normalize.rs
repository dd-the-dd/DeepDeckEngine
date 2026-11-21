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
