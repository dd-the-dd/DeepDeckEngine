use super::super::*;

pub(in crate::oracle::canonical) fn create_token_effect(text: &str) -> Option<Value> {
    let normalized;
    let text = if let Some((instruction, reminder)) = text.rsplit_once(" (")
        && reminder.ends_with(')')
    {
        normalized = instruction.to_string();
        normalized.as_str()
    } else {
        text
    };
    let actor_normalized;
    let text = if let Some(rest) = text
        .strip_prefix("You create ")
        .or_else(|| text.strip_prefix("you create "))
    {
        actor_normalized = format!("create {rest}");
        actor_normalized.as_str()
    } else {
        text
    };
    let token_ability_sentence_re = Regex::new(
        r#"(?i)^(create .+? creature tokens?)\. (?:it has|they have|those tokens have) \"([^\"]+)\"\.?$"#,
    )
    .expect("separate token ability sentence regex compiles");
    let token_with_ability;
    let text = if let Some(captures) = token_ability_sentence_re.captures(text) {
        token_with_ability = format!("{} with \"{}\".", &captures[1], &captures[2]);
        token_with_ability.as_str()
    } else {
        text
    };
    let tapped_token_re = Regex::new(&format!(
        r"(?i)^create (?:{}) tapped ",
        quantity_word_pattern(),
    ))
    .expect("tapped token prefix regex compiles");
    let tapped = tapped_token_re.is_match(text);
    let untapped_text;
    let text = if tapped {
        untapped_text = text.replacen(" tapped ", " ", 1);
        untapped_text.as_str()
    } else {
        text
    };
    let named_artifact_with_equip_re = Regex::new(&format!(
        r#"(?i)^create ({}) (colorless|white|blue|black|red|green) ([A-Za-z -]+?) artifact tokens? named ([A-Za-z0-9 ',.-]+) with \"([^\"]+)\" and equip ((?:\{{[^}}]+\}})+)\.$"#,
        quantity_word_pattern(),
    ))
    .expect("named artifact token with embedded ability and equip regex compiles");
    if let Some(captures) = named_artifact_with_equip_re.captures(text) {
        let static_rule = parse_special_static_ability(captures.get(5)?.as_str())?.rule;
        let equip_rule = parse_special_static_ability(&format!("Equip {}", &captures[6]))?.rule;
        let color = captures[2].to_ascii_lowercase();
        return Some(with_token_entry_state(
            json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": parse_quantity_expression(&captures[1])?,
                "token": {
                    "name": captures[4].trim(),
                    "colors": if color == "colorless" { Vec::<String>::new() } else { vec![color] },
                    "types": ["Artifact"],
                    "subtypes": captures[3].split_whitespace().collect::<Vec<_>>(),
                    "power": 0,
                    "toughness": 0,
                    "abilities": [static_rule, equip_rule],
                },
            }),
            tapped,
        ));
    }
    let legendary_named_token_re =
        Regex::new(r#"(?i)^create ([A-Za-z0-9 ',-]+), a legendary .+ creature token with .+\.$"#)
            .expect("legendary named token regex compiles");
    if let Some(captures) = legendary_named_token_re.captures(text)
        && let Some(name) = captures.get(1).map(|value| value.as_str().trim())
        && named_token_printing(name).ok().flatten().is_some()
    {
        return Some(with_token_entry_state(
            json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": { "kind": "namedToken", "name": name },
            }),
            tapped,
        ));
    }
    let legendary_variable_token_re = Regex::new(
        r#"(?i)^create ([A-Za-z0-9 ',-]+), a legendary (white|blue|black|red|green|colorless) ([A-Za-z -]+) creature token with \"(?:[^\"]+)'s power and toughness are each equal to the number of lands you control\.\"$"#,
    )
    .expect("legendary variable-stat token regex compiles");
    if let Some(captures) = legendary_variable_token_re.captures(text) {
        let land_count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": card_type("Land"),
        });
        return Some(with_token_entry_state(
            json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": {
                    "name": captures[1].trim(),
                    "colors": [captures[2].to_ascii_lowercase()],
                    "types": ["Creature"],
                    "subtypes": captures[3].split_whitespace().collect::<Vec<_>>(),
                    "power": 0,
                    "toughness": 0,
                    "abilities": [{
                        "kind": "staticAbility",
                        "source": self_ref(),
                        "activeWhile": active_while_battlefield(),
                        "modifiers": [{
                            "kind": "modifyPowerToughness",
                            "objects": self_ref(),
                            "power": land_count.clone(),
                            "toughness": land_count,
                        }],
                    }],
                },
            }),
            tapped,
        ));
    }
    let named_token_re = Regex::new(&format!(
        r"(?i)^create ({}) ([A-Za-z][A-Za-z '-]*?) tokens?\.?$",
        quantity_word_pattern(),
    ))
    .expect("named token regex compiles");
    let variable_named_token_re = Regex::new(&format!(
        r"(?i)^create X ([A-Za-z][A-Za-z '-]*?) tokens?, where X is ({})\.?$",
        variable_clause_pattern(),
    ))
    .expect("variable named token regex compiles");
    if let Some(captures) = variable_named_token_re.captures(text)
        && let Some(name) = captures.get(1).map(|value| value.as_str().trim())
        && named_token_printing(name).ok().flatten().is_some()
    {
        return Some(with_token_entry_state(
            json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": x_variable_expression(captures.get(2)?.as_str())?,
                "token": { "kind": "namedToken", "name": name },
            }),
            tapped,
        ));
    }
    if let Some(captures) = named_token_re.captures(text)
        && let Some(name) = captures.get(2).map(|value| value.as_str().trim())
        && named_token_printing(name).ok().flatten().is_some()
    {
        return Some(with_token_entry_state(
            json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": parse_quantity_expression(&captures[1])?,
                "token": {
                    "kind": "namedToken",
                    "name": name,
                },
            }),
            tapped,
        ));
    }
    let token_re = Regex::new(&format!(
        r#"(?i)^create ({}) (\d+)/(\d+) ([A-Za-z, ]+?)( artifact)? creature tokens?(?: named ([A-Za-z0-9 ',.-]+))?(?: with (?:"([^"]+)"|'([^']+)'|([^.,]+)))?(?:, where X is ({}))?\.?$"#,
        quantity_word_pattern(),
        variable_clause_pattern(),
    ))
    .expect("creature token regex compiles");
    let captures = token_re.captures(text)?;
    let quantity = if captures[1].eq_ignore_ascii_case("x") {
        captures
            .get(10)
            .and_then(|where_clause| x_variable_expression(where_clause.as_str()))
            .unwrap_or_else(|| json!({ "kind": "sourceCastXValue" }))
    } else {
        parse_quantity_expression(&captures[1])?
    };
    let abilities = captures
        .get(7)
        .or_else(|| captures.get(8))
        .and_then(|ability| parse_embedded_token_rule(ability.as_str()).map(|rule| vec![rule]))
        .or_else(|| {
            captures.get(9).map(|abilities| {
                abilities
                    .as_str()
                    .split(" and ")
                    .filter_map(|ability| {
                        let normalized = ability.trim().to_ascii_lowercase();
                        oracle_keyword_kind(&normalized)
                            .map(|kind| json!({ "kind": kind }))
                            .or_else(|| {
                                Regex::new(r"^firebending (\d+)$")
                                    .expect("token firebending regex compiles")
                                    .captures(&normalized)
                                    .and_then(|captures| {
                                        Some(firebending_ability(integer(
                                            captures[1].parse::<i64>().ok()?,
                                        )))
                                    })
                            })
                    })
                    .collect::<Vec<_>>()
            })
        })
        .unwrap_or_default();
    let (colors, subtypes) = parse_color_prefix(&captures[4])?;
    let mut token = json!({
        "colors": colors,
        "types": if captures.get(5).is_some() {
            vec!["Artifact".to_string(), "Creature".to_string()]
        } else {
            vec!["Creature".to_string()]
        },
        "subtypes": subtypes,
        "power": captures[2].parse::<i64>().ok()?,
        "toughness": captures[3].parse::<i64>().ok()?,
        "abilities": abilities,
    });
    if let Some(name) = captures.get(6) {
        token["name"] = Value::String(name.as_str().to_string());
    }
    let effect = json!({
        "kind": "createTokens",
        "controller": controller(),
        "quantity": quantity,
        "token": token,
    });
    Some(with_token_entry_state(effect, tapped))
}

