use super::super::model::RecognizedEntity;
use crate::oracle::ability_catalog::{
    AbilityCategory, AbilityLexemeMatch, match_catalog_mechanic, match_longest_exact,
};
use crate::oracle::characteristic_vocabulary::{AcceptedForms, lookup_creature_subtype_tokens};

#[path = "token_vocabulary.rs"]
mod token_vocabulary;

#[cfg(test)]
thread_local! {
    static ORACLE_TOKENIZATION_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

fn is_quantity_token(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase().replace('-', " ");
    if normalized == "x" || normalized.parse::<i64>().is_ok() {
        return true;
    }
    let mut words = normalized.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let first_is_quantity = matches!(
        first,
        "a" | "an"
            | "one"
            | "two"
            | "three"
            | "four"
            | "five"
            | "six"
            | "seven"
            | "eight"
            | "nine"
            | "ten"
            | "eleven"
            | "twelve"
            | "thirteen"
            | "fourteen"
            | "fifteen"
            | "sixteen"
            | "seventeen"
            | "eighteen"
            | "nineteen"
            | "twenty"
            | "thirty"
            | "forty"
            | "fifty"
            | "sixty"
            | "seventy"
            | "eighty"
            | "ninety"
    );
    if !first_is_quantity {
        return false;
    }
    let Some(second) = words.next() else {
        return true;
    };
    words.next().is_none()
        && matches!(
            second,
            "one" | "two" | "three" | "four" | "five" | "six" | "seven" | "eight" | "nine"
        )
}

fn is_ordinal_token(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "first"
            | "second"
            | "third"
            | "fourth"
            | "fifth"
            | "sixth"
            | "seventh"
            | "eighth"
            | "ninth"
            | "tenth"
    ) {
        return true;
    }
    ["st", "nd", "rd", "th"].iter().any(|suffix| {
        normalized.strip_suffix(suffix).is_some_and(|digits| {
            !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
        })
    })
}

fn token_type(raw: &str) -> (&'static str, &'static str) {
    token_vocabulary::classify(raw)
}

fn is_object_type_word(normalized: &str) -> bool {
    token_vocabulary::OBJECT_TYPE_WORDS.contains(&normalized)
}

fn is_power_toughness_value(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    let mut position = usize::from(matches!(bytes.first(), Some(b'+') | Some(b'-')));
    let left_start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    if position == left_start || bytes.get(position) != Some(&b'/') {
        return false;
    }
    position += 1;
    position += usize::from(matches!(bytes.get(position), Some(b'+') | Some(b'-')));
    let right_start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    position > right_start && position == bytes.len()
}

pub(crate) fn recognize_entities(text: &str) -> Vec<RecognizedEntity> {
    let mut entities = scan_token_ranges(text)
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| {
            let raw = &text[start..end];
            let (category, entity_type) = token_type(raw);
            RecognizedEntity {
                category: category.to_string(),
                index,
                raw: raw.to_string(),
                span_start: start,
                span_end: end,
                token_type: entity_type.to_string(),
            }
        })
        .collect::<Vec<_>>();
    classify_creature_subtype_lexemes(&mut entities);
    classify_ability_catalog_lexemes(&mut entities);
    classify_flavor_word_label(&mut entities);
    entities
}

fn classify_creature_subtype_lexemes(tokens: &mut [RecognizedEntity]) {
    let mut position = 0;
    while position < tokens.len() {
        let Some((_, width)) = lookup_creature_subtype_tokens(
            &tokens[position].raw,
            tokens.get(position + 1).map(|token| token.raw.as_str()),
            AcceptedForms::SingularOrPlural,
        ) else {
            position += 1;
            continue;
        };
        let lexically_unclaimed = tokens[position..position + width]
            .iter()
            .all(|token| matches!(token.token_type.as_str(), "unknown" | "subtypeWord"));
        let exact_type_context = tokens[position..position + width]
            .iter()
            .all(|token| token.category != "boundary" && token.token_type != "separator")
            && tokens.get(position + width).is_some_and(|token| {
                token.token_type == "typeWord"
                    && matches!(
                        token.raw.to_ascii_lowercase().as_str(),
                        "creature" | "creatures"
                    )
            });
        if !lexically_unclaimed && !exact_type_context {
            position += 1;
            continue;
        }
        for token in &mut tokens[position..position + width] {
            token.category = "entityReference".to_string();
            token.token_type = "subtypeWord".to_string();
        }
        position += width;
    }
}

