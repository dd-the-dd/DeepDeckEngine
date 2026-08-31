use super::super::*;

pub(in crate::oracle::canonical) fn fixed_mana_sequence(instruction: &str) -> Option<String> {
    let captures = Regex::new(r"(?i)^Add ((?:\{(?:W|U|B|R|G|C)\})+)\.$")
        .expect("fixed mana sequence regex compiles")
        .captures(instruction.trim())?;
    Some(captures[1].to_ascii_uppercase())
}

pub(in crate::oracle::canonical) fn parse_direct_add_mana_effect(
    instruction: &str,
) -> Option<Value> {
    let instruction = instruction.trim();
    if let Some(mana) = fixed_mana_sequence(instruction) {
        return Some(json!({
            "kind": "addMana",
            "player": controller(),
            "mana": mana,
        }));
    }

    let fixed_mana_for_each_re = Regex::new(r"(?i)^Add \{(W|U|B|R|G|C)\} for each (.+)\.$")
        .expect("fixed mana for each numeric expression regex compiles");
    if let Some(captures) = fixed_mana_for_each_re.captures(instruction) {
        let count_text = format!("the number of {}", captures.get(2)?.as_str());
        return Some(json!({
            "kind": "addMana",
            "player": controller(),
            "mana": {
                "kind": "fixedMana",
                "symbol": captures.get(1)?.as_str().to_ascii_uppercase(),
                "amount": parse_numeric_expression_text(&count_text)?,
            },
        }));
    }

    let any_color_re = Regex::new(&format!(
        r"(?i)^Add ({}) mana (of any (?:one )?color|in any combination of colors)\.(?: Spend this mana only to cast (?:an? )?.+? spells?\.)?$",
        count_word_pattern(),
    ))
    .expect("direct any-color mana effect regex compiles");
    let captures = any_color_re.captures(instruction)?;
    let amount = integer(parse_number_word(captures.get(1)?.as_str())?);
    let combination = captures
        .get(2)?
        .as_str()
        .eq_ignore_ascii_case("in any combination of colors");
    let mut effect = json!({
        "kind": "addMana",
        "player": controller(),
        "mana": {
            "kind": if combination { "chooseColors" } else { "chooseColor" },
            "amount": amount,
        },
    });
    if instruction.contains("Spend this mana only to cast") {
        effect["spendRestriction"] = json!({
            "kind": "castSpell",
            "where": mana_cast_restriction_filter(instruction)?,
        });
    }
    Some(effect)
}

