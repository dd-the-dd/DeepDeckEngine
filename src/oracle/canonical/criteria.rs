use super::*;

pub(super) fn singular_card_term(value: &str) -> String {
    let trimmed = value.trim();
    for (plural, singular) in [
        ("Allies", "Ally"),
        ("Dwarves", "Dwarf"),
        ("Elves", "Elf"),
        ("Mice", "Mouse"),
        ("Oxen", "Ox"),
        ("Plains", "Plains"),
        ("Wolves", "Wolf"),
        ("Zombies", "Zombie"),
    ] {
        if trimmed.eq_ignore_ascii_case(plural) {
            return singular.to_string();
        }
    }
    if trimmed.len() > 3 && trimmed.to_ascii_lowercase().ends_with("ies") {
        return format!("{}y", &trimmed[..trimmed.len() - 3]);
    }
    if trimmed.len() > 1
        && trimmed.to_ascii_lowercase().ends_with('s')
        && !trimmed.to_ascii_lowercase().ends_with("ss")
    {
        return trimmed[..trimmed.len() - 1].to_string();
    }
    trimmed.to_string()
}

pub(super) fn normalized_keyword_name(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "deathtouch" => Some("deathtouch"),
        "double strike" => Some("doubleStrike"),
        "first strike" => Some("firstStrike"),
        "flying" => Some("flying"),
        "haste" => Some("haste"),
        "hexproof" => Some("hexproof"),
        "indestructible" => Some("indestructible"),
        "lifelink" => Some("lifelink"),
        "menace" => Some("menace"),
        "reach" => Some("reach"),
        "trample" => Some("trample"),
        "vigilance" => Some("vigilance"),
        _ => None,
    }
}

pub(super) fn source_reference_matches(subject: &str, face_name: &str) -> bool {
    let subject = subject.trim();
    if matches!(
        subject.to_ascii_lowercase().as_str(),
        "this artifact"
            | "this aura"
            | "this card"
            | "this creature"
            | "this enchantment"
            | "this equipment"
            | "this land"
            | "this permanent"
            | "this saga"
            | "this spell"
            | "this token"
            | "it"
            | "he"
            | "she"
            | "they"
    ) {
        return true;
    }
    let lower_subject = subject.to_ascii_lowercase();
    let lower_name = face_name.to_ascii_lowercase();
    lower_name
        .strip_prefix(&lower_subject)
        .is_some_and(|suffix| {
            suffix.is_empty()
                || suffix
                    .chars()
                    .next()
                    .is_some_and(|character| !character.is_alphanumeric())
        })
}

pub(super) fn known_card_type_filter(value: &str) -> Option<Value> {
    let card_type_name = match singular_card_term(value).to_ascii_lowercase().as_str() {
        "artifact" => "Artifact",
        "battle" => "Battle",
        "conspiracy" => "Conspiracy",
        "creature" => "Creature",
        "dungeon" => "Dungeon",
        "enchantment" => "Enchantment",
        "instant" => "Instant",
        "kindred" => "Kindred",
        "land" => "Land",
        "phenomenon" => "Phenomenon",
        "plane" => "Plane",
        "planeswalker" => "Planeswalker",
        "scheme" => "Scheme",
        "sorcery" => "Sorcery",
        "vanguard" => "Vanguard",
        _ => return None,
    };
    Some(card_type(card_type_name))
}

pub(super) fn oracle_keyword_kind(value: &str) -> Option<&'static str> {
    match value.trim().to_ascii_lowercase().as_str() {
        "can't attack" => Some("cantAttack"),
        "can't block" => Some("cantBlock"),
        "can't be blocked" => Some("cantBeBlocked"),
        "cascade" => Some("cascade"),
        "deathtouch" => Some("deathtouch"),
        "defender" => Some("defender"),
        "double strike" => Some("doubleStrike"),
        "first strike" => Some("firstStrike"),
        "flying" => Some("flying"),
        "fear" => Some("fear"),
        "haste" => Some("haste"),
        "hexproof" => Some("hexproof"),
        "horsemanship" => Some("horsemanship"),
        "indestructible" => Some("indestructible"),
        "lifelink" => Some("lifelink"),
        "infect" => Some("infect"),
        "intimidate" => Some("intimidate"),
        "menace" => Some("menace"),
        "prowess" => Some("prowess"),
        "reach" => Some("reach"),
        "shadow" => Some("shadow"),
        "plainswalk" => Some("plainswalk"),
        "islandwalk" => Some("islandwalk"),
        "swampwalk" => Some("swampwalk"),
        "mountainwalk" => Some("mountainwalk"),
        "forestwalk" => Some("forestwalk"),
        "shroud" => Some("shroud"),
        "skulk" => Some("skulk"),
        "trample" => Some("trample"),
        "undying" => Some("undying"),
        "vigilance" => Some("vigilance"),
        _ => None,
    }
}

