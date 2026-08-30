use super::*;

pub(super) fn activation_cost_atom_starts(text: &str) -> bool {
    let trimmed = text.trim_start();
    let lower = trimmed.to_ascii_lowercase();
    trimmed.starts_with('{')
        || trimmed.starts_with('+')
        || trimmed.starts_with('-')
        || trimmed.starts_with('0')
        || [
            "pay ",
            "tap ",
            "untap ",
            "discard ",
            "exile ",
            "sacrifice ",
            "remove ",
            "return ",
            "waterbend ",
            "blight ",
        ]
        .iter()
        .any(|prefix| lower.starts_with(prefix))
}

pub(super) fn split_activation_cost_atoms(cost_text: &str) -> Vec<&str> {
    let mut atoms = Vec::new();
    let mut start = 0;
    let mut parenthesis_depth = 0_u32;
    let mut quoted = false;
    for (index, character) in cost_text.char_indices() {
        match character {
            '"' => quoted = !quoted,
            '(' if !quoted => parenthesis_depth += 1,
            ')' if !quoted => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            ',' if !quoted
                && parenthesis_depth == 0
                && activation_cost_atom_starts(&cost_text[index + character.len_utf8()..]) =>
            {
                atoms.push(&cost_text[start..index]);
                start = index + character.len_utf8();
            }
            _ => {}
        }
    }
    atoms.push(&cost_text[start..]);
    atoms
}

