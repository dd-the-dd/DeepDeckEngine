use super::super::model::RecognizedEntity;
use crate::oracle::ability_catalog::{AbilityCategory, AbilityLexemeMatch, match_longest_exact};
use crate::oracle::characteristic_vocabulary::{
    AcceptedForms, ObjectCharacteristic, lookup_amass_army_subtype, lookup_creature_subtype_tokens,
    lookup_object_characteristic,
};

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
    let normalized = raw.to_ascii_lowercase();
    if raw.starts_with('{') && raw.ends_with('}') && raw.len() >= 3 {
        if normalized == "{e}" {
            return ("magicVocabulary", "counterSymbol");
        }
        if normalized == "{t}" || normalized == "{q}" {
            return ("cost", "tapSymbol");
        }
        return ("cost", "manaSymbol");
    }
    if is_ordinal_token(raw) {
        return ("logic", "ordinalWord");
    }
    if ["when", "whenever", "at"].contains(&normalized.as_str()) {
        return ("trigger", "triggerWord");
    }
    if [
        "die", "dies", "died", "enter", "enters", "entered", "entering", "leave", "leaves", "left",
        "attacks",
    ]
    .contains(&normalized.as_str())
    {
        return ("trigger", "eventWord");
    }
    if normalized == "cause" {
        return ("trigger", "causationWord");
    }
    if normalized == "trigger" {
        return ("trigger", "triggerActionWord");
    }
    if [
        "if", "unless", "and", "or", "not", "then", "instead", "don't", "either",
    ]
    .contains(&normalized.as_str())
    {
        return ("logic", "logicWord");
    }
    if normalized == "except" {
        return ("logic", "exceptionWord");
    }
    if ["number", "up"].contains(&normalized.as_str()) {
        return ("logic", "cardinalityWord");
    }
    if ["half", "plus", "minus", "times", "multiplied", "divided"].contains(&normalized.as_str()) {
        return ("logic", "numericOperatorWord");
    }
    if normalized == "rounded" {
        return ("logic", "roundingWord");
    }
    if normalized == "down" {
        return ("syntax", "directionWord");
    }
    if ["can", "may"].contains(&normalized.as_str()) {
        return ("logic", "modalityWord");
    }
    if normalized == "would" {
        return ("logic", "replacementModalWord");
    }
    if ["rather", "than"].contains(&normalized.as_str()) {
        return ("logic", "substitutionWord");
    }
    if normalized == "without" {
        return ("cost", "costOmissionWord");
    }
    if normalized == "the" {
        return ("logic", "determinerWord");
    }
    if normalized == "there" {
        return ("logic", "existentialWord");
    }
    if ["a", "an", "all", "another", "any", "each", "every"].contains(&normalized.as_str()) {
        return ("logic", "quantifierWord");
    }
    if [
        "this", "that", "those", "her", "his", "him", "it", "it's", "its", "you", "your", "they",
        "they're", "them", "their", "that's", "that’s", "other",
    ]
    .contains(&normalized.as_str())
    {
        return ("entityReference", "referenceWord");
    }
    if normalized == "target" {
        return ("entityReference", "targetMarker");
    }
    if normalized == "targets" {
        return ("entityReference", "targetNoun");
    }
    if [
        "commander's",
        "commander’s",
        "owner's",
        "owner’s",
        "opponent's",
        "opponent’s",
        "player's",
        "player’s",
    ]
    .contains(&normalized.as_str())
    {
        return ("entityReference", "possessivePlayerRoleWord");
    }
    if normalized
        .strip_suffix("'s")
        .is_some_and(is_object_type_word)
    {
        return ("entityReference", "possessiveTypeWord");
    }
    if normalized == "card's" {
        return ("entityReference", "possessiveTypeWord");
    }
    if normalized == "spell's" || normalized == "spell’s" {
        return ("entityReference", "possessiveTypeWord");
    }
    if normalized == "turn's" || normalized == "turn’s" {
        return ("syntax", "possessiveTurnWord");
    }
    if normalized == "basic" || normalized == "legendary" {
        return ("entityReference", "supertypeWord");
    }
    if normalized == "nonbasic" {
        return ("entityReference", "negativeSupertypeWord");
    }
    if normalized
        .strip_prefix("non")
        .is_some_and(is_object_type_word)
    {
        return ("entityReference", "negativeTypeWord");
    }
    if normalized
        .strip_prefix("non")
        .is_some_and(|raw| lookup_object_characteristic(raw, AcceptedForms::SingularOnly).is_some())
    {
        return ("entityReference", "negativeCharacteristicWord");
    }
    if is_object_type_word(&normalized) {
        return ("entityReference", "typeWord");
    }
    if lookup_object_characteristic(raw, AcceptedForms::SingularOrPlural)
        == Some(ObjectCharacteristic::Commander)
    {
        return ("entityReference", "objectCharacteristicWord");
    }
    if ["plains", "island", "swamp", "mountain", "forest"].contains(&normalized.as_str()) {
        return ("entityReference", "subtypeWord");
    }
    if lookup_amass_army_subtype(raw, AcceptedForms::SingularOrPlural).is_some() {
        return ("entityReference", "subtypeWord");
    }
    if [
        "add",
        "activate",
        "amass",
        "attach",
        "attack",
        "block",
        "becomes",
        "cast",
        "casting",
        "casts",
        "choose",
        "chooses",
        "copy",
        "counter",
        "countered",
        "create",
        "creates",
        "deal",
        "deals",
        "destroy",
        "change",
        "discard",
        "discards",
        "draw",
        "draws",
        "exile",
        "exiled",
        "exiles",
        "explore",
        "explores",
        "gain",
        "gains",
        "get",
        "gets",
        "investigate",
        "look",
        "lose",
        "loses",
        "mill",
        "mills",
        "play",
        "plays",
        "produce",
        "put",
        "putting",
        "return",
        "reveal",
        "reveals",
        "sacrifice",
        "search",
        "searches",
        "shuffle",
        "shuffles",
    ]
    .contains(&normalized.as_str())
    {
        return ("magicVocabulary", "actionWord");
    }
    if normalized == "while" {
        return ("syntax", "temporalConnectorWord");
    }
    if normalized == "could" {
        return ("logic", "modalWord");
    }
    if normalized == "kicked" {
        return ("magicVocabulary", "keywordStateWord");
    }
    if ["activated", "triggered"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "abilityKindWord");
    }
    if [
        "ability",
        "abilities",
        "cards",
        "copies",
        "counters",
        "emblem",
        "source",
        "sources",
        "token",
        "tokens",
    ]
    .contains(&normalized.as_str())
    {
        return ("magicVocabulary", "objectWord");
    }
    if [
        "also",
        "alone",
        "back",
        "matter",
        "new",
        "next",
        "only",
        "random",
        "still",
        "transformed",
        "way",
    ]
    .contains(&normalized.as_str())
    {
        return ("magicVocabulary", "qualifierWord");
    }
    if normalized == "until" {
        return ("syntax", "durationBoundaryWord");
    }
    if normalized == "end" {
        return ("syntax", "durationPointWord");
    }
    if ["top", "bottom"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "zonePositionWord");
    }
    if normalized == "order" {
        return ("magicVocabulary", "collectionOrderWord");
    }
    if normalized == "rest" {
        return ("magicVocabulary", "collectionRemainderWord");
    }
    if ["time", "turn", "turns"].contains(&normalized.as_str()) {
        return ("logic", "repetitionUnitWord");
    }
    if ["step", "steps"].contains(&normalized.as_str()) {
        return ("syntax", "turnStepWord");
    }
    if normalized == "upkeep" {
        return ("syntax", "turnStepWord");
    }
    if normalized == "beginning" {
        return ("syntax", "temporalMarkerWord");
    }
    if ["pay", "paid", "paying", "pays", "spend"].contains(&normalized.as_str()) {
        return ("cost", "costActionWord");
    }
    if ["additional", "mana", "cost", "costs"].contains(&normalized.as_str()) {
        return ("cost", "costWord");
    }
    if ["white", "blue", "black", "red", "green", "colorless"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "colorWord");
    }
    if normalized == "color" {
        return ("magicVocabulary", "colorNoun");
    }
    if normalized == "identity" {
        return ("magicVocabulary", "identityWord");
    }
    if ["battlefield", "graveyard", "graveyards", "hand", "library"].contains(&normalized.as_str())
    {
        return ("magicVocabulary", "zoneWord");
    }
    if normalized == "anywhere" {
        return ("magicVocabulary", "zoneScopeWord");
    }
    if ["prepared", "tapped"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "stateWord");
    }
    if normalized == "attacking" {
        return ("magicVocabulary", "combatStateWord");
    }
    if normalized == "opponent" || normalized == "opponents" {
        return ("entityReference", "playerQualifierWord");
    }
    if normalized == "control" || normalized == "controls" {
        return ("magicVocabulary", "controlWord");
    }
    if normalized == "controller" || normalized == "owner" {
        return ("entityReference", "playerRoleWord");
    }
    if ["damage", "life"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "resourceWord");
    }
    if ["power", "toughness", "value"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "propertyWord");
    }
    if ["name", "size", "text", "type", "types"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "propertyWord");
    }
    if normalized == "amount" {
        return ("logic", "quantityNoun");
    }
    if normalized == "energy" {
        return ("magicVocabulary", "counterTypeWord");
    }
    if normalized == "chosen" {
        return ("entityReference", "selectionReferenceWord");
    }
    if ["equal", "less", "greater", "more", "same"].contains(&normalized.as_str()) {
        return ("logic", "comparisonWord");
    }
    if normalized == "can't" || normalized == "can’t" {
        return ("logic", "logicWord");
    }
    if ["be", "do", "has", "have", "is", "was", "were"].contains(&normalized.as_str()) {
        return ("syntax", "predicateWord");
    }
    if matches!(
        normalized.as_str(),
        "wasn't" | "wasn’t" | "doesn't" | "doesn’t"
    ) {
        return ("syntax", "negativePredicateWord");
    }
    if normalized == "equipped" {
        return ("entityReference", "attachmentStateWord");
    }
    if ["came", "under"].contains(&normalized.as_str()) {
        return ("syntax", "relationWord");
    }
    if ["addition", "maximum", "only"].contains(&normalized.as_str()) {
        return ("magicVocabulary", "qualifierWord");
    }
    if normalized == "no" {
        return ("logic", "logicWord");
    }
    if is_power_toughness_value(raw) {
        return ("magicVocabulary", "counterValue");
    }
    if [
        "as", "at", "by", "for", "from", "in", "into", "of", "on", "onto", "to", "where", "with",
    ]
    .contains(&normalized.as_str())
    {
        return ("syntax", "relationWord");
    }
    if matches!(
        raw,
        ":" | "," | "." | ";" | "\u{2014}" | "\u{2022}" | "+" | "-" | "−" | "(" | ")" | "\""
    ) {
        return ("boundary", "separator");
    }
    if is_quantity_token(raw) {
        return ("logic", "quantity");
    }
    ("unknown", "unknown")
}