pub(in crate::oracle::canonical) fn parse_mana_ability(text: &str) -> Option<CanonicalRuleDraft> {
    let normalized = text.trim().trim_start_matches('(').trim_end_matches(')');
    let normalized = normalized
        .strip_prefix("Ferocious â€” ")
        .or_else(|| normalized.strip_prefix("Ferocious — "))
        .unwrap_or(normalized);
    let normalized = strip_short_oracle_label(normalized);
    let (cost_text, effect_text) = normalized.split_once(':')?;
    if !effect_text.trim_start().starts_with("Add ")
        && !effect_text.trim_start().starts_with("Choose a color. Add ")
    {
        return None;
    }
    let (costs, cost_decisions) = parse_activation_costs(cost_text)?;

    let effect_text = effect_text.trim();
    let first_effect_sentence = effect_text.split('.').next().unwrap_or(effect_text);
    let mana_symbols = Regex::new(r"\{[^}]+\}")
        .expect("mana symbol regex compiles")
        .find_iter(first_effect_sentence)
        .map(|matched| matched.as_str().to_string())
        .collect::<Vec<_>>();
    let any_color_re = Regex::new(&format!(
        r"^Add ({}) mana (?:of any (?:one )?color|in any combination of colors)",
        count_word_pattern(),
    ))
    .expect("any-color mana regex compiles");
    let any_color_amount = any_color_re
        .captures(effect_text)
        .and_then(|captures| parse_number_word(&captures[1]));
    let any_color = any_color_amount.is_some();
    let commander_color_identity = effect_text.contains("commander's color identity");
    let controlled_land_mana_type_re = Regex::new(&format!(
        r"^Add ({}) mana of any type that a land you control could produce",
        count_word_pattern(),
    ))
    .expect("controlled-land mana-type regex compiles");
    let controlled_land_mana_type_amount = controlled_land_mana_type_re
        .captures(effect_text)
        .and_then(|captures| parse_number_word(&captures[1]));
    let sacrificed_permanent_mana_re = Regex::new(&format!(
        r"(?i)^Add ({}) mana of any type the sacrificed (?:artifact|creature|land|permanent) could produce\.?$",
        count_word_pattern(),
    ))
    .expect("sacrificed-permanent mana-type regex compiles");
    let sacrificed_permanent_mana =
        sacrificed_permanent_mana_re
            .captures(effect_text)
            .and_then(|captures| {
                let amount = parse_number_word(captures.get(1)?.as_str())?;
                let decision_id = costs.iter().find_map(|cost| {
                    (cost["kind"].as_str() == Some("sacrificePermanent"))
                        .then(|| cost["permanent"]["id"].as_str())
                        .flatten()
                })?;
                Some((amount, decision_id.to_string()))
            });
    let opponents_land_mana_color_re = Regex::new(&format!(
        r"^Add ({}) mana of any color that a land an opponent controls could produce",
        count_word_pattern(),
    ))
    .expect("opponents-land mana-color regex compiles");
    let opponents_land_mana_color_amount = opponents_land_mana_color_re
        .captures(effect_text)
        .and_then(|captures| parse_number_word(&captures[1]));
    let linked_exiled_card_colors =
        Regex::new(r"(?i)^Add one mana of any of the exiled card's colors\.?$")
            .expect("linked exiled-card mana color regex compiles")
            .is_match(effect_text);
    let variable_fixed_mana_re = Regex::new(&format!(
        r"^Add an amount of \{{(W|U|B|R|G|C)\}} equal to ({})",
        variable_clause_pattern(),
    ))
    .expect("variable fixed-color mana regex compiles");
    let variable_fixed_mana = variable_fixed_mana_re
        .captures(effect_text)
        .and_then(|captures| {
            Some((
                captures.get(1)?.as_str().to_ascii_uppercase(),
                x_variable_expression(captures.get(2)?.as_str())?,
            ))
        });
    let variable_any_color_re = Regex::new(&format!(
        r"^Add X mana of any one color, where X is ({})",
        variable_clause_pattern(),
    ))
    .expect("variable any-color mana regex compiles");
    let variable_any_color = variable_any_color_re
        .captures(effect_text)
        .and_then(|captures| x_variable_expression(&captures[1]));
    let chosen_type_mana_re = Regex::new(&format!(
        r"^Choose a color\. Add an amount of mana of that color equal to ({})",
        variable_clause_pattern(),
    ))
    .expect("chosen-type variable mana regex compiles");
    let chosen_type_amount = chosen_type_mana_re
        .captures(effect_text)
        .and_then(|captures| x_variable_expression(&captures[1]));
    if !any_color
        && variable_any_color.is_none()
        && chosen_type_amount.is_none()
        && variable_fixed_mana.is_none()
        && controlled_land_mana_type_amount.is_none()
        && sacrificed_permanent_mana.is_none()
        && opponents_land_mana_color_amount.is_none()
        && !linked_exiled_card_colors
        && mana_symbols.is_empty()
    {
        return None;
    }

    let restricted = effect_text.contains("Spend this mana only to cast");
    let restriction = if effect_text.contains("creature spell of the chosen type") {
        Some(json!({
            "kind": "castSpell",
            "where": card_type("Creature"),
            "chosenCreatureTypeFromSource": true,
            "cantBeCountered": effect_text.contains("that spell can't be countered"),
        }))
    } else {
        restricted
            .then(|| mana_cast_restriction_filter(effect_text))
            .flatten()
            .and_then(|restriction_filter| {
                let activates_sources_re =
                    Regex::new(r"(?i)(?:and|or) activate abilities of (.+?) sources?\.")
                        .expect("mana activation-source restriction regex compiles");
                if let Some(captures) = activates_sources_re.captures(effect_text) {
                    let activation_filter =
                        parse_permanent_criteria(captures.get(1)?.as_str(), "")?;
                    if activation_filter != restriction_filter {
                        return None;
                    }
                    return Some(json!({
                        "kind": "castSpellOrActivateAbility",
                        "where": restriction_filter,
                    }));
                }
                json!({
                    "kind": "castSpell",
                    "where": restriction_filter,
                })
                .into()
            })
    };
    if restricted && restriction.is_none() {
        return None;
    }
    let mana = if linked_exiled_card_colors {
        json!({ "kind": "linkedExiledCardColors", "amount": integer(1) })
    } else if let Some((symbol, amount)) = variable_fixed_mana {
        json!({
            "kind": "fixedMana",
            "symbol": symbol,
            "amount": amount,
        })
    } else if let Some(amount) = chosen_type_amount {
        json!({
            "kind": "chooseColor",
            "amount": amount,
        })
    } else if let Some(amount) = controlled_land_mana_type_amount {
        json!({
            "kind": "manaTypesLandsYouControlCouldProduce",
            "amount": amount,
        })
    } else if let Some((amount, decision_id)) = sacrificed_permanent_mana {
        json!({
            "kind": "manaTypesOfSacrificedPermanent",
            "amount": integer(amount),
            "decisionId": decision_id,
        })
    } else if let Some(amount) = opponents_land_mana_color_amount {
        json!({
            "kind": "manaColorsLandsOpponentsControlCouldProduce",
            "amount": amount,
        })
    } else if let Some(amount) = variable_any_color {
        json!({
            "kind": "chooseColor",
            "amount": amount,
        })
    } else if commander_color_identity {
        json!({
            "kind": "chooseCommanderColor",
            "amount": any_color_amount.unwrap_or(1),
        })
    } else if any_color {
        let mut value = json!({
            "kind": "chooseColor",
            "amount": any_color_amount.unwrap_or(1),
        });
        if let Some(restriction) = &restriction {
            value["spendRestriction"] = restriction.clone();
        }
        value
    } else if first_effect_sentence.contains(" or ") {
        json!({
            "kind": "chooseOne",
            "options": mana_symbols,
        })
    } else {
        Value::String(mana_symbols.join(""))
    };
    let mut effect = json!({
        "kind": "addMana",
        "player": controller(),
        "mana": mana,
    });
    if !any_color && let Some(restriction) = restriction {
        effect["spendRestriction"] = restriction;
    }

    let activation_condition = Regex::new(r"(?i)Activate only if (.+?)\.")
        .expect("mana activation condition regex compiles")
        .captures(effect_text)
        .and_then(|captures| parse_controlled_permanent_condition(&captures[1], ""));

    let mut rule = json!({
        "kind": "activatedAbility",
        "source": self_ref(),
        "costs": costs,
        "effects": [effect],
    });
    if !cost_decisions.is_empty() {
        rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": cost_decisions,
        });
    }
    if let Some(condition) = activation_condition {
        rule.as_object_mut()
            .expect("mana ability is object")
            .insert("activationCondition".to_string(), condition);
        let object = rule.as_object_mut().expect("mana ability is object");
        let costs = object.remove("costs").expect("mana costs");
        let effects = object.remove("effects").expect("mana effects");
        object.insert("costs".to_string(), costs);
        object.insert("effects".to_string(), effects);
    }
    if effect_text.contains("Activate only once each turn.") {
        rule["activationLimit"] = json!({
            "kind": "oncePerTurn",
            "id": "manaAbility",
        });
    }
    if costs.iter().any(|cost| {
        cost["kind"] == "discardCard" && cost["card"]["kind"] == "self"
            || cost["kind"] == "exileSource" && cost["zone"] == "hand"
    }) {
        rule["activationZone"] = Value::String("hand".to_string());
    }

    Some(draft(
        rule,
        &[
            "Parse activated ability",
            "Resolve activation costs",
            "Resolve produced mana",
        ],
    ))
}

