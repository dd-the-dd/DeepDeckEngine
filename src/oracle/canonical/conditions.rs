use super::*;

pub(super) fn parse_condition_text(text: &str) -> Option<Value> {
    let original_condition = text.trim().trim_end_matches('.');
    let condition = original_condition.to_ascii_lowercase();
    let player_zone_threshold_re = Regex::new(&format!(
        r"(?i)^a (graveyard|hand|library) has ({}) or (more|fewer) cards in it$",
        count_word_pattern(),
    ))
    .expect("player-zone card-count threshold condition regex compiles");
    if let Some(captures) = player_zone_threshold_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countPlayerZonesByCardCount",
                "zone": captures[1].to_ascii_lowercase(),
                "operator": if captures[3].eq_ignore_ascii_case("more") { ">=" } else { "<=" },
                "count": integer(parse_number_word(&captures[2])?),
            }),
            integer(1),
        ));
    }
    let triggering_spell_mana_source_re =
        Regex::new(r"(?i)^mana from (?:a|an) (.+?) was spent to cast (?:it|that spell)$")
            .expect("triggering spell mana-source condition regex compiles");
    if let Some(captures) = triggering_spell_mana_source_re.captures(original_condition) {
        return Some(json!({
            "kind": "triggeringSpellManaSourceMatches",
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
        }));
    }
    if condition == "you have an enduring story" {
        return Some(json!({
            "kind": "hasEnduringStory",
            "player": controller(),
        }));
    }
    if condition == "you have the city's blessing" {
        return Some(json!({
            "kind": "hasCityBlessing",
            "player": controller(),
        }));
    }
    let triggering_permanent_matches_re =
        Regex::new(r"(?i)^that (?:creature|permanent) is (?:(?:a|an) )?(.+)$")
            .expect("triggering permanent condition regex compiles");
    if let Some(captures) = triggering_permanent_matches_re.captures(original_condition) {
        return Some(json!({
            "kind": "objectMatchesFilter",
            "object": { "kind": "triggeringPermanent" },
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
        }));
    }
    let distinct_controlled_names_re = Regex::new(&format!(
        r"(?i)^you control ({}) or more (.+?) with different names$",
        count_word_pattern(),
    ))
    .expect("distinct controlled permanent names condition regex compiles");
    if let Some(captures) = distinct_controlled_names_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countDistinctPermanentNames",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(2)?.as_str()),
                    "",
                )?,
            }),
            integer(parse_number_word(captures.get(1)?.as_str())?),
        ));
    }
    let cast_from_zone_re = Regex::new(
        r"(?i)^this spell was cast from (?:(?:a|the) )?(graveyard|hand|exile|command zone)$",
    )
    .expect("spell cast source-zone condition regex compiles");
    if let Some(captures) = cast_from_zone_re.captures(original_condition) {
        let zone = match captures[1].to_ascii_lowercase().as_str() {
            "command zone" => "commandZone",
            "graveyard" => "graveyard",
            "hand" => "hand",
            "exile" => "exile",
            _ => return None,
        };
        return Some(json!({
            "kind": "wasCastFromZone",
            "object": self_ref(),
            "zone": zone,
        }));
    }
    if condition == "the gift was promised" {
        return Some(json!({
            "kind": "selectionNotEmpty",
            "selection": decision_result("giftRecipient"),
        }));
    }
    let distinct_graveyard_types_re = Regex::new(&format!(
        r"(?i)^there are ({}) or more card types among cards in (your graveyard|all graveyards)$",
        count_word_pattern(),
    ))
    .expect("distinct graveyard card-type threshold condition regex compiles");
    if let Some(captures) = distinct_graveyard_types_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countDistinctCardTypes",
                "zone": if captures[2].eq_ignore_ascii_case("all graveyards") {
                    json!({ "kind": "anyGraveyard" })
                } else {
                    graveyard(controller())
                },
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let graveyard_pair_re =
        Regex::new(r"(?i)^there is (?:a|an) (.+?) card and (?:a|an) (.+?) card in your graveyard$")
            .expect("paired graveyard-card condition regex compiles");
    if let Some(captures) = graveyard_pair_re.captures(original_condition) {
        return Some(and(vec![
            compare(
                ">=",
                json!({
                    "kind": "countCards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&captures[1], "")?,
                }),
                integer(1),
            ),
            compare(
                ">=",
                json!({
                    "kind": "countCards",
                    "zone": graveyard(controller()),
                    "where": parse_permanent_criteria(&captures[2], "")?,
                }),
                integer(1),
            ),
        ]));
    }
    if let Some((left, right)) = original_condition.split_once(" and ") {
        return Some(and(vec![
            parse_condition_text(left)?,
            parse_condition_text(right)?,
        ]));
    }
    if condition == "this spell is the first spell you've cast this game" {
        return Some(compare(
            "==",
            json!({
                "kind": "countEvents",
                "event": "spellCast",
                "player": controller(),
            }),
            integer(0),
        ));
    }
    let opponent_controls_re = Regex::new(r"(?i)^an opponent controls (?:a|an) (.+)$")
        .expect("opponent controls permanent condition regex compiles");
    if let Some(captures) = opponent_controls_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "greatestOpponentPermanentCount",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], "")?,
            }),
            integer(1),
        ));
    }
    if condition.starts_with("you control ") {
        return parse_controlled_permanent_condition(original_condition, "");
    }
    if matches!(
        condition.as_str(),
        "this creature isn't prepared" | "it isn't prepared"
    ) {
        return Some(not(json!({ "kind": "isPrepared", "object": self_ref() })));
    }
    if condition == "you gained life this turn" {
        return Some(compare(
            ">",
            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
            integer(0),
        ));
    }
    let life_gained_threshold_re = Regex::new(&format!(
        r"(?i)^you(?:'ve| have)? gained ({}) or more life this turn$",
        count_word_pattern(),
    ))
    .expect("life gained this turn threshold regex compiles");
    if let Some(captures) = life_gained_threshold_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
            integer(parse_number_word(captures.get(1)?.as_str())?),
        ));
    }
    if condition == "you created a token this turn" {
        return Some(compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "tokenCreated",
                "player": controller(),
            }),
            integer(1),
        ));
    }
    if condition == "a creature died this turn" {
        return Some(compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "permanentDied",
                "where": card_type("Creature"),
            }),
            integer(1),
        ));
    }
    let drawn_threshold_re = Regex::new(&format!(
        r"(?i)^you(?:'ve| have) drawn ({}) or more cards? this turn$",
        count_word_pattern(),
    ))
    .expect("cards-drawn threshold condition regex compiles");
    if let Some(captures) = drawn_threshold_re.captures(original_condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "cardDrawn",
                "player": controller(),
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let drawn_more_than_re = Regex::new(&format!(
        r"(?i)^you(?:'ve| have) drawn more than ({}) cards? this turn$",
        count_word_pattern(),
    ))
    .expect("cards-drawn strict threshold condition regex compiles");
    if let Some(captures) = drawn_more_than_re.captures(original_condition) {
        return Some(compare(
            ">",
            json!({
                "kind": "countEventsThisTurn",
                "event": "cardDrawn",
                "player": controller(),
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let controlled_departure_re =
        Regex::new(r"^(?:a|an) (.+?) left the battlefield under your control this turn$")
            .expect("controlled battlefield-departure condition regex compiles");
    if let Some(captures) = controlled_departure_re.captures(&condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "permanentLeftBattlefield",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], "")?,
            }),
            integer(1),
        ));
    }
    if condition == "an opponent has more cards in hand than you" {
        return Some(compare(
            ">",
            json!({ "kind": "greatestOpponentHandSize" }),
            json!({
                "kind": "countCards",
                "zone": { "kind": "hand", "player": controller() },
                "where": Value::Null,
            }),
        ));
    }
    if condition == "a player has one or fewer cards in hand" {
        return Some(compare(
            "<=",
            json!({ "kind": "minimumHandSize" }),
            integer(1),
        ));
    }
    let controller_hand_size_re = Regex::new(&format!(
        r"^you have ({}) or fewer cards in hand$",
        count_word_pattern(),
    ))
    .expect("controller hand-size threshold condition regex compiles");
    if let Some(captures) = controller_hand_size_re.captures(&condition) {
        return Some(compare(
            "<=",
            json!({
                "kind": "countCards",
                "zone": hand(controller()),
                "where": Value::Null,
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    if condition == "an opponent controls more creatures than you" {
        return Some(compare(
            ">",
            json!({ "kind": "greatestOpponentPermanentCount", "where": card_type("Creature") }),
            json!({ "kind": "countPermanents", "player": controller(), "where": card_type("Creature") }),
        ));
    }
    let died_re = Regex::new(&format!(
        r"^({}) or more creatures died this turn$",
        count_word_pattern(),
    ))
    .expect("prepare death threshold regex compiles");
    if let Some(captures) = died_re.captures(&condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "permanentDied",
                "where": card_type("Creature"),
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let graveyard_re = Regex::new(&format!(
        r"^there are ({}) or more(?: (.+?))? cards in your graveyard$",
        count_word_pattern(),
    ))
    .expect("prepare graveyard threshold regex compiles");
    if let Some(captures) = graveyard_re.captures(&condition) {
        let where_filter = captures
            .get(2)
            .map(|qualifier| {
                card_qualifier_list_filter(qualifier.as_str(), "")
                    .or_else(|| parse_permanent_criteria(qualifier.as_str(), ""))
            })
            .unwrap_or(Some(Value::Null))?;
        return Some(compare(
            ">=",
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": where_filter,
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let controlled_permanent_threshold_re = Regex::new(&format!(
        r"^you control ({}) or more (.+?)$",
        count_word_pattern(),
    ))
    .expect("controlled permanent threshold condition regex compiles");
    if let Some(captures) = controlled_permanent_threshold_re.captures(&condition) {
        return Some(compare(
            ">=",
            json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(2)?.as_str()),
                    "",
                )?,
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    None
}