pub(super) fn parse_activation_costs(cost_text: &str) -> Option<(Vec<Value>, Vec<Value>)> {
    let mana_cost_re =
        Regex::new(r"^(?:\{[^}]+\})+$").expect("activation mana cost regex compiles");
    let life_cost_re =
        Regex::new(r"(?i)^Pay (\d+) life$").expect("activation life cost regex compiles");
    let waterbend_cost_re =
        Regex::new(r"(?i)^Waterbend (?:\{)?(\d+)(?:\})?$").expect("waterbend cost regex compiles");
    let blight_cost_re = Regex::new(&format!(r"(?i)^Blight ({})$", count_word_pattern(),))
        .expect("blight cost regex compiles");
    let mut costs = Vec::new();
    let mut decisions = Vec::new();

    for raw_cost in split_activation_cost_atoms(cost_text) {
        let cost = raw_cost
            .trim()
            .trim_start_matches('(')
            .trim_end_matches(')');
        let normalized_loyalty = cost.replace('−', "-").replace("âˆ’", "-");
        if let Some(captures) = Regex::new(r"^([+-]?)(\d+)$")
            .expect("loyalty activation cost regex compiles")
            .captures(&normalized_loyalty)
        {
            let magnitude = captures[2].parse::<i64>().ok()?;
            if captures[1].is_empty() && magnitude != 0 {
                return None;
            }
            costs.push(json!({
                "kind": "payLoyalty",
                "object": self_ref(),
                "amount": integer(if &captures[1] == "-" { -magnitude } else { magnitude }),
            }));
            continue;
        }
        if cost == "{T}" {
            costs.push(json!({ "kind": "tap", "object": self_ref() }));
            continue;
        }
        if Regex::new(r"^(?:\{E\})+$")
            .expect("energy payment regex compiles")
            .is_match(cost)
        {
            costs.push(json!({
                "kind": "payPlayerCounters",
                "player": controller(),
                "counter": "energy",
                "count": integer(cost.matches("{E}").count() as i64),
            }));
            continue;
        }
        if mana_cost_re.is_match(cost) {
            let symbols = Regex::new(r"\{([^}]+)\}")
                .expect("activation mana symbol regex compiles")
                .captures_iter(cost)
                .map(|captures| captures[1].to_ascii_uppercase())
                .collect::<Vec<_>>();
            if symbols
                .iter()
                .any(|symbol| matches!(symbol.as_str(), "T" | "Q" | "Z"))
            {
                return None;
            }
            costs.push(json!({ "kind": "payMana", "manaCost": cost }));
            continue;
        }
        if let Some(captures) = life_cost_re.captures(cost) {
            costs.push(json!({
                "kind": "payLife",
                "player": controller(),
                "amount": integer(captures[1].parse::<i64>().ok()?),
            }));
            continue;
        }
        if let Some(captures) = waterbend_cost_re.captures(cost) {
            costs.push(json!({
                "kind": "payWaterbend",
                "amount": integer(captures[1].parse::<i64>().ok()?),
            }));
            continue;
        }
        if cost.eq_ignore_ascii_case(
            "Pay life equal to the number of colors in your commanders' color identity",
        ) {
            costs.push(json!({
                "kind": "payLife",
                "player": controller(),
                "amount": {
                    "kind": "commanderColorIdentityCount",
                    "player": controller(),
                },
            }));
            continue;
        }
        if cost.eq_ignore_ascii_case("pay X life") {
            costs.push(json!({
                "kind": "payLife",
                "player": controller(),
                "amount": { "kind": "sourceCastXValue" },
            }));
            continue;
        }

        let lower = cost.to_ascii_lowercase();
        if let Some(captures) = blight_cost_re.captures(cost) {
            let count = parse_number_word(captures.get(1)?.as_str())?;
            if count <= 0 {
                return None;
            }
            let decision_id = format!("blightCost{}", decisions.len() + 1);
            let where_filter = card_type("Creature");
            decisions.push(target_decision(
                &decision_id,
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": where_filter.clone(),
                    "ignoreTargetingRestrictions": true,
                }),
                1,
                1,
            ));
            costs.push(json!({
                "kind": "putCounters",
                "permanent": chosen_target(&decision_id),
                "controller": controller(),
                "where": where_filter,
                "counter": "-1/-1",
                "count": integer(count),
            }));
            continue;
        }
        if lower == "tap two untapped creatures you control" {
            let candidates = json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
                "ignoreTargetingRestrictions": true,
            });
            for decision_id in ["tapCreatureCostOne", "tapCreatureCostTwo"] {
                decisions.push(target_decision(decision_id, candidates.clone(), 1, 1));
                costs.push(json!({
                    "kind": "tap",
                    "object": chosen_target(decision_id),
                }));
            }
            continue;
        }
        let tap_one_permanent_re = Regex::new(r"^tap an untapped ([a-z][a-z '-]*) you control$")
            .expect("tap permanent activation cost regex compiles");
        if let Some(captures) = tap_one_permanent_re.captures(&lower) {
            let criteria = captures[1]
                .split_whitespace()
                .map(|word| {
                    let mut chars = word.chars();
                    chars
                        .next()
                        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join(" ");
            let decision_id = format!("tapSubtypeCost{}", decisions.len() + 1);
            decisions.push(target_decision(
                &decision_id,
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": and(vec![
                        parse_permanent_criteria(&criteria, "")?,
                        not(json!({ "kind": "isTapped" })),
                    ]),
                    "ignoreTargetingRestrictions": true,
                }),
                1,
                1,
            ));
            costs.push(json!({
                "kind": "tap",
                "object": chosen_target(&decision_id),
            }));
            continue;
        }
        let remove_counter_re = Regex::new(&format!(
            r"(?i)^remove ({}) ([^ ]+) counters? from (.+)$",
            count_word_pattern(),
        ))
        .expect("remove activation counter cost regex compiles");
        if let Some(captures) = remove_counter_re.captures(cost) {
            let source_reference = captures.get(3)?.as_str().trim();
            let explicit_source = matches!(
                source_reference.to_ascii_lowercase().as_str(),
                "it" | "this artifact" | "this creature" | "this enchantment" | "this permanent"
            );
            let named_source = source_reference
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
                && !source_reference
                    .to_ascii_lowercase()
                    .contains(" you control")
                && !source_reference.to_ascii_lowercase().starts_with("target ");
            if !explicit_source && !named_source {
                return None;
            }
            let count = parse_number_word(&captures[1])?;
            costs.push(json!({
                "kind": "removeCounters",
                "permanent": self_ref(),
                "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
                "count": integer(count),
            }));
            continue;
        }
        let remove_unspecified_counter_re = Regex::new(&format!(
            r"(?i)^remove ({}) counters? from (.+)$",
            count_word_pattern(),
        ))
        .expect("remove unspecified activation counter cost regex compiles");
        if let Some(captures) = remove_unspecified_counter_re.captures(cost) {
            let source_reference = captures.get(2)?.as_str().trim();
            let explicit_source = matches!(
                source_reference.to_ascii_lowercase().as_str(),
                "it" | "this artifact"
                    | "this creature"
                    | "this enchantment"
                    | "this land"
                    | "this permanent"
            );
            let named_source = source_reference
                .chars()
                .next()
                .is_some_and(char::is_uppercase)
                && !source_reference
                    .to_ascii_lowercase()
                    .contains(" you control")
                && !source_reference.to_ascii_lowercase().starts_with("target ");
            if !explicit_source && !named_source {
                return None;
            }
            let count = parse_number_word(captures.get(1)?.as_str())?;
            if count <= 0 {
                return None;
            }
            let decision_id = format!("removeCounterCost{}", costs.len() + 1);
            costs.push(json!({
                "kind": "removeCounters",
                "permanent": self_ref(),
                "counter": decision_result(&decision_id),
                "count": integer(count),
            }));
            continue;
        }
        let discard_count = match lower.as_str() {
            "discard a card" | "discard one card" => Some(1),
            "discard two cards" => Some(2),
            "discard three cards" => Some(3),
            _ => None,
        };
        if let Some(discard_count) = discard_count {
            for _ in 0..discard_count {
                let decision_id = format!("discardCost{}", decisions.len() + 1);
                decisions.push(target_decision(
                    &decision_id,
                    json!({
                        "kind": "cards",
                        "zone": { "kind": "hand", "player": controller() },
                        "where": Value::Null,
                    }),
                    1,
                    1,
                ));
                costs.push(json!({
                    "kind": "discardCard",
                    "card": chosen_target(&decision_id),
                }));
            }
            continue;
        }
        let discard_qualified_card_re = Regex::new(
            r"^discard (?:a|an|one) (.+?) card( with the same name as a legendary permanent you control)?$",
        )
            .expect("qualified discard activation cost regex compiles");
        if let Some(captures) = discard_qualified_card_re.captures(&lower) {
            let decision_id = format!("discardCost{}", decisions.len() + 1);
            let criteria_text = captures.get(1)?.as_str();
            let mut candidates = json!({
                "kind": "cards",
                "zone": { "kind": "hand", "player": controller() },
                "where": color_filter(criteria_text)
                    .or_else(|| card_qualifier_list_filter(criteria_text, ""))
                    .or_else(|| parse_permanent_criteria(criteria_text, ""))?,
            });
            if captures.get(2).is_some() {
                candidates["sameNameAsControlledLegendary"] = Value::Bool(true);
            }
            decisions.push(target_decision(&decision_id, candidates, 1, 1));
            costs.push(json!({
                "kind": "discardCard",
                "card": chosen_target(&decision_id),
            }));
            continue;
        }
        if lower == "discard this card" {
            costs.push(json!({
                "kind": "discardCard",
                "card": self_ref(),
            }));
            continue;
        }
        let exile_qualified_hand_card_re =
            Regex::new(r"^exile (?:a|an|one) (.+?) card from your hand$")
                .expect("qualified hand-card exile cost regex compiles");
        if let Some(captures) = exile_qualified_hand_card_re.captures(&lower) {
            let decision_id = format!("exileHandCost{}", decisions.len() + 1);
            decisions.push(target_decision(
                &decision_id,
                json!({
                    "kind": "cards",
                    "zone": { "kind": "hand", "player": controller() },
                    "where": color_filter(&captures[1])
                        .or_else(|| parse_permanent_criteria(&captures[1], ""))?,
                }),
                1,
                1,
            ));
            costs.push(json!({
                "kind": "exileCard",
                "card": chosen_target(&decision_id),
            }));
            continue;
        }
        let exile_source_re = Regex::new(
            r"^exile this (card from your hand|artifact|creature|enchantment|permanent)$",
        )
        .expect("source-exile activation cost regex compiles");
        if let Some(captures) = exile_source_re.captures(&lower) {
            costs.push(json!({
                "kind": "exileSource",
                "object": self_ref(),
                "zone": if captures[1].eq_ignore_ascii_case("card from your hand") {
                    "hand"
                } else {
                    "battlefield"
                },
            }));
            continue;
        }
        let exile_top_graveyard_re = Regex::new(r"^exile the top (.+?) card of your graveyard$")
            .expect("top graveyard-card activation cost regex compiles");
        if let Some(captures) = exile_top_graveyard_re.captures(&lower) {
            costs.push(json!({
                "kind": "exileTopMatchingGraveyardCard",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], "")?,
            }));
            continue;
        }
        let exile_graveyard_re = Regex::new(r"^exile (?:a|an) (.+?) card from your graveyard$")
            .expect("graveyard-card activation cost regex compiles");
        if let Some(captures) = exile_graveyard_re.captures(&lower) {
            let decision_id = format!("exileGraveyardCost{}", decisions.len() + 1);
            decisions.push(target_decision(
                &decision_id,
                json!({
                    "kind": "cards",
                    "zone": { "kind": "graveyard", "player": controller() },
                    "where": parse_permanent_criteria(&captures[1], "")?,
                }),
                1,
                1,
            ));
            costs.push(json!({
                "kind": "exileGraveyardCard",
                "card": chosen_target(&decision_id),
            }));
            continue;
        }
        let return_controlled_re = Regex::new(&format!(
            r"^return ({}) (.+) you control to (?:its|their) owner's hand$",
            count_word_pattern(),
        ))
        .expect("return controlled permanents activation cost regex compiles");
        if let Some(captures) = return_controlled_re.captures(&lower) {
            let count = parse_number_word(&captures[1])?;
            let subtype_name = singular_card_term(&captures[2]);
            let candidates = json!({
                "kind": "permanents",
                "controller": controller(),
                "where": subtype(&subtype_name),
                "ignoreTargetingRestrictions": true,
            });
            for index in 0..count {
                let decision_id = format!("returnPermanentCost{}", index + 1);
                decisions.push(target_decision(&decision_id, candidates.clone(), 1, 1));
                costs.push(json!({
                    "kind": "returnPermanentToOwnersHand",
                    "permanent": chosen_target(&decision_id),
                }));
            }
            continue;
        }
        let Some(sacrifice_lower) = lower.strip_prefix("sacrifice ") else {
            return None;
        };
        let sacrifice_text = cost
            .get(cost.len().saturating_sub(sacrifice_lower.len())..)
            .unwrap_or(sacrifice_lower)
            .trim();
        if sacrifice_lower.starts_with("this ") {
            costs.push(json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            }));
            continue;
        }
        if !sacrifice_lower.starts_with("a ")
            && !sacrifice_lower.starts_with("an ")
            && !sacrifice_lower.starts_with("another ")
        {
            costs.push(json!({
                "kind": "sacrificePermanent",
                "permanent": self_ref(),
            }));
            continue;
        }

        let exclude_source = sacrifice_lower.starts_with("another ");
        let criteria_text = sacrifice_text
            .strip_prefix("another ")
            .or_else(|| sacrifice_text.strip_prefix("Another "))
            .map(str::trim)
            .unwrap_or_else(|| strip_leading_article(sacrifice_text));
        let filter = parse_permanent_criteria(criteria_text, "")?;
        let decision_id = format!("sacrificeCost{}", decisions.len() + 1);
        let mut candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": filter,
        });
        if exclude_source {
            candidates["excludeSource"] = Value::Bool(true);
        }
        decisions.push(target_decision(&decision_id, candidates, 1, 1));
        costs.push(json!({
            "kind": "sacrificePermanent",
            "permanent": chosen_target(&decision_id),
        }));
    }

    (!costs.is_empty()).then_some((costs, decisions))
}