fn is_object_type_word(normalized: &str) -> bool {
    [
        "artifact",
        "artifacts",
        "battle",
        "battles",
        "card",
        "creature",
        "creatures",
        "enchantment",
        "enchantments",
        "instant",
        "instants",
        "land",
        "lands",
        "permanent",
        "permanents",
        "planeswalker",
        "planeswalkers",
        "player",
        "players",
        "sorcery",
        "sorceries",
        "spell",
        "spells",
    ]
    .contains(&normalized)
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
    entities
}

fn classify_creature_subtype_lexemes(tokens: &mut [RecognizedEntity]) {
    let mut position = 0;
    while position < tokens.len() {
        let Some((_, width)) = lookup_creature_subtype_tokens(
            &tokens[position].raw,
            tokens.get(position + 1).map(|token| token.raw.as_str()),
            AcceptedForms::SingularOnly,
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
        let contains_specialized_token = tokens[matched.start_token..matched.end_token]
            .iter()
            .any(|token| token.category != "unknown");
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
    matches!(
        previous.token_type.as_str(),
        "referenceWord"
            | "typeWord"
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
                if candidate.is_alphanumeric() || matches!(candidate, '\'' | '\u{2019}' | '/' | '-')
                {
                    end = character_end + relative + candidate.len_utf8();
                } else {
                    break;
                }
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