pub(in crate::oracle::canonical) fn contains_rule_kind(value: &Value, expected: &str) -> bool {
    if value["kind"].as_str() == Some(expected) {
        return true;
    }
    match value {
        Value::Array(values) => values
            .iter()
            .any(|value| contains_rule_kind(value, expected)),
        Value::Object(object) => object
            .values()
            .any(|value| contains_rule_kind(value, expected)),
        _ => false,
    }
}

pub(in crate::oracle::canonical) fn promote_activated_mana_ability(
    mut parsed: CanonicalRuleDraft,
) -> CanonicalRuleDraft {
    let effects = parsed.rule["effects"].as_array();
    let could_add_mana = effects.is_some_and(|effects| {
        effects
            .iter()
            .any(|effect| contains_rule_kind(effect, "addMana"))
    });
    let effect_requires_target = effects.is_some_and(|effects| {
        effects
            .iter()
            .any(|effect| contains_rule_kind(effect, "chosenTarget"))
    });
    let loyalty_ability = parsed.rule["costs"].as_array().is_some_and(|costs| {
        costs
            .iter()
            .any(|cost| contains_rule_kind(cost, "payLoyalty"))
    });
    if parsed.rule["kind"] == "activatedAbility"
        && could_add_mana
        && !effect_requires_target
        && !loyalty_ability
    {
        parsed.rule["kind"] = Value::String("manaAbility".to_string());
        parsed
            .operations
            .push("Apply mana-ability criteria from rule 605.1a".to_string());
    }
    parsed
}