pub(super) fn parse_keyword_cost(text: &str, keyword: &str) -> Option<Value> {
    let keyword_text = text
        .split_once(" (")
        .filter(|(_, reminder)| reminder.ends_with(')'))
        .map(|(keyword_text, _)| keyword_text)
        .unwrap_or(text)
        .trim_end_matches('.');
    if !keyword_text
        .get(..keyword.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(keyword))
    {
        return None;
    }
    let remainder = keyword_text.get(keyword.len()..)?.trim_start();
    let remainder = remainder.trim_start_matches(['\u{2014}', '\u{2013}']);
    let cost_text = ["ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â", "Ã¢â‚¬â€", "â€”", "—", "–", "-"]
        .into_iter()
        .find_map(|separator| remainder.strip_prefix(separator))
        .unwrap_or(remainder)
        .trim();
    let (mut costs, decisions) = parse_activation_costs(cost_text)?;
    if costs.len() != 1 {
        return None;
    }
    let mut cost = costs.remove(0);
    if decisions.is_empty() {
        return Some(cost);
    }
    if decisions.len() != 1 {
        return None;
    }
    let decision = &decisions[0];
    let decision_id = decision["id"].as_str()?;
    let selected_reference = match cost["kind"].as_str() {
        Some("sacrificePermanent") => &cost["permanent"],
        Some("putCounters") => &cost["permanent"],
        Some("discardCard" | "exileCard") => &cost["card"],
        Some("tap") => &cost["object"],
        _ => return None,
    };
    if selected_reference["kind"].as_str() != Some("chosenTarget")
        || selected_reference["id"].as_str() != Some(decision_id)
    {
        return None;
    }
    let candidates = &decision["candidates"];
    match cost["kind"].as_str() {
        Some("sacrificePermanent") => {
            cost.as_object_mut()?.remove("permanent");
            cost["where"] = candidates["where"].clone();
            if candidates["excludeSource"].as_bool() == Some(true) {
                cost["excludeSource"] = Value::Bool(true);
            }
        }
        Some("putCounters") => {
            cost.as_object_mut()?.remove("permanent");
            cost["where"] = candidates["where"].clone();
        }
        Some("discardCard") => {
            cost.as_object_mut()?.remove("card");
            cost["where"] = candidates["where"].clone();
        }
        Some("exileCard") => {
            cost.as_object_mut()?.remove("card");
            cost["zone"] = candidates["zone"].clone();
            cost["where"] = candidates["where"].clone();
        }
        Some("tap") => {
            cost.as_object_mut()?.remove("object");
            cost["where"] = candidates["where"].clone();
        }
        _ => return None,
    }
    Some(cost)
}