pub(super) fn oracle_keyword_list(value: &str) -> Option<Vec<&'static str>> {
    let normalized = value
        .trim()
        .trim_end_matches('.')
        .replace(", and ", ", ")
        .replace(" and ", ", ");
    let keywords = normalized
        .split(',')
        .map(str::trim)
        .filter(|term| !term.is_empty())
        .map(oracle_keyword_kind)
        .collect::<Option<Vec<_>>>()?;
    (!keywords.is_empty()).then_some(keywords)
}

pub(super) fn card_qualifier_filter(value: &str, face_name: &str) -> Option<Value> {
    let trimmed = value
        .trim()
        .trim_start_matches("a ")
        .trim_start_matches("an ")
        .trim_start_matches("each ")
        .trim_end_matches(" cards")
        .trim_end_matches(" card")
        .trim();
    if trimmed.eq_ignore_ascii_case("multicolored") {
        return Some(compare(
            ">",
            json!({
                "kind": "colorCountOf",
                "object": { "kind": "candidate" },
            }),
            integer(1),
        ));
    }
    if trimmed.eq_ignore_ascii_case("monocolored") {
        return Some(compare(
            "==",
            json!({
                "kind": "colorCountOf",
                "object": { "kind": "candidate" },
            }),
            integer(1),
        ));
    }
    if trimmed.eq_ignore_ascii_case("colorless") {
        return Some(colorless_filter());
    }
    if trimmed.eq_ignore_ascii_case("legendary") {
        return Some(json!({ "kind": "isLegendary" }));
    }
    if trimmed.eq_ignore_ascii_case("nonlegendary") || trimmed.eq_ignore_ascii_case("non-legendary")
    {
        return Some(not(json!({ "kind": "isLegendary" })));
    }
    if trimmed.eq_ignore_ascii_case("token") || trimmed.eq_ignore_ascii_case("tokens") {
        return Some(json!({ "kind": "isToken" }));
    }
    if trimmed.eq_ignore_ascii_case("nontoken") || trimmed.eq_ignore_ascii_case("non-token") {
        return Some(not(json!({ "kind": "isToken" })));
    }
    let qualified_token_re =
        Regex::new(r"(?i)^(.+?) tokens?$").expect("qualified token criteria regex compiles");
    if let Some(captures) = qualified_token_re.captures(trimmed) {
        return Some(and(vec![
            card_qualifier_filter(captures.get(1)?.as_str(), face_name)?,
            json!({ "kind": "isToken" }),
        ]));
    }
    if trimmed.eq_ignore_ascii_case("basic") {
        return Some(json!({ "kind": "typeLineContains", "value": "Basic" }));
    }
    if trimmed.eq_ignore_ascii_case("nonbasic") || trimmed.eq_ignore_ascii_case("non-basic") {
        return Some(not(json!({ "kind": "typeLineContains", "value": "Basic" })));
    }
    if trimmed.to_ascii_lowercase().starts_with("non") {
        let qualifier = trimmed
            .strip_prefix("non-")
            .or_else(|| trimmed.get(3..))?
            .trim();
        if let Some(type_filter) = known_card_type_filter(qualifier) {
            return Some(not(type_filter));
        }
        if let Some(color) = color_filter(qualifier) {
            return Some(not(color));
        }
    }
    if let Some(filter) = known_card_type_filter(trimmed) {
        return Some(filter);
    }
    if let Some(filter) = color_filter(trimmed) {
        return Some(filter);
    }

    let words = trimmed.split_whitespace().collect::<Vec<_>>();
    if words.len() >= 2
        && let Some(card_type_filter) = words.last().and_then(|word| known_card_type_filter(word))
    {
        let mut filters = Vec::new();
        for word in &words[..words.len() - 1] {
            let qualifier = word.trim_matches(',');
            let filter = if qualifier.eq_ignore_ascii_case("legendary") {
                json!({ "kind": "isLegendary" })
            } else if qualifier.eq_ignore_ascii_case("nonlegendary")
                || qualifier.eq_ignore_ascii_case("non-legendary")
            {
                not(json!({ "kind": "isLegendary" }))
            } else if qualifier.eq_ignore_ascii_case("token") {
                json!({ "kind": "isToken" })
            } else if qualifier.eq_ignore_ascii_case("nontoken")
                || qualifier.eq_ignore_ascii_case("non-token")
            {
                not(json!({ "kind": "isToken" }))
            } else if qualifier.eq_ignore_ascii_case("basic") {
                json!({ "kind": "typeLineContains", "value": "Basic" })
            } else if qualifier.eq_ignore_ascii_case("nonbasic")
                || qualifier.eq_ignore_ascii_case("non-basic")
            {
                not(json!({ "kind": "typeLineContains", "value": "Basic" }))
            } else if qualifier.eq_ignore_ascii_case("colorless") {
                colorless_filter()
            } else if qualifier.eq_ignore_ascii_case("monocolored") {
                compare(
                    "==",
                    json!({
                        "kind": "colorCountOf",
                        "object": { "kind": "candidate" },
                    }),
                    integer(1),
                )
            } else if qualifier.eq_ignore_ascii_case("multicolored") {
                compare(
                    ">",
                    json!({
                        "kind": "colorCountOf",
                        "object": { "kind": "candidate" },
                    }),
                    integer(1),
                )
            } else if let Some(rest) = qualifier.strip_prefix("non-") {
                let singular = singular_card_term(rest);
                if let Some(type_filter) = known_card_type_filter(&singular) {
                    not(type_filter)
                } else if let Some(color) = color_filter(&singular) {
                    not(color)
                } else {
                    not(subtype(&singular))
                }
            } else if qualifier.to_ascii_lowercase().starts_with("non") {
                let singular = singular_card_term(qualifier.get(3..)?);
                if let Some(type_filter) = known_card_type_filter(&singular) {
                    not(type_filter)
                } else if let Some(color) = color_filter(&singular) {
                    not(color)
                } else {
                    return None;
                }
            } else if let Some(type_filter) = known_card_type_filter(qualifier) {
                type_filter
            } else if let Some(color) = color_filter(qualifier) {
                color
            } else {
                let singular = singular_card_term(qualifier);
                if !(singular.chars().next().is_some_and(char::is_uppercase)
                    && singular
                        .chars()
                        .all(|character| character.is_alphabetic() || character == '-'))
                {
                    return None;
                }
                subtype(&singular)
            };
            filters.push(filter);
        }
        filters.push(card_type_filter);
        return Some(and(filters));
    }

    let singular = singular_card_term(trimmed);
    if let Some(filter) = known_card_type_filter(&singular) {
        return Some(filter);
    }
    let lower_face_name = face_name.to_ascii_lowercase();
    let lower_singular = singular.to_ascii_lowercase();
    let matches_name_prefix = lower_face_name
        .strip_prefix(&lower_singular)
        .is_some_and(|suffix| {
            suffix.is_empty()
                || suffix
                    .chars()
                    .next()
                    .is_some_and(|character| !character.is_alphanumeric())
        });
    if !face_name.is_empty() && (face_name.eq_ignore_ascii_case(&singular) || matches_name_prefix) {
        return Some(json!({ "kind": "nameStartsWith", "value": singular }));
    }

    let subtype_terms = singular.split_whitespace().collect::<Vec<_>>();
    if subtype_terms.iter().all(|term| {
        term.chars().next().is_some_and(char::is_uppercase)
            && term
                .chars()
                .all(|character| character.is_alphabetic() || character == '-')
    }) {
        let filters = subtype_terms.into_iter().map(subtype).collect::<Vec<_>>();
        return match filters.as_slice() {
            [] => None,
            [filter] => Some(filter.clone()),
            _ => Some(and(filters)),
        };
    }
    None
}