pub(in crate::oracle::canonical) fn parse_embedded_token_rule(text: &str) -> Option<Value> {
    let normalized = text.trim().trim_end_matches('.');
    if let Some(parsed) = parse_mana_ability(normalized).map(promote_activated_mana_ability) {
        return Some(parsed.rule);
    }
    if let Some(parsed) = parse_simple_activated_ability(normalized) {
        return Some(parsed.rule);
    }
    let scaling_stats_re = Regex::new(
        r"(?i)^This (?:token|creature) gets \+(\d+)/\+(\d+) for each (.+?) you control$",
    )
    .expect("scaling token stats regex compiles");
    if let Some(captures) = scaling_stats_re.captures(normalized) {
        let counted = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&singular_card_term(&captures[3]), "")?,
        });
        let power_factor = captures[1].parse::<i64>().ok()?;
        let toughness_factor = captures[2].parse::<i64>().ok()?;
        return Some(json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "modifyPowerToughness",
                "objects": self_ref(),
                "power": if power_factor == 1 {
                    counted.clone()
                } else {
                    json!({ "kind": "multiply", "left": integer(power_factor), "right": counted.clone() })
                },
                "toughness": if toughness_factor == 1 {
                    counted.clone()
                } else {
                    json!({ "kind": "multiply", "left": integer(toughness_factor), "right": counted })
                },
            }],
        }));
    }
    if normalized.eq_ignore_ascii_case("Whenever this token attacks, you may mill a card") {
        return Some(json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "declaredAttacker", "object": self_ref() },
            "effects": [{
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [],
            }],
        }));
    }
    None
}
