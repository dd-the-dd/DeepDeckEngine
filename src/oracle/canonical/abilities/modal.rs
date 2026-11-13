use super::super::*;

pub(in crate::oracle::canonical) fn replace_decision_ids(
    value: &mut Value,
    replacements: &[(String, String)],
) {
    match value {
        Value::Array(values) => {
            for value in values {
                replace_decision_ids(value, replacements);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                replace_decision_ids(value, replacements);
            }
        }
        Value::String(value) => {
            if let Some((_, replacement)) =
                replacements.iter().find(|(original, _)| original == value)
            {
                *value = replacement.clone();
            }
        }
        _ => {}
    }
}

pub(in crate::oracle::canonical) fn modal_bullet_instruction(line: &str) -> Option<&str> {
    if let Some(instruction) = line.trim().strip_prefix("\u{2022}") {
        return Some(instruction.trim());
    }
    let line = line.trim();
    line.strip_prefix('•')
        .or_else(|| line.strip_prefix("â€¢"))
        .map(str::trim)
}

pub(in crate::oracle::canonical) fn parse_general_modal_spell(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    // Oracle text normally renders modal bullets on separate lines, but some
    // data sources flatten the same structure to `Choose ... — • ... • ...`.
    // Normalize both surface forms before parsing the modal grammar.
    let inline_lines = if !text.contains('\n') && text.contains('•') {
        let mut parts = text.split('•');
        let header = parts
            .next()?
            .trim()
            .trim_end_matches([' ', '-', '—'])
            .trim()
            .to_string();
        let mut lines = vec![header];
        lines.extend(parts.map(|part| format!("• {}", part.trim())));
        Some(lines)
    } else {
        None
    };
    let lines = inline_lines
        .as_ref()
        .map(|lines| lines.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_else(|| text.lines().map(str::trim).collect::<Vec<_>>());
    if lines.len() < 3 {
        return None;
    }
    let header = lines[0].trim_end_matches([' ', 'â', '€', '”', '—']).trim();
    let header = header.trim_end_matches([' ', '-', '—']).trim();
    let repeated_header_re = Regex::new(&format!(
        r"(?i)^Choose ({}). You may choose the same mode more than once\.$",
        count_word_pattern(),
    ))
    .expect("repeatable modal header regex compiles");
    let conditional_maximum_header_re = Regex::new(&format!(
        r"(?i)^Choose one\. If you control (?:a|an) (.+?) as you cast this spell, you may choose ({}) instead\.$",
        count_word_pattern(),
    ))
    .expect("conditional modal maximum header regex compiles");
    let teamwork_maximum_header_re = Regex::new(
        r"(?i)^Choose one\. If this spell was cast using teamwork, choose both instead\.$",
    )
    .expect("teamwork modal maximum header regex compiles");
    let (minimum, maximum_kind, allow_repeated, conditional_maximum) =
        if let Some(captures) = repeated_header_re.captures(header) {
            let count = parse_number_word(&captures[1])?;
            (count, Some(count), true, None)
        } else if teamwork_maximum_header_re.is_match(header) {
            (
                1,
                None,
                false,
                Some(json!({
                    "kind": "conditionalValue",
                    "condition": {
                        "kind": "decisionWas",
                        "decisionId": "teamworkPaid",
                        "value": true,
                    },
                    "ifTrue": integer(2),
                    "ifFalse": integer(1),
                })),
            )
        } else if let Some(captures) = conditional_maximum_header_re.captures(header) {
            let maximum = parse_number_word(captures.get(2)?.as_str())?;
            if maximum <= 1 {
                return None;
            }
            (
                1,
                None,
                false,
                Some(json!({
                    "kind": "conditionalValue",
                    "condition": {
                        "kind": "controlsPermanent",
                        "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
                    },
                    "ifTrue": integer(maximum),
                    "ifFalse": integer(1),
                })),
            )
        } else if header.eq_ignore_ascii_case("Choose one") {
            (1_i64, Some(1_i64), false, None)
        } else if header.eq_ignore_ascii_case("Choose two") {
            (2, Some(2), false, None)
        } else if header.eq_ignore_ascii_case("Choose one or more") {
            (1, None, false, None)
        } else if header.eq_ignore_ascii_case("Choose one or both") {
            (1, None, false, None)
        } else {
            return None;
        };
    let mode_lines = lines
        .iter()
        .skip(1)
        .map(|line| modal_bullet_instruction(line))
        .collect::<Option<Vec<_>>>()?;
    if mode_lines.len() < minimum as usize {
        return None;
    }

    let options = (0..mode_lines.len())
        .map(|index| format!("mode{}", index + 1))
        .collect::<Vec<_>>();
    let mut decisions = vec![json!({
        "id": "chosenModes",
        "kind": "chooseModes",
        "minimum": minimum,
        "maximum": conditional_maximum
            .unwrap_or_else(|| integer(maximum_kind.unwrap_or(mode_lines.len() as i64))),
        "options": options,
        "allowRepeated": allow_repeated,
    })];
    let mut effects = Vec::new();

    for (index, raw_instruction) in mode_lines.iter().enumerate() {
        let mode_id = format!("mode{}", index + 1);
        let instruction = strip_short_oracle_label(raw_instruction);
        let instruction = if instruction.ends_with('.') {
            instruction.to_string()
        } else {
            format!("{instruction}.")
        };
        let (mode_effects, mode_decisions) = parse_general_effect_sequence(&instruction, "")
            .or_else(|| parse_general_effect_instruction(&instruction, ""))?;
        let occurrence_count = if allow_repeated { maximum_kind? } else { 1 };
        for occurrence in 1..=occurrence_count {
            let mut occurrence_effects = mode_effects.clone();
            let mut occurrence_decisions = mode_decisions.clone();
            let decision_scope = if allow_repeated {
                format!("{mode_id}:{occurrence}")
            } else {
                mode_id.clone()
            };
            let replacements = occurrence_decisions
                .iter()
                .filter_map(|decision| decision["id"].as_str())
                .filter(|id| *id != "xValue")
                .map(|id| (id.to_string(), format!("{decision_scope}:{id}")))
                .collect::<Vec<_>>();
            for effect in &mut occurrence_effects {
                replace_decision_ids(effect, &replacements);
            }
            let selected = if allow_repeated {
                selection_count_at_least("chosenModes", &mode_id, occurrence)
            } else {
                selection("chosenModes", &mode_id)
            };
            for decision in &mut occurrence_decisions {
                let is_shared_x = decision["id"].as_str() == Some("xValue");
                replace_decision_ids(decision, &replacements);
                if is_shared_x {
                    decision.as_object_mut()?.remove("condition");
                } else {
                    decision["condition"] = if let Some(existing) = decision.get("condition") {
                        and(vec![selected.clone(), existing.clone()])
                    } else {
                        selected.clone()
                    };
                }
            }
            for decision in occurrence_decisions {
                let is_duplicate_shared_x = decision["id"].as_str() == Some("xValue")
                    && decisions
                        .iter()
                        .any(|existing| existing["id"].as_str() == Some("xValue"));
                if !is_duplicate_shared_x {
                    decisions.push(decision);
                }
            }
            effects.push(json!({
                "kind": "conditional",
                "condition": selected,
                "then": occurrence_effects,
            }));
        }
    }

    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
            },
            "effects": effects,
        }),
        &[
            "Parse the generic modal header",
            "Parse every mode with the shared effect grammar",
            "Scope declarations and effects to their selected modes",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_persistent_unused_modal_instruction(
    instruction: &str,
    face_name: &str,
) -> Option<Value> {
    let lines = instruction.lines().map(str::trim).collect::<Vec<_>>();
    if lines.len() < 3 {
        return None;
    }
    let header = lines[0]
        .trim_end_matches([' ', '-', '\u{2014}', '\u{2013}', 'â', '€', '”'])
        .trim();
    if !header.eq_ignore_ascii_case("Choose one that hasn't been chosen") {
        return None;
    }
    let mode_lines = lines
        .iter()
        .skip(1)
        .map(|line| modal_bullet_instruction(line))
        .collect::<Option<Vec<_>>>()?;
    let mut modes = Vec::new();
    for (index, raw_instruction) in mode_lines.into_iter().enumerate() {
        let mode_instruction = if raw_instruction.ends_with('.') {
            raw_instruction.to_string()
        } else {
            format!("{raw_instruction}.")
        };
        let (effects, decisions) = parse_general_effect_sequence(&mode_instruction, face_name)
            .or_else(|| parse_general_effect_instruction(&mode_instruction, face_name))?;
        if !decisions.is_empty() {
            return None;
        }
        modes.push(json!({
            "id": format!("mode{}", index + 1),
            "label": raw_instruction.trim_end_matches('.'),
            "effects": effects,
        }));
    }
    Some(json!({
        "kind": "chooseUnusedMode",
        "id": stable_rule_id("persistentModes", &format!("{face_name}\n{instruction}")),
        "player": controller(),
        "modes": modes,
    }))
}

pub(in crate::oracle::canonical) fn parse_tiered(text: &str) -> Option<CanonicalRuleDraft> {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != 4 || !lines[0].starts_with("Tiered ") {
        return None;
    }
    let tier_re = Regex::new(
        r"^\u{2022} ([A-Za-z]+) \u{2014} ((?:\{[^}]+\})+) \u{2014} [^.]+ deals (\d+) damage to (target creature|each creature)\.$",
    )
    .expect("tiered mode regex compiles");
    let mut modes = Vec::new();
    for line in &lines[1..] {
        let captures = tier_re.captures(line)?;
        modes.push((
            captures[1].to_ascii_lowercase(),
            captures[2].to_string(),
            captures[3].parse::<i64>().ok()?,
            captures[4].to_string(),
        ));
    }
    let target_kind = modes.first()?.3.as_str();
    if modes.iter().any(|mode| mode.3 != target_kind) {
        return None;
    }
    let options = modes
        .iter()
        .map(|mode| Value::String(mode.0.clone()))
        .collect::<Vec<_>>();
    let mut decisions = vec![json!({
        "id": "chosenModes",
        "kind": "chooseModes",
        "minimum": 1,
        "maximum": 1,
        "options": options,
    })];
    if target_kind == "target creature" {
        decisions.push(target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
            }),
            1,
            1,
        ));
    }
    let additional_costs = modes
        .iter()
        .map(|(mode, mana_cost, _, _)| {
            json!({
                "kind": "conditional",
                "condition": selection("chosenModes", mode),
                "then": [{
                    "kind": "payMana",
                    "manaCost": mana_cost,
                }],
            })
        })
        .collect::<Vec<_>>();
    let effects = modes
        .iter()
        .map(|(mode, _, amount, _)| {
            let recipient = if target_kind == "target creature" {
                chosen_target("targetCreature")
            } else {
                json!({
                    "kind": "eachPermanent",
                    "where": card_type("Creature"),
                })
            };
            json!({
                "kind": "conditional",
                "condition": selection("chosenModes", mode),
                "then": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(*amount),
                    "recipient": recipient,
                }],
            })
        })
        .collect::<Vec<_>>();

    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": decisions,
                "additionalCosts": additional_costs,
            },
            "effects": effects,
        }),
        &[
            "Group tiered continuation lines",
            "Extract exactly-one mode decision",
            "Attach mode-dependent additional costs",
            "Share equivalent target declarations",
            "Attach mode-dependent damage effects",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_spree(text: &str) -> Option<CanonicalRuleDraft> {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.len() != 3
        || !lines[0].starts_with("Spree ")
        || !lines[1].contains("Copy target instant spell")
        || !lines[2].contains("Change the target of target spell or ability with a single target")
    {
        return None;
    }
    let cost_re = Regex::new(r"^\+ ((?:\{[^}]+\})+) \u{2014}").expect("spree cost regex compiles");
    let copy_cost = cost_re.captures(lines[1])?[1].to_string();
    let change_cost = cost_re.captures(lines[2])?[1].to_string();

    Some(draft(
        json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": {
                "kind": "castingDeclaration",
                "decisions": [
                    {
                        "id": "chosenModes",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 2,
                        "options": ["copy", "changeTarget"],
                    },
                    {
                        "id": "targetToCopy",
                        "kind": "chooseTargets",
                        "condition": selection("chosenModes", "copy"),
                        "minimum": 1,
                        "maximum": 1,
                        "candidates": {
                            "kind": "stackItems",
                            "where": or(vec![
                                and(vec![
                                    json!({ "kind": "isSpell" }),
                                    or(vec![
                                        card_type("Instant"),
                                        card_type("Sorcery"),
                                    ]),
                                ]),
                                json!({ "kind": "isActivatedAbility" }),
                                json!({ "kind": "isTriggeredAbility" }),
                            ]),
                        },
                    },
                    {
                        "id": "targetToChange",
                        "kind": "chooseTargets",
                        "condition": selection("chosenModes", "changeTarget"),
                        "minimum": 1,
                        "maximum": 1,
                        "candidates": {
                            "kind": "stackItems",
                            "where": and(vec![
                                or(vec![
                                    json!({ "kind": "isSpell" }),
                                    json!({ "kind": "isActivatedAbility" }),
                                    json!({ "kind": "isTriggeredAbility" }),
                                ]),
                                compare(
                                    "==",
                                    json!({
                                        "kind": "targetCountOf",
                                        "object": { "kind": "candidate" },
                                    }),
                                    integer(1),
                                ),
                            ]),
                        },
                    },
                ],
                "additionalCosts": [
                    {
                        "kind": "conditional",
                        "condition": selection("chosenModes", "copy"),
                        "then": [{
                            "kind": "payMana",
                            "manaCost": copy_cost,
                        }],
                    },
                    {
                        "kind": "conditional",
                        "condition": selection("chosenModes", "changeTarget"),
                        "then": [{
                            "kind": "payMana",
                            "manaCost": change_cost,
                        }],
                    },
                ],
            },
            "effects": [
                {
                    "kind": "conditional",
                    "condition": selection("chosenModes", "copy"),
                    "then": [{
                        "kind": "copyStackItem",
                        "object": chosen_target("targetToCopy"),
                        "controller": controller(),
                        "newTargets": {
                            "kind": "mayChoose",
                            "player": controller(),
                        },
                    }],
                },
                {
                    "kind": "conditional",
                    "condition": selection("chosenModes", "changeTarget"),
                    "then": [{
                        "kind": "changeSingleTarget",
                        "object": chosen_target("targetToChange"),
                        "player": controller(),
                    }],
                },
            ],
        }),
        &[
            "Group spree continuation lines",
            "Create nonempty mode-subset decision",
            "Attach conditional stack-item targets",
            "Attach one additional cost per mode",
            "Resolve copy and retarget vocabulary",
        ],
    ))
}