pub(super) fn card_qualifier_list_filter(value: &str, face_name: &str) -> Option<Value> {
    let terms = Regex::new(r"(?:\s*,\s*(?:(?:and|or)\s+)?|\s+(?:and|or)\s+)")
        .expect("card qualifier list regex compiles")
        .split(value)
        .map(str::trim)
        .collect::<Vec<_>>();
    let filters = terms
        .iter()
        .map(|term| card_qualifier_filter(term, face_name))
        .collect::<Option<Vec<_>>>()?;
    match filters.as_slice() {
        [] => None,
        [filter] => Some(filter.clone()),
        _ => Some(or(filters)),
    }
}

pub(super) fn color_filter(value: &str) -> Option<Value> {
    let color = match value.trim().to_ascii_lowercase().as_str() {
        "white" => "white",
        "blue" => "blue",
        "black" => "black",
        "red" => "red",
        "green" => "green",
        _ => return None,
    };
    Some(json!({ "kind": "colorContains", "value": color }))
}

pub(super) fn colorless_filter() -> Value {
    compare(
        "==",
        json!({
            "kind": "colorCountOf",
            "object": { "kind": "candidate" },
        }),
        integer(0),
    )
}

pub(super) fn parse_player_static_modifiers(text: &str) -> Option<Vec<Value>> {
    Regex::new(r"(?i)^You have no maximum hand size\.$")
        .expect("no maximum hand-size regex compiles")
        .is_match(text.trim())
        .then(|| {
            vec![json!({
                "kind": "noMaximumHandSize",
                "player": controller(),
            })]
        })
}