fn classify_ability_catalog_lexemes(tokens: &mut [RecognizedEntity]) {
    let mut position = 0;
    while position < tokens.len() {
        let Some(matched) = match_longest_exact(tokens, position) else {
            position += 1;
            continue;
        };
        let eligible = match matched.entry.category {
            AbilityCategory::KeywordAction => keyword_action_position(tokens, matched),
            AbilityCategory::AbilityWord => ability_word_position(tokens, matched),
            AbilityCategory::KeywordAbility => keyword_ability_position(tokens, matched),
            AbilityCategory::Marker | AbilityCategory::StaticOrCostMechanic => false,
        };
        // An ordinal inside an exact keyword-ability name (`first strike`) is
        // part of that name, not an independent ordinal reference.
        let contains_specialized_token =
            tokens[matched.start_token..matched.end_token]
                .iter()
                .any(|token| {
                    token.category != "unknown"
                        && !(matched.entry.category == AbilityCategory::KeywordAbility
                            && token.token_type == "ordinalWord")
                });
        // An exact ability-word span at ability start followed by an em dash is
        // a stronger contextual marker than the individual token classes. This
        // matters for official labels such as `descend 4`, `join forces`, and
        // `will of the council`, whose component tokens are otherwise quantity,
        // action, relation, or determiner vocabulary.
        let contextual_ability_word =
            eligible && matched.entry.category == AbilityCategory::AbilityWord;
        if eligible && (!contains_specialized_token || contextual_ability_word) {
            let token_type = match matched.entry.category {
                AbilityCategory::KeywordAction => "actionWord",
                AbilityCategory::AbilityWord => "abilityWord",
                AbilityCategory::KeywordAbility => "keywordWord",
                AbilityCategory::Marker | AbilityCategory::StaticOrCostMechanic => unreachable!(),
            };
            for token in &mut tokens[matched.start_token..matched.end_token] {
                token.category = "magicVocabulary".to_string();
                token.token_type = token_type.to_string();
            }
        }
        position = matched.end_token.max(position + 1);
    }
}

/// CR 207.2c: an italic label before an em dash at the start of an ability is
/// an ability word or a flavor word, and has no rules meaning. Official
/// ability words are already classified from the catalogue; any other
/// capitalized label is recognized by its position, not by its words:
///
/// ```text
/// FlavorLabel := LabelWord (LabelWord | "!" | "?" | ",")* "—" Body
/// ```
///
/// A label of plain words that the lexicon does not know is a flavor word
/// before any body. A label whose words the lexicon also knows (`Exile
/// Cannon`, `Warp Blast`, `Exterminate!`) is a flavor word only before a
/// trigger word (CR 603.1), the one position where a label cannot be a cost
/// or a mode header. A label that is exactly the name of a catalog mechanic
/// (`Exhaust`, `Max speed`, `Kicker`, `Equip`) has rules even when the
/// lexicon has no grammar for it yet (CR 702), and Saga chapter numerals
/// number chapter abilities (CR 714.2b): neither is ever a flavor word, so a
/// mechanic with rules is never silently dropped.
fn classify_flavor_word_label(tokens: &mut [RecognizedEntity]) {
    const MAX_LABEL_TOKENS: usize = 8;
    let Some(dash) = tokens
        .iter()
        .take(MAX_LABEL_TOKENS + 1)
        .position(|token| token.raw == "\u{2014}")
    else {
        return;
    };
    if dash == 0
        || tokens[..dash]
            .iter()
            .all(|token| token.token_type == "abilityWord")
        || !tokens[0]
            .raw
            .starts_with(|ch: char| ch.is_ascii_uppercase() || ch.is_ascii_digit())
    {
        return;
    }
    let label = &tokens[..dash];
    let label_word = |token: &RecognizedEntity| {
        token.raw.chars().any(char::is_alphanumeric)
            && token.raw.chars().all(|ch| {
                ch.is_alphanumeric() || matches!(ch, '\'' | '\u{2019}' | '-' | '.' | '!' | ',')
            })
    };
    // Exclamation and question marks, and the comma of a written number or
    // an apposition, are punctuation inside the label's own words.
    let label_punctuation =
        |token: &RecognizedEntity| matches!(token.raw.as_str(), "!" | "?" | ",");
    let well_formed = label_word(&label[0])
        && label
            .iter()
            .all(|token| label_word(token) || label_punctuation(token))
        // A label may end with `!` or `?`, never with a comma.
        && label[dash - 1].raw != ",";
    // `I, II —`: the Roman numerals of a Saga chapter list (CR 714.2b).
    let chapter_numerals = label.iter().all(|token| {
        token.raw == ","
            || (!token.raw.is_empty() && token.raw.chars().all(|ch| matches!(ch, 'I' | 'V' | 'X')))
    });
    let exact_mechanic =
        match_catalog_mechanic(tokens, 0).is_some_and(|mechanic| mechanic.end_token == dash);
    // A trigger word opens an ability's own event clause, never its label.
    let opens_with_trigger_word = ["at", "when", "whenever"]
        .iter()
        .any(|word| label[0].raw.eq_ignore_ascii_case(word));
    if !well_formed || chapter_numerals || exact_mechanic || opens_with_trigger_word {
        return;
    }
    // A label of words only; punctuation inside it is read only before a
    // trigger word, where no cost or mode header can stand.
    let plain_words = label.iter().all(|token| {
        token.token_type != "separator"
            && token.category != "boundary"
            && !label_punctuation(token)
            && token.raw.chars().any(|ch| ch.is_alphabetic())
    });
    let catalogue_lexeme =
        (0..dash).any(|position| match_longest_exact(tokens, position).is_some());
    let typed_mechanic = label.iter().any(|token| {
        matches!(
            token.token_type.as_str(),
            "keywordWord" | "actionWord" | "abilityWord"
        )
    });
    let lexically_plain = plain_words && !catalogue_lexeme && !typed_mechanic;
    let before_trigger_word = tokens.get(dash + 1).is_some_and(|token| {
        ["at", "when", "whenever"]
            .iter()
            .any(|word| token.raw.eq_ignore_ascii_case(word))
    });
    if !lexically_plain && !before_trigger_word {
        return;
    }
    for token in &mut tokens[..dash] {
        token.category = "magicVocabulary".to_string();
        token.token_type = "abilityWord".to_string();
    }
}

