use regex::Regex;

use super::AbilityInput;

pub(crate) fn activation_parts(text: &str) -> Option<(&str, &str)> {
    let text = text
        .split_once(" — ")
        .map(|(_, activation)| activation)
        .unwrap_or(text);
    let activation_re = Regex::new(
        r"(?i)^\s*(?:\(\s*)?(?:(?:\{[^}]+\})+|Pay \d+ life)(?:\s*,\s*(?:(?:\{[^}]+\})+|Pay \d+ life))*\s*:",
    )
    .expect("activation prefix regex compiles");
    if let Some(matched) = activation_re.find(text) {
        return Some((&text[..matched.end() - 1], &text[matched.end()..]));
    }
    let mut parenthesis_depth = 0_u32;
    let mut quoted = false;
    let mut separator = None;
    for (index, character) in text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => parenthesis_depth += 1,
            ')' if !quoted => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            ':' if !quoted && parenthesis_depth == 0 => {
                separator = Some(index);
                break;
            }
            _ => {}
        }
    }
    let separator = separator?;
    let cost = &text[..separator];
    let effect = &text[separator + 1..];
    let lower = cost.trim().to_ascii_lowercase();
    (lower.starts_with("sacrifice ")
        || lower.starts_with("remove ")
        || lower.starts_with("equip—")
        || lower.contains(", sacrifice "))
    .then_some((cost, effect))
}

pub(crate) fn classify_ability(input: &AbilityInput<'_>) -> &'static str {
    let text = input.source.text.trim();
    let normalized = text.trim_start_matches('(').trim_start();
    let lower = normalized.to_ascii_lowercase();

    if activation_parts(text).is_some() {
        return "activatedAbility";
    }
    if lower.starts_with("when ") || lower.starts_with("whenever ") || lower.starts_with("at ") {
        return "triggeredAbility";
    }
    let standalone_enters_with = !lower.starts_with("graft ")
        && lower
            .split_once(" enters with ")
            .is_some_and(|(subject, _)| !subject.contains('.'));
    let standalone_enters_tapped = lower
        .strip_suffix(" enters tapped.")
        .is_some_and(|subject| !subject.contains('.'));
    if Regex::new(r"(?i)^as .+ enters,")
        .expect("as-enters classifier regex compiles")
        .is_match(normalized)
        || lower.contains(" enters tapped unless ")
        || standalone_enters_with
        || standalone_enters_tapped
        || lower.contains(" enters prepared")
    {
        return "replacementEffect";
    }
    let keyword_list_candidate = (lower.contains(",") || lower.contains(" and "))
        && (Regex::new(
            r"(?i)^(changeling|deathtouch|defender|devoid|double strike|first strike|flash|flying|haste|hexproof|indestructible|lifelink|menace|myriad|prowess|reach|trample|vigilance)\b|^firebending\s+(\d+|x\b)|^mobilize\s+\d+|^ward(\s+\{|—|â€”|Ã¢â‚¬â€)",
        )
        .expect("keyword list classifier regex compiles")
        .is_match(normalized)
            || Regex::new(r"(?i)^blitz\s+\{")
                .expect("blitz keyword list classifier regex compiles")
                .is_match(normalized));
    if keyword_list_candidate {
        return "keywordAbilityGroup";
    }
    let firebending_keyword_candidate = Regex::new(r"(?i)^firebending\s+(\d+|x\b)")
        .expect("firebending keyword classifier regex compiles")
        .is_match(normalized);
    if lower == "flying"
        || lower.starts_with("graft ")
        || lower.starts_with("kicker ")
        || lower.starts_with("blitz ")
        || lower.starts_with("ward")
        || firebending_keyword_candidate
        || lower.starts_with("offspring ")
        || lower.starts_with("paradigm ")
    {
        return "keywordAbility";
    }
    if lower.starts_with("this spell can't be countered")
        || lower.starts_with("activated abilities of sources")
        || lower.starts_with("lands with the chosen name")
        || lower.starts_with("creatures entering don't cause")
    {
        return "staticAbility";
    }
    if input
        .face_type_line
        .split(|character: char| !character.is_alphanumeric())
        .any(|part| part.eq_ignore_ascii_case("instant") || part.eq_ignore_ascii_case("sorcery"))
    {
        return "spellAbility";
    }
    "staticAbility"
}