pub(super) fn parse_resolution_cost_text(cost_text: &str) -> Option<Value> {
    let normalized = cost_text
        .trim()
        .trim_end_matches('.')
        .trim_end_matches(" of their choice")
        .trim();
    if let Some(cost) = parse_keyword_cost(&format!("Cost\u{2014}{normalized}"), "Cost") {
        return Some(cost);
    }
    let criteria = normalized.strip_prefix("sacrifice ")?;
    Some(json!({
        "kind": "sacrificePermanent",
        "where": parse_permanent_criteria(criteria, "")?,
    }))
}

pub(super) fn parse_alternative_cost_ability(text: &str) -> Option<CanonicalRuleDraft> {
    let composite_re = Regex::new(
        r"(?i)^You may pay (\d+) life and exile (?:an? )?(.+?) card from your hand rather than pay this spell's mana cost\.$",
    )
    .expect("composite nonmana alternative cost regex compiles");
    let ability = if let Some(cost) = parse_keyword_cost(text, "Evoke") {
        json!({
            "kind": "alternativeCost",
            "mode": "evoke",
            "costs": [cost],
        })
    } else if let Some(captures) = composite_re.captures(text) {
        json!({
            "kind": "alternativeCost",
            "costs": [
                {
                    "kind": "payLife",
                    "player": controller(),
                    "amount": integer(captures[1].parse::<i64>().ok()?),
                },
                {
                    "kind": "exileCard",
                    "zone": hand(controller()),
                    "where": color_filter(&captures[2])
                        .or_else(|| parse_permanent_criteria(&captures[2], ""))?,
                },
            ],
        })
    } else if let Some(captures) = Regex::new(
        r"(?i)^(If it's not your turn, )?you may exile (?:an? )?(.+?) card from your hand rather than pay this spell's mana cost\.$",
    )
    .expect("hand-exile alternative cost regex compiles")
    .captures(text)
    {
        let mut ability = json!({
            "kind": "alternativeCost",
            "costs": [{
                "kind": "exileCard",
                "zone": hand(controller()),
                "where": color_filter(&captures[2])
                    .or_else(|| parse_permanent_criteria(&captures[2], ""))?,
            }],
        });
        if captures.get(1).is_some() {
            ability["condition"] = json!({ "kind": "notYourTurn" });
        }
        ability
    } else if let Some(captures) = Regex::new(
        r"(?i)^You may return (?:an? )?(.+?) you control to its owner's hand rather than pay this spell's mana cost\.$",
    )
    .expect("controlled-permanent return alternative cost regex compiles")
    .captures(text)
    {
        json!({
            "kind": "alternativeCost",
            "costs": [{
                "kind": "returnPermanentToOwnersHand",
                "where": parse_permanent_criteria(&captures[1], "")?,
            }],
        })
    } else if let Some(captures) = Regex::new(
        r"(?i)^If you control (?:an? )?(.+?), you may pay (\d+) life rather than pay this spell's mana cost\.$",
    )
    .expect("controlled-permanent life alternative cost regex compiles")
    .captures(text)
    {
        json!({
            "kind": "alternativeCost",
            "condition": {
                "kind": "controlsPermanent",
                "where": parse_permanent_criteria(&captures[1], "")?,
            },
            "costs": [{
                "kind": "payLife",
                "player": controller(),
                "amount": integer(captures[2].parse::<i64>().ok()?),
            }],
        })
    } else if let Some(captures) = Regex::new(
        r"(?i)^If an opponent cast (\w+) or more spells this turn, you may pay ((?:\{[^}]+\})+) rather than pay this spell's mana cost\.$",
    )
    .expect("opponent-spell-count alternative cost regex compiles")
    .captures(text)
    {
        let (costs, decisions) = parse_activation_costs(&captures[2])?;
        if !decisions.is_empty() {
            return None;
        }
        json!({
            "kind": "alternativeCost",
            "condition": {
                "kind": "opponentCastSpellsThisTurn",
                "minimum": integer(parse_number_word(&captures[1])?),
            },
            "costs": costs,
        })
    } else if let Some(captures) = Regex::new(
        r"(?i)^If an opponent searched their library this turn, you may pay ((?:\{[^}]+\})+) rather than pay this spell's mana cost\.$",
    )
    .expect("opponent-library-search alternative cost regex compiles")
    .captures(text)
    {
        let (costs, decisions) = parse_activation_costs(&captures[1])?;
        if !decisions.is_empty() {
            return None;
        }
        json!({
            "kind": "alternativeCost",
            "condition": { "kind": "opponentSearchedLibraryThisTurn" },
            "costs": costs,
        })
    } else if let Some(captures) = Regex::new(
        r"(?i)^If (.+?), you may cast (?:this spell|it) without paying its mana cost\.$",
    )
    .expect("conditional free-casting alternative cost regex compiles")
    .captures(text)
    {
        json!({
            "kind": "alternativeCost",
            "condition": parse_condition_text(&captures[1])?,
            "costs": [{
                "kind": "payMana",
                "manaCost": "{0}",
            }],
        })
    } else {
        return None;
    };
    Some(draft(
        json!({
            "kind": "keywordAbility",
            "source": self_ref(),
            "ability": ability,
        }),
        &[
            "Parse the alternative-cost condition",
            "Delegate each payment to the shared cost and criteria grammars",
            "Replace only the spell's base mana cost",
        ],
    ))
}

