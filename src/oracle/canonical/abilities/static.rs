use super::super::*;

pub(in crate::oracle::canonical) fn stored_land_name() -> Value {
    stored_card_name("chosenLandName")
}

pub(in crate::oracle::canonical) fn stored_card_name(decision_id: &str) -> Value {
    json!({
        "kind": "storedDecision",
        "object": self_ref(),
        "decisionId": decision_id,
    })
}

pub(in crate::oracle::canonical) fn chosen_creature_type() -> Value {
    json!({
        "kind": "chosenCreatureType",
        "decisionId": "chosenCreatureType",
    })
}

pub(in crate::oracle::canonical) fn parse_common_static_ability(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let static_rule = |modifiers: Vec<Value>| {
        draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            }),
            &["Resolve affected permanents", "Apply continuous modifiers"],
        )
    };

    let supplement_named_token_creation_re = Regex::new(
        r"(?i)^If you would create (?:a|an) ([A-Za-z][A-Za-z '-]+) token, instead create (?:a|an) ([A-Za-z][A-Za-z '-]+) token and (?:a|an) ([A-Za-z][A-Za-z '-]+) token\.$",
    )
    .expect("supplement named token creation replacement regex compiles");
    if let Some(captures) = supplement_named_token_creation_re.captures(text) {
        let replaced = captures.get(1)?.as_str().trim();
        let retained = captures.get(2)?.as_str().trim();
        let additional = captures.get(3)?.as_str().trim();
        if !replaced.eq_ignore_ascii_case(retained) {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "supplementNamedTokenCreation",
            "player": controller(),
            "token": replaced,
            "additionalTokens": [additional],
        })]));
    }

    let add_controlled_type_re = Regex::new(
        r"(?i)^(Nontoken )?(.+?) you control are (?:(Plains|Island|Swamp|Mountain|Forest) )?(artifacts|creatures|enchantments|lands|planeswalkers) in addition to their other types\.(?: \(.+\))?$",
    )
    .expect("controlled permanent type-addition regex compiles");
    if let Some(captures) = add_controlled_type_re.captures(text) {
        let mut where_filter =
            parse_permanent_criteria(&singular_card_term(captures.get(2)?.as_str()), face_name)?;
        if captures.get(1).is_some() {
            where_filter = and(vec![where_filter, not(json!({ "kind": "isToken" }))]);
        }
        let card_type_name = match singular_card_term(captures.get(4)?.as_str())
            .to_ascii_lowercase()
            .as_str()
        {
            "artifact" => "Artifact",
            "creature" => "Creature",
            "enchantment" => "Enchantment",
            "land" => "Land",
            "planeswalker" => "Planeswalker",
            _ => return None,
        };
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": where_filter,
        });
        let mut modifiers = vec![json!({
            "kind": "addCardType",
            "objects": objects.clone(),
            "cardType": card_type_name,
        })];
        if let Some(subtype_name) = captures.get(3) {
            modifiers.push(json!({
                "kind": "addSubtype",
                "objects": objects,
                "subtype": subtype_name.as_str(),
            }));
        }
        return Some(static_rule(modifiers));
    }

    if text.eq_ignore_ascii_case("Players can cast spells only during their own turns.") {
        return Some(static_rule(vec![json!({
            "kind": "castOnlyDuringOwnTurn",
            "players": { "kind": "eachPlayer" },
        })]));
    }

    let controlled_cant_attack_re = Regex::new(r"(?i)^(.+?) you control can't attack\.$")
        .expect("controlled permanent attack-prohibition regex compiles");
    if let Some(captures) = controlled_cant_attack_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(1)?.as_str()),
                    face_name,
                )?,
            },
            "keyword": "cantAttack",
        })]));
    }

    if text.eq_ignore_ascii_case(
        "Once during each of your turns, you may cast a permanent spell from your graveyard by sacrificing a land in addition to paying its other costs.",
    ) {
        return Some(static_rule(vec![json!({
            "kind": "castingPermission",
            "players": controller(),
            "sourceZone": "graveyard",
            "where": and(vec![
                not(card_type("Instant")),
                not(card_type("Sorcery")),
                not(card_type("Land")),
            ]),
            "withoutPayingManaCost": false,
            "asThoughFlash": false,
            "onceEachTurn": true,
            "additionalCost": {
                "kind": "sacrificePermanent",
                "where": card_type("Land"),
            },
        })]));
    }

    let token_copy_not_legendary_re =
        Regex::new(r"^(.+?) isn't legendary if (?:it's|he's|she's|they're) a token\.$")
            .expect("token-copy legendary exception regex compiles");
    if let Some(captures) = token_copy_not_legendary_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{ "kind": "removeLegendaryFromTokenCopy" }],
            }),
            &[
                "Recognize token-copy characteristic exception",
                "Remove legendary supertype",
            ],
        ));
    }
    let enters_with_counter_per_other_subtype_re = Regex::new(
        r"(?i)^(.+?) enters with a ([^ ]+) counter on (?:it|him|her|them) for each (?:other )?(.+?) you control\.$",
    )
    .expect("entering counter per other subtype regex compiles");
    if let Some(captures) = enters_with_counter_per_other_subtype_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        let counter = captures.get(2)?.as_str();
        let where_filter = parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?;
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": counter,
                    "count": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": where_filter,
                    },
                }],
            }),
            &[
                "Count other controlled permanents matching the subtype criteria",
                "Enter with that many counters",
            ],
        ));
    }

    let meld_reference_re =
        Regex::new(r"(?i)^\(Melds with (.+?)\.\)$").expect("meld reference marker regex compiles");
    if let Some(captures) = meld_reference_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "text": text,
                "meldsWith": captures.get(1)?.as_str().trim(),
            }),
            &["Preserve the linked meld-face reference as card metadata"],
        ));
    }
    let normalized;
    let text = if let Some((main, _)) = text.split_once(". (") {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        text
    };
    let text = strip_short_oracle_label(text);
    if let Some(cost) = parse_keyword_cost(text, "Reconfigure") {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [cost],
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "toggleReconfigure",
                }],
                "timing": { "kind": "sorcerySpeed" },
            }),
            &[
                "Parse the reconfigure cost through the shared cost grammar",
                "Toggle attachment only at sorcery speed",
            ],
        ));
    }
    let conditional_source_creature_re = Regex::new(
        r"(?i)^During your turn, as long as (.+?) has one or more ([^ ]+) counters? on (?:him|her|it|them), (?:he's|she's|it's|they're) a (\d+)/(\d+) (.+?) creature and has (.+)\.$",
    )
    .expect("turn-bound source creature characteristic regex compiles");
    if let Some(captures) = conditional_source_creature_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        let condition = and(vec![
            json!({ "kind": "duringControllerTurn", "player": controller() }),
            compare(
                ">=",
                json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": captures.get(2)?.as_str(),
                }),
                integer(1),
            ),
        ]);
        let mut modifiers = vec![
            json!({
                "kind": "addCardType",
                "objects": self_ref(),
                "cardType": "Creature",
                "condition": condition.clone(),
            }),
            json!({
                "kind": "setBasePowerToughness",
                "objects": self_ref(),
                "power": integer(captures[3].parse::<i64>().ok()?),
                "toughness": integer(captures[4].parse::<i64>().ok()?),
                "condition": condition.clone(),
            }),
        ];
        modifiers.extend(captures[5].split_whitespace().map(|subtype_name| {
            json!({
                "kind": "addSubtype",
                "objects": self_ref(),
                "subtype": subtype_name,
                "condition": condition.clone(),
            })
        }));
        modifiers.extend(
            oracle_keyword_list(&captures[6])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": keyword,
                        "condition": condition.clone(),
                    })
                }),
        );
        return Some(static_rule(modifiers));
    }

    let graveyard_destination_replacement_re = Regex::new(
        r"(?i)^If a card would be put into an opponent's graveyard from anywhere, exile it instead\.$",
    )
    .expect("graveyard destination replacement regex compiles");
    if graveyard_destination_replacement_re.is_match(text) {
        return Some(static_rule(vec![json!({
            "kind": "replaceGraveyardDestination",
            "graveyardOwners": {
                "kind": "opponentsOf",
                "player": controller(),
            },
            "from": { "kind": "anyZone" },
            "to": { "kind": "exile" },
        })]));
    }

    let dungeon_entry_re =
        Regex::new(r#"(?i)^You can't enter this dungeon unless you \"([^\"]+)\"\.?$"#)
            .expect("dungeon entry restriction regex compiles");
    if let Some(captures) = dungeon_entry_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "dungeonEntryRestriction",
            "procedure": captures.get(1)?.as_str().trim().trim_end_matches('.'),
        })]));
    }

    let grant_quoted_trigger_re = Regex::new(r#"(?i)^All (.+?) have "(.+)"\.?$"#)
        .expect("global granted-trigger regex compiles");
    if let Some(captures) = grant_quoted_trigger_re.captures(text) {
        let granted = parse_expansion_triggered(&captures[2], face_name)?;
        if granted.rule["kind"] != "triggeredAbility" {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "grantTriggeredAbility",
            "objects": {
                "kind": "permanents",
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            },
            "ability": granted.rule,
        })]));
    }

    let conditional_draw_replacement_re = Regex::new(&format!(
        r"(?i)^As long as (.+?), if you would draw one or more cards, you draw that many cards plus ({}) instead\.$",
        count_word_pattern(),
    ))
    .expect("conditional additional-draw replacement regex compiles");
    if let Some(captures) = conditional_draw_replacement_re.captures(text) {
        let mut rule = static_rule(vec![json!({
            "kind": "increaseDrawReplacementCount",
            "player": controller(),
            "amount": integer(parse_number_word(&captures[2])?),
        })]);
        rule.rule["condition"] = parse_condition_text(&captures[1])?;
        return Some(rule);
    }
    let each_draw_replacement_re = Regex::new(&format!(
        r"(?i)^If you would draw a card( except the first one you draw in each of your draw steps)?, draw ({}) cards instead\.$",
        count_word_pattern(),
    ))
    .expect("per-card draw replacement regex compiles");
    if let Some(captures) = each_draw_replacement_re.captures(text) {
        let replacement_count = parse_number_word(captures.get(2)?.as_str())?;
        if replacement_count <= 1 {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "increaseEachDrawCount",
            "player": controller(),
            "amount": integer(replacement_count - 1),
            "exceptFirstInDrawStep": captures.get(1).is_some(),
        })]));
    }

    let graveyard_card_type_characteristic_re = Regex::new(&format!(
        r"(?i)^(.+?)'s power is equal to the number of card types among cards in (your graveyard|all graveyards) and its toughness is equal to that number plus ({})\.$",
        count_word_pattern(),
    ))
    .expect("graveyard card-type characteristic regex compiles");
    if let Some(captures) = graveyard_card_type_characteristic_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        let count = json!({
            "kind": "countDistinctCardTypes",
            "zone": if captures[2].eq_ignore_ascii_case("all graveyards") {
                json!({ "kind": "anyGraveyard" })
            } else {
                graveyard(controller())
            },
        });
        return Some(static_rule(vec![json!({
            "kind": "setBasePowerToughness",
            "objects": self_ref(),
            "power": count.clone(),
            "toughness": {
                "kind": "add",
                "left": count,
                "right": integer(parse_number_word(&captures[3])?),
            },
        })]));
    }

    let linked_exile_characteristic_re = Regex::new(
        r"(?i)^(.+?)'s power and toughness are each equal to the number of cards exiled with (.+?)\.$",
    )
    .expect("linked-exile characteristic regex compiles");
    if let Some(captures) = linked_exile_characteristic_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
        && source_reference_matches(&captures[2], face_name)
    {
        let count = json!({
            "kind": "countCardsExiledWithSource",
            "object": self_ref(),
        });
        return Some(static_rule(vec![json!({
            "kind": "setBasePowerToughness",
            "objects": self_ref(),
            "power": count.clone(),
            "toughness": count,
        })]));
    }

    let equipped_source_counter_bonus_re = Regex::new(
        r"(?i)^Equipped creature gets ([+-]\d+)/([+-]\d+) for each ([^ ]+) counter on (.+?)\.$",
    )
    .expect("equipped source-counter bonus regex compiles");
    if let Some(captures) = equipped_source_counter_bonus_re.captures(text)
        && source_reference_matches(&captures[4], face_name)
    {
        let amount = json!({
            "kind": "countCounters",
            "object": self_ref(),
            "counter": captures.get(3)?.as_str(),
        });
        let scale = |value: i64| {
            if value == 1 {
                amount.clone()
            } else {
                json!({
                    "kind": "multiply",
                    "left": amount.clone(),
                    "right": integer(value),
                })
            }
        };
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
            "power": scale(captures[1].parse::<i64>().ok()?),
            "toughness": scale(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let counters_among_controlled_threshold_re = Regex::new(&format!(
        r"(?i)^As long as there are ({}) or more ([^ ]+) counters among (.+?) you control, (.+?) (?:has|have) (.+?)\.$",
        count_word_pattern(),
    ))
    .expect("counter total among controlled permanents threshold regex compiles");
    if let Some(captures) = counters_among_controlled_threshold_re.captures(text)
        && source_reference_matches(captures.get(4)?.as_str(), face_name)
    {
        let condition = compare(
            ">=",
            json!({
                "kind": "countCountersOnPermanents",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
                "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
            }),
            integer(parse_number_word(captures.get(1)?.as_str())?),
        );
        let modifiers = oracle_keyword_list(captures.get(5)?.as_str())?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": keyword,
                    "condition": condition.clone(),
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let card_type_threshold_re = Regex::new(&format!(
        r"(?i)^As long as there are ({}) or more card types among cards in (your|all) graveyards?, (.+?) gets ([+-]\d+)/([+-]\d+), has (.+?), and attacks each combat if able\.$",
        count_word_pattern(),
    ))
    .expect("card-type threshold static modifier regex compiles");
    if let Some(captures) = card_type_threshold_re.captures(text)
        && source_reference_matches(&captures[3], face_name)
    {
        let zone = if captures[2].eq_ignore_ascii_case("all") {
            json!({ "kind": "anyGraveyard" })
        } else {
            graveyard(controller())
        };
        let condition = compare(
            ">=",
            json!({
                "kind": "countDistinctCardTypes",
                "zone": zone,
            }),
            integer(parse_number_word(&captures[1])?),
        );
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": integer(captures[4].parse::<i64>().ok()?),
            "toughness": integer(captures[5].parse::<i64>().ok()?),
            "condition": condition,
        })];
        modifiers.extend(
            oracle_keyword_list(&captures[6])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": keyword,
                        "condition": condition,
                    })
                }),
        );
        modifiers.push(json!({
            "kind": "attackEachCombatIfAble",
            "object": self_ref(),
            "condition": condition,
        }));
        return Some(static_rule(modifiers));
    }

    let universal_spell_tax_re = Regex::new(
        r"(?i)^(Spells|Noncreature spells|Spells with the chosen name) cost ((?:\{[^}]+\})+) more to cast\.$",
    )
    .expect("universal spell casting tax regex compiles");
    if let Some(captures) = universal_spell_tax_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(&captures[2])?;
        if !decisions.is_empty() || costs.len() != 1 {
            return None;
        }
        let where_filter = if captures[1].eq_ignore_ascii_case("spells") {
            Value::Null
        } else if captures[1].eq_ignore_ascii_case("noncreature spells") {
            not(card_type("Creature"))
        } else {
            json!({
                "kind": "nameEquals",
                "value": stored_card_name("chosenCardName"),
            })
        };
        return Some(static_rule(vec![json!({
            "kind": "additionalCastingCost",
            "players": { "kind": "eachPlayer" },
            "where": where_filter,
            "cost": costs[0].clone(),
        })]));
    }

    let opposing_chosen_name_spell_tax_re = Regex::new(
        r"(?i)^Spells your opponents cast with the chosen name cost ((?:\{[^}]+\})+) more to cast\.$",
    )
    .expect("opposing chosen-name spell casting tax regex compiles");
    if let Some(captures) = opposing_chosen_name_spell_tax_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(&captures[1])?;
        if !decisions.is_empty() || costs.len() != 1 {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "additionalCastingCost",
            "players": { "kind": "opponentsOf", "player": controller() },
            "where": {
                "kind": "nameEquals",
                "value": stored_card_name("chosenCardName"),
            },
            "cost": costs[0].clone(),
        })]));
    }

    let chosen_name_activation_tax_re = Regex::new(
        r"(?i)^Activated abilities of sources with the chosen name cost ((?:\{[^}]+\})+) more to activate unless they're mana abilities\.$",
    )
    .expect("chosen-name activated-ability tax regex compiles");
    if let Some(captures) = chosen_name_activation_tax_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(&captures[1])?;
        if !decisions.is_empty() || costs.len() != 1 {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "additionalActivationCost",
            "players": { "kind": "opponentsOf", "player": controller() },
            "abilities": {
                "kind": "activatedAbilities",
                "sourceWhere": {
                    "kind": "nameEquals",
                    "value": stored_card_name("chosenCardName"),
                },
                "where": { "kind": "not", "operand": { "kind": "isManaAbility" } },
            },
            "cost": costs[0].clone(),
        })]));
    }

    let off_turn_spell_tax_re = Regex::new(
        r"(?i)^Each spell costs ((?:\{[^}]+\})+) more to cast except during its controller's turn\.$",
    )
    .expect("off-turn spell casting tax regex compiles");
    if let Some(captures) = off_turn_spell_tax_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(&captures[1])?;
        if !decisions.is_empty() || costs.len() != 1 {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "additionalCastingCost",
            "players": { "kind": "eachPlayer" },
            "where": Value::Null,
            "cost": costs[0].clone(),
            "condition": { "kind": "notYourTurn" },
        })]));
    }

    let minimum_casting_mana_re = Regex::new(&format!(
        r"(?i)^As long as this (?:artifact|creature|enchantment|permanent) is untapped, each spell that would cost less than ({0}) mana to cast costs ({0}) mana to cast\.(?: \(.+\))?$",
        count_word_pattern(),
    ))
    .expect("minimum total casting mana regex compiles");
    if let Some(captures) = minimum_casting_mana_re.captures(text) {
        let threshold = parse_number_word(&captures[1])?;
        if parse_number_word(&captures[2])? != threshold {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "minimumCastingMana",
            "players": { "kind": "eachPlayer" },
            "where": Value::Null,
            "amount": integer(threshold),
            "whileSourceUntapped": true,
        })]));
    }

    let shared_free_flash_re = Regex::new(&format!(
        r"(?i)^Any player may cast (.+?) spells with mana value ({}) or less without paying their mana costs and as though they had flash\.$",
        count_word_pattern(),
    ))
    .expect("shared bounded free-casting permission regex compiles");
    if let Some(captures) = shared_free_flash_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "castingPermission",
            "players": { "kind": "eachPlayer" },
            "sourceZone": "hand",
            "where": and(vec![
                parse_permanent_criteria(&captures[1], face_name)?,
                compare(
                    "<=",
                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                    integer(parse_number_word(&captures[2])?),
                ),
            ]),
            "withoutPayingManaCost": true,
            "asThoughFlash": true,
        })]));
    }

    let per_turn_spell_limit_re = Regex::new(
        r"(?i)^Each player can't cast more than (one|two|three|four)(?: (.+?))? spells? each turn\.$",
    )
    .expect("per-turn spell limit regex compiles");
    if let Some(captures) = per_turn_spell_limit_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "castLimitPerTurn",
            "players": { "kind": "eachPlayer" },
            "where": captures
                .get(2)
                .map(|criteria| parse_permanent_criteria(criteria.as_str(), face_name))
                .unwrap_or(Some(Value::Null))?,
            "maximum": integer(parse_number_word(&captures[1])?),
        })]));
    }

    let activated_ability_prohibition_re = Regex::new(
        r"(?i)^Activated abilities of (.+?) can't be activated(?: unless they're mana abilities)?\.$",
    )
    .expect("activated ability prohibition regex compiles");
    if let Some(captures) = activated_ability_prohibition_re.captures(text)
        && let Some(source_where) = parse_permanent_criteria(&captures[1], face_name)
    {
        let excludes_mana = text
            .to_ascii_lowercase()
            .contains("unless they're mana abilities");
        return Some(static_rule(vec![json!({
            "kind": "prohibitActivation",
            "abilities": {
                "kind": "activatedAbilities",
                "sourceWhere": source_where,
                "where": if excludes_mana {
                    not(json!({ "kind": "isManaAbility" }))
                } else {
                    Value::Null
                },
            },
        })]));
    }

    let flexible_activation_mana_re = Regex::new(
        r"(?i)^You may spend mana as though it were mana of any color to activate abilities of (.+?) you control\.$",
    )
    .expect("flexible activation mana regex compiles");
    if let Some(captures) = flexible_activation_mana_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "spendManaAsAnyColorForActivatedAbilities",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            },
        })]));
    }

    let linked_exile_activated_abilities_re = Regex::new(
        r"(?i)^(.+?) you control with ([^ ]+) counters? on them have all activated abilities of all (.+?) cards exiled with (.+?)\.$",
    )
    .expect("linked-exile activated abilities regex compiles");
    if let Some(captures) = linked_exile_activated_abilities_re.captures(text)
        && source_reference_matches(&captures[4], face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "grantActivatedAbilitiesFromLinkedExile",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": and(vec![
                    parse_permanent_criteria(&captures[1], face_name)?,
                    json!({ "kind": "hasCounter", "counter": &captures[2] }),
                ]),
            },
            "cards": {
                "kind": "cards",
                "zone": { "kind": "exile", "player": { "kind": "eachPlayer" } },
                "where": parse_permanent_criteria(&captures[3], face_name)?,
                "linkedSource": self_ref(),
            },
        })]));
    }

    let source_has_zone_activated_abilities_re = Regex::new(
        r"(?i)^(.+?) has all activated abilities of all (.+?) cards in your (graveyard|exile)\.$",
    )
    .expect("source inherits activated abilities from a zone regex compiles");
    if let Some(captures) = source_has_zone_activated_abilities_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        let zone_kind = if captures[3].eq_ignore_ascii_case("graveyard") {
            "graveyard"
        } else {
            "exile"
        };
        return Some(static_rule(vec![json!({
            "kind": "grantActivatedAbilitiesFromZone",
            "objects": { "kind": "self" },
            "cards": {
                "kind": "cards",
                "zone": { "kind": zone_kind, "player": controller() },
                "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
            },
        })]));
    }

    let player_keyword_re = Regex::new(r"(?i)^You have (hexproof|shroud)\.$")
        .expect("player keyword static ability regex compiles");
    if let Some(captures) = player_keyword_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": if captures[1].eq_ignore_ascii_case("hexproof") {
                "playerHexproof"
            } else {
                "playerShroud"
            },
            "player": controller(),
        })]));
    }
    let prohibit_uncast_entry_re =
        Regex::new(r"(?i)^If a (.+?) would enter and it wasn't cast, exile it instead\.$")
            .expect("uncast permanent entry prohibition regex compiles");
    if let Some(captures) = prohibit_uncast_entry_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "prohibitUncastBattlefieldEntry",
            "where": parse_permanent_criteria(&captures[1], face_name)?,
            "destination": "exile",
        })]));
    }
    let prohibit_cast_zones_re = Regex::new(
        r"(?i)^Players can't cast spells from (graveyards|libraries)(?: or (graveyards|libraries))?\.$",
    )
    .expect("zone casting prohibition regex compiles");
    if let Some(captures) = prohibit_cast_zones_re.captures(text) {
        let zones = captures
            .iter()
            .skip(1)
            .flatten()
            .map(|zone| match zone.as_str().to_ascii_lowercase().as_str() {
                "libraries" => "library".to_string(),
                "graveyards" => "graveyard".to_string(),
                zone => zone.trim_end_matches('s').to_string(),
            })
            .collect::<Vec<_>>();
        return Some(static_rule(vec![json!({
            "kind": "prohibitCastFromZones",
            "players": { "kind": "eachPlayer" },
            "zones": zones,
            "where": Value::Null,
        })]));
    }
    let prohibit_enter_zones_re = Regex::new(
        r"(?i)^(.+?) cards in (graveyards|libraries)(?: and (graveyards|libraries))? can't enter the battlefield\.$",
    )
    .expect("zone battlefield-entry prohibition regex compiles");
    if let Some(captures) = prohibit_enter_zones_re.captures(text) {
        let zones = captures
            .iter()
            .skip(2)
            .flatten()
            .map(|zone| match zone.as_str().to_ascii_lowercase().as_str() {
                "libraries" => "library".to_string(),
                "graveyards" => "graveyard".to_string(),
                zone => zone.trim_end_matches('s').to_string(),
            })
            .collect::<Vec<_>>();
        return Some(static_rule(vec![json!({
            "kind": "prohibitEnterBattlefieldFromZones",
            "zones": zones,
            "where": parse_permanent_criteria(&captures[1], face_name)?,
        })]));
    }

    let conditional_source_bonus_re = Regex::new(
        r"(?i)^(?:[^.]+? (?:—|â€”|Ã¢â‚¬â€) )?This creature gets ([+-]\d+)/([+-]\d+) as long as (.+)\.$",
    )
    .expect("conditional source power toughness regex compiles");
    if let Some(captures) = conditional_source_bonus_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "condition": parse_condition_text(&captures[3])?,
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": self_ref(),
                    "power": integer(captures[1].parse::<i64>().ok()?),
                    "toughness": integer(captures[2].parse::<i64>().ok()?),
                }],
            }),
            &[
                "Parse reusable continuous-effect condition",
                "Apply source power and toughness modifier while true",
            ],
        ));
    }
    let conditional_controlled_bonus_re = Regex::new(
        r"(?i)^(?:[^.]+? (?:—|â€”|Ã¢â‚¬â€) )?(.+?) you control get ([+-]\d+)/([+-]\d+) and have (.+) as long as (.+)\.$",
    )
    .expect("conditional controlled power toughness and keyword regex compiles");
    if let Some(captures) = conditional_controlled_bonus_re.captures(text) {
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": parse_permanent_criteria(&captures[1], face_name)?,
        });
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": objects.clone(),
            "power": integer(captures[2].parse::<i64>().ok()?),
            "toughness": integer(captures[3].parse::<i64>().ok()?),
        })];
        for keyword in oracle_keyword_list(&captures[4])? {
            modifiers.push(json!({
                "kind": "grantKeyword",
                "objects": objects.clone(),
                "keyword": keyword,
            }));
        }
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "condition": parse_condition_text(&captures[5])?,
                "modifiers": modifiers,
            }),
            &[
                "Parse controlled permanent criteria",
                "Parse reusable continuous-effect condition",
                "Apply power, toughness, and keyword modifiers while true",
            ],
        ));
    }

    let maximum_hand_size_re = Regex::new(&format!(
        r"(?i)^Your maximum hand size is ({})\.$",
        count_word_pattern(),
    ))
    .expect("maximum hand size regex compiles");
    if let Some(captures) = maximum_hand_size_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "maximumHandSize",
            "player": controller(),
            "amount": integer(parse_number_word(&captures[1])?),
        })]));
    }

    let graveyard_count_entering_counters_re = Regex::new(
        r"(?i)^(?:This creature|This permanent|[A-Z][^.]+) enters with a ([^ ]+) counter on it for each (.+?) card in your graveyard\.$",
    )
    .expect("graveyard-count entering counters regex compiles");
    if let Some(captures) = graveyard_count_entering_counters_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": captures[1].to_string(),
                    "count": {
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": parse_permanent_criteria(&captures[2], face_name)?,
                    },
                }],
            }),
            &[
                "Count matching cards in the controller's graveyard",
                "Enter with that many counters",
            ],
        ));
    }

    let cast_exile_count_entering_counters_re = Regex::new(
        r"(?i)^(?:This creature|This permanent|[A-Z][^.]+) enters with a ([^ ]+) counter on it for each (.+?) card exiled with (?:it|this spell)\.$",
    )
    .expect("cast-exile-count entering counters regex compiles");
    if let Some(captures) = cast_exile_count_entering_counters_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": captures[1].to_string(),
                    "count": {
                        "kind": "countDecisionCardsMatching",
                        "decisionId": "delveCards",
                        "where": parse_permanent_criteria(&captures[2], face_name)?,
                    },
                }],
            }),
            &[
                "Read cards exiled to pay the resolving spell's cost",
                "Filter those cards through reusable card criteria",
                "Apply the counters as an entry replacement",
            ],
        ));
    }

    if text.eq_ignore_ascii_case("Your opponents play with their hands revealed.") {
        return Some(static_rule(vec![json!({
            "kind": "revealHands",
            "players": {
                "kind": "opponentsOf",
                "player": controller(),
            },
        })]));
    }

    let craft_re = Regex::new(
        r"^Craft with (\w+) or more nonlands with activated abilities ((?:\{[^}]+\})+) \(.+\)$",
    )
    .expect("craft activated-ability cards regex compiles");
    if let Some(captures) = craft_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": &captures[2] }],
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [{
                    "kind": "craftTransform",
                    "player": controller(),
                    "minimum": integer(parse_number_word(&captures[1])?),
                    "where": and(vec![
                        not(card_type("Land")),
                        json!({ "kind": "hasActivatedAbility" }),
                    ]),
                    "oncePerInheritedAbilityEachTurn": true,
                }],
            }),
            &[
                "Pay the craft mana cost",
                "Choose qualifying battlefield or graveyard cards",
                "Exile the source and selected cards",
                "Return the source transformed with inherited activated abilities",
            ],
        ));
    }
    if text
        == "Locus of Enlightenment has each activated ability of the exiled cards used to craft it. You may activate each of those abilities only once each turn."
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "inheritsCraftedActivatedAbilities": true,
            }),
            &[
                "Read the cards exiled by the craft action",
                "Expose their activated abilities",
                "Limit each inherited ability to once per turn",
            ],
        ));
    }

    let early_turn_cast_re =
        Regex::new(r"^You can't cast .+ during your (.+) turns? of the game\.$")
            .expect("early-turn casting restriction regex compiles");
    if let Some(captures) = early_turn_cast_re.captures(text) {
        let ordinals = [
            "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
            "tenth",
        ];
        let restricted_through = ordinals
            .iter()
            .enumerate()
            .filter(|(_, ordinal)| {
                captures[1]
                    .split(|character: char| !character.is_alphabetic())
                    .any(|word| word.eq_ignore_ascii_case(ordinal))
            })
            .map(|(index, _)| index + 1)
            .max()?;
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "cantCastThroughTurn": restricted_through,
            }),
            &[
                "Parse the prohibited early turns",
                "Apply the casting restriction",
            ],
        ));
    }

    let must_attack_re = Regex::new(r"(?i)^(.+?) attacks each combat if able\.$")
        .expect("source must-attack regex compiles");
    if let Some(captures) = must_attack_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "attackEachCombatIfAble",
            "object": self_ref(),
        })]));
    }

    let during_controller_turn_controlled_keywords_re =
        Regex::new(r"(?i)^During your turn, (.+?) you control(?: that are (.+?))? have (.+)\.$")
            .expect("controller-turn controlled keyword regex compiles");
    if let Some(captures) = during_controller_turn_controlled_keywords_re.captures(text) {
        let base = captures.get(1)?.as_str();
        let criteria = if let Some(qualifier) = captures.get(2) {
            parse_permanent_criteria(
                &format!("{} {}", qualifier.as_str(), singular_card_term(base)),
                face_name,
            )?
        } else {
            parse_permanent_criteria(base, face_name)?
        };
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": criteria,
        });
        let condition = json!({
            "kind": "duringControllerTurn",
            "player": controller(),
        });
        let modifiers = oracle_keyword_list(&captures[3])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": objects.clone(),
                    "keyword": keyword,
                    "condition": condition,
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let during_controller_turn_attached_keywords_re =
        Regex::new(r"(?i)^During your turn, (equipped|enchanted) creature (?:has|have) (.+)\.$")
            .expect("controller-turn attached keyword regex compiles");
    if let Some(captures) = during_controller_turn_attached_keywords_re.captures(text) {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        let condition = json!({
            "kind": "duringControllerTurn",
            "player": controller(),
        });
        let modifiers = oracle_keyword_list(captures.get(2)?.as_str())?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": attached.clone(),
                    "keyword": keyword,
                    "condition": condition.clone(),
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let during_controller_turn_keywords_re =
        Regex::new(r"(?i)^During your turn, (.+?) (?:has|have) (.+)\.$")
            .expect("controller-turn source keyword regex compiles");
    if let Some(captures) = during_controller_turn_keywords_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        let condition = json!({
            "kind": "duringControllerTurn",
            "player": controller(),
        });
        let modifiers = oracle_keyword_list(&captures[2])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": keyword,
                    "condition": condition,
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let attacking_alone_unblockable_re =
        Regex::new(r"(?i)^(.+?) can't be blocked as long as (?:it's|it is) attacking alone\.$")
            .expect("attacking-alone unblockable regex compiles");
    if let Some(captures) = attacking_alone_unblockable_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "cantBeBlocked",
            "object": self_ref(),
            "condition": { "kind": "sourceAttackingAlone" },
        })]));
    }

    let cant_be_blocked_re =
        Regex::new(r"(?i)^(.+?) can't be blocked\.$").expect("source unblockable regex compiles");
    if let Some(captures) = cant_be_blocked_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "cantBeBlocked",
            "object": self_ref(),
        })]));
    }

    let filtered_source_blockers_re = Regex::new(r"(?i)^(.+?) can't be blocked by (.+?)\.$")
        .expect("source filtered blocker restriction regex compiles");
    if let Some(captures) = filtered_source_blockers_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "blockRestriction",
            "attackers": self_ref(),
            "blockers": {
                "kind": "permanents",
                "where": parse_permanent_criteria(captures.get(2)?.as_str(), face_name)?,
            },
        })]));
    }

    let source_cant_block_unless_re = Regex::new(r"(?i)^(.+?) can't block unless (.+?)\.$")
        .expect("conditional source blocking prohibition regex compiles");
    if let Some(captures) = source_cant_block_unless_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        let allowed = parse_condition_text(captures.get(2)?.as_str()).or_else(|| {
            parse_controlled_permanent_condition(captures.get(2)?.as_str(), face_name)
        })?;
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": "cantBlock",
            "condition": not(allowed),
        })]));
    }

    if Regex::new(r"^Storied(?: \(.+\))?$")
        .expect("storied keyword regex compiles")
        .is_match(text)
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "storied" },
            }),
            &[
                "Recognize the storied keyword",
                "Track the enduring-story threshold",
            ],
        ));
    }

    let controlled_bonus_and_ward_re =
        Regex::new(r"(?i)^(.+?) you control get ([+-]\d+)/([+-]\d+) and have ward (.+?)\.$")
            .expect("controlled bonus and ward regex compiles");
    if let Some(captures) = controlled_bonus_and_ward_re.captures(text) {
        let (ward_costs, ward_decisions) = parse_activation_costs(captures.get(4)?.as_str())?;
        if !ward_decisions.is_empty() || ward_costs.len() != 1 {
            return None;
        }
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
        });
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": objects.clone(),
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
            }),
            json!({
                "kind": "grantWard",
                "objects": objects,
                "cost": ward_costs.into_iter().next()?,
            }),
        ]));
    }

    let controlled_ward_re =
        Regex::new(r"(?i)^([^,]+?) you control have ward (.+?)\.(?: \(.+\))?$")
            .expect("controlled permanents gain ward regex compiles");
    if let Some(captures) = controlled_ward_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(captures.get(2)?.as_str())?;
        if !decisions.is_empty() || costs.len() != 1 {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "grantWard",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
                    .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?,
            },
            "cost": costs[0].clone(),
        })]));
    }

    let conditional_static_re = Regex::new(r"(?i)^As long as (.+?), (.+)$")
        .expect("conditional static ability regex compiles");
    if let Some(captures) = conditional_static_re.captures(text) {
        let condition = parse_condition_text(captures.get(1)?.as_str()).or_else(|| {
            parse_controlled_permanent_condition(captures.get(1)?.as_str(), face_name)
        })?;
        let mut parsed = parse_common_static_ability(captures.get(2)?.as_str(), face_name)?;
        if parsed.rule["kind"].as_str() != Some("staticAbility") {
            return None;
        }
        parsed.rule["condition"] = if let Some(existing) = parsed.rule.get("condition") {
            and(vec![condition, existing.clone()])
        } else {
            condition
        };
        parsed
            .operations
            .insert(0, "Evaluate the continuous-effect condition".to_string());
        return Some(parsed);
    }

    let first_activated_ability_alternative_cost_re = Regex::new(
        r"(?i)^You may pay ((?:\{[^}]+\})+) rather than pay the (equip) cost of the first (equip) ability you activate each turn\.$",
    )
    .expect("first activated ability alternative cost regex compiles");
    if let Some(captures) = first_activated_ability_alternative_cost_re.captures(text) {
        let cost_kind = captures[2].to_ascii_lowercase();
        let ability_kind = captures[3].to_ascii_lowercase();
        if cost_kind != ability_kind {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "firstActivatedAbilityAlternativeCost",
            "player": controller(),
            "abilityKind": ability_kind,
            "cost": {
                "kind": "payMana",
                "manaCost": captures[1].to_string(),
            },
        })]));
    }

    let controlled_selector = |description: &str, exclude_source: bool| {
        let where_filter = card_qualifier_list_filter(description, face_name)
            .or_else(|| parse_permanent_criteria(description, face_name))?;
        let mut selector = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": where_filter,
        });
        if exclude_source {
            selector["excludeSource"] = Value::Bool(true);
        }
        Some(selector)
    };

    let controlled_subject_selectors = |subject: &str| {
        subject
            .split(" and ")
            .map(str::trim)
            .map(|term| {
                let exclude_source = term.starts_with("Other ") || term.starts_with("other ");
                let description = term
                    .strip_prefix("Other ")
                    .or_else(|| term.strip_prefix("other "))
                    .unwrap_or(term)
                    .strip_suffix(" you control")?
                    .trim();
                controlled_selector(description, exclude_source)
            })
            .collect::<Option<Vec<_>>>()
    };

    let controlled_bonus_keywords_re =
        Regex::new(r"^(.+? you control) get ([+-]\d+)/([+-]\d+) and (?:has|have) (.+)\.$")
            .expect("controlled static bonus and keywords regex compiles");
    if let Some(captures) = controlled_bonus_keywords_re.captures(text) {
        let selectors = controlled_subject_selectors(&captures[1])?;
        let keywords = oracle_keyword_list(&captures[4])?;
        let mut modifiers = Vec::new();
        for objects in selectors {
            modifiers.push(json!({
                "kind": "modifyPowerToughness",
                "objects": objects.clone(),
                "power": integer(captures[2].parse::<i64>().ok()?),
                "toughness": integer(captures[3].parse::<i64>().ok()?),
            }));
            modifiers.extend(keywords.iter().map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": objects.clone(),
                    "keyword": keyword,
                })
            }));
        }
        return Some(static_rule(modifiers));
    }

    let opposing_bonus_re = Regex::new(r"^(.+?) your opponents control get ([+-]\d+)/([+-]\d+)\.$")
        .expect("opposing permanent static bonus regex compiles");
    if let Some(captures) = opposing_bonus_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            },
            "power": integer(captures[2].parse::<i64>().ok()?),
            "toughness": integer(captures[3].parse::<i64>().ok()?),
        })]));
    }

    let controlled_counted_bonus_re = Regex::new(
        r"^(Other )?(.+?) you control get ([+-]\d+)/([+-]\d+) for each (.+?) you control\.$",
    )
    .expect("controlled permanent counted static bonus regex compiles");
    if let Some(captures) = controlled_counted_bonus_re.captures(text) {
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[5], face_name)?,
        });
        let scaled = |amount: i64| {
            if amount == 0 {
                integer(0)
            } else if amount == 1 {
                count.clone()
            } else {
                json!({
                    "kind": "multiply",
                    "left": count.clone(),
                    "right": integer(amount),
                })
            }
        };
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": controlled_selector(&captures[2], captures.get(1).is_some())?,
            "power": scaled(captures[3].parse::<i64>().ok()?),
            "toughness": scaled(captures[4].parse::<i64>().ok()?),
        })]));
    }

    let controlled_bonus_re = Regex::new(r"^(Other )?(.+?) you control get ([+-]\d+)/([+-]\d+)\.$")
        .expect("controlled permanent static bonus regex compiles");
    if let Some(captures) = controlled_bonus_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": controlled_selector(&captures[2], captures.get(1).is_some())?,
            "power": integer(captures[3].parse::<i64>().ok()?),
            "toughness": integer(captures[4].parse::<i64>().ok()?),
        })]));
    }

    let controlled_keywords_re = Regex::new(r"^(Other )?(.+?) you control (?:has|have) (.+)\.$")
        .expect("controlled permanent static keywords regex compiles");
    if let Some(captures) = controlled_keywords_re.captures(text) {
        let objects = controlled_selector(&captures[2], captures.get(1).is_some())?;
        let modifiers = oracle_keyword_list(&captures[3])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": objects.clone(),
                    "keyword": keyword,
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let global_other_keywords_re = Regex::new(r"^Other (.+?) (?:has|have) (.+)\.$")
        .expect("global other-permanent static keywords regex compiles");
    if let Some(captures) = global_other_keywords_re.captures(text) {
        let objects = json!({
            "kind": "permanents",
            "where": parse_permanent_criteria(&captures[1], face_name)?,
            "excludeSource": true,
        });
        let modifiers = oracle_keyword_list(&captures[2])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": objects.clone(),
                    "keyword": keyword,
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let controlled_counter_keyword_re =
        Regex::new(r"(?i)^Each (.+?) you control with (?:a|an) ([^ ]+) counter on it has (.+)\.$")
            .expect("controlled counter-bearing keyword regex compiles");
    if let Some(captures) = controlled_counter_keyword_re.captures(text) {
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": and(vec![
                parse_permanent_criteria(&captures[1], face_name)?,
                json!({ "kind": "hasCounter", "counter": captures[2].to_string() }),
            ]),
        });
        let modifiers = oracle_keyword_list(&captures[3])?
            .into_iter()
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword", "objects": objects.clone(), "keyword": keyword,
                })
            })
            .collect();
        return Some(static_rule(modifiers));
    }

    let source_re = if face_name.is_empty() {
        r"^This (?:artifact|creature|enchantment|permanent|token)".to_string()
    } else {
        format!(
            r"^(?:This (?:artifact|creature|enchantment|permanent|token)|{})",
            regex::escape(face_name),
        )
    };
    let source_restriction_re = Regex::new(r"(?i)^(.+?) (can't block|can't be blocked)\.$")
        .expect("source combat restriction regex compiles");
    if let Some(captures) = source_restriction_re.captures(text)
        && (matches!(
            captures.get(1)?.as_str().to_ascii_lowercase().as_str(),
            "this artifact"
                | "this creature"
                | "this enchantment"
                | "this permanent"
                | "this token"
        ) || source_reference_matches(captures.get(1)?.as_str(), face_name))
    {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": oracle_keyword_kind(&captures[2])?,
        })]));
    }

    let source_bonus_keywords_re =
        Regex::new(r"(?i)^(.+?) gets ([+-]\d+)/([+-]\d+) and has (.+)\.$")
            .expect("source static bonus and keywords regex compiles");
    if let Some(captures) = source_bonus_keywords_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": integer(captures[2].parse::<i64>().ok()?),
            "toughness": integer(captures[3].parse::<i64>().ok()?),
        })];
        modifiers.extend(
            oracle_keyword_list(&captures[4])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": keyword,
                    })
                }),
        );
        return Some(static_rule(modifiers));
    }

    let conditional_source_keyword_re = Regex::new(&format!(
        r"{} (?:has|have) ([a-z ]+) as long as you control another (.+)\.$",
        source_re,
    ))
    .expect("conditional source keyword regex compiles");
    if let Some(captures) = conditional_source_keyword_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": oracle_keyword_kind(&captures[1])?,
            "condition": {
                "kind": "controlsPermanent",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[2], face_name)?,
                "excludeSource": true,
            },
        })]));
    }

    let source_count_bonus_re = Regex::new(&format!(
        r"{} gets ([+-]\d+)/([+-]\d+) for each (.+) you control\.$",
        source_re,
    ))
    .expect("source counted static bonus regex compiles");
    if let Some(captures) = source_count_bonus_re.captures(text) {
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[3], face_name)?,
        });
        let scaled = |amount: i64| {
            if amount == 0 {
                integer(0)
            } else if amount == 1 {
                count.clone()
            } else {
                json!({ "kind": "multiply", "value": count.clone(), "factor": integer(amount) })
            }
        };
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": scaled(captures[1].parse::<i64>().ok()?),
            "toughness": scaled(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let attached_count_bonus_re =
        Regex::new(r"^Equipped creature gets ([+-]\d+)/([+-]\d+) for each (.+) you control\.$")
            .expect("equipped creature counted bonus regex compiles");
    if let Some(captures) = attached_count_bonus_re.captures(text) {
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[3], face_name)?,
        });
        let scale = |amount: i64| {
            if amount == 0 {
                integer(0)
            } else if amount == 1 {
                count.clone()
            } else {
                json!({
                    "kind": "multiply",
                    "left": count.clone(),
                    "right": integer(amount),
                })
            }
        };
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
            "power": scale(captures[1].parse::<i64>().ok()?),
            "toughness": scale(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let attached_bonus_protection_re =
        Regex::new(r"^Equipped creature gets ([+-]\d+)/([+-]\d+) and has protection from (.+)\.$")
            .expect("equipped creature protection bonus regex compiles");
    if let Some(captures) = attached_bonus_protection_re.captures(text) {
        let qualities = captures[3]
            .replace(", and from ", ", ")
            .replace(" and from ", ", ")
            .replace("from ", "")
            .split(',')
            .map(str::trim)
            .filter(|quality| !quality.is_empty())
            .map(str::to_ascii_lowercase)
            .collect::<Vec<_>>();
        if qualities.is_empty() {
            return None;
        }
        let objects = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": objects.clone(),
                "power": integer(captures[1].parse::<i64>().ok()?),
                "toughness": integer(captures[2].parse::<i64>().ok()?),
            }),
            json!({
                "kind": "grantProtection",
                "objects": objects,
                "from": qualities,
            }),
        ]));
    }

    if text.starts_with("During your turn, you may cast cards exiled with Azula") {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "azulaExiledCasting": true,
            }),
            &[
                "Link cards exiled with this source",
                "Permit their casting during the controller's turn",
                "Allow flash timing and mana of any type",
            ],
        ));
    }
    if text.starts_with("Enchanted creature gets +1/+1 and has \"Whenever this creature deals combat damage to a player")
        && text.contains("gains flashback until end of turn")
    {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                "power": integer(1),
                "toughness": integer(1),
            }),
            json!({
                "kind": "attachedCombatDamageGrantsFlashback",
                "attachment": self_ref(),
            }),
        ]));
    }
    if text == "If you would lose unspent mana, that mana becomes red instead." {
        return Some(static_rule(vec![json!({
            "kind": "convertUnspentMana",
            "player": controller(),
            "to": "R",
        })]));
    }
    if text == "Ozai has flying and indestructible as long as you have six or more unspent mana." {
        let condition = compare(
            ">=",
            json!({ "kind": "manaPoolSize", "player": controller() }),
            integer(6),
        );
        return Some(static_rule(vec![
            json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "flying",
                "condition": condition.clone(),
            }),
            json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "indestructible",
                "condition": condition,
            }),
        ]));
    }
    if text
        == "You may cast this spell as though it had flash if you control an attacking legendary creature."
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
                "flashWithAttackingLegendary": true,
            }),
            &[
                "Detect an attacking legendary creature",
                "Grant flash timing to this spell",
            ],
        ));
    }
    let land_subtype_setting_re =
        Regex::new(r"(?i)^Nonbasic lands are (Plains|Islands|Swamps|Mountains|Forests)\.$")
            .expect("land subtype setting regex compiles");
    if let Some(captures) = land_subtype_setting_re.captures(text) {
        let land_subtype = match captures[1].to_ascii_lowercase().as_str() {
            "plains" => "Plains",
            "islands" => "Island",
            "swamps" => "Swamp",
            "mountains" => "Mountain",
            "forests" => "Forest",
            _ => return None,
        };
        return Some(static_rule(vec![json!({
            "kind": "setLandSubtype",
            "objects": {
                "kind": "permanents",
                "where": and(vec![
                    card_type("Land"),
                    not(json!({ "kind": "typeLineContains", "value": "Basic" })),
                ]),
            },
            "subtype": land_subtype,
        })]));
    }
    if text == "Nonbasic lands enter tapped." {
        return Some(static_rule(vec![json!({
            "kind": "nonbasicLandsEnterTapped",
        })]));
    }
    let opposing_permanents_enter_tapped_re =
        Regex::new(r"(?i)^(.+?) your opponents control enter tapped\.$")
            .expect("opposing permanents enter tapped regex compiles");
    if let Some(captures) = opposing_permanents_enter_tapped_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "permanentsEnterTapped",
            "objects": {
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            },
        })]));
    }
    if text.starts_with(
        "As long as Zhao has a conqueror counter on him, nonbasic lands are Mountains.",
    ) {
        return Some(static_rule(vec![json!({
            "kind": "nonbasicLandsBecomeMountains",
            "condition": compare(
                ">=",
                json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": "conqueror",
                }),
                integer(1),
            ),
        })]));
    }
    if text == "This token can't block or be blocked by non-Spirit creatures." {
        let non_spirit_creatures = json!({
            "kind": "permanents",
            "where": and(vec![card_type("Creature"), not(subtype("Spirit"))]),
        });
        return Some(static_rule(vec![
            json!({
                "kind": "blockRestriction",
                "attackers": self_ref(),
                "blockers": non_spirit_creatures.clone(),
            }),
            json!({
                "kind": "blockRestriction",
                "attackers": non_spirit_creatures,
                "blockers": self_ref(),
            }),
        ]));
    }

    if text == "You don't lose unspent red mana as steps and phases end." {
        return Some(static_rule(vec![json!({
            "kind": "retainUnspentMana",
            "player": controller(),
            "symbols": ["R"],
        })]));
    }
    if text == "Noncreature spells you cast cost {1} less to cast." {
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": not(card_type("Creature")),
            "amount": integer(1),
        })]));
    }
    if text == "Enchanted creature is a copy of the chosen creature." {
        return Some(static_rule(vec![json!({
            "kind": "copyAttachedFromStoredCreature",
            "attachment": self_ref(),
            "decisionId": "chosenCreature",
        })]));
    }
    if text
        == "If a land is tapped for two or more mana, it produces {C} instead of any other type and amount."
    {
        return Some(static_rule(vec![json!({ "kind": "dampingSphereMana" })]));
    }
    if text
        == "Each spell a player casts costs {1} more to cast for each other spell that player has cast this turn."
    {
        return Some(static_rule(vec![
            json!({ "kind": "dampingSphereSpellTax" }),
        ]));
    }
    if text.starts_with("For Auld Lang Syne")
        && text.contains("cast an artifact or Human spell from your graveyard")
    {
        return Some(static_rule(vec![json!({
            "kind": "arcadeGraveyardCasting"
        })]));
    }
    if text.starts_with("Equipped creature has defender and")
        && text.contains("Other creatures you control gain trample")
    {
        return Some(static_rule(vec![
            json!({
                "kind": "grantKeyword",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                "keyword": "defender",
            }),
            json!({
                "kind": "grantDragonThroneAbility",
                "attachment": self_ref(),
            }),
        ]));
    }
    let controlled_creatures = json!({
        "kind": "permanents",
        "controller": controller(),
        "where": card_type("Creature"),
    });

    let enchanted_permanent = json!({
        "kind": "attachedPermanent",
        "attachment": self_ref(),
    });
    if text
        == "Loyalty abilities of planeswalkers your opponents control cost {1} more to activate."
    {
        return Some(static_rule(vec![json!({
            "kind": "increaseOpponentPlaneswalkerLoyaltyActivationCost",
            "amount": integer(1),
        })]));
    }
    if text == "As Stenn enters, choose a card type other than creature or land." {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "decisions": [{
                    "id": "chosenCardType",
                    "kind": "chooseCardType",
                    "options": [
                        "Artifact",
                        "Battle",
                        "Enchantment",
                        "Instant",
                        "Kindred",
                        "Planeswalker",
                        "Sorcery"
                    ],
                }],
                "replacement": [{
                    "kind": "storeDecision",
                    "decisionId": "chosenCardType",
                }],
            }),
            &[
                "Choose a noncreature nonland card type",
                "Persist the chosen type on Stenn",
            ],
        ));
    }
    if text == "Spells you cast of the chosen type cost {1} less to cast." {
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": {
                "kind": "chosenCardType",
                "decisionId": "chosenCardType",
            },
            "amount": integer(1),
            "duringOtherPlayersTurns": false,
        })]));
    }
    if text == "12+ | {U}, {T}: Add {U} for each artifact you control." {
        let (costs, _) = parse_activation_costs("{U}, {T}")?;
        return Some(draft(
            json!({
                "kind": "manaAbility",
                "source": self_ref(),
                "costs": costs,
                "activationCondition": compare(
                    ">=",
                    json!({
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": "charge",
                    }),
                    integer(12),
                ),
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": {
                        "kind": "fixedMana",
                        "symbol": "U",
                        "amount": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": card_type("Artifact"),
                        },
                    },
                }],
            }),
            &[
                "Require twelve charge counters",
                "Pay the mana and tap costs",
                "Add blue mana for each controlled artifact",
            ],
        ));
    }
    if Regex::new(r"^(?:This artifact|This creature|[A-Z][^.]+) enters tapped\.$")
        .expect("generic enters-tapped regex compiles")
        .is_match(text)
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &["Apply the permanent's tapped entering state"],
        ));
    }
    if Regex::new(
        r"^(?:This creature|[A-Z][^.]+) enters tapped and doesn't untap during your untap step\.$",
    )
    .expect("combined tapped and no-untap regex compiles")
    .is_match(text)
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [
                    { "kind": "setEnteringState", "object": self_ref(), "tapped": true },
                    { "kind": "setEnteringFlag", "object": self_ref(), "flag": "doesNotUntap" },
                ],
            }),
            &["Enter tapped", "Retain the intrinsic untap restriction"],
        ));
    }
    let self_untap_re =
        Regex::new(r"^(.+?) doesn't untap during your untap step(?: unless (.+))?\.$")
            .expect("generic self untap restriction regex compiles");
    if let Some(captures) = self_untap_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        let condition = if let Some(condition) = captures.get(2) {
            Some(json!({
                "kind": "not",
                "operand": parse_condition_text(condition.as_str())?,
            }))
        } else {
            None
        };
        let mut modifier = json!({
            "kind": "doesNotUntap",
            "objects": self_ref(),
        });
        if let Some(condition) = condition {
            modifier["condition"] = condition;
        }
        return Some(static_rule(vec![modifier]));
    }
    let filtered_untap_prohibition_re =
        Regex::new(r"(?i)^(.+?) don't untap during their controllers' untap steps\.$")
            .expect("filtered untap prohibition regex compiles");
    if let Some(captures) = filtered_untap_prohibition_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "doesNotUntap",
            "objects": {
                "kind": "permanents",
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            },
        })]));
    }

    let conditional_self_bonus_re = Regex::new(&format!(
        r"(?i)^(?:[^—]+— )?As long as there are ({}) or more (.+?) in your graveyard, this (?:creature|permanent) gets ([+-]\d+)/([+-]\d+) and (?:has )?(.+)\.$",
        count_word_pattern(),
    ))
    .expect("graveyard-threshold self bonus regex compiles");
    if let Some(captures) = conditional_self_bonus_re.captures(text) {
        let condition = compare(
            ">=",
            json!({
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": if matches!(captures[2].to_ascii_lowercase().as_str(), "card" | "cards") {
                    Value::Null
                } else {
                    parse_permanent_criteria(&captures[2], face_name)?
                },
            }),
            integer(parse_number_word(&captures[1])?),
        );
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": integer(captures[3].parse::<i64>().ok()?),
            "toughness": integer(captures[4].parse::<i64>().ok()?),
            "condition": condition.clone(),
        })];
        modifiers.extend(
            oracle_keyword_list(&captures[5])?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": keyword,
                        "condition": condition.clone(),
                    })
                }),
        );
        return Some(static_rule(modifiers));
    }
    let attack_tax_re = Regex::new(
        r"(?i)^Creatures can't attack you(?: or planeswalkers you control)? unless their controller pays \{(\d+)\} for each (?:of those creatures|creature they control that's attacking you)\.$",
    )
    .expect("generic attack-tax regex compiles");
    if let Some(captures) = attack_tax_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "attackTax",
            "protectedPlayer": controller(),
            "amount": integer(captures[1].parse::<i64>().ok()?),
        })]));
    }
    if text == "Daxos can't be blocked by creatures with power 3 or greater." {
        return Some(static_rule(vec![json!({
            "kind": "blockRestriction",
            "attackers": self_ref(),
            "blockers": {
                "kind": "permanents",
                "where": compare(
                    ">=",
                    json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                    integer(3),
                ),
            },
        })]));
    }
    let conditional_hexproof = match text {
        "Sokrates has hexproof as long as it's untapped." => Some(json!({
            "kind": "not",
            "operand": { "kind": "isTapped", "object": self_ref() },
        })),
        "Tromokratis has hexproof unless it's attacking or blocking." => Some(json!({
            "kind": "not",
            "operand": {
                "kind": "or",
                "operands": [
                    { "kind": "isAttacking", "object": self_ref() },
                    { "kind": "isBlocking", "object": self_ref() },
                ],
            },
        })),
        _ => None,
    };
    if let Some(condition) = conditional_hexproof {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": "hexproof",
            "condition": condition,
        })]));
    }
    if text
        == "Tromokratis can't be blocked unless all creatures defending player controls block it. (If any creature that player controls doesn't block this creature, it can't be blocked.)"
    {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": "allMustBlockIfAble",
        })]));
    }
    if text
        == "Equipped creature gets +X/+X, where X is the number of creature cards in all graveyards."
    {
        let count = json!({
            "kind": "countCards",
            "zone": { "kind": "graveyard", "player": controller() },
            "allPlayers": true,
            "where": card_type("Creature"),
        });
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": enchanted_permanent.clone(),
            "power": count.clone(),
            "toughness": count,
        })]));
    }
    if text
        == "Enchanted creature is legendary, gets +1/+1, and has flying, vigilance, and lifelink."
    {
        return Some(static_rule(vec![
            json!({ "kind": "addSupertype", "objects": enchanted_permanent.clone(), "supertype": "Legendary" }),
            json!({ "kind": "modifyPowerToughness", "objects": enchanted_permanent.clone(), "power": integer(1), "toughness": integer(1) }),
            json!({ "kind": "grantKeyword", "objects": enchanted_permanent.clone(), "keyword": "flying" }),
            json!({ "kind": "grantKeyword", "objects": enchanted_permanent.clone(), "keyword": "vigilance" }),
            json!({ "kind": "grantKeyword", "objects": enchanted_permanent, "keyword": "lifelink" }),
        ]));
    }
    if text == "Enchanted permanent is legendary." {
        return Some(static_rule(vec![json!({
            "kind": "addSupertype",
            "objects": enchanted_permanent.clone(),
            "supertype": "Legendary",
        })]));
    }
    if matches!(
        text,
        "You control enchanted permanent."
            | "You control enchanted creature."
            | "You control enchanted artifact."
            | "You control enchanted enchantment."
    ) {
        return Some(static_rule(vec![json!({
            "kind": "controlAttachedPermanent",
            "attachment": self_ref(),
        })]));
    }
    if text == "Creatures you control have base power and toughness 9/9." {
        return Some(static_rule(vec![json!({
            "kind": "setBasePowerToughness",
            "objects": controlled_creatures.clone(),
            "power": integer(9),
            "toughness": integer(9),
        })]));
    }
    if text == "Other tapped legendary creatures you control have indestructible." {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "excludeSource": true,
                "where": and(vec![
                    card_type("Creature"),
                    json!({ "kind": "isLegendary" }),
                    json!({ "kind": "isTapped" }),
                ]),
            },
            "keyword": "indestructible",
        })]));
    }
    let all_lands_subtype_re = Regex::new(
        r"(?i)^(?:All|Each) lands? (?:is|are) (?:an? )?([A-Za-z]+) in addition to (?:its|their) other land types\.$",
    )
    .expect("all lands gain subtype regex compiles");
    if let Some(captures) = all_lands_subtype_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "addSubtype",
            "objects": {
                "kind": "permanents",
                "where": card_type("Land"),
            },
            "subtype": captures[1].to_string(),
        })]));
    }
    if text == "Creatures without flying or islandwalk can't attack." {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": {
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    not(json!({ "kind": "hasKeyword", "value": "flying" })),
                    not(json!({ "kind": "hasKeyword", "value": "islandwalk" })),
                ]),
            },
            "keyword": "cantAttack",
        })]));
    }
    let variable_spell_reduction_re = Regex::new(
        r"(?i)^(.+?) spells you cast cost \{X\} less to cast, where X is (this (?:creature|permanent)|equipped creature)'s power\.$",
    )
    .expect("variable spell-cost reduction regex compiles");
    if let Some(captures) = variable_spell_reduction_re.captures(text) {
        let amount_object = if captures[2].eq_ignore_ascii_case("equipped creature") {
            json!({ "kind": "attachedPermanent", "attachment": self_ref() })
        } else {
            self_ref()
        };
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
                .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?,
            "amount": { "kind": "powerOf", "object": amount_object },
            "duringOtherPlayersTurns": false,
        })]));
    }

    let attack_control_threshold_re = Regex::new(&format!(
        r"(?i)^This (?:creature|permanent) can't attack unless you control ({}) or more other (.+?)\.$",
        count_word_pattern(),
    ))
    .expect("attack controlled-permanent threshold regex compiles");
    if let Some(captures) = attack_control_threshold_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "cantAttackUnlessControls",
            "object": self_ref(),
            "player": controller(),
            "where": parse_permanent_criteria(
                &singular_card_term(captures.get(2)?.as_str()),
                face_name,
            )?,
            "minimum": integer(parse_number_word(captures.get(1)?.as_str())?),
            "excludeSource": true,
        })]));
    }
    if text
        == "(You may cast a legendary sorcery only if you control a legendary creature or planeswalker.)"
    {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
            }),
            &["Recognize the legendary-sorcery casting reminder"],
        ));
    }
    if text
        == "Each legendary permanent you control has ward {2}. (Whenever it becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)"
    {
        return Some(static_rule(vec![json!({
            "kind": "grantWard",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": { "kind": "isLegendary" },
            },
            "cost": { "kind": "payMana", "manaCost": "{2}" },
        })]));
    }
    if text == "Enchanted permanent has ward {1}." {
        return Some(static_rule(vec![json!({
            "kind": "grantWard",
            "objects": enchanted_permanent.clone(),
            "cost": { "kind": "payMana", "manaCost": "{1}" },
        })]));
    }
    if text
        == "If a source an opponent controls would deal damage to you, prevent 1 of that damage."
    {
        return Some(static_rule(vec![json!({
            "kind": "preventOpponentDamageAmount",
            "player": controller(),
            "amount": integer(1),
        })]));
    }
    if text
        == "Spells and abilities your opponents control can't cause you to sacrifice permanents."
    {
        return Some(static_rule(vec![json!({
            "kind": "preventOpponentForcedSacrifice",
            "player": controller(),
        })]));
    }
    let enchant_text = text
        .split_once(" (")
        .map(|(keyword, _)| keyword)
        .unwrap_or(text)
        .trim_end_matches('.');
    let zone_enchant_re =
        Regex::new(r"(?i)^Enchant (.+?) card in (a|your|an opponent's) graveyard$")
            .expect("zone enchant restriction regex compiles");
    if let Some(captures) = zone_enchant_re.captures(enchant_text) {
        let zone = match captures[2].to_ascii_lowercase().as_str() {
            "a" => json!({ "kind": "anyGraveyard" }),
            "your" => graveyard(controller()),
            "an opponent's" => json!({
                "kind": "graveyard",
                "player": { "kind": "opponentsOf", "player": controller() },
            }),
            _ => return None,
        };
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "enchant",
                    "where": parse_permanent_criteria(&captures[1], face_name)?,
                    "zone": zone,
                },
            }),
            &[
                "Recognize an enchant restriction that targets a card in a zone",
                "Parse the enchanted card criterion through the shared criteria grammar",
                "Preserve the graveyard ownership scope",
            ],
        ));
    }
    let enchant_target = match enchant_text {
        "Enchant creature" => Some((card_type("Creature"), None)),
        "Enchant creature you control" => Some((card_type("Creature"), Some(controller()))),
        "Enchant artifact you control" => Some((card_type("Artifact"), Some(controller()))),
        "Enchant artifact" => Some((card_type("Artifact"), None)),
        "Enchant enchantment" => Some((card_type("Enchantment"), None)),
        "Enchant artifact or creature" => {
            Some((or(vec![card_type("Artifact"), card_type("Creature")]), None))
        }
        "Enchant land" => Some((card_type("Land"), None)),
        "Enchant permanent" => Some((Value::Null, None)),
        "Enchant legendary creature" => Some((
            and(vec![
                card_type("Creature"),
                json!({ "kind": "isLegendary" }),
            ]),
            None,
        )),
        "Enchant nonland permanent" => Some((not(card_type("Land")), None)),
        _ => None,
    };
    if let Some((where_filter, target_controller)) = enchant_target {
        let mut ability = json!({ "kind": "enchant", "where": where_filter });
        if let Some(target_controller) = target_controller {
            ability["controller"] = target_controller;
        }
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": ability,
            }),
            &[
                "Recognize enchant restriction",
                "Constrain legal Aura attachment",
            ],
        ));
    }

    if text == "Enchanted land has \"{T}: Add two mana of any one color.\"" {
        return Some(static_rule(vec![json!({
            "kind": "grantManaAbility",
            "objects": enchanted_permanent.clone(),
            "mana": { "kind": "chooseColor", "amount": integer(2) },
        })]));
    }
    let affinity_re = Regex::new(
        r"(?i)^Affinity for (.+?) \(This spell costs \{1\} less to cast for each (.+?) you control\.\)$",
    )
    .expect("generic affinity criteria regex compiles");
    if let Some(captures) = affinity_re.captures(text) {
        let label_criteria = parse_permanent_criteria(&captures[1], face_name)?;
        let counted_criteria = parse_permanent_criteria(&captures[2], face_name)?;
        if label_criteria != counted_criteria {
            return None;
        }
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": Value::Null,
            "amount": {
                "kind": "countPermanents",
                "player": controller(),
                "where": counted_criteria,
            },
            "duringOtherPlayersTurns": false,
        })]));
    }
    let granted_affinity_re = Regex::new(r"(?i)^(.+?) spells you cast have affinity for (.+)\.$")
        .expect("granted affinity criteria regex compiles");
    if let Some(captures) = granted_affinity_re.captures(text) {
        let spell_criteria = card_qualifier_list_filter(&captures[1], face_name)
            .or_else(|| parse_permanent_criteria(&captures[1], face_name))?;
        let counted_criteria = parse_permanent_criteria(&captures[2], face_name)?;
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": spell_criteria,
            "amount": {
                "kind": "countPermanents",
                "player": controller(),
                "where": counted_criteria,
            },
            "duringOtherPlayersTurns": false,
        })]));
    }
    let historic_cost_modifier = if text
        .contains("This spell costs {1} less to cast for each historic card in your graveyard.")
    {
        Some((
            Value::Null,
            json!({
                "kind": "countCards",
                "zone": { "kind": "graveyard", "player": controller() },
                "where": { "kind": "historic" },
            }),
            false,
        ))
    } else {
        match text {
            "Historic spells you cast cost {1} less to cast. (Artifacts, legendaries, and Sagas are historic.)" => {
                Some((json!({ "kind": "historic" }), integer(1), false))
            }
            "This spell costs {X} less to cast, where X is the total mana value of noncreature artifacts you control." => {
                Some((
                    Value::Null,
                    json!({
                        "kind": "sumManaValues",
                        "zone": { "kind": "battlefield", "player": controller() },
                        "where": and(vec![card_type("Artifact"), not(card_type("Creature"))]),
                    }),
                    false,
                ))
            }
            "During turns other than yours, spells you cast cost {1} less to cast." => {
                Some((Value::Null, integer(1), true))
            }
            _ => None,
        }
    };
    if let Some((where_filter, amount, during_other_turns)) = historic_cost_modifier {
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": where_filter,
            "amount": amount,
            "duringOtherPlayersTurns": during_other_turns,
        })]));
    }
    if text
        == "You may cast historic spells as though they had flash. (Artifacts, legendaries, and Sagas are historic.)"
    {
        return Some(static_rule(vec![json!({
            "kind": "grantFlashCasting",
            "player": controller(),
            "where": { "kind": "historic" },
            "firstEachTurn": false,
        })]));
    }
    if text == "Enchanted land is the chosen color." {
        return Some(static_rule(vec![json!({
            "kind": "setStoredColor",
            "objects": enchanted_permanent.clone(),
            "decisionId": "chosenColor",
        })]));
    }
    if text == "Enchanted land is a 6/4 green Elemental creature. It's still a land." {
        return Some(static_rule(vec![
            json!({
                "kind": "setBasePowerToughness",
                "objects": enchanted_permanent.clone(),
                "power": integer(6),
                "toughness": integer(4),
            }),
            json!({
                "kind": "addCardType",
                "objects": enchanted_permanent.clone(),
                "cardType": "Creature",
            }),
            json!({
                "kind": "addSubtype",
                "objects": enchanted_permanent.clone(),
                "subtype": "Elemental",
            }),
            json!({
                "kind": "setColor",
                "objects": enchanted_permanent.clone(),
                "colors": ["G"],
            }),
        ]));
    }
    if text
        == "Enchanted creature gets +1/+1 and has \"Whenever this creature deals damage to an opponent, you may draw a card.\""
    {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": enchanted_permanent.clone(),
                "power": integer(1),
                "toughness": integer(1),
            }),
            json!({
                "kind": "attachedDamageDraw",
                "attachment": self_ref(),
            }),
        ]));
    }
    let timber_paladin_re = Regex::new(&format!(
        r"^As long as this creature is enchanted by (?:exactly ({})|({}) or more) Auras?, it has base power and toughness (\d+)/(\d+)(?:, vigilance, and trample| and vigilance)?\.$",
        count_word_pattern(),
        count_word_pattern(),
    ))
    .expect("Timber Paladin threshold regex compiles");
    if let Some(captures) = timber_paladin_re.captures(text) {
        let (operator, count) = if let Some(exact_count) = captures.get(1) {
            ("==", parse_number_word(exact_count.as_str())?)
        } else if let Some(minimum_count) = captures.get(2) {
            (">=", parse_number_word(minimum_count.as_str())?)
        } else {
            return None;
        };
        let condition = compare(
            operator,
            json!({
                "kind": "countAttachments",
                "object": self_ref(),
                "where": subtype("Aura"),
            }),
            integer(count),
        );
        let mut modifiers = vec![json!({
            "kind": "setBasePowerToughness",
            "objects": self_ref(),
                        "power": integer(captures[3].parse::<i64>().ok()?),
                        "toughness": integer(captures[4].parse::<i64>().ok()?),
            "condition": condition.clone(),
        })];
        if text.contains("vigilance") {
            modifiers.push(json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "vigilance",
                "condition": condition.clone(),
            }));
        }
        if text.contains("trample") {
            modifiers.push(json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "trample",
                "condition": condition,
            }));
        }
        return Some(static_rule(modifiers));
    }

    let aura_keyword = |value: &str| match value.trim() {
        "double strike" => Some("doubleStrike"),
        "first strike" => Some("firstStrike"),
        "flying" => Some("flying"),
        "hexproof" => Some("hexproof"),
        "indestructible" => Some("indestructible"),
        "lifelink" => Some("lifelink"),
        "reach" => Some("reach"),
        "trample" => Some("trample"),
        "vigilance" => Some("vigilance"),
        _ => None,
    };
    let aura_keyword_modifier = |value: &str| {
        if let Some(keyword) = aura_keyword(value) {
            return Some(json!({
                "kind": "grantKeyword",
                "objects": enchanted_permanent.clone(),
                "keyword": keyword,
            }));
        }
        let ward_cost = Regex::new(r"(?i)^ward (.+)$")
            .expect("attached ward-cost regex compiles")
            .captures(value.trim())?;
        let (costs, decisions) = parse_activation_costs(ward_cost.get(1)?.as_str())?;
        (decisions.is_empty() && costs.len() == 1).then(|| {
            json!({
                "kind": "grantWard",
                "objects": enchanted_permanent.clone(),
                "cost": costs[0].clone(),
            })
        })
    };
    let reminder_free = text.split(" (").next().unwrap_or(text);
    let aura_keywords_re =
        Regex::new(r"^Enchanted creature has (.+)\.$").expect("enchanted keyword regex compiles");
    if let Some(captures) = aura_keywords_re.captures(reminder_free) {
        let normalized = captures[1].replace(", and ", ", ").replace(" and ", ", ");
        let modifiers = normalized
            .split(", ")
            .filter_map(aura_keyword_modifier)
            .collect::<Vec<_>>();
        if !modifiers.is_empty() && modifiers.len() == normalized.split(", ").count() {
            return Some(static_rule(modifiers));
        }
    }
    let aura_bonus_re =
        Regex::new(r"^Enchanted creature gets ([+-]\d+)/([+-]\d+)(?: and has (.+))?\.$")
            .expect("enchanted bonus regex compiles");
    if let Some(captures) = aura_bonus_re.captures(reminder_free) {
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": enchanted_permanent.clone(),
            "power": integer(captures[1].parse::<i64>().ok()?),
            "toughness": integer(captures[2].parse::<i64>().ok()?),
        })];
        if let Some(keyword_text) = captures.get(3) {
            let normalized = keyword_text
                .as_str()
                .replace(", and ", ", ")
                .replace(" and ", ", ");
            let keyword_modifiers = normalized
                .split(", ")
                .filter_map(aura_keyword_modifier)
                .collect::<Vec<_>>();
            if keyword_modifiers.len() != normalized.split(", ").count() {
                modifiers.clear();
            } else {
                modifiers.extend(keyword_modifiers);
            }
        }
        if !modifiers.is_empty() {
            return Some(static_rule(modifiers));
        }
    }

    let aura_bonus_protection_re = Regex::new(
        r"^Enchanted creature gets \+(\d+)/([+-]?\d+) and has protection from creatures\.$",
    )
    .expect("enchanted bonus protection regex compiles");
    if let Some(captures) = aura_bonus_protection_re.captures(reminder_free) {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": enchanted_permanent.clone(),
                "power": integer(captures[1].parse::<i64>().ok()?),
                "toughness": integer(captures[2].parse::<i64>().ok()?),
            }),
            json!({
                "kind": "grantProtection",
                "objects": enchanted_permanent.clone(),
                "from": ["creatures"],
            }),
        ]));
    }

    let aura_bonus_max_blocker_re = Regex::new(
        r"^Enchanted creature gets \+(\d+)/([+-]?\d+) and can't be blocked by more than one creature\.$",
    )
    .expect("enchanted bonus max-blocker regex compiles");
    if let Some(captures) = aura_bonus_max_blocker_re.captures(reminder_free) {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": enchanted_permanent.clone(),
                "power": integer(captures[1].parse::<i64>().ok()?),
                "toughness": integer(captures[2].parse::<i64>().ok()?),
            }),
            json!({
                "kind": "grantKeyword",
                "objects": enchanted_permanent.clone(),
                "keyword": "maxOneBlocker",
            }),
        ]));
    }

    let attached_count = |where_filter: Value, factor: i64| {
        json!({
            "kind": "multiply",
            "value": {
                "kind": "countAttachedPermanents",
                "attachmentSource": self_ref(),
                "where": where_filter,
            },
            "factor": factor,
        })
    };
    let battlefield_count = |where_filter: Value, all_players: bool, factor: i64| {
        json!({
            "kind": "multiply",
            "value": {
                "kind": "countPermanents",
                "player": controller(),
                "allPlayers": all_players,
                "where": where_filter,
            },
            "factor": factor,
        })
    };
    let variable_aura_bonus = match text {
        "This creature gets +1/+1 for each Aura on the battlefield." => Some((
            self_ref(),
            battlefield_count(subtype("Aura"), true, 1),
            Vec::new(),
        )),
        "This creature gets +2/+2 for each Aura attached to it." => {
            Some((self_ref(), attached_count(subtype("Aura"), 2), Vec::new()))
        }
        "Enchanted creature gets +1/+1 for each enchantment you control and has first strike." => {
            Some((
                enchanted_permanent.clone(),
                battlefield_count(card_type("Enchantment"), false, 1),
                vec!["firstStrike"],
            ))
        }
        "Enchanted creature gets +1/+1 for each Aura you control that's attached to a creature." => {
            Some((
                enchanted_permanent.clone(),
                battlefield_count(subtype("Aura"), false, 1),
                Vec::new(),
            ))
        }
        "Enchanted creature gets +2/+2 for each Aura and Equipment attached to it." => Some((
            enchanted_permanent.clone(),
            attached_count(or(vec![subtype("Aura"), subtype("Equipment")]), 2),
            Vec::new(),
        )),
        _ => None,
    };
    if let Some((objects, amount, keywords)) = variable_aura_bonus {
        let mut modifiers = vec![json!({
            "kind": "modifyPowerToughness",
            "objects": objects.clone(),
            "power": amount.clone(),
            "toughness": amount,
        })];
        modifiers.extend(keywords.into_iter().map(|keyword| {
            json!({
                "kind": "grantKeyword",
                "objects": objects.clone(),
                "keyword": keyword,
            })
        }));
        return Some(static_rule(modifiers));
    }

    let casting_reduction = match text {
        "Enchantment spells you cast cost {1} less to cast." => Some(card_type("Enchantment")),
        "Aura spells you cast cost {1} less to cast." => Some(subtype("Aura")),
        _ => None,
    };
    if let Some(where_filter) = casting_reduction {
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": where_filter,
            "amount": integer(1),
        })]));
    }

    if text.starts_with("(This token's mana cost is ") && text.ends_with(".)") {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
            }),
            &["Recognize token mana-cost reminder text"],
        ));
    }

    if text == "This creature can't block." {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": "cantBlock",
        })]));
    }

    if text == "As long as you've lost life this turn, this creature has flying and vigilance." {
        let condition = compare(
            ">=",
            json!({
                "kind": "lifeLostThisTurn",
                "player": controller(),
            }),
            integer(1),
        );
        return Some(static_rule(vec![
            json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "flying",
                "condition": condition.clone(),
            }),
            json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "vigilance",
                "condition": condition,
            }),
        ]));
    }

    if text
        == "As long as this creature is attacking, it gets +X/+0, where X is the number of lands defending player controls."
    {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": {
                "kind": "countDefendingPlayerPermanents",
                "attacker": self_ref(),
                "where": card_type("Land"),
            },
            "toughness": integer(0),
            "condition": {
                "kind": "isAttacking",
                "object": self_ref(),
            },
        })]));
    }

    if text.starts_with("Trample, myriad (") {
        return Some(draft(
            json!({
                "kind": "keywordAbilityGroup",
                "source": self_ref(),
                "abilities": [
                    { "kind": "trample" },
                    { "kind": "myriad" },
                ],
            }),
            &["Partition keyword list", "Recognize trample and myriad"],
        ));
    }

    if text
        == "If this card is in your opening hand, you may begin the game with it on the battlefield."
    {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "openingHandBattlefield" },
            }),
            &[
                "Recognize the opening-hand pregame action",
                "Offer the optional battlefield move after mulligans",
            ],
        ));
    }

    if text == "You may cast spells as though they had flash."
        || text
            == "You may cast the first creature spell you cast each turn as though it had flash."
    {
        return Some(static_rule(vec![json!({
            "kind": "grantFlashCasting",
            "player": controller(),
            "where": if text.contains("first creature") {
                card_type("Creature")
            } else {
                Value::Null
            },
            "firstEachTurn": text.contains("first creature"),
        })]));
    }

    let first_spell_reduction_and_flash_re = Regex::new(
        r"(?i)^The first (.+?) spell you cast each turn costs \{(\d+)\} less to cast and can be cast as though it had flash\.$",
    )
    .expect("first matching spell reduction and flash regex compiles");
    if let Some(captures) = first_spell_reduction_and_flash_re.captures(text) {
        let where_filter = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        let amount = captures.get(2)?.as_str().parse::<i64>().ok()?;
        if amount <= 0 {
            return None;
        }
        return Some(static_rule(vec![
            json!({
                "kind": "reduceCastingCost",
                "player": controller(),
                "where": where_filter.clone(),
                "amount": integer(amount),
                "firstEachTurn": true,
            }),
            json!({
                "kind": "grantFlashCasting",
                "player": controller(),
                "where": where_filter,
                "firstEachTurn": true,
            }),
        ]));
    }

    if text == "Lands you control are every basic land type in addition to their other types." {
        return Some(static_rule(vec![json!({
            "kind": "addAllBasicLandTypes",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Land"),
            },
        })]));
    }

    if text == "If you tap a permanent for mana, it produces twice as much of that mana instead." {
        return Some(static_rule(vec![json!({
            "kind": "multiplyManaProduction",
            "player": controller(),
            "factor": integer(2),
        })]));
    }

    if text == "Untap this artifact during each other player's untap step." {
        return Some(static_rule(vec![json!({
            "kind": "untapDuringOtherPlayersUntap",
            "object": self_ref(),
        })]));
    }
    let saga_chapter = |chapters: Vec<i64>, effects: Vec<Value>| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": chapters,
                },
                "effects": effects,
            }),
            &[
                "Recognize the Saga chapter symbols",
                "Trigger when the matching lore counter is added",
                "Resolve the chapter instruction",
            ],
        )
    };

    if text.starts_with("(As this Saga enters and after your draw step, add a lore counter.") {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
            }),
            &["Recognize intrinsic Saga lore-counter progression"],
        ));
    }
    if text.contains("Tap target creature an opponent controls.")
        && text.contains(
            "It doesn't untap during its controller's untap step for as long as you control this Saga.",
        )
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(1), integer(2)],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "tapPermanent",
                        "permanent": chosen_target("targetCreature"),
                    },
                    {
                        "kind": "installUntapRestriction",
                        "permanent": chosen_target("targetCreature"),
                        "duration": { "kind": "whileSourceControlled" },
                    },
                ],
            }),
            &[
                "Trigger on chapters one and two",
                "Tap the opposing creature",
                "Keep it tapped while the Saga remains controlled",
            ],
        ));
    }
    if text.ends_with("Return all tapped creatures to their owners' hands.") {
        return Some(saga_chapter(
            vec![3],
            vec![json!({
                "kind": "returnPermanentsToOwnersHands",
                "where": and(vec![card_type("Creature"), json!({ "kind": "isTapped" })]),
            })],
        ));
    }
    if text.ends_with(
        "Exile this Saga, then return it to the battlefield transformed under your control.",
    ) {
        return Some(saga_chapter(
            vec![3],
            vec![json!({
                "kind": "transformPermanent",
                "object": self_ref(),
            })],
        ));
    }
    if text.ends_with("Scry 2, then draw a card.") {
        return Some(saga_chapter(
            vec![1, 2],
            vec![
                json!({ "kind": "scry", "player": controller(), "count": integer(2) }),
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
            ],
        ));
    }
    if text.ends_with("Exile the top three cards of your library. Until the end of your next turn, you may play those cards.") {
        return Some(saga_chapter(
            vec![1],
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(controller()),
                    "count": integer(3),
                    "faceDown": false,
                    "bind": "rokuExiled",
                }),
                json!({
                    "kind": "grantPermission",
                    "player": controller(),
                    "action": {
                        "kind": "play",
                        "card": {
                            "kind": "boundObject",
                            "binding": "rokuExiled",
                        },
                        "normalTimingApplies": true,
                        "normalCostsApply": true,
                    },
                    "duration": {
                        "kind": "untilEndOfNextTurn",
                        "player": controller(),
                    },
                }),
            ],
        ));
    }
    if text.ends_with("Add one mana of any color.") && text.starts_with("II") {
        return Some(saga_chapter(
            vec![2],
            vec![json!({
                "kind": "addMana",
                "player": controller(),
                "mana": { "kind": "chooseColor", "amount": 1 },
            })],
        ));
    }
    if text.ends_with("Draw cards equal to the greatest power among creatures you control.") {
        return Some(saga_chapter(
            vec![1],
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "greatestPower",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
            })],
        ));
    }
    if text.ends_with("Earthbend X, where X is the number of cards in your hand. That land becomes an Island in addition to its other types.") {
        return Some(saga_chapter(
            vec![2],
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "kyoshiEarthbendHand",
            })],
        ));
    }
    if text.ends_with("Starting with you, each player chooses up to one permanent with mana value 3 or greater from among permanents your opponents control. Exile those permanents.") {
        return Some(saga_chapter(
            vec![1],
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "yangchenExileSelections",
            })],
        ));
    }
    if text.ends_with("You may have target opponent draw three cards. If you do, draw three cards.")
    {
        return Some(saga_chapter(
            vec![2],
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "yangchenSharedDraw",
            })],
        ));
    }

    if matches!(
        text,
        "As this creature enters, choose a creature type."
            | "As this artifact enters, choose a creature type."
            | "As this enchantment enters, choose a creature type."
            | "As this land enters, choose a creature type."
            | "As Three Tree City enters, choose a creature type."
    ) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "decisions": [{
                    "id": "chosenCreatureType",
                    "kind": "chooseCreatureType",
                }],
                "replacement": [{
                    "kind": "storeDecision",
                    "decisionId": "chosenCreatureType",
                }],
            }),
            &[
                "Recognize an as-enters creature-type choice",
                "Offer creature types present in the game",
                "Persist the chosen type on the permanent",
            ],
        ));
    }

    if text == "This creature is the chosen type in addition to its other types." {
        return Some(static_rule(vec![json!({
            "kind": "addChosenSubtype",
            "object": self_ref(),
            "choice": chosen_creature_type(),
        })]));
    }

    if text == "Creature spells of the chosen type cost {2} less to cast." {
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": chosen_creature_type(),
            "amount": integer(2),
        })]));
    }

    if text
        == "If a triggered ability of another creature you control of the chosen type triggers, it triggers an additional time."
    {
        return Some(static_rule(vec![json!({
            "kind": "multiplyTriggeredAbility",
            "sources": {
                "kind": "permanents",
                "controller": controller(),
                "excludeSource": true,
                "where": chosen_creature_type(),
            },
            "additionalTriggers": integer(1),
        })]));
    }

    let quoted_granted_mana_re = Regex::new(
        r#"(?i)^(?:Each )?(.+?) you control (?:has|have) (?:(.+?) and )?\"([^\"]+)\"\.?$"#,
    )
    .expect("quoted granted mana ability regex compiles");
    if let Some(captures) = quoted_granted_mana_re.captures(text) {
        let parsed =
            parse_mana_ability(captures.get(3)?.as_str()).map(promote_activated_mana_ability)?;
        let costs = parsed.rule["costs"].as_array()?;
        if costs.len() != 1 || costs[0]["kind"] != "tap" || costs[0]["object"]["kind"] != "self" {
            return None;
        }
        let effects = parsed.rule["effects"].as_array()?;
        if effects.len() != 1 || effects[0]["kind"] != "addMana" {
            return None;
        }
        let objects = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)?,
        });
        let mut modifiers = if let Some(keywords) = captures.get(2) {
            oracle_keyword_list(keywords.as_str())?
                .into_iter()
                .map(|keyword| {
                    json!({
                        "kind": "grantKeyword",
                        "objects": objects.clone(),
                        "keyword": keyword,
                    })
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let mut grant_mana = json!({
            "kind": "grantManaAbility",
            "objects": objects,
            "mana": effects[0]["mana"].clone(),
        });
        if let Some(restriction) = effects[0]
            .get("spendRestriction")
            .or_else(|| effects[0]["mana"].get("spendRestriction"))
        {
            grant_mana["spendRestriction"] = restriction.clone();
        }
        modifiers.push(grant_mana);
        return Some(static_rule(modifiers));
    }

    let generic_granted_mana = Regex::new(
        r#"^(?:Each )?(.+?) you control (?:has|have) \"\{T\}: Add (one|three) mana of any(?: one)? color\.\"$"#,
    )
    .expect("granted mana ability regex compiles")
    .captures(text)
    .and_then(|captures| {
        Some((
            card_qualifier_list_filter(captures[1].trim(), face_name)?,
            integer(parse_number_word(&captures[2])?),
            None,
        ))
    });
    let granted_mana = match text {
        "Creature tokens you control have \"{T}: Add one mana of any color.\"" => Some((
            and(vec![card_type("Creature"), json!({ "kind": "isToken" })]),
            integer(1),
            None,
        )),
        "Lands you control have \"{T}: Add one mana of any color.\"" => {
            Some((card_type("Land"), integer(1), None))
        }
        "Lands you control have \"{T}: Add three mana of any one color.\"" => {
            Some((card_type("Land"), integer(3), None))
        }
        "As long as you control six or more lands, lands you control have \"{T}: Add one mana of any color.\"" => {
            Some((
                card_type("Land"),
                integer(1),
                Some(compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                    }),
                    integer(6),
                )),
            ))
        }
        _ => generic_granted_mana,
    };
    if let Some((filter, amount, condition)) = granted_mana {
        let mut modifier = json!({
            "kind": "grantManaAbility",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": filter,
            },
            "mana": {
                "kind": "chooseColor",
                "amount": amount,
            },
        });
        if let Some(condition) = condition {
            modifier["condition"] = condition;
        }
        return Some(static_rule(vec![modifier]));
    }

    let self_selector = self_ref();
    let controlled_land_count = json!({
        "kind": "countPermanents",
        "player": controller(),
        "where": card_type("Land"),
    });
    let hand_count = json!({
        "kind": "countCards",
        "zone": hand(controller()),
        "where": Value::Null,
    });
    let lesson_grave_count = json!({
        "kind": "countCards",
        "zone": graveyard(controller()),
        "where": subtype("Lesson"),
    });
    let self_stat_modifier = |power: Value, toughness: Value| {
        static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_selector.clone(),
            "power": power,
            "toughness": toughness,
        })])
    };
    let source_land_count_stats_re = Regex::new(
        r"(?i)^(?:This creature|[A-Z][^']*)'s power and toughness are each equal to the number of lands you control\.$",
    )
    .expect("source land-count stats regex compiles");
    if source_land_count_stats_re.is_match(text) {
        return Some(self_stat_modifier(
            controlled_land_count.clone(),
            controlled_land_count.clone(),
        ));
    }
    let source_single_count_stat_re = Regex::new(
        r"(?i)^(.+?)'s (power|toughness) is equal to the number of (.+?) you control\.$",
    )
    .expect("single source characteristic from controlled permanent count regex compiles");
    if let Some(captures) = source_single_count_stat_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[3], face_name)?,
        });
        return if captures[2].eq_ignore_ascii_case("power") {
            Some(self_stat_modifier(count, integer(0)))
        } else {
            Some(self_stat_modifier(integer(0), count))
        };
    }
    let source_hand_count_power_re =
        Regex::new(r"(?i)^(.+?)'s power is equal to the number of cards in your hand\.$")
            .expect("source power from controller hand count regex compiles");
    if let Some(captures) = source_hand_count_power_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some(self_stat_modifier(hand_count.clone(), integer(0)));
    }
    match text {
        "Psychosis Crawler's power and toughness are each equal to the number of cards in your hand." =>
        {
            return Some(self_stat_modifier(hand_count.clone(), hand_count));
        }
        "Lumra's power and toughness are each equal to the number of lands you control."
        | "Ashaya's power and toughness are each equal to the number of lands you control." => {
            return Some(self_stat_modifier(
                controlled_land_count.clone(),
                controlled_land_count,
            ));
        }
        "Katara gets +1/+1 for each Lesson card in your graveyard." => {
            return Some(self_stat_modifier(
                lesson_grave_count.clone(),
                lesson_grave_count,
            ));
        }
        "Toph's power is equal to the number of +1/+1 counters on lands you control." => {
            return Some(self_stat_modifier(
                json!({
                    "kind": "countCountersOnPermanents",
                    "player": controller(),
                    "where": card_type("Land"),
                    "counter": "+1/+1",
                }),
                integer(0),
            ));
        }
        _ => {}
    }

    if text
        == "As long as this enchantment has seven or more quest counters on it, creatures you control get +5/+5."
    {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": controlled_creatures.clone(),
            "power": integer(5),
            "toughness": integer(5),
            "condition": compare(
                ">=",
                json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": "quest",
                }),
                integer(7),
            ),
        })]));
    }

    if text
        == "Creatures you control of the chosen type get +1/+1 for each fellowship counter on this artifact."
    {
        let fellowship = json!({
            "kind": "countCounters",
            "object": self_ref(),
            "counter": "fellowship",
        });
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": chosen_creature_type(),
            },
            "power": fellowship.clone(),
            "toughness": fellowship,
        })]));
    }

    let chosen_type_bonus_re =
        Regex::new(r"(?i)^(.+?) you control of the chosen type get ([+-]\d+)/([+-]\d+)\.$")
            .expect("chosen creature-type bonus regex compiles");
    if let Some(captures) = chosen_type_bonus_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": and(vec![
                    parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                    chosen_creature_type(),
                ]),
            },
            "power": integer(captures[2].parse::<i64>().ok()?),
            "toughness": integer(captures[3].parse::<i64>().ok()?),
        })]));
    }

    if let Some(captures) = Regex::new(r"^Creatures you control get \+(\d+)/\+(\d+)\.$")
        .expect("controlled creature bonus regex compiles")
        .captures(text)
    {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": controlled_creatures,
            "power": integer(captures[1].parse::<i64>().ok()?),
            "toughness": integer(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let additional_land_re = Regex::new(&format!(
        r"(?i)^You may play (?:an|({})) additional lands? on each of your turns\.$",
        count_word_pattern(),
    ))
    .expect("additional land-play regex compiles");
    if let Some(captures) = additional_land_re.captures(text) {
        let count = captures
            .get(1)
            .and_then(|value| parse_number_word(value.as_str()))
            .unwrap_or(1);
        return Some(static_rule(vec![json!({
            "kind": "additionalLandPlay",
            "player": controller(),
            "count": integer(count),
        })]));
    }
    if text == "You may play lands from your graveyard." {
        return Some(static_rule(vec![json!({
            "kind": "playLandsFromGraveyard",
            "player": controller(),
        })]));
    }

    let zone_casting_reduction_re = Regex::new(
        r"(?i)^Spells you cast from (anywhere other than your hand|your graveyard|your hand|exile) cost \{(\d+)\} less to cast\.$",
    )
    .expect("source-zone casting reduction regex compiles");
    if let Some(captures) = zone_casting_reduction_re.captures(text) {
        let mut modifier = json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": Value::Null,
            "amount": integer(captures[2].parse::<i64>().ok()?),
        });
        match captures[1].to_ascii_lowercase().as_str() {
            "anywhere other than your hand" => modifier["sourceZoneNot"] = json!("hand"),
            "your graveyard" => modifier["sourceZone"] = json!("graveyard"),
            "your hand" => modifier["sourceZone"] = json!("hand"),
            "exile" => modifier["sourceZone"] = json!("exile"),
            _ => return None,
        }
        return Some(static_rule(vec![modifier]));
    }

    let casting_reduction_re = Regex::new(r"^(.+?) spells you cast cost \{(\d+)\} less to cast\.$")
        .expect("global casting reduction regex compiles");
    if let Some(captures) = casting_reduction_re.captures(text) {
        let filter = if captures[1].eq_ignore_ascii_case("noncreature") {
            not(card_type("Creature"))
        } else {
            card_qualifier_list_filter(&captures[1], "")
                .or_else(|| parse_permanent_criteria(&captures[1], ""))?
        };
        return Some(static_rule(vec![json!({
            "kind": "reduceCastingCost",
            "player": controller(),
            "where": filter,
            "amount": integer(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let repeated_trigger_re = Regex::new(
        r"(?i)^If a triggered ability of (.+?) triggers, that ability triggers an additional time\.$",
    )
    .expect("repeated trigger scope regex compiles");
    if let Some(captures) = repeated_trigger_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        let sources = if subject.eq_ignore_ascii_case("equipped creature") {
            json!({
                "kind": "attachedPermanent",
                "attachment": self_ref(),
            })
        } else {
            let criteria = subject.strip_suffix(" you control")?;
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": parse_permanent_criteria(criteria, face_name)?,
            })
        };
        return Some(static_rule(vec![json!({
            "kind": "multiplyTriggeredAbility",
            "sources": sources,
            "additionalTriggers": integer(1),
        })]));
    }

    if text
        == "If a land entering causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time."
    {
        return Some(static_rule(vec![json!({
            "kind": "multiplyTriggeredAbility",
            "sources": {
                "kind": "permanents",
                "controller": controller(),
                "where": Value::Null,
            },
            "eventWhere": {
                "kind": "permanentEntered",
                "where": card_type("Land"),
            },
            "additionalTriggers": integer(1),
        })]));
    }

    let entry_caused_repeated_trigger_re = Regex::new(
        r"(?i)^If (.+?) you control entering the battlefield causes a triggered ability of (.+?) you control to trigger, that ability triggers an additional time\.$",
    )
    .expect("filtered entry caused repeated trigger regex compiles");
    if let Some(captures) = entry_caused_repeated_trigger_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "multiplyTriggeredAbility",
            "sources": {
                "kind": "permanents",
                "controller": controller(),
                "where": parse_permanent_criteria(&captures[2], face_name)?,
            },
            "eventWhere": {
                "kind": "permanentEntered",
                "where": card_qualifier_list_filter(&captures[1], face_name)
                    .or_else(|| parse_permanent_criteria(&captures[1], face_name))?,
            },
            "additionalTriggers": integer(1),
        })]));
    }

    let retain_mana_re = Regex::new(
        r"^You don't lose unspent (white|blue|black|red|green|colorless) mana as steps and phases end\.$",
    )
    .expect("retained mana color regex compiles");
    if let Some(captures) = retain_mana_re.captures(text) {
        let symbol = match &captures[1] {
            "white" => "W",
            "blue" => "U",
            "black" => "B",
            "red" => "R",
            "green" => "G",
            "colorless" => "C",
            _ => return None,
        };
        return Some(static_rule(vec![json!({
            "kind": "retainUnspentMana",
            "player": controller(),
            "symbols": [symbol],
        })]));
    }

    let unspent_mana_stats_re = Regex::new(
        r"(?i)^(.+?) gets \+(\d+)/\+(\d+) for each unspent (white|blue|black|red|green|colorless) mana you have\.$",
    )
    .expect("unspent mana stats regex compiles");
    if let Some(captures) = unspent_mana_stats_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        let symbol = match &captures[4] {
            "white" => "W",
            "blue" => "U",
            "black" => "B",
            "red" => "R",
            "green" => "G",
            "colorless" => "C",
            _ => return None,
        };
        let mana_count = json!({
            "kind": "manaPoolSymbolCount",
            "player": controller(),
            "symbol": symbol,
        });
        let scaled = |amount: i64| {
            json!({
                "kind": "multiply",
                "value": mana_count.clone(),
                "factor": integer(amount),
            })
        };
        return Some(self_stat_modifier(
            scaled(captures[2].parse::<i64>().ok()?),
            scaled(captures[3].parse::<i64>().ok()?),
        ));
    }

    if text.starts_with("All creatures have shroud.") {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": {
                "kind": "permanents",
                "where": card_type("Creature"),
            },
            "keyword": "shroud",
        })]));
    }

    let attached_count_bonus_re = Regex::new(
        r"^(?:Equipped|Enchanted) creature gets \+1/\+1 for each (artifact|artifact and/or enchantment) you control\.$",
    )
    .expect("attached count bonus regex compiles");
    if let Some(captures) = attached_count_bonus_re.captures(text) {
        let filter = if &captures[1] == "artifact" {
            card_type("Artifact")
        } else {
            or(vec![card_type("Artifact"), card_type("Enchantment")])
        };
        let count = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": filter,
        });
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": {
                "kind": "attachedPermanent",
                "attachment": self_ref(),
            },
            "power": count.clone(),
            "toughness": count,
        })]));
    }

    let scoped_keywords_re = Regex::new(
        r"^(Land creatures|Allies|Creatures|[A-Z][A-Za-z'-]+ creatures) you control(?: with \+1/\+1 counters on them)? have ([A-Za-z ]+?)(?: and ([A-Za-z ]+?))?\.(?: \(.+\))?$",
    )
    .expect("controlled keyword scope regex compiles");
    if let Some(captures) = scoped_keywords_re.captures(text) {
        let filter = match &captures[1] {
            "Land creatures" => and(vec![card_type("Land"), card_type("Creature")]),
            "Allies" => subtype("Ally"),
            "Creatures" => card_type("Creature"),
            value if value.ends_with(" creatures") => and(vec![
                card_type("Creature"),
                subtype(value.trim_end_matches(" creatures")),
            ]),
            _ => return None,
        };
        let filter = if text.contains("with +1/+1 counters on them") {
            and(vec![
                filter,
                json!({ "kind": "hasCounter", "counter": "+1/+1" }),
            ])
        } else {
            filter
        };
        let parse_keyword = |value: &str| match value.trim().to_ascii_lowercase().as_str() {
            "double strike" => Some("doubleStrike"),
            "flying" => Some("flying"),
            "haste" => Some("haste"),
            "hexproof" => Some("hexproof"),
            "indestructible" => Some("indestructible"),
            "lifelink" => Some("lifelink"),
            "reach" => Some("reach"),
            "trample" => Some("trample"),
            "vigilance" => Some("vigilance"),
            _ => None,
        };
        let keywords = [captures.get(2), captures.get(3)]
            .into_iter()
            .flatten()
            .filter_map(|capture| parse_keyword(capture.as_str()))
            .collect::<Vec<_>>();
        if !keywords.is_empty() {
            let objects = json!({
                "kind": "permanents",
                "controller": controller(),
                "where": filter,
            });
            return Some(static_rule(
                keywords
                    .into_iter()
                    .map(|keyword| {
                        json!({
                            "kind": "grantKeyword",
                            "objects": objects.clone(),
                            "keyword": keyword,
                        })
                    })
                    .collect(),
            ));
        }
    }

    let own_cost_by_power_re = Regex::new(
        r"(?i)^This spell costs \{X\} less to cast, where X is the (greatest|total) power (?:among|of) (.+?) you control((?: with .+)?)\.$",
    )
    .expect("own casting reduction by controlled power regex compiles");
    if let Some(captures) = own_cost_by_power_re.captures(text) {
        let criteria = format!("{}{}", &captures[2], &captures[3]);
        let where_filter = parse_permanent_criteria(criteria.trim(), face_name)?;
        let amount_kind = if captures[1].eq_ignore_ascii_case("greatest") {
            "greatestPower"
        } else {
            "sumPowers"
        };
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": {
                        "kind": amount_kind,
                        "player": controller(),
                        "where": where_filter,
                    },
                }],
            }),
            &[
                "Parse the controlled permanent criteria",
                "Aggregate their power",
                "Reduce only this spell's generic casting cost",
            ],
        ));
    }

    let own_cost_by_greatest_opponent_permanent_count_re = Regex::new(
        r"(?i)^This spell costs \{X\} less to cast, where X is the greatest number of (.+?) an opponent controls\.$",
    )
    .expect("own casting reduction by greatest opponent permanent count regex compiles");
    if let Some(captures) = own_cost_by_greatest_opponent_permanent_count_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": {
                        "kind": "greatestOpponentPermanentCount",
                        "player": controller(),
                        "where": parse_permanent_criteria(
                            &singular_card_term(captures.get(1)?.as_str()),
                            face_name,
                        )?,
                    },
                }],
            }),
            &[
                "Parse the opponent-controlled permanent criteria",
                "Take the greatest matching count among opponents",
                "Reduce only this spell's generic casting cost",
            ],
        ));
    }

    let other_controlled_enters_with_source_stat_counters_re = Regex::new(
        r"(?i)^Each other (.+?) you control enters with a number of additional ([^ ]+) counters? on it equal to (.+?)'s (power|toughness)\.$",
    )
    .expect("other controlled entry counters by source stat regex compiles");
    if let Some(captures) = other_controlled_enters_with_source_stat_counters_re.captures(text)
        && source_reference_matches(captures.get(3)?.as_str(), face_name)
    {
        return Some(static_rule(vec![json!({
            "kind": "addEnteringCounters",
            "objects": {
                "kind": "permanents",
                "controller": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                "excludeSource": true,
            },
            "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
            "count": {
                "kind": if captures[4].eq_ignore_ascii_case("power") {
                    "powerOf"
                } else {
                    "toughnessOf"
                },
                "object": self_ref(),
            },
        })]));
    }

    let self_bonus_per_sized_player_zone_re = Regex::new(&format!(
        r"(?i)^This creature gets ([+-]\d+)/([+-]\d+) for each (graveyard|hand|library) with ({}) or (more|fewer) cards in it\.$",
        count_word_pattern(),
    ))
    .expect("self bonus per sized player zone regex compiles");
    if let Some(captures) = self_bonus_per_sized_player_zone_re.captures(text) {
        let zones = json!({
            "kind": "countPlayerZonesByCardCount",
            "zone": captures[3].to_ascii_lowercase(),
            "operator": if captures[5].eq_ignore_ascii_case("more") { ">=" } else { "<=" },
            "count": integer(parse_number_word(&captures[4])?),
        });
        let scaled = |amount: i64| {
            if amount == 1 {
                zones.clone()
            } else {
                json!({
                    "kind": "multiply",
                    "value": zones.clone(),
                    "factor": integer(amount),
                })
            }
        };
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": self_ref(),
            "power": scaled(captures[1].parse::<i64>().ok()?),
            "toughness": scaled(captures[2].parse::<i64>().ok()?),
        })]));
    }

    let attached_loses_abilities_and_does_not_untap_re = Regex::new(
        r"(?i)^Enchanted (creature|permanent) loses all abilities and doesn't untap during its controller's untap step\.$",
    )
    .expect("attached permanent loses-abilities and untap-restriction regex compiles");
    if attached_loses_abilities_and_does_not_untap_re.is_match(text) {
        let objects = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(static_rule(vec![
            json!({ "kind": "loseAllAbilities", "objects": objects.clone() }),
            json!({ "kind": "doesNotUntap", "objects": objects }),
        ]));
    }
    let attached_does_not_untap_re = Regex::new(
        r"(?i)^Enchanted (creature|permanent) doesn't untap during its controller's untap step\.$",
    )
    .expect("attached permanent untap-restriction regex compiles");
    if attached_does_not_untap_re.is_match(text) {
        return Some(static_rule(vec![json!({
            "kind": "doesNotUntap",
            "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
        })]));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_direct_landfall_static_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if !text.starts_with("Landfall ") {
        return None;
    }
    let instruction_index = text.find("Whenever a land you control enters")?;
    let instruction = &text[instruction_index..];
    let triggered = |effects: Vec<Value>| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "effects": effects,
            }),
            &[
                "Normalize the Landfall separator",
                "Recognize the controlled land entry",
                "Resolve the Landfall instruction",
            ],
        )
    };
    match instruction {
        "Whenever a land you control enters, you get an experience counter." => {
            Some(triggered(vec![json!({
                "kind": "addPlayerCounters",
                "player": controller(),
                "counter": "experience",
                "count": integer(1),
            })]))
        }
        "Whenever a land you control enters, mill a card." => Some(triggered(vec![json!({
            "kind": "mill",
            "player": controller(),
            "count": integer(1),
        })])),
        "Whenever a land you control enters, add one mana of any color." => {
            Some(triggered(vec![json!({
                "kind": "addMana",
                "player": controller(),
                "mana": { "kind": "chooseColor", "amount": 1 },
            })]))
        }
        "Whenever a land you control enters, create a 4/4 red Dragon creature token with flying." => {
            Some(triggered(vec![create_token_effect(
                "Create a 4/4 red Dragon creature token with flying.",
            )?]))
        }
        "Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn." => {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "advanceEarthbenderAscension",
            })]))
        }
        value
            if value.starts_with(
                "Whenever a land you control enters, create a Food token or a Treasure token.",
            ) =>
        {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "chooseLandfallProvision",
            })]))
        }
        "Whenever a land you control enters, exile target permanent card with mana value 3 or less from your graveyard." => {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "exileLandfallGravePermanent",
            })]))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_landfall_static_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let instruction = text
        .strip_prefix("Landfall â€” ")
        .or_else(|| text.strip_prefix("Landfall Ã¢â‚¬â€ "))?;
    let triggered = |effects: Vec<Value>| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "effects": effects,
            }),
            &[
                "Recognize the landfall event",
                "Resolve the controller's entering land",
                "Apply the landfall instruction",
            ],
        )
    };

    match instruction {
        "Whenever a land you control enters, you get an experience counter." => {
            Some(triggered(vec![json!({
                "kind": "addPlayerCounters",
                "player": controller(),
                "counter": "experience",
                "count": integer(1),
            })]))
        }
        "Whenever a land you control enters, mill a card." => Some(triggered(vec![json!({
            "kind": "mill",
            "player": controller(),
            "count": integer(1),
        })])),
        "Whenever a land you control enters, add one mana of any color." => {
            Some(triggered(vec![json!({
                "kind": "addMana",
                "player": controller(),
                "mana": { "kind": "chooseColor", "amount": 1 },
            })]))
        }
        "Whenever a land you control enters, create a 4/4 red Dragon creature token with flying." => {
            Some(triggered(vec![create_token_effect(
                "Create a 4/4 red Dragon creature token with flying.",
            )?]))
        }
        "Whenever a land you control enters, put a quest counter on this enchantment. When you do, if it has four or more quest counters on it, put a +1/+1 counter on target creature you control. It gains trample until end of turn." => {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "advanceEarthbenderAscension",
            })]))
        }
        value
            if value.starts_with(
                "Whenever a land you control enters, create a Food token or a Treasure token.",
            ) =>
        {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "chooseLandfallProvision",
            })]))
        }
        "Whenever a land you control enters, exile target permanent card with mana value 3 or less from your graveyard." => {
            Some(triggered(vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "exileLandfallGravePermanent",
            })]))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_avatar_deck_static(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let static_rule = |modifiers: Vec<Value>| {
        draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            }),
            &[
                "Resolve an Avatar deck continuous effect",
                "Select affected objects and players",
                "Apply the modifier while the source remains on the battlefield",
            ],
        )
    };
    let activated = |costs: Vec<Value>, operation: &str, activation_condition: Option<Value>| {
        let mut rule = json!({
            "kind": "activatedAbility",
            "source": self_ref(),
            "costs": costs,
            "effects": [{
                "kind": "resolveTriggeredInstruction",
                "operation": operation,
            }],
            "timing": { "kind": "sorcerySpeed" },
        });
        if let Some(condition) = activation_condition {
            rule["activationCondition"] = condition;
        }
        draft(
            rule,
            &[
                "Expose the printed activated ability",
                "Pay all costs before resolution",
                "Resolve the complete instruction",
            ],
        )
    };
    let triggered = |event: Value, operation: &str| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": operation,
                }],
            }),
            &["Normalize the printed static keyword as its triggered game action"],
        )
    };
    let equipment_targeting_re = Regex::new(
        r"(?i)^(Activated abilities of Equipment you control|Equip abilities you activate) that target (this creature|enchanted creature) cost \{(\d+)\} less to activate\.$",
    )
    .expect("Equipment targeting activation reduction regex compiles");
    if let Some(captures) = equipment_targeting_re.captures(text) {
        let mut modifier = json!({
            "kind": "reduceEquipmentTargetingActivation",
            "object": if captures[2].eq_ignore_ascii_case("enchanted creature") {
                json!({ "kind": "attachedPermanent", "attachment": self_ref() })
            } else {
                self_ref()
            },
            "amount": integer(captures[3].parse::<i64>().ok()?),
        });
        if captures[1]
            .to_ascii_lowercase()
            .starts_with("equip abilities")
        {
            modifier["abilityKind"] = Value::String("equip".to_string());
        }
        return Some(static_rule(vec![modifier]));
    }
    match text {
        "If you would create a Clue, Food, or Treasure token, instead create one of each." => {
            return Some(static_rule(vec![json!({
                "kind": "academyManufactorReplacement",
                "player": controller(),
            })]));
        }
        value if value.starts_with("Creatures your opponents control with power less than Baeloth Barrityl's power are goaded") => {
            return Some(static_rule(vec![json!({
                "kind": "baelothGoad",
                "player": controller(),
                "powerSource": self_ref(),
            })]));
        }
        "During your turn, equipped creatures you control have double strike and haste." => {
            return Some(static_rule(vec![json!({
                "kind": "blacksmithTalentCombatKeywords",
                "player": controller(),
            })]));
        }
        "Aura spells you cast that target enchanted creature cost {3} less to cast." => {
            return Some(static_rule(vec![json!({
                "kind": "reduceAuraTargetingCasting",
                "object": { "kind": "attachedPermanent", "attachment": self_ref() },
                "amount": integer(3),
            })]));
        }
        "Goaded creatures your opponents control can't block." => {
            return Some(static_rule(vec![json!({
                "kind": "goadedOpponentsCantBlock",
                "player": controller(),
            })]));
        }
        "Enchanted creature gets +3/+0 and can only attack alone." => {
            return Some(static_rule(vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "power": integer(3),
                    "toughness": integer(0),
                }),
                json!({ "kind": "attackAlone", "object": { "kind": "attachedPermanent", "attachment": self_ref() } }),
            ]));
        }
        value if value.starts_with("+2: Search your library for a basic Mountain card") => {
            let mut result = activated(
                vec![json!({ "kind": "payLoyalty", "object": self_ref(), "amount": integer(2) })],
                "kothSearchMountain",
                None,
            );
            result.rule["startingLoyalty"] = integer(4);
            result.rule["activationLimit"] = json!({ "kind": "oncePerTurn", "id": "loyaltyAbility" });
            return Some(result);
        }
        value if value.contains("Koth deals damage to target creature equal to the number of Mountains you control") => {
            let mut result = activated(
                vec![json!({ "kind": "payLoyalty", "object": self_ref(), "amount": integer(-3) })],
                "kothMountainDamage",
                None,
            );
            result.rule["startingLoyalty"] = integer(4);
            result.rule["activationLimit"] = json!({ "kind": "oncePerTurn", "id": "loyaltyAbility" });
            return Some(result);
        }
        value if value.contains("You get an emblem with \"Whenever a Mountain you control enters") => {
            let mut result = activated(
                vec![json!({ "kind": "payLoyalty", "object": self_ref(), "amount": integer(-7) })],
                "kothCreateEmblem",
                None,
            );
            result.rule["startingLoyalty"] = integer(4);
            result.rule["activationLimit"] = json!({ "kind": "oncePerTurn", "id": "loyaltyAbility" });
            return Some(result);
        }
        "You may have this Equipment enter as a copy of any Equipment on the battlefield." => {
            return Some(draft(
                json!({
                    "kind": "replacementEffect",
                    "source": self_ref(),
                    "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                    "replacement": [{ "kind": "copyEnteringEquipment" }],
                }),
                &["Choose any Equipment on the battlefield", "Enter as a copy of it"],
            ));
        }
        "Enchant creature you control" => {
            return Some(draft(
                json!({
                    "kind": "keywordAbility",
                    "source": self_ref(),
                    "ability": { "kind": "enchant", "where": card_type("Creature"), "controller": controller() },
                }),
                &["Restrict the Aura to a creature you control"],
            ));
        }
        value if Regex::new(
            r"^Enchanted creature gets ([+-]\d+)/([+-]\d+) and is goaded\. \(It attacks each combat if able and attacks a player other than you if able\.\)$",
        )
        .expect("attached goad bonus regex compiles")
        .captures(value)
        .is_some() => {
            let captures = Regex::new(
                r"^Enchanted creature gets ([+-]\d+)/([+-]\d+) and is goaded\. \(It attacks each combat if able and attacks a player other than you if able\.\)$",
            )
            .expect("attached goad bonus regex compiles")
            .captures(value)?;
            return Some(static_rule(vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "power": integer(captures[1].parse::<i64>().ok()?),
                    "toughness": integer(captures[2].parse::<i64>().ok()?),
                }),
                json!({ "kind": "goadAttachedPermanent", "attachment": self_ref() }),
            ]));
        }
        value if value.starts_with("Equipped creature gets +1/+1 and has trample") => {
            return Some(static_rule(vec![json!({
                "kind": "reaverCleaver",
                "attachment": self_ref(),
            })]));
        }
        "have it.)" => {
            return Some(draft(json!({ "kind": "rulesMarker", "text": text }), &["Preserve reminder-text continuation"]));
        }
        "Mentor (Whenever this creature attacks, put a +1/+1 counter on target attacking creature with lesser power.)" => {
            return Some(triggered(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "resolveMentor",
            ));
        }
        "Firebending X, where X is the number of creatures you control. (Whenever this creature attacks, add X {R}. This mana lasts until end of combat.)" => {
            return Some(triggered(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "sunWarriorsFirebending",
            ));
        }
        "Firebending X, where X is the number of experience counters you have. (Whenever this creature attacks, add X {R}. This mana lasts until end of combat.)" => {
            return Some(triggered(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "zukoExperienceFirebending",
            ));
        }
        "The Watcher in the Water enters tapped with nine stun counters on it. (If a permanent with a stun counter would become untapped, remove one from it instead.)" => {
            return Some(draft(
                json!({
                    "kind": "replacementEffect",
                    "source": self_ref(),
                    "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                    "replacement": [
                        { "kind": "setEnteringState", "tapped": true },
                        { "kind": "putEnteringCounters", "counter": "stun", "count": integer(9) },
                    ],
                }),
                &["Enter tapped", "Enter with nine stun counters"],
            ));
        }
        "This land enters tapped. As it enters, choose a color other than white." => {
            return Some(draft(
                json!({
                    "kind": "replacementEffect",
                    "source": self_ref(),
                    "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                    "decisions": [{
                        "id": "thrivingColor",
                        "kind": "chooseColor",
                        "options": ["U", "B", "R", "G"],
                    }],
                    "replacement": [
                        { "kind": "setEnteringState", "tapped": true },
                        { "kind": "storeDecision", "decisionId": "thrivingColor" },
                    ],
                }),
                &["Enter tapped", "Choose and retain a nonwhite color"],
            ));
        }
        value if value.contains("Waterbend {20}: Take an extra turn after this one") => {
            let mut result = activated(
                vec![json!({ "kind": "payWaterbend", "amount": integer(20) })],
                "legendOfKurukExtraTurn",
                None,
            );
            result.rule["exhaust"] = Value::Bool(true);
            return Some(result);
        }
        "Spells you cast cost {W}{U}{B}{R}{G} less to cast. (This can reduce generic costs.)" => {
            return Some(static_rule(vec![json!({
                "kind": "reduceCastingCost",
                "player": controller(),
                "where": Value::Null,
                "amount": integer(5),
            })]));
        }
        _ => {}
    }
    if text.starts_with("Station (Tap another creature you control:") {
        let mut result = activated(
            vec![json!({
                "kind": "tap",
                "object": chosen_target("stationCreature"),
                "bindPowerAs": "stationCreaturePower",
            })],
            "stationCreaturePower",
            Some(json!({ "kind": "sorceryTiming" })),
        );
        result.rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [target_decision(
                "stationCreature",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                    "ignoreTargetingRestrictions": true,
                }),
                1,
                1,
            )],
        });
        return Some(result);
    }
    if text.starts_with("12+ | {3}{W}, {T}: Create a token that's a copy of target artifact or enchantment you control")
    {
        let mut result = activated(
            vec![
                json!({ "kind": "payMana", "manaCost": "{3}{W}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            "adagiaLegendaryCopy",
            Some(compare(
                ">=",
                json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": "charge",
                }),
                integer(12),
            )),
        );
        result.rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "copyTarget",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "controller": controller(),
                    "where": or(vec![card_type("Artifact"), card_type("Enchantment")]),
                },
            }],
        });
        return Some(result);
    }
    if text
        == "If this land would enter, sacrifice two untapped lands instead. If you do, put this land onto the battlefield. If you don't, put it into its owner's graveyard."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "entryLandSacrifices",
                    "kind": "chooseUntappedLands",
                    "minimum": 0,
                    "maximum": 2,
                }],
                "replacement": [{
                    "kind": "sacrificeLandsOrReplaceWithGraveyard",
                    "decisionId": "entryLandSacrifices",
                    "required": 2,
                }],
            }),
            &[
                "Choose two untapped lands controlled before this land enters",
                "Sacrifice the chosen lands as the replacement is applied",
                "Put the entering land into its owner's graveyard if the cost is not paid",
            ],
        ));
    }
    let crew_text = text
        .split_once(" (")
        .map(|(keyword, _)| keyword)
        .unwrap_or(text);
    if let Some(captures) = Regex::new(r"^Crew (\d+)$")
        .expect("crew regex compiles")
        .captures(crew_text)
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [{ "kind": "payMana", "manaCost": "{0}" }],
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "crewVehicle",
                    "minimumPower": integer(captures[1].parse::<i64>().ok()?),
                }],
            }),
            &[
                "Collect untapped controlled creatures",
                "Choose creatures with sufficient total power",
                "Tap them and animate the Vehicle for the turn",
            ],
        ));
    }
    if text == "{5}, {T}, Exile this artifact: Exile all nonland permanents." {
        return Some(activated(
            vec![
                json!({ "kind": "payMana", "manaCost": "{5}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            "perilousVaultExileAll",
            None,
        ));
    }
    let additional_counter_re = Regex::new(
        r"^If one or more (.+) counters would be put on a (permanent|creature) you control, that many plus one (.+) counters are put on it instead\.$",
    )
    .expect("additional counter-placement regex compiles");
    if let Some(captures) = additional_counter_re.captures(text)
        && captures[1].eq_ignore_ascii_case(&captures[3])
    {
        return Some(static_rule(vec![json!({
            "kind": "addCounterPlacement",
            "player": controller(),
            "where": if captures[2].eq_ignore_ascii_case("creature") {
                card_type("Creature")
            } else {
                Value::Null
            },
            "counter": &captures[1],
            "amount": 1,
        })]));
    }
    if text.starts_with("Bestow ") {
        let mana_cost = text
            .strip_prefix("Bestow ")
            .unwrap_or_default()
            .trim()
            .to_string();
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "bestow", "manaCost": mana_cost },
            }),
            &["Cast the creature for its bestow cost as an Aura"],
        ));
    }
    if text == "Enchanted creature gets +1/+1." {
        return Some(static_rule(vec![json!({
            "kind": "modifyPowerToughness",
            "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
            "power": integer(1),
            "toughness": integer(1),
        })]));
    }
    if text.contains("Whenever a land you control enters, you may pay {1}{G}")
        && text.contains("create a 1/1 green Insect creature token")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "springheartNantukoLandfall",
                }],
            }),
            &["Resolve Springheart Nantuko's optional copy-or-Insect landfall"],
        ));
    }
    let opponent_death_exile_re = Regex::new(
        r"(?i)^If (?:a|an) (.+?) an opponent controls would die, exile it instead\.(?: When you do, (.+))?$",
    )
    .expect("opponent permanent death-exile replacement regex compiles");
    if let Some(captures) = opponent_death_exile_re.captures(text) {
        let mut modifier = json!({
            "kind": "exileOpponentCreatureInsteadOfDying",
            "player": controller(),
            "where": parse_permanent_criteria(captures.get(1)?.as_str(), "")?,
        });
        if let Some(followup) = captures.get(2) {
            let (effects, decisions) = parse_general_effect_sequence(followup.as_str(), "")
                .or_else(|| parse_general_effect_instruction(followup.as_str(), ""))?;
            let mut ability = json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                "effects": effects,
            });
            if !decisions.is_empty() {
                ability["declaration"] = json!({
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                });
            }
            modifier["onReplaced"] = ability;
        }
        return Some(static_rule(vec![modifier]));
    }
    if text.starts_with(
        "Creatures you control have \"Whenever this creature becomes tapped for the first time",
    ) {
        return Some(static_rule(vec![json!({
            "kind": "grantFirstTapCounterTrigger",
            "player": controller(),
        })]));
    }
    if text.starts_with("Equipped creature gets +1/+1 and has \"{15}, Exile The Dominion Bracelet:")
    {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                "power": integer(1),
                "toughness": integer(1),
            }),
            json!({ "kind": "dominionBraceletControlAbility" }),
        ]));
    }
    if text
        == "{2}, {T}, Exile The Stasis Coffin: You gain protection from everything until your next turn."
    {
        return Some(activated(
            vec![
                json!({ "kind": "payMana", "manaCost": "{2}" }),
                json!({ "kind": "tap", "object": self_ref() }),
            ],
            "stasisCoffinProtection",
            None,
        ));
    }
    if text
        == "As this artifact enters, choose a creature type. This artifact enters with a fellowship counter on it for each creature you control of the chosen type."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "chosenCreatureType",
                    "kind": "chooseCreatureType",
                }],
                "replacement": [
                    { "kind": "storeDecision", "decisionId": "chosenCreatureType" },
                    {
                        "kind": "putEnteringCountersForChosenType",
                        "counter": "fellowship",
                        "decisionId": "chosenCreatureType",
                    },
                ],
            }),
            &[
                "Choose a creature type",
                "Store the choice",
                "Enter with fellowship counters",
            ],
        ));
    }
    if text
        == "If an effect would put one or more counters on a permanent you control, it puts twice that many of those counters on that permanent instead."
        || text
            == "If you would put one or more counters on a permanent or player, put twice that many of each of those kinds of counters on that permanent or player instead."
    {
        return Some(static_rule(vec![json!({
            "kind": "multiplyCounterPlacement",
            "player": controller(),
            "factor": 2,
        })]));
    }
    if text == "Permanents you control with counters on them have ward {1}." {
        return Some(static_rule(vec![json!({
            "kind": "grantWardToCounteredPermanents",
            "player": controller(),
            "cost": { "kind": "payMana", "manaCost": "{1}" },
        })]));
    }
    if text.starts_with("If this card is in your opening hand and you're not the starting player") {
        return Some(static_rule(vec![json!({
            "kind": "gemstoneCavernsOpeningHand",
            "player": controller(),
        })]));
    }
    if text == "You may look at the top card of your library any time." {
        return Some(static_rule(vec![json!({
            "kind": "lookAtTopLibrary",
            "player": controller(),
        })]));
    }
    if text == "You may cast creature spells of the chosen type from the top of your library." {
        return Some(static_rule(vec![json!({
            "kind": "castChosenCreatureFromTop",
            "player": controller(),
        })]));
    }
    if text.starts_with(
        "You may have Sakashima the Impostor enter as a copy of any creature on the battlefield",
    ) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "sakashimaCopy",
                    "kind": "chooseBattlefieldCreature",
                    "optional": true,
                }],
                "replacement": [{
                    "kind": "copyEnteringCreatureAsSakashima",
                    "decisionId": "sakashimaCopy",
                }],
            }),
            &[
                "Choose any creature",
                "Copy it while preserving Sakashima's exceptions",
            ],
        ));
    }
    let optional_tapped_entry_copy_re = Regex::new(
        r"(?i)^You may have this (?:artifact|creature|enchantment|land|permanent) enter tapped as a copy of any (.+?) on the battlefield\.$",
    )
    .expect("optional tapped entry copy regex compiles");
    if let Some(captures) = optional_tapped_entry_copy_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "decisions": [{
                    "id": "entryCopy",
                    "kind": "chooseBattlefieldPermanent",
                    "where": parse_permanent_criteria(&captures[1], "")?,
                    "optional": true,
                }],
                "replacement": [{
                    "kind": "copyEnteringPermanent",
                    "decisionId": "entryCopy",
                    "tapped": true,
                }],
            }),
            &[
                "Choose an optional matching battlefield permanent",
                "Copy its copiable values before entry",
                "Apply the requested tapped entry state",
            ],
        ));
    }
    if text == "The Duke enters with a +1/+1 counter on him." {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &["Enter with one +1/+1 counter"],
        ));
    }
    if text.starts_with("{T}, Remove a counter from The Duke: Put a +1/+1 counter on another target creature you control")
    {
        let mut result = activated(
            vec![
                json!({ "kind": "tap", "object": self_ref() }),
                json!({
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": 1,
                }),
            ],
            "dukeMoveCounter",
            None,
        );
        result.rule["declaration"] = json!({
            "kind": "castingDeclaration",
            "decisions": [{
                "id": "targetCreature",
                "kind": "chooseTargets",
                "minimum": 1,
                "maximum": 1,
                "candidates": {
                    "kind": "permanents",
                    "controller": controller(),
                    "excludeSource": true,
                    "where": card_type("Creature"),
                },
            }],
        });
        return Some(result);
    }
    if text.starts_with("Waterbend {5}: Enchanted creature's owner shuffles it into their library.")
    {
        return Some(activated(
            vec![json!({
                "kind": "payWaterbend",
                "amount": integer(5),
            })],
            "wateryGraspShuffleCreature",
            None,
        ));
    }
    let controlled_permanents = |filter: Value| {
        json!({
            "kind": "permanents",
            "controller": controller(),
            "where": filter,
        })
    };
    let controller_turn = || {
        json!({
            "kind": "duringControllerTurn",
            "player": controller(),
        })
    };

    if text.contains("During your turn, Freya Crescent has flying") {
        return Some(static_rule(vec![json!({
            "kind": "grantKeyword",
            "objects": self_ref(),
            "keyword": "flying",
            "condition": controller_turn(),
        })]));
    }
    if text.contains("Whenever a land you control enters, put a +1/+1 counter on target creature") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "bristlyBillLandfall",
                }],
            }),
            &[
                "Resolve landfall for the source controller",
                "Choose a creature during resolution",
                "Put one +1/+1 counter on it",
            ],
        ));
    }
    if text == "(Place your experience counters here.)" {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "text": text,
            }),
            &["Recognize the experience counter game marker"],
        ));
    }
    if text
        == "Equipped creature gets +2/+2, has haste, can't attack you or planeswalkers you control, and can't be sacrificed."
    {
        return Some(static_rule(vec![
            json!({
                "kind": "modifyPowerToughness",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                "power": integer(2),
                "toughness": integer(2),
            }),
            json!({
                "kind": "grantKeyword",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                "keyword": "haste",
            }),
            json!({
                "kind": "cantAttackSourceController",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
            }),
            json!({
                "kind": "cantBeSacrificed",
                "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
            }),
        ]));
    }
    if text.contains("Creatures can't attack you unless their controller pays")
        && text.contains("number of basic land types among lands you control")
    {
        return Some(static_rule(vec![json!({
            "kind": "attackTax",
            "protectedPlayer": controller(),
            "amount": {
                "kind": "domainCount",
                "player": controller(),
            },
        })]));
    }
    if text.contains("Creatures can't attack you or planeswalkers you control unless")
        && text.contains("number of enchantments you control")
    {
        return Some(static_rule(vec![json!({
            "kind": "attackTax",
            "protectedPlayer": controller(),
            "amount": {
                "kind": "countPermanents",
                "player": controller(),
                "where": card_type("Enchantment"),
            },
        })]));
    }
    if text
        == "If a source you control would deal damage to an opponent or a permanent an opponent controls, it deals that much damage plus an amount of damage equal to the number of fire counters on this enchantment instead."
    {
        return Some(static_rule(vec![json!({
            "kind": "increaseDamageBySourceCounter",
            "player": controller(),
            "counterSource": self_ref(),
            "counter": "fire",
        })]));
    }
    if text == "This enchantment enters with X fire counters on it." {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "fire",
                    "count": { "kind": "decisionResult", "decisionId": "xValue" },
                }],
                "useCastXValue": true,
            }),
            &[
                "Read X from the permanent spell declaration",
                "Enter with X fire counters",
            ],
        ));
    }
    if text == "Skip your draw step." {
        return Some(static_rule(vec![json!({
            "kind": "skipDrawStep",
            "player": controller(),
        })]));
    }
    if text.starts_with("You have shroud.") {
        return Some(static_rule(vec![json!({
            "kind": "playerShroud",
            "player": controller(),
        })]));
    }
    if text == "Prevent all damage that would be dealt to you." {
        return Some(static_rule(vec![json!({
            "kind": "preventPlayerDamage",
            "player": controller(),
        })]));
    }
    match text {
        "This creature gets +2/+2 as long as there are three or more land cards in your graveyard." => {
            Some(static_rule(vec![json!({
                "kind": "modifyPowerToughness",
                "objects": self_ref(),
                "power": integer(2),
                "toughness": integer(2),
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": card_type("Land"),
                    }),
                    integer(3),
                ),
            })]))
        }
        "Your opponents can't cast spells from anywhere other than their hands." => {
            Some(static_rule(vec![json!({
                "kind": "prohibitCastFromNonHand",
                "players": {
                    "kind": "opponentsOf",
                    "player": controller(),
                },
            })]))
        }
        "Each nonland permanent you control is all colors." => Some(static_rule(vec![json!({
            "kind": "setAllColors",
            "objects": controlled_permanents(not(card_type("Land"))),
        })])),
        "Lands you control have trample and hexproof." => Some(static_rule(vec![
            json!({
                "kind": "grantKeyword",
                "objects": controlled_permanents(card_type("Land")),
                "keyword": "trample",
            }),
            json!({
                "kind": "grantKeyword",
                "objects": controlled_permanents(card_type("Land")),
                "keyword": "hexproof",
            }),
        ])),
        "During your turn, Allies you control have double strike and lifelink." => {
            Some(static_rule(vec![
                json!({
                    "kind": "grantKeyword",
                    "objects": controlled_permanents(subtype("Ally")),
                    "keyword": "doubleStrike",
                    "condition": controller_turn(),
                }),
                json!({
                    "kind": "grantKeyword",
                    "objects": controlled_permanents(subtype("Ally")),
                    "keyword": "lifelink",
                    "condition": controller_turn(),
                }),
            ]))
        }
        "Jump â€” During your turn, Freya Crescent has flying." => {
            Some(static_rule(vec![json!({
                "kind": "grantKeyword",
                "objects": self_ref(),
                "keyword": "flying",
                "condition": controller_turn(),
            })]))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_special_static_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let static_rule = |modifiers: Vec<Value>| {
        draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": modifiers,
            }),
            &["Apply battlefield static modifiers"],
        )
    };
    if text.starts_with("I ") && text.ends_with("Destroy all creatures.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(1)],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": card_type("Creature"),
                    },
                }],
            }),
            &[
                "Resolve Saga chapter I",
                "Destroy every creature simultaneously",
            ],
        ));
    }
    if text.starts_with("II ")
        && text
            .contains("Choose a card name. Search target opponent's graveyard, hand, and library")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(2)],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetOpponent",
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "chooseCardName",
                        "id": "chosenCardName",
                        "player": controller(),
                        "where": Value::Null,
                    },
                    {
                        "kind": "exileNamedCardsFromZones",
                        "player": chosen_target("targetOpponent"),
                        "chooser": controller(),
                        "decisionId": "chosenCardName",
                        "maximum": integer(4),
                        "zones": ["graveyard", "hand", "library"],
                        "shuffleLibrary": true,
                    },
                ],
            }),
            &[
                "Resolve Saga chapter II",
                "Choose a card name and target opponent",
                "Exile up to four matching cards",
                "Shuffle the searched library",
            ],
        ));
    }
    if text.starts_with("I ") && text.contains("Target creature you control gains indestructible") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(1)],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "installLinkedKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "indestructible",
                    "whileSourceControlled": true,
                }],
            }),
            &["Resolve chapter I and link indestructible to the Saga"],
        ));
    }
    if text.starts_with("II ")
        && text.contains("Return target creature card from your graveyard to the battlefield.")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(2)],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreatureCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "moveTargetCard",
                    "card": chosen_target("targetCreatureCard"),
                    "to": "battlefield",
                    "tapped": false,
                }],
            }),
            &["Resolve chapter II creature return"],
        ));
    }
    if text.starts_with("III ")
        && text.contains("Up to two target creatures you control each gain lifelink")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "sagaChapterReached",
                    "object": self_ref(),
                    "chapters": [integer(3)],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreatures",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        }),
                        0,
                        2,
                    )],
                },
                "effects": [{
                    "kind": "grantKeywordToTargets",
                    "objects": { "kind": "chosenTargets", "id": "targetCreatures" },
                    "keyword": "lifelink",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &["Resolve chapter III lifelink targets"],
        ));
    }
    if text
        == "(This land isn't a spell, it's affected by summoning sickness, and it has \"{T}: Add {G}.\")"
    {
        return Some(draft(
            json!({ "kind": "rulesMarker", "source": self_ref(), "text": text }),
            &["Recognize Dryad Arbor's intrinsic land-creature rules"],
        ));
    }
    let station_threshold_re =
        Regex::new(r"^(\d+)\+ \| (.+)$").expect("station threshold static regex compiles");
    if let Some(captures) = station_threshold_re.captures(text) {
        let station_condition = compare(
            ">=",
            json!({
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "charge",
            }),
            integer(captures[1].parse::<i64>().ok()?),
        );
        let normalized = captures[2].replace(", and ", ", ").replace(" and ", ", ");
        let keywords = normalized
            .split(", ")
            .filter_map(|keyword| match keyword.to_ascii_lowercase().as_str() {
                "flying" => Some("flying"),
                "haste" => Some("haste"),
                "trample" => Some("trample"),
                "vigilance" => Some("vigilance"),
                _ => None,
            })
            .collect::<Vec<_>>();
        if keywords.len() == normalized.split(", ").count() {
            return Some(draft(
                json!({
                    "kind": "staticAbility",
                    "source": self_ref(),
                    "activeWhile": active_while_battlefield(),
                    "condition": station_condition,
                    "modifiers": keywords.into_iter().map(|keyword| json!({
                        "kind": "grantKeyword",
                        "objects": self_ref(),
                        "keyword": keyword,
                    })).collect::<Vec<_>>(),
                }),
                &["Gate Spacecraft keywords by station charge counters"],
            ));
        }
        if let Some(mut parsed) = parse_common_static_ability(captures.get(2)?.as_str(), "") {
            if parsed.rule["kind"].as_str() != Some("staticAbility") {
                return None;
            }
            parsed.rule["condition"] = if let Some(existing) = parsed.rule.get("condition") {
                and(vec![station_condition, existing.clone()])
            } else {
                station_condition
            };
            parsed.operations.push(
                "Gate the reusable static leaf by the Spacecraft charge threshold".to_string(),
            );
            return Some(parsed);
        }
    }
    if text
        == "If one or more counters would be put on a creature, Spacecraft, or Planet you control, twice that many of each of those kinds of counters are put on it instead."
    {
        return Some(static_rule(vec![json!({
            "kind": "multiplyCounterPlacement",
            "player": controller(),
            "factor": 2,
        })]));
    }
    if text == "This token can block only creatures with flying." {
        return Some(static_rule(vec![json!({
            "kind": "blockRestriction",
            "attackers": {
                "kind": "permanents",
                "where": and(vec![
                    card_type("Creature"),
                    not(json!({ "kind": "hasKeyword", "value": "flying" })),
                ]),
            },
            "blockers": self_ref(),
        })]));
    }
    if text == "Spells you cast with mana value 6 or greater have cascade." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantCascadeToSpells",
                    "player": controller(),
                    "minimumManaValue": 6,
                }],
            }),
            &[
                "Inspect spells cast by the controller",
                "Grant cascade at mana value six or greater",
            ],
        ));
    }
    if text == "Play with the top card of your library revealed." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "lookAtTopLibrary",
                    "player": controller(),
                    "revealed": true,
                }],
            }),
            &["Expose the controller's top library card"],
        ));
    }
    let top_library_filter = match text {
        "You may play lands and cast spells from the top of your library." => Some(Value::Null),
        "You may cast creature spells from the top of your library." => Some(card_type("Creature")),
        _ => None,
    }
    .or_else(|| {
        Regex::new(r"(?i)^You may play lands and cast (.+?) spells from the top of your library\.$")
            .expect("land play and filtered top-library casting regex compiles")
            .captures(text)
            .and_then(|captures| {
                card_qualifier_filter(&captures[1], "")
                    .or_else(|| parse_permanent_criteria(&captures[1], ""))
                    .map(|spell_filter| or(vec![card_type("Land"), spell_filter]))
            })
    });
    if let Some(where_filter) = top_library_filter {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "playCardsFromTopLibrary",
                    "player": controller(),
                    "where": where_filter,
                }],
            }),
            &[
                "Inspect the controller's top library card",
                "Expose legal land plays and spell casts with normal timing and costs",
            ],
        ));
    }
    let controlled_enter_untapped_re = Regex::new(r"(?i)^(.+?) you control enter untapped\.$")
        .expect("controlled permanents enter untapped regex compiles");
    if let Some(captures) = controlled_enter_untapped_re.captures(text) {
        return Some(static_rule(vec![json!({
            "kind": "controlledPermanentsEnterUntapped",
            "player": controller(),
            "where": parse_permanent_criteria(&captures[1], "")?,
        })]));
    }
    if text == "You may cast spells from your hand without paying their mana costs." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "castHandSpellsWithoutPayingManaCost",
                    "player": controller(),
                }],
            }),
            &[
                "Resolve spells cast from the controller's hand",
                "Waive the base mana cost while retaining additional costs",
            ],
        ));
    }
    let grant_replicate_re = Regex::new(&format!(
        r"^Each spell you cast that's exactly ({}) colors has replicate ((?:\{{[^}}]+\}})+)\. \(When you cast it, copy it for each time you paid its replicate cost\. You may choose new targets for the copies\. A copy of a permanent spell becomes a token\.\)$",
        count_word_pattern(),
    ))
    .expect("granted replicate regex compiles");
    if let Some(captures) = grant_replicate_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantReplicate",
                    "spells": {
                        "kind": "spells",
                        "controller": controller(),
                        "where": compare(
                            "==",
                            json!({
                                "kind": "colorCountOf",
                                "object": { "kind": "candidate" },
                            }),
                            integer(parse_number_word(&captures[1])?),
                        ),
                    },
                    "cost": {
                        "kind": "payMana",
                        "manaCost": &captures[2],
                    },
                }],
            }),
            &[
                "Resolve controlled spells with the exact color count",
                "Grant replicate with its repeatable cost",
                "Register permanent spell copies as tokens",
            ],
        ));
    }
    if let Some(modifiers) = parse_player_static_modifiers(text) {
        return Some(static_rule(modifiers));
    }
    let enters_with_counters_re = Regex::new(
        r"^(?:This permanent|This creature|This artifact|This enchantment|This land|[A-Z][^.]+) enters with (\w+) ([^ ]+) counters? on (?:it|him|her|them)\.$",
    )
    .expect("enters-with-counters regex compiles");
    if let Some(captures) = enters_with_counters_re.captures(text) {
        let count = if captures[1].eq_ignore_ascii_case("X") {
            decision_result("xValue")
        } else {
            integer(parse_number_word(&captures[1])?)
        };
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": &captures[2],
                    "count": count,
                }],
            }),
            &[
                "Resolve entering counter type",
                "Apply counters before entry",
            ],
        ));
    }
    let kicked_enters_with_counters_re =
        Regex::new(r"^If .+ was kicked, it enters with (\w+) ([^ ]+) counters? on it\.$")
            .expect("kicked enters-with-counters regex compiles");
    if let Some(captures) = kicked_enters_with_counters_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": { "kind": "wasKicked", "spell": self_ref() },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": &captures[2],
                    "count": integer(parse_number_word(&captures[1])?),
                }],
            }),
            &[
                "Inspect whether the entering spell was kicked",
                "Apply its entering counters",
            ],
        ));
    }
    let revolt_enters_with_counters_re = Regex::new(
        r"^Revolt .+ This creature enters with (\w+) ([^ ]+) counters? on it if a permanent left the battlefield under your control this turn\.$",
    )
    .expect("revolt enters-with-counters regex compiles");
    if let Some(captures) = revolt_enters_with_counters_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "condition": compare(
                    ">",
                    json!({
                        "kind": "countEventsThisTurn",
                        "event": "permanentLeftBattlefield",
                        "player": controller(),
                    }),
                    integer(0),
                ),
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": &captures[2],
                    "count": integer(parse_number_word(&captures[1])?),
                }],
            }),
            &[
                "Inspect controlled permanents that left this turn",
                "Apply revolt counters",
            ],
        ));
    }
    let variable_entering_counters_re = Regex::new(
        r"(?i)^(?:This permanent|This creature|This artifact|This enchantment|This land|[A-Z][^.]+) enters with a number of ([^ ]+) counters? on (?:it|him|her|them) equal to (.+?)\. If (.+?), (?:it|he|she|they) enters tapped\.(?: .*)?$",
    )
    .expect("variable entering counters regex compiles");
    if let Some(captures) = variable_entering_counters_re.captures(text) {
        let counter_count = parse_numeric_expression_text(captures.get(2)?.as_str())?;
        let tapped_condition = parse_numeric_comparison_text(captures.get(3)?.as_str())?;
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": { "kind": "wouldEnterBattlefield", "object": self_ref() },
                "replacement": [
                    {
                        "kind": "putEnteringCounters",
                        "counter": captures.get(1)?.as_str(),
                        "count": counter_count,
                    },
                    {
                        "kind": "conditional",
                        "condition": tapped_condition,
                        "then": [{ "kind": "setEnteringState", "tapped": true }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Parse the entering counter expression",
                "Apply the computed counters",
                "Evaluate the tapped-entry comparison",
            ],
        ));
    }
    if text
        == "If you control five or more untapped lands, this creature enters with two +1/+1 counters and a lifelink counter on it."
    {
        let condition = compare(
            ">=",
            json!({
                "kind": "countPermanents",
                "player": controller(),
                "where": and(vec![card_type("Land"), not(json!({ "kind": "isTapped" }))]),
            }),
            integer(5),
        );
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "condition": condition,
                "replacement": [
                    {
                        "kind": "putEnteringCounters",
                        "counter": "+1/+1",
                        "count": integer(2),
                    },
                    {
                        "kind": "putEnteringCounters",
                        "counter": "lifelink",
                        "count": integer(1),
                    },
                ],
            }),
            &["Count untapped lands", "Apply both entering counter types"],
        ));
    }
    if text
        == "This creature enters with a +1/+1 counter on it plus an additional +1/+1 counter on it for each other creature you control."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "+1/+1",
                    "count": {
                        "kind": "add",
                        "left": integer(1),
                        "right": {
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                    },
                }],
            }),
            &[
                "Count other controlled creatures",
                "Apply entering +1/+1 counters",
            ],
        ));
    }
    if text
        == "This creature enters with three +1/+1 counters on it if you didn't cast it from your hand."
    {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "condition": not(json!({
                    "kind": "wasCastFromHand",
                    "object": self_ref(),
                })),
                "replacement": [{
                    "kind": "putEnteringCounters",
                    "counter": "+1/+1",
                    "count": integer(3),
                }],
            }),
            &[
                "Inspect the cast origin",
                "Apply three entering counters outside hand",
            ],
        ));
    }
    if text
        == "Alexios attacks each combat if able, can't be sacrificed, and can't attack its owner."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    { "kind": "attackEachCombatIfAble", "object": self_ref() },
                    { "kind": "cantBeSacrificed", "object": self_ref() },
                    { "kind": "cantAttackOwner", "object": self_ref() },
                ],
            }),
            &[
                "Require attacks when legal",
                "Prohibit sacrifice",
                "Exclude the owner as defender",
            ],
        ));
    }
    if text == "If you would lose unspent mana, that mana becomes black instead." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "convertUnspentMana",
                    "player": controller(),
                    "to": "B",
                }],
            }),
            &[
                "Resolve controller's unspent mana",
                "Replace mana loss with black mana",
            ],
        ));
    }
    if text.starts_with("Super Nova")
        && text
            .contains("Whenever a creature dies, target opponent loses 1 life and you gain 1 life.")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentTransformed",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "createDeathDrainEmblem",
                }],
            }),
            &[
                "Recognize the transform event",
                "Create the persistent emblem",
                "Install the emblem death trigger",
            ],
        ));
    }
    let equip_mana_re =
        Regex::new(r"^Equip ((?:\{[^}]+\})+)(?: \(.+\))?$").expect("equip mana regex compiles");
    if let Some(captures) = equip_mana_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "equip",
                    "costs": [{
                        "kind": "payMana",
                        "manaCost": &captures[1],
                    }],
                },
            }),
            &[
                "Recognize equip keyword",
                "Resolve activation cost",
                "Constrain attachment to controlled creature",
                "Apply sorcery timing",
            ],
        ));
    }
    let equip_cost_list_re =
        Regex::new(r"(?i)^Equip(?:â€”|—|-)\s*(.+?)\.?$").expect("equip cost-list regex compiles");
    if let Some(captures) = equip_cost_list_re.captures(text) {
        let (costs, decisions) = parse_activation_costs(captures.get(1)?.as_str())?;
        if !decisions.is_empty() {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": { "kind": "equip", "costs": costs },
            }),
            &[
                "Recognize equip keyword",
                "Parse its composite activation cost list",
                "Constrain attachment to a controlled creature at sorcery timing",
            ],
        ));
    }
    let restricted_equip_re = Regex::new(r"(?i)^Equip (.+?) ((?:\{[^}]+\})+)(?: \(.+\))?$")
        .expect("restricted equip mana regex compiles");
    if let Some(captures) = restricted_equip_re.captures(text) {
        let criteria = captures.get(1)?.as_str();
        let restriction = if criteria.to_ascii_lowercase().ends_with(" creature") {
            parse_permanent_criteria(criteria, "")?
        } else {
            and(vec![
                card_type("Creature"),
                parse_permanent_criteria(criteria, "")?,
            ])
        };
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "equip",
                    "where": restriction,
                    "costs": [{
                        "kind": "payMana",
                        "manaCost": &captures[2],
                    }],
                },
            }),
            &[
                "Recognize a characteristic-restricted equip keyword",
                "Reuse permanent criteria for its legal attachment targets",
                "Apply the ordinary equip cost and timing",
            ],
        ));
    }
    if text == "Enchant creature" {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "enchant",
                    "where": card_type("Creature"),
                },
            }),
            &[
                "Recognize enchant keyword",
                "Constrain Aura target to creature",
            ],
        ));
    }
    if text == "Enchant creature with another Aura attached to it" {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "enchant",
                    "where": and(vec![
                        card_type("Creature"),
                        json!({ "kind": "hasAttachedAura" }),
                    ]),
                },
            }),
            &["Recognize enchant", "Require another attached Aura"],
        ));
    }
    if matches!(
        text,
        "(Gain the next level as a sorcery to add its ability.)"
            | "(This token can be used to represent a token that's a copy of a permanent.)"
            | "Choose a Background (You can have a Background as a second commander.)"
            | "(You may cast either half. That door unlocks on the battlefield. As a sorcery, you may pay the mana cost of a locked door to unlock it.)"
    ) {
        return Some(draft(
            json!({
                "kind": "rulesMarker",
                "source": self_ref(),
                "text": text,
            }),
            &["Recognize rules marker", "Preserve rules-system semantics"],
        ));
    }
    let static_haste_scope = match text {
        "All creatures have haste." => Some(Value::Null),
        "Creatures you control have haste. (They can attack and {T} as soon as they come under your control.)" => {
            Some(controller())
        }
        _ => None,
    };
    if let Some(player) = static_haste_scope {
        let mut objects = json!({
            "kind": "permanents",
            "where": card_type("Creature"),
        });
        if !player.is_null() {
            objects["controller"] = player;
        }
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": objects,
                    "keyword": "haste",
                }],
            }),
            &["Resolve creature scope", "Grant haste continuously"],
        ));
    }
    if text == "Creature tokens you control get +2/+2." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            json!({ "kind": "isToken" }),
                        ]),
                    },
                    "power": integer(2),
                    "toughness": integer(2),
                }],
            }),
            &[
                "Resolve controlled creature tokens",
                "Apply continuous +2/+2",
            ],
        ));
    }
    let equipped_fixed_re = Regex::new(r"^Equipped creature gets \+(\d+)/([+-]\d+)\.?$")
        .expect("equipped modifier regex compiles");
    if let Some(captures) = equipped_fixed_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": {
                        "kind": "attachedPermanent",
                        "attachment": self_ref(),
                    },
                    "power": integer(captures[1].parse::<i64>().ok()?),
                    "toughness": integer(captures[2].parse::<i64>().ok()?),
                }],
            }),
            &[
                "Resolve equipped creature",
                "Apply continuous power/toughness modifier",
            ],
        ));
    }
    let equipped_keyword = |phrase: &str| match phrase.trim() {
        "can't be blocked" => Some("cantBeBlocked"),
        "deathtouch" => Some("deathtouch"),
        "double strike" => Some("doubleStrike"),
        "first strike" => Some("firstStrike"),
        "flying" => Some("flying"),
        "haste" => Some("haste"),
        "indestructible" => Some("indestructible"),
        "lifelink" => Some("lifelink"),
        "menace" => Some("menace"),
        "prowess" => Some("prowess"),
        "reach" => Some("reach"),
        "shroud" => Some("shroud"),
        "trample" => Some("trample"),
        "vigilance" => Some("vigilance"),
        _ => None,
    };
    let equipped_keyword_re = Regex::new(
        r"^Equipped creature (?:has )?([a-z' ]+?)(?:(?: and has | and )([a-z' ]+?))?\.(?: \(.+\))?$",
    )
    .expect("equipped keyword regex compiles");
    if let Some(captures) = equipped_keyword_re.captures(text) {
        let keywords = [captures.get(1), captures.get(2)]
            .into_iter()
            .flatten()
            .filter_map(|capture| equipped_keyword(capture.as_str()))
            .collect::<Vec<_>>();
        if !keywords.is_empty() {
            let objects = json!({
                "kind": "attachedPermanent",
                "attachment": self_ref(),
            });
            return Some(draft(
                json!({
                    "kind": "staticAbility",
                    "source": self_ref(),
                    "activeWhile": active_while_battlefield(),
                    "modifiers": keywords.into_iter().map(|keyword| json!({
                        "kind": "grantKeyword",
                        "objects": objects.clone(),
                        "keyword": keyword,
                    })).collect::<Vec<_>>(),
                }),
                &[
                    "Resolve equipped creature",
                    "Grant equipment keywords continuously",
                ],
            ));
        }
    }
    let equipped_bonus_keyword_re =
        Regex::new(r"(?i)^Equipped creature gets \+(\d+)/([+-]\d+) and has (.+?)\.(?: \(.+\))?$")
            .expect("equipped bonus keyword regex compiles");
    if let Some(captures) = equipped_bonus_keyword_re.captures(text) {
        let objects = json!({
            "kind": "attachedPermanent",
            "attachment": self_ref(),
        });
        let granted_modifier = captures
            .get(3)
            .and_then(|capture| equipped_keyword(capture.as_str()))
            .map(|keyword| {
                json!({
                    "kind": "grantKeyword",
                    "objects": objects.clone(),
                    "keyword": keyword,
                })
            })
            .or_else(|| {
                parse_keyword_cost(captures.get(3)?.as_str(), "Ward").map(|cost| {
                    json!({
                        "kind": "grantWard",
                        "objects": objects.clone(),
                        "cost": cost,
                    })
                })
            });
        if let Some(granted_modifier) = granted_modifier {
            return Some(draft(
                json!({
                    "kind": "staticAbility",
                    "source": self_ref(),
                    "activeWhile": active_while_battlefield(),
                    "modifiers": [
                        {
                            "kind": "modifyPowerToughness",
                            "objects": objects.clone(),
                            "power": integer(captures[1].parse::<i64>().ok()?),
                            "toughness": integer(captures[2].parse::<i64>().ok()?),
                        },
                        granted_modifier,
                    ],
                }),
                &[
                    "Resolve equipped creature",
                    "Apply continuous power/toughness modifier",
                    "Grant equipment keyword continuously",
                ],
            ));
        }
    }
    if text == "Equipped creature gets +2/+2 and has protection from green and from white." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": {
                            "kind": "attachedPermanent",
                            "attachment": self_ref(),
                        },
                        "power": integer(2),
                        "toughness": integer(2),
                    },
                    {
                        "kind": "grantProtection",
                        "objects": {
                            "kind": "attachedPermanent",
                            "attachment": self_ref(),
                        },
                        "from": ["green", "white"],
                    },
                ],
            }),
            &[
                "Resolve equipped creature",
                "Apply continuous +2/+2",
                "Grant protection from green and white",
            ],
        ));
    }
    if text.starts_with("Equipped creature has myriad.") {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": {
                        "kind": "attachedPermanent",
                        "attachment": self_ref(),
                    },
                    "keyword": "myriad",
                }],
            }),
            &["Resolve equipped creature", "Grant myriad continuously"],
        ));
    }
    if text == "This land can't be blocked." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "cantBeBlocked",
                    "object": self_ref(),
                }],
            }),
            &["Resolve source permanent", "Prohibit blockers"],
        ));
    }
    if text == "This creature can't be blocked." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "cantBeBlocked",
                    "object": self_ref(),
                }],
            }),
            &["Resolve source creature", "Prohibit blockers"],
        ));
    }
    if text == "Birds you control get +1/+1 and have vigilance." {
        let birds = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": subtype("Bird"),
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "modifyPowerToughness",
                        "objects": birds.clone(),
                        "power": integer(1),
                        "toughness": integer(1),
                    },
                    {
                        "kind": "grantKeyword",
                        "objects": birds,
                        "keyword": "vigilance",
                    },
                ],
            }),
            &["Resolve controlled Birds", "Apply +1/+1 and vigilance"],
        ));
    }
    if text == "Enchanted creature gets +1/+1 for each enchantment you control." {
        let amount = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": card_type("Enchantment"),
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": {
                        "kind": "attachedPermanent",
                        "attachment": self_ref(),
                    },
                    "power": amount.clone(),
                    "toughness": amount,
                }],
            }),
            &[
                "Resolve enchanted creature",
                "Count controlled enchantments",
            ],
        ));
    }
    if text == "Enchanted creature gets +2/+2 for each other enchantment on the battlefield." {
        let amount = json!({
            "kind": "multiply",
            "value": {
                "kind": "subtract",
                "left": {
                    "kind": "countPermanents",
                    "allPlayers": true,
                    "where": card_type("Enchantment"),
                },
                "right": integer(1),
            },
            "factor": 2,
        });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "modifyPowerToughness",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "power": amount.clone(),
                    "toughness": amount,
                }],
            }),
            &["Count all other enchantments", "Apply twice that bonus"],
        ));
    }
    if text
        == "Enchanted creature gets +4/+4, has flying and first strike, and is an Angel in addition to its other types."
    {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    { "kind": "modifyPowerToughness", "objects": attached.clone(), "power": integer(4), "toughness": integer(4) },
                    { "kind": "grantKeyword", "objects": attached.clone(), "keyword": "flying" },
                    { "kind": "grantKeyword", "objects": attached.clone(), "keyword": "firstStrike" },
                    { "kind": "addSubtype", "objects": attached, "subtype": "Angel" },
                ],
            }),
            &["Resolve enchanted creature", "Apply Angel characteristics"],
        ));
    }
    if text == "Enchanted creature has protection from creatures." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantProtection",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "from": ["creatures"],
                }],
            }),
            &[
                "Resolve enchanted creature",
                "Grant protection from creatures",
            ],
        ));
    }
    if text
        == "Enchanted creature has ward {2} and \"{T}: Add five mana in any combination of colors. Spend this mana only to cast spells.\""
    {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "grantWard",
                        "objects": attached.clone(),
                        "cost": { "kind": "payMana", "manaCost": "{2}" },
                    },
                    {
                        "kind": "grantManaAbility",
                        "objects": attached,
                        "mana": { "kind": "chooseColor", "amount": integer(5) },
                    },
                ],
            }),
            &[
                "Resolve enchanted creature",
                "Grant ward and five-mana ability",
            ],
        ));
    }
    if text == "Enchanted creature can't attack or block." {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    { "kind": "grantKeyword", "objects": attached.clone(), "keyword": "cantAttack" },
                    { "kind": "grantKeyword", "objects": attached, "keyword": "cantBlock" },
                ],
            }),
            &[
                "Resolve enchanted creature",
                "Prohibit attacking and blocking",
            ],
        ));
    }
    if text == "All creatures able to block enchanted creature do so." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "keyword": "allMustBlockIfAble",
                }],
            }),
            &["Resolve enchanted creature", "Require all legal blockers"],
        ));
    }
    if text == "This creature must be blocked if able." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": "mustBeBlockedIfAble",
                }],
            }),
            &["Resolve source creature", "Require a legal blocker"],
        ));
    }
    if text == "This creature can't be blocked by more than one creature." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantKeyword",
                    "objects": self_ref(),
                    "keyword": "maxOneBlocker",
                }],
            }),
            &["Resolve source creature", "Limit it to one blocker"],
        ));
    }
    let filtered_blocker_re = Regex::new(r"(?i)^(.+?) can't be blocked by (.+?)\.$")
        .expect("filtered blocker restriction regex compiles");
    if let Some(captures) = filtered_blocker_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), "")
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockRestriction",
                    "attackers": self_ref(),
                    "blockers": {
                        "kind": "permanents",
                        "where": parse_permanent_criteria(captures.get(2)?.as_str(), "")?,
                    },
                }],
            }),
            &[
                "Resolve source attacker",
                "Exclude blockers matching reusable criteria",
            ],
        ));
    }
    let low_power_blocker_re =
        Regex::new(r"^(.+?) can't be blocked by creatures with power (\d+) or less\.$")
            .expect("low-power blocker regex compiles");
    if let Some(captures) = low_power_blocker_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockRestriction",
                    "attackers": self_ref(),
                    "blockers": {
                        "kind": "permanents",
                        "where": compare(
                            "<=",
                            json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                            integer(captures[2].parse::<i64>().ok()?),
                        ),
                    },
                }],
            }),
            &["Resolve source attacker", "Exclude low-power blockers"],
        ));
    }
    if text == "Creatures with power less than this creature's power can't block it." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockByLowerPowerRestriction",
                    "object": self_ref(),
                }],
            }),
            &[
                "Compare blocker and attacker power",
                "Prohibit lower-power blockers",
            ],
        ));
    }
    if text == "Creatures with flying can't block creatures you control." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockRestriction",
                    "attackers": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    },
                    "blockers": {
                        "kind": "permanents",
                        "where": json!({ "kind": "hasKeyword", "value": "flying" }),
                    },
                }],
            }),
            &["Resolve controlled attackers", "Prohibit flying blockers"],
        ));
    }
    if text == "This creature can't be blocked except by creatures with flying." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockRestriction",
                    "attackers": self_ref(),
                    "blockers": {
                        "kind": "permanents",
                        "where": not(json!({ "kind": "hasKeyword", "value": "flying" })),
                    },
                }],
            }),
            &["Resolve source attacker", "Allow only flying blockers"],
        ));
    }
    if text == "Spells you control can't be countered." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "cantBeCountered",
                    "objects": {
                        "kind": "spells",
                        "controller": controller(),
                    },
                }],
            }),
            &["Resolve controlled spells", "Prohibit countering"],
        ));
    }
    if text == "Your opponents can't cast spells during your turn." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "prohibitCast",
                    "players": {
                        "kind": "opponentsOf",
                        "player": controller(),
                    },
                    "duringTurnOf": controller(),
                }],
            }),
            &[
                "Resolve opponents",
                "Restrict spell casting during controller turn",
            ],
        ));
    }
    if text
        == "Creatures your opponents control can be the targets of spells and abilities as though they didn't have hexproof. Ward abilities of those creatures don't trigger."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [
                    {
                        "kind": "ignoreHexproof",
                        "objects": {
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        },
                    },
                    {
                        "kind": "suppressWard",
                        "objects": {
                            "kind": "permanents",
                            "controller": {
                                "kind": "opponentsOf",
                                "player": controller(),
                            },
                            "where": card_type("Creature"),
                        },
                    },
                ],
            }),
            &[
                "Resolve opposing creatures",
                "Ignore hexproof for targeting",
                "Suppress ward triggers",
            ],
        ));
    }
    let controlled_token_multiplier_re = Regex::new(
        r"(?i)^If (?:an effect would create one or more tokens|one or more (?:creature )?tokens would be created) under your control, (?:it creates )?(twice|three times) that many of those tokens (?:are created )?instead\.$",
    )
    .expect("controlled token multiplier regex compiles");
    if let Some(captures) = controlled_token_multiplier_re.captures(text) {
        let factor = if captures[1].eq_ignore_ascii_case("twice") {
            2
        } else {
            3
        };
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "multiplyTokenCreation",
                    "player": controller(),
                    "factor": integer(factor),
                }],
            }),
            &[
                "Resolve token-creation replacement",
                "Double created quantity",
            ],
        ));
    }
    if text == "Prevent all damage that would be dealt to creatures you control." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "preventDamage",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                    },
                }],
            }),
            &[
                "Resolve controlled creatures",
                "Prevent all incoming damage",
            ],
        ));
    }
    let granted_ward_re = Regex::new(r#"^Other creatures you control have "(.+)"$"#)
        .expect("granted ward regex compiles");
    if let Some(cost) = granted_ward_re
        .captures(text)
        .and_then(|captures| parse_keyword_cost(&captures[1], "Ward"))
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantWard",
                    "objects": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    },
                    "cost": cost,
                }],
            }),
            &[
                "Resolve other controlled creatures",
                "Grant ward life payment",
            ],
        ));
    }
    if text
        == "Creatures you control with power 2 or less can't be blocked by creatures with power 3 or greater."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "blockRestriction",
                    "attackers": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                "<=",
                                json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                                integer(2),
                            ),
                        ]),
                    },
                    "blockers": {
                        "kind": "permanents",
                        "where": compare(
                            ">=",
                            json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                            integer(3),
                        ),
                    },
                }],
            }),
            &[
                "Resolve controlled low-power attackers",
                "Resolve high-power blockers",
                "Prohibit matching blocks",
            ],
        ));
    }
    if text
        == "If a triggered ability of a creature you control with power 2 or less triggers, that ability triggers an additional time."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "multiplyTriggeredAbility",
                    "sources": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                "<=",
                                json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                                integer(2),
                            ),
                        ]),
                    },
                    "additionalTriggers": integer(1),
                }],
            }),
            &[
                "Resolve low-power creature sources",
                "Add one trigger instance",
            ],
        ));
    }
    if text
        == "If a creature attacking causes a triggered ability of a permanent you control to trigger, that ability triggers an additional time."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "multiplyTriggeredAbility",
                    "sources": {
                        "kind": "permanents",
                        "controller": controller(),
                        "where": { "kind": "isAttacking" },
                    },
                    "additionalTriggers": integer(1),
                }],
            }),
            &[
                "Resolve attacking permanent sources",
                "Add one trigger instance",
            ],
        ));
    }
    if text == "As long as your devotion to black is less than five, Erebos isn't a creature." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "removeCardTypeWhile",
                    "object": self_ref(),
                    "cardType": "Creature",
                    "condition": compare(
                        "<",
                        json!({
                            "kind": "devotion",
                            "player": controller(),
                            "color": "black",
                        }),
                        integer(5),
                    ),
                }],
            }),
            &[
                "Count black devotion",
                "Remove creature type below five devotion",
            ],
        ));
    }
    if text
        == "This spell costs {1} less to cast for each instant and sorcery card in your graveyard."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": {
                    "kind": "inZone",
                    "object": self_ref(),
                    "zone": { "kind": "stackOrCast" },
                },
                "modifiers": [{
                    "kind": "reduceOwnGenericCastingCost",
                    "amount": {
                        "kind": "countCards",
                        "zone": graveyard(controller()),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    },
                }],
            }),
            &[
                "Count instant and sorcery cards in controller graveyard",
                "Reduce own generic casting cost",
            ],
        ));
    }
    if text.starts_with("Each red or green instant or sorcery spell you cast has conspire.") {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantConspire",
                    "spells": {
                        "kind": "spells",
                        "controller": controller(),
                        "where": and(vec![
                            or(vec![card_type("Instant"), card_type("Sorcery")]),
                            or(vec![
                                json!({ "kind": "colorContains", "value": "Red" }),
                                json!({ "kind": "colorContains", "value": "Green" }),
                            ]),
                        ]),
                    },
                }],
            }),
            &[
                "Resolve controlled red or green instant/sorcery spells",
                "Grant conspire",
            ],
        ));
    }
    if text.starts_with("Survival â€” At the beginning of your second main phase")
        || text.starts_with("Survival — At the beginning of your second main phase")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "player": controller(),
                    "step": "postcombatMain",
                },
                "condition": {
                    "kind": "isTapped",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "manifestDread",
                    "player": controller(),
                }],
            }),
            &[
                "Resolve controller second main phase",
                "Require tapped source",
                "Manifest dread",
            ],
        ));
    }
    let void_effects = match text {
        value
            if value.starts_with("Void â€” At the beginning of your end step")
                || value.starts_with("Void — At the beginning of your end step") =>
        {
            if value.ends_with("create a 2/2 colorless Robot artifact creature token.") {
                Some(vec![json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "types": ["Artifact", "Creature"],
                        "subtypes": ["Robot"],
                        "power": 2,
                        "toughness": 2,
                    },
                })])
            } else if value.ends_with("you draw a card and lose 1 life.") {
                Some(vec![
                    json!({
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    }),
                    json!({
                        "kind": "loseLife",
                        "player": controller(),
                        "amount": integer(1),
                    }),
                ])
            } else if value.ends_with("attach this Equipment to target creature you control.") {
                Some(vec![
                    json!({
                        "kind": "choosePermanents",
                        "id": "voidAttachTarget",
                        "player": controller(),
                        "minimum": integer(1),
                        "maximum": integer(1),
                        "candidates": {
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                        },
                    }),
                    json!({
                        "kind": "attachPermanent",
                        "attachment": self_ref(),
                        "to": {
                            "kind": "decisionResult",
                            "decisionId": "voidAttachTarget",
                        },
                    }),
                ])
            } else {
                None
            }
        }
        _ => None,
    };
    if let Some(effects) = void_effects {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "player": controller(),
                    "step": "endStep",
                },
                "condition": {
                    "kind": "voidOccurredThisTurn",
                    "player": controller(),
                },
                "effects": effects,
            }),
            &[
                "Resolve controller end step",
                "Check nonland departure or warped spell",
                "Resolve Void effect",
            ],
        ));
    }
    if text.starts_with("Infusion â€” At the beginning of your end step")
        || text.starts_with("Infusion — At the beginning of your end step")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "player": controller(),
                    "step": "endStep",
                },
                "condition": compare(
                    ">",
                    json!({
                        "kind": "lifeGainedThisTurn",
                        "player": controller(),
                    }),
                    integer(0),
                ),
                "effects": [{
                    "kind": "returnCreatureByLifeGained",
                    "player": controller(),
                    "maximum": integer(1),
                    "tapped": false,
                }],
            }),
            &[
                "Resolve controller end step",
                "Require life gained this turn",
                "Choose creature card within gained-life mana value",
                "Return it to battlefield",
            ],
        ));
    }
    if text
        == "Enchanted creature has \"{T}: Create a token that's a copy of this creature, except it has haste. Exile that token at the beginning of the next end step.\""
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantSplinterTwinAbility",
                    "object": {
                        "kind": "attachedPermanent",
                        "attachment": self_ref(),
                    },
                }],
            }),
            &[
                "Resolve enchanted creature",
                "Grant tap activation",
                "Create hasty token copy",
                "Register next-end-step exile",
            ],
        ));
    }
    if text == "âˆ’3: Destroy target nonland permanent."
        || text == "−3: Destroy target nonland permanent."
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "startingLoyalty": integer(5),
                "costs": [{
                    "kind": "payLoyalty",
                    "object": self_ref(),
                    "amount": integer(-3),
                }],
                "activationLimit": {
                    "kind": "oncePerTurn",
                    "id": "loyaltyAbility",
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "targetPermanent",
                            json!({
                                "kind": "permanents",
                                "where": not(card_type("Land")),
                            }),
                            1,
                            1,
                        ),
                    ],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("targetPermanent"),
                }],
            }),
            &[
                "Recognize loyalty ability",
                "Apply minus-three loyalty cost",
                "Declare nonland permanent target",
                "Destroy target",
            ],
        ));
    }
    if text.starts_with("âˆ’7: Create three 1/1 black Assassin creature tokens")
        || text.starts_with("−7: Create three 1/1 black Assassin creature tokens")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "startingLoyalty": integer(5),
                "costs": [{
                    "kind": "payLoyalty",
                    "object": self_ref(),
                    "amount": integer(-7),
                }],
                "activationLimit": {
                    "kind": "oncePerTurn",
                    "id": "loyaltyAbility",
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(3),
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Assassin"],
                        "power": 1,
                        "toughness": 1,
                        "abilities": [{
                            "kind": "triggeredAbility",
                            "source": self_ref(),
                            "event": {
                                "kind": "combatDamageToPlayer",
                                "source": self_ref(),
                            },
                            "effects": [{
                                "kind": "loseGame",
                                "player": {
                                    "kind": "triggeringPlayer",
                                },
                            }],
                        }],
                    },
                }],
            }),
            &[
                "Recognize loyalty ability",
                "Apply minus-seven loyalty cost",
                "Create three Assassin tokens",
                "Attach combat-damage loss trigger",
            ],
        ));
    }
    if text
        .starts_with("+1: Until your next turn, whenever a creature deals combat damage to Vraska")
    {
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "startingLoyalty": integer(5),
                "costs": [{
                    "kind": "payLoyalty",
                    "object": self_ref(),
                    "amount": integer(1),
                }],
                "activationLimit": {
                    "kind": "oncePerTurn",
                    "id": "loyaltyAbility",
                },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [{
                    "kind": "installRetaliation",
                    "object": self_ref(),
                    "duration": {
                        "kind": "untilNextTurn",
                        "player": controller(),
                    },
                }],
            }),
            &[
                "Recognize loyalty ability",
                "Apply plus-one loyalty cost",
                "Install combat-damage retaliation until next turn",
            ],
        ));
    }
    if text == "You may choose not to untap this artifact during your untap step." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "optionalUntap",
                    "object": self_ref(),
                }],
            }),
            &["Resolve untap-step choice", "Allow source to remain tapped"],
        ));
    }
    if text == "This land is the chosen type." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "retainChosenBasicLandType",
                    "decisionId": "basicLandType",
                }],
            }),
            &[
                "Resolve stored basic-land-type choice",
                "Retain chosen land characteristics",
            ],
        ));
    }
    if text == "This land enters tapped." {
        return Some(draft(
            json!({
                "kind": "replacementEffect",
                "source": self_ref(),
                "event": {
                    "kind": "wouldEnterBattlefield",
                    "object": self_ref(),
                },
                "replacement": [{
                    "kind": "setEnteringState",
                    "object": self_ref(),
                    "tapped": true,
                }],
            }),
            &["Resolve entering land", "Apply tapped entering state"],
        ));
    }

    if text == "Creatures entering don't cause abilities to trigger." {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "suppressTriggerCreation",
                    "causedBy": {
                        "kind": "enterBattlefield",
                        "object": {
                            "kind": "eventObject",
                            "where": card_type("Creature"),
                        },
                    },
                }],
            }),
            &[
                "Constrain rule to battlefield",
                "Resolve creature-entering event",
                "Install trigger-creation suppression",
            ],
        ));
    }

    if text
        == "Activated abilities of sources with the chosen name can't be activated unless they're mana abilities."
    {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "prohibitActivation",
                    "abilities": {
                        "kind": "activatedAbilities",
                        "sourceWhere": {
                            "kind": "nameEquals",
                            "value": stored_card_name("chosenCardName"),
                        },
                        "where": not(json!({ "kind": "isManaAbility" })),
                    },
                }],
            }),
            &[
                "Resolve stored chosen-name reference",
                "Select matching activated abilities",
                "Exclude mana abilities",
                "Install activation prohibition",
            ],
        ));
    }

    if text == "Lands with the chosen name have \"{T}: Add {C}.\"" {
        return Some(draft(
            json!({
                "kind": "staticAbility",
                "source": self_ref(),
                "activeWhile": active_while_battlefield(),
                "modifiers": [{
                    "kind": "grantAbility",
                    "objects": {
                        "kind": "permanents",
                        "where": and(vec![
                            card_type("Land"),
                            json!({
                                "kind": "nameEquals",
                                "value": stored_land_name(),
                            }),
                        ]),
                    },
                    "ability": {
                        "kind": "manaAbility",
                        "costs": [{
                            "kind": "tap",
                            "object": { "kind": "abilitySource" },
                        }],
                        "effects": [{
                            "kind": "addMana",
                            "player": {
                                "kind": "controllerOf",
                                "object": { "kind": "abilitySource" },
                            },
                            "mana": "{C}",
                        }],
                    },
                }],
            }),
            &[
                "Resolve matching land permanents",
                "Resolve stored chosen-name reference",
                "Parse quoted mana ability",
                "Install granted ability",
            ],
        ));
    }

    None
}