fn keyword_action_position(tokens: &[RecognizedEntity], matched: AbilityLexemeMatch) -> bool {
    if matched.start_token == 0 {
        return true;
    }
    let normalized_lexeme = matched.lexeme.to_ascii_lowercase();
    let looks_participial = normalized_lexeme.ends_with("ed")
        || normalized_lexeme.ends_with("ing")
        || normalized_lexeme == "waterbent";
    if looks_participial
        && tokens
            .get(matched.end_token)
            .is_some_and(|token| token.token_type == "typeWord")
    {
        return false;
    }

    let previous = &tokens[matched.start_token - 1];
    // A results-table row (`10—19 | Scry 2.`) begins its instruction after
    // the row separator, like a sentence start.
    if previous.raw == "|" {
        return true;
    }
    // A player role (`its controller scries 2`, `its owner investigates`) is
    // a player subject before its verb like a player type word (CR 109.5).
    matches!(
        previous.token_type.as_str(),
        "referenceWord"
            | "typeWord"
            | "playerRoleWord"
            | "playerQualifierWord"
            | "modalityWord"
            | "logicWord"
            | "predicateWord"
            | "separator"
    )
}

fn ability_word_position(tokens: &[RecognizedEntity], matched: AbilityLexemeMatch) -> bool {
    matched.start_token == 0
        && tokens
            .get(matched.end_token)
            .is_some_and(|token| token.raw == "\u{2014}")
}

fn keyword_ability_position(tokens: &[RecognizedEntity], matched: AbilityLexemeMatch) -> bool {
    if matched.start_token == 0 {
        return true;
    }
    let previous = tokens[matched.start_token - 1].raw.to_ascii_lowercase();
    if previous == "its"
        && tokens
            .get(matched.end_token)
            .is_some_and(|token| token.raw.eq_ignore_ascii_case("cost"))
    {
        return true;
    }
    if previous == "." || previous == ";" {
        return true;
    }
    // A grant's opening quote establishes a local keyword-body position.
    // The exact ability catalogue resolves the lexeme; quoted names and
    // arbitrary quoted rules do not acquire a global keyword word class.
    if previous == "\""
        && matched.start_token >= 2
        && tokens.get(matched.start_token - 2).is_some_and(|token| {
            token.raw.eq_ignore_ascii_case("have") || token.raw.eq_ignore_ascii_case("has")
        })
    {
        return true;
    }
    if ["gain", "gains", "has", "have", "lose", "loses", "with"].contains(&previous.as_str()) {
        return true;
    }
    if previous != "," && previous != "and" {
        return false;
    }
    let clause_start = tokens[..matched.start_token]
        .iter()
        .rposition(|token| matches!(token.raw.as_str(), "." | ":" | ";"))
        .map_or(0, |index| index + 1);
    tokens[clause_start..matched.start_token]
        .iter()
        .any(|token| token.token_type == "keywordWord")
}