pub(super) fn parse_buyback_ability(text: &str) -> Option<CanonicalRuleDraft> {
    let keyword_text = text
        .split_once(" (")
        .filter(|(_, reminder)| reminder.ends_with(')'))
        .map(|(keyword_text, _)| keyword_text)
        .unwrap_or(text)
        .trim_end_matches('.');
    let remainder = keyword_text.get("Buyback".len()..)?;
    if !keyword_text[.."Buyback".len()].eq_ignore_ascii_case("Buyback") {
        return None;
    }
    let cost_text = [
        "ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â€šÂ¬Ã‚Â",
        "ÃƒÂ¢Ã¢â€šÂ¬Ã¢â‚¬Â",
        "Ã¢â‚¬â€",
        "â€”",
        "â€“",
        "—",
        "–",
        "-",
    ]
    .into_iter()
    .find_map(|separator| remainder.trim_start().strip_prefix(separator))
    .unwrap_or(remainder)
    .trim();
    let (costs, mut cost_decisions) = parse_activation_costs(cost_text)?;
    for decision in &mut cost_decisions {
        decision["condition"] = selection("buybackMode", "buyback");
    }
    let mut decisions = vec![json!({
        "id": "buybackMode",
        "kind": "chooseModes",
        "minimum": 1,
        "maximum": 1,
        "options": ["decline", "buyback"],
    })];
    decisions.extend(cost_decisions);
    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
                "additionalCosts": [{
                    "kind": "conditional",
                    "condition": selection("buybackMode", "buyback"),
                    "then": costs,
                    "else": [],
                }],
            },
            "effects": [],
        }),
        &[
            "Parse buyback through the shared cost grammar",
            "Offer the optional additional cost while casting",
            "Return the spell to hand after resolution when paid",
        ],
    ))
}

pub(super) fn parse_cumulative_upkeep_ability(text: &str) -> Option<CanonicalRuleDraft> {
    let mut cost = parse_keyword_cost(text, "Cumulative upkeep")?;
    if let Some(amount) = cost.get_mut("amount") {
        *amount = json!({
            "kind": "multiply",
            "left": amount.clone(),
            "right": {
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "age",
            },
        });
    } else if let Some(count) = cost.get_mut("count") {
        *count = json!({
            "kind": "multiply",
            "left": count.clone(),
            "right": {
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "age",
            },
        });
    }
    Some(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "stepBegan", "step": "upkeep", "player": controller() },
            "effects": [
                {
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "age",
                    "count": integer(1),
                },
                {
                    "kind": "payCostOrMoveSource",
                    "cost": cost,
                    "otherwise": "sacrifice",
                },
            ],
        }),
        &[
            "Add one age counter at upkeep",
            "Scale the reusable upkeep cost by the age-counter count",
            "Sacrifice the source when the payment is declined",
        ],
    ))
}
