use super::super::*;

pub(in crate::oracle::canonical) fn parse_prepare_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let instruction = text
        .split_once(" (While it's prepared,")
        .map(|(instruction, _)| instruction)
        .unwrap_or(text)
        .trim();
    let entry_player_token_prepare_re = Regex::new(
        r"(?i)^When this creature enters, target player (creates? .+?)\. Then if (.+), this creature becomes prepared\.$",
    )
    .expect("entry target-player token prepare regex compiles");
    if let Some(captures) = entry_player_token_prepare_re.captures(instruction) {
        let token_instruction = captures[1]
            .replacen("creates", "Create", 1)
            .replacen("create", "Create", 1);
        let mut token_effect = create_token_effect(&token_instruction)?;
        token_effect["controller"] = chosen_target("targetPlayer");
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision("targetPlayer", json!({ "kind": "players" }), 1, 1)],
                },
                "effects": [
                    token_effect,
                    { "kind": "conditionalEffect", "condition": parse_condition_text(&captures[2])?, "then": [prepared_effect()], "else": [] },
                ],
            }),
            &[
                "Declare the target player",
                "Create the described token",
                "Evaluate the preparation condition",
            ],
        ));
    }

    let next_spell_copy_re = Regex::new(
        r"(?i)^When you next cast an instant or sorcery spell this turn, copy that spell\. You may choose new targets for the copy\.$",
    )
    .expect("next instant-or-sorcery copy regex compiles");
    if next_spell_copy_re.is_match(instruction) {
        return Some(draft(
            json!({
                "kind": "spellAbility", "source": self_ref(),
                "effects": [{
                    "kind": "installDelayedSpellCastTrigger",
                    "player": controller(),
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                    "effects": [{
                        "kind": "copyStackItem", "object": { "kind": "triggeringStackObject" },
                        "controller": controller(), "mayChooseNewTargets": true,
                    }],
                }],
            }),
            &[
                "Watch the next instant or sorcery cast",
                "Copy its stack object",
                "Allow new targets",
            ],
        ));
    }

    let conjure_prepare_re = Regex::new(
        r"(?i)^Whenever you cast a spell that isn't from your starting deck, conjure a card named ([A-Za-z0-9 ',.-]+) onto the battlefield\. This creature becomes prepared\.$",
    )
    .expect("non-starting-deck conjure prepare regex compiles");
    if let Some(captures) = conjure_prepare_re.captures(instruction) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "spellCast", "player": controller(), "where": Value::Null, "fromStartingDeck": false },
                "effects": [
                    { "kind": "conjureNamedCard", "player": controller(), "name": captures[1].trim(), "destination": "battlefield" },
                    prepared_effect(),
                ],
            }),
            &[
                "Detect a spell outside the starting deck",
                "Conjure the named card",
                "Set prepared designation",
            ],
        ));
    }
    if instruction.eq_ignore_ascii_case(
        "Whenever you conjure one or more cards, this creature becomes prepared.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "cardConjured", "player": controller() },
                "effects": [prepared_effect()],
            }),
            &["Detect a conjure event", "Set prepared designation"],
        ));
    }

    let scry_surveil_bonus_re = Regex::new(
        r"(?i)^Whenever you scry or surveil, creatures you control get ([+-]\d+)/([+-]\d+) until end of turn\. This ability triggers only once each turn\.$",
    )
    .expect("scry-or-surveil bonus regex compiles");
    if let Some(captures) = scry_surveil_bonus_re.captures(instruction) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "oneOf", "events": [
                    { "kind": "scried", "player": controller() },
                    { "kind": "surveilled", "player": controller() },
                ]},
                "triggerLimit": { "kind": "onceEachTurn", "id": "scryOrSurveilBonus" },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": { "kind": "eachPermanent", "player": controller(), "where": card_type("Creature") },
                    "power": integer(captures[1].parse::<i64>().ok()?),
                    "toughness": integer(captures[2].parse::<i64>().ok()?),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Observe scry and surveil events",
                "Limit the trigger to once each turn",
                "Apply the team bonus",
            ],
        ));
    }

    let attack_target_nested_prepare_re = Regex::new(
        r"(?i)^Whenever you attack, target (.+?) gains (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\. Whenever that creature deals combat damage to a player this combat, this creature becomes prepared\.$",
    )
    .expect("attack target nested prepare regex compiles");
    if let Some(captures) = attack_target_nested_prepare_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        let keyword = oracle_keyword_list(&captures[2])?.into_iter().next()?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility", "source": self_ref(),
                "event": { "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": integer(1) },
                "declaration": { "kind": "castingDeclaration", "decisions": [target_decision(
                    "targetPermanent", permanent_target_candidates(&captures[1], "")?, 1, 1,
                )]},
                "effects": [
                    { "kind": "grantKeyword", "object": target.clone(), "keyword": keyword, "duration": { "kind": "untilEndOfCurrentTurn" } },
                    { "kind": "installCombatDamageTrigger", "object": target, "duration": { "kind": "currentCombat" }, "effects": [prepared_effect()] },
                ],
            }),
            &[
                "Declare the attack trigger target",
                "Grant the temporary keyword",
                "Install the linked combat-damage trigger",
            ],
        ));
    }

    if !instruction
        .to_ascii_lowercase()
        .contains("becomes prepared")
    {
        return None;
    }

    let end_step_target_counter_re = Regex::new(
        r"(?i)^At the beginning of your end step, put (?:a|an) ([^ ]+) counter on target (.+?)\. Then if that creature has power (\d+) or greater, this creature becomes prepared\.$",
    )
    .expect("prepare end-step target-counter regex compiles");
    if let Some(captures) = end_step_target_counter_re.captures(instruction) {
        let target = chosen_target("targetPermanent");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        permanent_target_candidates(&captures[2], "")?,
                        1,
                        1,
                    )],
                },
                "effects": [
                    { "kind": "putCounters", "permanent": target.clone(), "counter": captures[1].to_string(), "count": integer(1) },
                    {
                        "kind": "conditionalEffect",
                        "condition": compare(
                            ">=",
                            json!({ "kind": "powerOf", "object": target }),
                            integer(captures[3].parse::<i64>().ok()?),
                        ),
                        "then": [prepared_effect()],
                        "else": [],
                    },
                ],
            }),
            &[
                "Parse end-step target and counter",
                "Re-evaluate the target's current power",
                "Set prepared designation",
            ],
        ));
    }

    let step_re = Regex::new(
        r"(?i)^At the beginning of (your|each) (upkeep|end step|first main phase|second main phase), (?:if (.+), )?(?:this creature|it|[A-Z][A-Za-z0-9 ',.-]+) becomes prepared\.$",
    )
    .expect("prepare step trigger regex compiles");
    if let Some(captures) = step_re.captures(instruction) {
        let step = match captures[2].to_ascii_lowercase().as_str() {
            "upkeep" => "upkeep",
            "end step" => "endStep",
            "first main phase" => "precombatMain",
            "second main phase" => "postcombatMain",
            _ => return None,
        };
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "stepBegan",
                "step": step,
                "player": if captures[1].eq_ignore_ascii_case("each") {
                    json!({ "kind": "eachPlayer" })
                } else {
                    controller()
                },
            },
            "effects": [prepared_effect()],
        });
        if let Some(condition) = captures.get(3) {
            rule["condition"] = parse_condition_text(condition.as_str())?;
        }
        return Some(draft(
            rule,
            &[
                "Parse phase trigger",
                "Parse preparation condition",
                "Set prepared designation",
            ],
        ));
    }

    let (event, condition, trigger_limit) = if instruction
        .to_ascii_lowercase()
        .starts_with("whenever an opponent draws their second card each turn,")
    {
        (
            json!({
                "kind": "cardDrawn",
                "opponentOfSourceController": true,
                "drawOrdinal": integer(2),
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever one or more cards leave your graveyard,")
    {
        (
            json!({ "kind": "cardsLeftGraveyard", "player": controller() }),
            None,
            None,
        )
    } else if instruction.to_ascii_lowercase().starts_with(
        "whenever one or more creatures you control deal combat damage to a player,",
    ) {
        (
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
            }),
            None,
            None,
        )
    } else if instruction.to_ascii_lowercase().starts_with(
        "when this creature enters and whenever one or more creature tokens you control deal combat damage to a player,",
    ) {
        (
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    {
                        "kind": "controlledCreaturesCombatDamageToPlayer",
                        "player": controller(),
                        "where": { "kind": "isToken" },
                    },
                ],
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever one or more tokens you control enter,")
    {
        (
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": { "kind": "isToken" },
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever you cast a creature spell,")
    {
        (
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever you cast your third spell each turn,")
    {
        (
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "spellCastOrdinal": integer(3),
            }),
            None,
            None,
        )
    } else if instruction.to_ascii_lowercase().starts_with(
        "whenever you cast an instant or sorcery spell with mana value 5 or greater from your hand,",
    ) {
        (
            json!({
                "kind": "spellCast",
                "player": controller(),
                "fromZone": "hand",
                "where": and(vec![
                    or(vec![card_type("Instant"), card_type("Sorcery")]),
                    compare(
                        ">=",
                        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                        integer(5),
                    ),
                ]),
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever you gain life for the first time each turn,")
    {
        (
            json!({ "kind": "lifeGained", "player": controller() }),
            None,
            Some(json!({ "kind": "onceEachTurn", "id": "prepareOnFirstLifeGain" })),
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("landfall — whenever a land you control enters,")
    {
        (
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Land"),
            }),
            None,
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever this creature attacks, if you control eight or more lands,")
    {
        (
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            parse_condition_text("you control eight or more lands"),
            None,
        )
    } else if instruction
        .to_ascii_lowercase()
        .starts_with("whenever you attack with two or more creatures,")
    {
        (
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": integer(2),
            }),
            None,
            None,
        )
    } else {
        return None;
    };

    let mut rule = json!({
        "kind": "triggeredAbility",
        "source": self_ref(),
        "event": event,
        "effects": [prepared_effect()],
    });
    if let Some(condition) = condition {
        rule["condition"] = condition;
    }
    if let Some(trigger_limit) = trigger_limit {
        rule["triggerLimit"] = trigger_limit;
    }
    Some(draft(
        rule,
        &[
            "Parse reusable trigger event",
            "Evaluate preparation condition",
            "Set prepared designation",
        ],
    ))
}