pub(super) fn parse_protection_qualities(value: &str) -> Option<Vec<String>> {
    let normalized = value
        .trim()
        .replace(", and from ", ", ")
        .replace(" and from ", ", ")
        .replace(", and ", ", ")
        .replace(" and ", ", ")
        .replace("from ", "");
    let qualities = normalized
        .split(',')
        .map(str::trim)
        .filter(|quality| !quality.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>();
    (!qualities.is_empty()
        && qualities
            .iter()
            .all(|quality| quality != "the color of your choice"))
    .then_some(qualities)
}

pub(super) fn strip_leading_article(value: &str) -> &str {
    value
        .trim()
        .strip_prefix("a ")
        .or_else(|| value.trim().strip_prefix("an "))
        .or_else(|| value.trim().strip_prefix("the "))
        .unwrap_or_else(|| value.trim())
}

pub(super) fn parse_permanent_criteria(value: &str, face_name: &str) -> Option<Value> {
    let mut description = strip_leading_article(value).trim_end_matches('.').trim();
    description = description
        .strip_suffix(" cards")
        .or_else(|| description.strip_suffix(" card"))
        .unwrap_or(description)
        .trim();
    let mut predicates = Vec::new();

    for card_kind in [
        "artifact",
        "battle",
        "creature",
        "enchantment",
        "instant",
        "land",
        "planeswalker",
        "sorcery",
        "tribal",
    ] {
        if description.eq_ignore_ascii_case(&format!("non{card_kind}")) {
            return Some(not(known_card_type_filter(card_kind)?));
        }
    }

    if let Some(color) = color_filter(description) {
        return Some(color);
    }
    let shared_color_type_re = Regex::new(
        r"(?i)^((?:white|blue|black|red|green)(?:\s+(?:or|and/or)\s+(?:white|blue|black|red|green))+)[ ]+(permanent|creature|artifact|enchantment|planeswalker|land)s?$",
    )
    .expect("shared color and card-type criteria regex compiles");
    if let Some(captures) = shared_color_type_re.captures(description) {
        let colors = Regex::new(r"(?i)white|blue|black|red|green")
            .expect("color list regex compiles")
            .find_iter(&captures[1])
            .map(|matched| color_filter(matched.as_str()))
            .collect::<Option<Vec<_>>>()?;
        let type_filter = known_card_type_filter(&captures[2]);
        return Some(match type_filter {
            Some(type_filter) => and(vec![or(colors), type_filter]),
            None => or(colors),
        });
    }

    let shared_card_types_re = Regex::new(
        r"(?i)^(artifact|battle|creature|enchantment|instant|kindred|land|planeswalker|sorcery)s?(?:\s+(?:and|or|and/or)\s+(artifact|battle|creature|enchantment|instant|kindred|land|planeswalker|sorcery)s?)+$",
    )
    .expect("shared card-type criteria regex compiles");
    if shared_card_types_re.is_match(description) {
        let card_types = Regex::new(
            r"(?i)artifact|battle|creature|enchantment|instant|kindred|land|planeswalker|sorcery",
        )
        .expect("card-type list regex compiles")
        .find_iter(description)
        .map(|matched| known_card_type_filter(matched.as_str()))
        .collect::<Option<Vec<_>>>()?;
        return Some(or(card_types));
    }

    if matches!(
        description.to_ascii_lowercase().as_str(),
        "attacking or blocking creature" | "attacking or blocking creatures"
    ) {
        return Some(and(vec![
            card_type("Creature"),
            or(vec![
                json!({ "kind": "isAttacking" }),
                json!({ "kind": "isBlocking" }),
            ]),
        ]));
    }

    let lower_description = description.to_ascii_lowercase();
    if let Some(rest) = lower_description
        .strip_suffix(" tokens")
        .or_else(|| lower_description.strip_suffix(" token"))
    {
        let retained_length = rest.len();
        predicates.push(json!({ "kind": "isToken" }));
        description = description.get(..retained_length)?.trim_end();
    } else if let Some(rest) = lower_description
        .strip_suffix(" nontokens")
        .or_else(|| lower_description.strip_suffix(" nontoken"))
    {
        let retained_length = rest.len();
        predicates.push(not(json!({ "kind": "isToken" })));
        description = description.get(..retained_length)?.trim_end();
    }

    loop {
        let lower = description.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("nontoken ") {
            predicates.push(not(json!({ "kind": "isToken" })));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("tapped ") {
            predicates.push(json!({ "kind": "isTapped" }));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("untapped ") {
            predicates.push(not(json!({ "kind": "isTapped" })));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("attacking ") {
            predicates.push(json!({ "kind": "isAttacking" }));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("blocking ") {
            predicates.push(json!({ "kind": "isBlocking" }));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("equipped ") {
            predicates.push(json!({ "kind": "isEquipped" }));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower
            .strip_prefix("nonland ")
            .or_else(|| lower.strip_prefix("nonland, "))
        {
            predicates.push(not(card_type("Land")));
            description = description[description.len() - rest.len()..]
                .trim_start_matches(',')
                .trim_start();
            continue;
        }
        if let Some(rest) = lower.strip_prefix("nonbasic ") {
            predicates.push(not(json!({ "kind": "typeLineContains", "value": "Basic" })));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        let mut consumed_negative_type = false;
        for card_kind in [
            "artifact",
            "battle",
            "creature",
            "enchantment",
            "instant",
            "planeswalker",
            "sorcery",
            "tribal",
        ] {
            let prefix = format!("non{card_kind} ");
            if let Some(rest) = lower.strip_prefix(&prefix) {
                predicates.push(not(card_type(card_kind)));
                description = &description[description.len() - rest.len()..].trim_start();
                consumed_negative_type = true;
                break;
            }
        }
        if consumed_negative_type {
            continue;
        }
        if let Some(rest) = lower.strip_prefix("legendary ") {
            predicates.push(json!({ "kind": "isLegendary" }));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some(rest) = lower
            .strip_prefix("nonlegendary ")
            .or_else(|| lower.strip_prefix("non-legendary "))
        {
            predicates.push(not(json!({ "kind": "isLegendary" })));
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        if let Some((qualifier, rest)) = description.split_once(' ')
            && qualifier.to_ascii_lowercase().starts_with("non-")
        {
            let excluded_subtype = qualifier.get(4..)?.trim();
            if !excluded_subtype.is_empty() {
                predicates.push(not(subtype(&singular_card_term(excluded_subtype))));
                description = rest.trim();
                continue;
            }
        }
        let mut consumed_color = false;
        if let Some(rest) = lower.strip_prefix("colorless ") {
            predicates.push(colorless_filter());
            description = &description[description.len() - rest.len()..].trim_start();
            continue;
        }
        for color in ["white", "blue", "black", "red", "green"] {
            let non_color_prefix = format!("non{color} ");
            if let Some(rest) = lower.strip_prefix(&non_color_prefix) {
                predicates.push(not(color_filter(color)?));
                description = &description[description.len() - rest.len()..].trim_start();
                consumed_color = true;
                break;
            }
            let color_prefix = format!("{color} ");
            if let Some(rest) = lower.strip_prefix(&color_prefix) {
                predicates.push(color_filter(color)?);
                description = &description[description.len() - rest.len()..].trim_start();
                consumed_color = true;
                break;
            }
        }
        if consumed_color {
            continue;
        }
        break;
    }

    if description.eq_ignore_ascii_case("land with a basic land type")
        || description.eq_ignore_ascii_case("land card with a basic land type")
    {
        predicates.push(and(vec![
            card_type("Land"),
            or(["Plains", "Island", "Swamp", "Mountain", "Forest"]
                .into_iter()
                .map(subtype)
                .collect()),
        ]));
        return match predicates.as_slice() {
            [filter] => Some(filter.clone()),
            _ => Some(and(predicates)),
        };
    }

    if let Some(captures) = Regex::new(r"(?i)^(.+?) that are one or more colors$")
        .expect("colored-card criteria regex compiles")
        .captures(description)
    {
        predicates.push(and(vec![
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            compare(
                ">=",
                json!({ "kind": "colorCountOf", "object": { "kind": "candidate" } }),
                integer(1),
            ),
        ]));
    } else if let Some(captures) = Regex::new(&format!(
        r"(?i)^(.+) with mana value ({}) or (greater|less)$",
        numeric_expression_pattern(),
    ))
    .expect("mana-value threshold criteria regex compiles")
    .captures(description)
    {
        let operator = if captures[3].eq_ignore_ascii_case("greater") {
            ">="
        } else {
            "<="
        };
        predicates.push(and(vec![
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            compare(
                operator,
                json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                parse_numeric_expression_text(&captures[2])?,
            ),
        ]));
    } else if let Some(captures) = Regex::new(r"(?i)^(.+) with mana value equal to (.+)$")
        .expect("variable mana-value equality criteria regex compiles")
        .captures(description)
    {
        predicates.push(and(vec![
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            compare(
                "==",
                json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                parse_numeric_expression_text(captures.get(2)?.as_str())?,
            ),
        ]));
    } else if let Some(captures) =
        Regex::new(r"(?i)^(.+) with (power|toughness) (\d+) or (greater|less)$")
            .expect("power or toughness threshold criteria regex compiles")
            .captures(description)
    {
        let characteristic = if captures[2].eq_ignore_ascii_case("power") {
            "powerOf"
        } else {
            "toughnessOf"
        };
        let operator = if captures[4].eq_ignore_ascii_case("greater") {
            ">="
        } else {
            "<="
        };
        let threshold = captures[3].parse::<i64>().ok()?;
        let base = captures.get(1)?.as_str();
        let mut filters = vec![parse_permanent_criteria(base, face_name)?];
        filters.push(compare(
            operator,
            json!({ "kind": characteristic, "object": { "kind": "candidate" } }),
            integer(threshold),
        ));
        predicates.push(and(filters));
    } else if let Some(captures) =
        Regex::new(r"(?i)^(.+) with power or toughness (\d+) or (greater|less)$")
            .expect("combined power toughness threshold criteria regex compiles")
            .captures(description)
    {
        let operator = if captures[3].eq_ignore_ascii_case("greater") {
            ">="
        } else {
            "<="
        };
        let threshold = integer(captures[2].parse::<i64>().ok()?);
        predicates.push(and(vec![
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            or(vec![
                compare(
                    operator,
                    json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                    threshold.clone(),
                ),
                compare(
                    operator,
                    json!({ "kind": "toughnessOf", "object": { "kind": "candidate" } }),
                    threshold,
                ),
            ]),
        ]));
    } else if let Some(captures) = Regex::new(r"(?i)^(.+) (with|without) ([a-z ]+)$")
        .expect("keyword-qualified permanent criteria regex compiles")
        .captures(description)
        && let Some(keyword) = oracle_keyword_kind(&captures[3])
    {
        let keyword_filter = json!({ "kind": "hasKeyword", "value": keyword });
        predicates.push(and(vec![
            parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            if captures[2].eq_ignore_ascii_case("without") {
                not(keyword_filter)
            } else {
                keyword_filter
            },
        ]));
    } else if description.eq_ignore_ascii_case("permanent")
        || description.eq_ignore_ascii_case("permanents")
    {
    } else {
        if description.contains(',') {
            let filters = description
                .split(',')
                .map(|term| {
                    let term = term.trim();
                    let term = term
                        .strip_prefix("and ")
                        .or_else(|| term.strip_prefix("or "))
                        .unwrap_or(term);
                    parse_permanent_criteria(term, face_name)
                })
                .collect::<Option<Vec<_>>>()?;
            let lower = description.to_ascii_lowercase();
            predicates.push(if lower.contains(", or ") || lower.contains("and/or") {
                or(filters)
            } else {
                and(filters)
            });
            return match predicates.as_slice() {
                [filter] => Some(filter.clone()),
                _ => Some(and(predicates)),
            };
        }
        let split_re = Regex::new(r"(?:\s*,\s*(?:(?:and/or|or)\s+)?|\s+(?:and/or|or)\s+)")
            .expect("criteria list regex compiles");
        let terms = split_re
            .split(description)
            .map(str::trim)
            .collect::<Vec<_>>();
        let base_filter = if terms.len() > 1 {
            let filters = terms
                .iter()
                .map(|term| parse_permanent_criteria(term, face_name))
                .collect::<Option<Vec<_>>>()?;
            let lower = description.to_ascii_lowercase();
            if lower.contains(" or ") || lower.contains("and/or") {
                or(filters)
            } else {
                and(filters)
            }
        } else {
            card_qualifier_filter(description, face_name)?
        };
        predicates.push(base_filter);
    }

    match predicates.as_slice() {
        [] => Some(Value::Null),
        [filter] => Some(filter.clone()),
        _ => Some(and(predicates)),
    }
}

pub(super) fn parse_controlled_permanent_condition(value: &str, face_name: &str) -> Option<Value> {
    let value = value.trim().trim_end_matches('.');
    if value.eq_ignore_ascii_case("an opponent controls more lands than you") {
        return Some(compare(
            ">",
            json!({
                "kind": "greatestOpponentPermanentCount",
                "player": controller(),
                "where": card_type("Land"),
            }),
            json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": card_type("Land"),
            }),
        ));
    }
    let controlled_count_re = Regex::new(&format!(
        r"(?i)^you control ({}) or (more|fewer) (.+)$",
        count_word_pattern(),
    ))
    .expect("controlled permanent count condition regex compiles");
    if let Some(captures) = controlled_count_re.captures(value) {
        return Some(compare(
            if captures[2].eq_ignore_ascii_case("more") {
                ">="
            } else {
                "<="
            },
            json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[3], face_name)?,
            }),
            integer(parse_number_word(&captures[1])?),
        ));
    }
    let another_controlled_re =
        Regex::new(r"(?i)^you control another (.+)$").expect("another controlled regex compiles");
    if let Some(captures) = another_controlled_re.captures(value) {
        return Some(json!({
            "kind": "controlsPermanent",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
            "excludeSource": true,
        }));
    }
    let criteria = value.strip_prefix("you control ")?;
    Some(json!({
        "kind": "controlsPermanent",
        "player": controller(),
        "where": parse_permanent_criteria(criteria, face_name)?,
    }))
}

pub(super) fn mana_cast_restriction_filter(effect_text: &str) -> Option<Value> {
    let captures = Regex::new(
        r"Spend this mana only to cast (?:an? )?(.+?) spells?(?:[.,]| (?:and|or) activate)",
    )
    .expect("mana cast restriction regex compiles")
    .captures(effect_text)?;
    let description = captures[1].trim();
    if description.eq_ignore_ascii_case("legendary") {
        return Some(json!({ "kind": "typeLineContains", "value": "Legendary" }));
    }
    if let Some(filter) = card_qualifier_list_filter(description, "") {
        return Some(filter);
    }
    if let Some((left, right)) = description
        .split_once(" or ")
        .or_else(|| description.split_once(" and "))
        && let (Some(left), Some(right)) = (
            known_card_type_filter(left.trim()),
            known_card_type_filter(right.trim()),
        )
    {
        return Some(or(vec![left, right]));
    }
    if let Some(filter) = known_card_type_filter(description) {
        return Some(filter);
    }
    if let Some((subtype_name, card_type_name)) = description.rsplit_once(' ')
        && let Some(card_type_filter) = known_card_type_filter(card_type_name)
        && singular_card_term(subtype_name)
            .chars()
            .all(|character| character.is_alphabetic() || character == '-')
    {
        return Some(and(vec![
            subtype(&singular_card_term(subtype_name)),
            card_type_filter,
        ]));
    }
    card_qualifier_filter(description, "").or_else(|| parse_permanent_criteria(description, ""))
}