/// The sole production entry point for tokenizing an Oracle ability.
/// Downstream audit and automata receive the resulting immutable token slice.
pub(crate) fn recognize_oracle_entities(text: &str) -> Vec<RecognizedEntity> {
    #[cfg(test)]
    ORACLE_TOKENIZATION_CALLS.with(|calls| calls.set(calls.get() + 1));
    recognize_entities(text)
}

#[cfg(test)]
pub(crate) fn reset_oracle_tokenization_count() {
    ORACLE_TOKENIZATION_CALLS.with(|calls| calls.set(0));
}

#[cfg(test)]
pub(crate) fn oracle_tokenization_count() -> usize {
    ORACLE_TOKENIZATION_CALLS.with(std::cell::Cell::get)
}

fn scan_token_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut position = 0;
    while position < text.len() {
        let Some(character) = text[position..].chars().next() else {
            break;
        };
        let character_end = position + character.len_utf8();

        if character.is_whitespace() {
            position = character_end;
            continue;
        }
        if character == '{' {
            if let Some(relative_end) = text[character_end..].find('}') {
                let end = character_end + relative_end + 1;
                ranges.push((position, end));
                position = end;
                continue;
            }
            ranges.push((position, character_end));
            position = character_end;
            continue;
        }
        if matches!(
            character,
            '.' | ',' | ':' | ';' | '\u{2022}' | '(' | ')' | '"'
        ) {
            ranges.push((position, character_end));
            position = character_end;
            continue;
        }
        if matches!(character, '+' | '-') {
            if let Some(end) = scan_power_toughness(text, position) {
                ranges.push((position, end));
                position = end;
                continue;
            }
            ranges.push((position, character_end));
            position = character_end;
            continue;
        }
        if character.is_alphanumeric() {
            let mut end = character_end;
            for (relative, candidate) in text[character_end..].char_indices() {
                // A slash before a signed right component is a structural
                // pair boundary, independent of the left expression's words.
                // Compact literal pairs were already consumed above.
                if candidate == '/'
                    && text
                        .as_bytes()
                        .get(character_end + relative + 1)
                        .is_some_and(|next| matches!(next, b'+' | b'-'))
                {
                    break;
                }
                if candidate.is_alphanumeric() || matches!(candidate, '\'' | '\u{2019}' | '/' | '-')
                {
                    end = character_end + relative + candidate.len_utf8();
                } else {
                    break;
                }
            }
            // A symbolic stat pair (`X/X`, `1/X`, `X/2`, CR 107.3) is three
            // tokens, component, separator, component, like the signed and
            // literal pairs the stat grammar reads; a pair of two literals
            // stays one compact `counterValue` token.
            if let Some(slash) = symbolic_stat_pair_slash(&text[position..end]) {
                ranges.push((position, position + slash));
                ranges.push((position + slash, position + slash + 1));
                ranges.push((position + slash + 1, end));
                position = end;
                continue;
            }
            ranges.push((position, end));
            position = end;
            continue;
        }
        // Unknown input is still a token. The lexical layer must be lossless so
        // a later automaton can either resolve it from context (for example a
        // card name) or report its exact source span.
        ranges.push((position, character_end));
        position = character_end;
    }
    ranges
}

/// The byte offset of the slash of `COMPONENT/COMPONENT` where each
/// component is the variable `X` or a digit literal and at least one is `X`.
fn symbolic_stat_pair_slash(word: &str) -> Option<usize> {
    let (left, right) = word.split_once('/')?;
    let component = |part: &str| {
        part.eq_ignore_ascii_case("x")
            || (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
    };
    let variable = left.eq_ignore_ascii_case("x") || right.eq_ignore_ascii_case("x");
    (component(left) && component(right) && variable).then_some(left.len())
}

fn scan_power_toughness(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut position = start;
    if matches!(bytes.get(position), Some(b'+') | Some(b'-')) {
        position += 1;
    }
    let left_start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    if position == left_start || bytes.get(position) != Some(&b'/') {
        return None;
    }
    position += 1;
    if matches!(bytes.get(position), Some(b'+') | Some(b'-')) {
        position += 1;
    }
    let right_start = position;
    while bytes.get(position).is_some_and(u8::is_ascii_digit) {
        position += 1;
    }
    (position > right_start).then_some(position)
}

#[cfg(test)]
#[path = "tokenizer_quoted_keyword_tests.rs"]
mod quoted_keyword_tests;
