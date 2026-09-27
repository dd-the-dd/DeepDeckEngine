use super::super::*;

pub(in crate::oracle::canonical) fn parse_expansion_trigger_event<'a>(
    text: &'a str,
    face_name: &str,
) -> Option<(Value, &'a str)> {
    let scry_or_surveil_re = Regex::new(r"(?i)^Whenever you scry or surveil, (.+)$")
        .expect("scry-or-surveil trigger regex compiles");
    if let Some(captures) = scry_or_surveil_re.captures(text) {
        return Some((
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "scried", "player": controller() },
                    { "kind": "surveilled", "player": controller() },
                ],
            }),
            captures.get(1)?.as_str(),
        ));
    }

    let gain_life_re = Regex::new(r"(?i)^Whenever you gain life, (.+)$")
        .expect("life-gain trigger regex compiles");
    if let Some(captures) = gain_life_re.captures(text) {
        return Some((
            json!({ "kind": "lifeGained", "player": controller() }),
            captures.get(1)?.as_str(),
        ));
    }

    let opponent_noncombat_damage_re = Regex::new(
        r"(?i)^Whenever (?:one or more )?opponents? (?:is|are) dealt noncombat damage, (.+)$",
    )
    .expect("opponent noncombat-damage trigger regex compiles");
    if let Some(captures) = opponent_noncombat_damage_re.captures(text) {
        return Some((
            json!({
                "kind": "opponentDealtDamage",
                "player": controller(),
                "noncombatOnly": true,
            }),
            captures.get(1)?.as_str(),
        ));
    }

    let loyalty_activation_re =
        Regex::new(r"(?i)^Whenever (you|an opponent) activates? (?:a|an) loyalty ability, (.+)$")
            .expect("loyalty-ability activation trigger regex compiles");
    if let Some(captures) = loyalty_activation_re.captures(text) {
        let mut event = json!({
            "kind": "abilityActivated",
            "where": card_type("Planeswalker"),
            "loyaltyOnly": true,
        });
        if captures[1].eq_ignore_ascii_case("an opponent") {
            event["opponentOfSourceController"] = Value::Bool(true);
        } else {
            event["player"] = controller();
        }
        return Some((event, captures.get(2)?.as_str()));
    }

    let any_player_cast_re = Regex::new(r"(?i)^When(?:ever)? a player casts a spell, (.+)$")
        .expect("any-player spell-cast trigger regex compiles");
    if let Some(captures) = any_player_cast_re.captures(text) {
        return Some((
            json!({ "kind": "spellCast", "anyPlayer": true, "where": Value::Null }),
            captures.get(1)?.as_str(),
        ));
    }

    let opponent_graveyard_card_re = Regex::new(
        r"(?i)^Whenever (?:a|an) (.+?) card is put into an opponent's graveyard from anywhere, (.+)$",
    )
    .expect("filtered opponent graveyard trigger regex compiles");
    if let Some(captures) = opponent_graveyard_card_re.captures(text) {
        return Some((
            json!({
                "kind": "opponentCardEnteredGraveyard",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }

    if let Some(instruction) = text.strip_prefix("Whenever you cast a spell, ") {
        return Some((
            json!({ "kind": "spellCast", "player": controller(), "where": Value::Null }),
            instruction,
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever you cast a spell during an opponent's turn, ")
    {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "duringOpponentTurn": true,
            }),
            instruction,
        ));
    }
    let unqualified_mana_value_spell_re = Regex::new(&format!(
        r"(?i)^Whenever you cast a spell with mana value ({}) or (greater|less), (.+)$",
        count_word_pattern(),
    ))
    .expect("unqualified mana-value spell trigger regex compiles");
    if let Some(captures) = unqualified_mana_value_spell_re.captures(text) {
        let operator = if captures[2].eq_ignore_ascii_case("greater") {
            ">="
        } else {
            "<="
        };
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": compare(
                    operator,
                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                    integer(parse_number_word(captures.get(1)?.as_str())?),
                ),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let single_target_instant_or_sorcery_re = Regex::new(
        r"(?i)^Whenever you cast an instant or sorcery spell with a single target, (.+)$",
    )
    .expect("single-target instant-or-sorcery cast trigger regex compiles");
    if let Some(captures) = single_target_instant_or_sorcery_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                "singleTarget": true,
            }),
            captures.get(1)?.as_str(),
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever you cast a spell of the chosen type, ") {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "creatureTypeDecisionId": "chosenCreatureType",
            }),
            instruction,
        ));
    }
    let chosen_type_permanent_entry_re =
        Regex::new(r"(?i)^Whenever a permanent you control of the chosen type enters, (.+)$")
            .expect("chosen creature-type permanent entry trigger regex compiles");
    if let Some(captures) = chosen_type_permanent_entry_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": Value::Null,
                "creatureTypeDecisionId": "chosenCreatureType",
            }),
            captures.get(1)?.as_str(),
        ));
    }
    if let Some(rest) = strip_expansion_landfall_prefix(text)
        && let Some(instruction) = rest.strip_prefix("Whenever a land you control enters, ")
    {
        return Some((
            json!({ "kind": "permanentEntered", "player": controller(), "where": card_type("Land") }),
            instruction,
        ));
    }
    let text = strip_short_oracle_label(text);
    if let Some(entry) = parse_source_entry_trigger(text, face_name) {
        return Some(entry);
    }

    let combat_damage_received_re =
        Regex::new(r"(?i)^Whenever one or more (.+?) deal combat damage to you, (.+)$")
            .expect("combat damage received trigger regex compiles");
    if let Some(captures) = combat_damage_received_re.captures(text) {
        return Some((
            json!({
                "kind": "combatDamageReceived",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(1)?.as_str()),
                    face_name,
                )?,
            }),
            captures.get(2)?.as_str(),
        ));
    }

    let source_dealt_noncombat_damage_re = Regex::new(
        r"(?i)^Whenever (this (?:creature|permanent)|[A-Z][A-Za-z0-9 ',.-]+) is dealt noncombat damage, (.+)$",
    )
    .expect("source dealt noncombat damage trigger regex compiles");
    if let Some(captures) = source_dealt_noncombat_damage_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            json!({
                "kind": "permanentDealtDamage",
                "object": self_ref(),
                "noncombatOnly": true,
            }),
            captures.get(2)?.as_str(),
        ));
    }

    let controlled_counter_placement_re = Regex::new(
        r"(?i)^Whenever you put one or more(?: ([^ ]+))? counters on (?:a|an) (.+?)(?: you control)?, (.+)$",
    )
    .expect("controlled counter-placement trigger regex compiles");
    if let Some(captures) = controlled_counter_placement_re.captures(text) {
        let mut event = json!({
            "kind": "countersPlaced",
            "player": controller(),
            "where": card_qualifier_list_filter(captures.get(2)?.as_str(), face_name)
                .or_else(|| parse_permanent_criteria(captures.get(2)?.as_str(), face_name))?,
        });
        if let Some(counter) = captures.get(1) {
            event["counter"] = Value::String(counter.as_str().to_ascii_lowercase());
        }
        return Some((event, captures.get(3)?.as_str()));
    }

    let player_loses_life_re = Regex::new(r"(?i)^Whenever a player loses life, (.+)$")
        .expect("player life-loss trigger regex compiles");
    if let Some(captures) = player_loses_life_re.captures(text) {
        return Some((
            json!({
                "kind": "lifeLost",
                "player": { "kind": "eachPlayer" },
            }),
            captures.get(1)?.as_str(),
        ));
    }

    let opponent_chosen_parity_spell_re = Regex::new(
        r"(?is)^Whenever an opponent casts a spell with mana value of the chosen quality, (.+)$",
    )
    .expect("opponent spell with stored mana-value parity regex compiles");
    if let Some(captures) = opponent_chosen_parity_spell_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": Value::Null,
                "manaValueParityDecisionId": "chosenManaValueParity",
            }),
            captures.get(1)?.as_str(),
        ));
    }

    let self_cast_and_death_re = Regex::new(
        r"(?i)^When you cast this spell and when this (?:creature|permanent) dies, (.+)$",
    )
    .expect("self cast-and-death trigger regex compiles");
    if let Some(captures) = self_cast_and_death_re.captures(text) {
        return Some((
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": Value::Null,
                        "sourceIsSelf": true,
                    },
                    { "kind": "permanentDied", "object": self_ref() },
                ],
            }),
            captures.get(1)?.as_str(),
        ));
    }

    let source_enters_from_zone_re = Regex::new(
        r"(?i)^When (this (?:artifact|creature|enchantment|permanent)|[A-Z][A-Za-z0-9 ',.-]+) enters from your (graveyard|hand|exile|library|command zone), (.+)$",
    )
    .expect("source entry from a specific zone regex compiles");
    if let Some(captures) = source_enters_from_zone_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let from_zone = if captures[2].eq_ignore_ascii_case("command zone") {
                "commandZone".to_string()
            } else {
                captures.get(2)?.as_str().to_ascii_lowercase()
            };
            return Some((
                json!({
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                    "fromZone": from_zone,
                }),
                captures.get(3)?.as_str(),
            ));
        }
    }

    let land_played_re = Regex::new(r"(?i)^When(?:ever)? you play (another|a) land, (.+)$")
        .expect("land-play trigger regex compiles");
    if let Some(captures) = land_played_re.captures(text) {
        let mut event = json!({ "kind": "landPlayed", "player": controller() });
        if captures[1].eq_ignore_ascii_case("another") {
            event["excludeSource"] = Value::Bool(true);
        }
        return Some((event, captures.get(2)?.as_str()));
    }

    let self_graveyard_re =
        Regex::new(r"(?i)^When (.+?) is put into a graveyard from anywhere, (.+)$")
            .expect("self graveyard entry trigger regex compiles");
    if let Some(captures) = self_graveyard_re.captures(text)
        && source_reference_matches(&captures[1], face_name)
    {
        return Some((
            json!({ "kind": "selfEnteredGraveyard", "object": self_ref() }),
            captures.get(2)?.as_str(),
        ));
    }

    let exploited_re = Regex::new(r"(?i)^When (.+?) exploits a creature, (.+)$")
        .expect("named exploit event regex compiles");
    if let Some(captures) = exploited_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            json!({ "kind": "creatureExploited", "object": self_ref() }),
            captures.get(2)?.as_str(),
        ));
    }

    if let Some(instruction) = text.strip_prefix("At the beginning of your first main phase, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "precombatMain", "player": controller() }),
            instruction,
        ));
    }

    let source_enters_or_transforms_re = Regex::new(
        r"(?i)^When(?:ever)? (this (?:creature|permanent)|[A-Z][A-Za-z0-9 ',.-]+) enters or transforms into [A-Z][A-Za-z0-9 ',.-]+, (.+)$",
    )
    .expect("source entry-or-transform trigger regex compiles");
    if let Some(captures) = source_enters_or_transforms_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            return Some((
                json!({
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "permanentTransformed", "object": self_ref() },
                    ],
                }),
                captures.get(2)?.as_str(),
            ));
        }
    }

    let source_transforms_and_main_re = Regex::new(
        r"(?i)^When(?:ever)? (this (?:creature|permanent)|[A-Z][A-Za-z0-9 ',.-]+) transforms into [A-Z][A-Za-z0-9 ',.-]+ and at the beginning of your first main phase, (.+)$",
    )
    .expect("source transform-and-main-phase trigger regex compiles");
    if let Some(captures) = source_transforms_and_main_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            return Some((
                json!({
                    "kind": "oneOf",
                    "events": [
                        { "kind": "permanentTransformed", "object": self_ref() },
                        { "kind": "stepBegan", "step": "precombatMain", "player": controller() },
                    ],
                }),
                captures.get(2)?.as_str(),
            ));
        }
    }

    let source_transforms_re = Regex::new(
        r"(?i)^When(?:ever)? (this (?:creature|permanent)|[A-Z][A-Za-z0-9 ',.-]+) transforms into [A-Z][A-Za-z0-9 ',.-]+, (.+)$",
    )
    .expect("source transform trigger regex compiles");
    if let Some(captures) = source_transforms_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            return Some((
                json!({ "kind": "permanentTransformed", "object": self_ref() }),
                captures.get(2)?.as_str(),
            ));
        }
    }

    let source_enters_or_second_event_re = Regex::new(
        r"(?i)^When(?:ever)? (.+?) enters or (attacks|dies|leaves the battlefield), (.+)$",
    )
    .expect("source enters-or-second-event trigger regex compiles");
    if let Some(captures) = source_enters_or_second_event_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.eq_ignore_ascii_case("this creature")
            || subject.eq_ignore_ascii_case("this permanent")
            || source_reference_matches(subject, face_name)
        {
            return Some((
                json!({
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        if captures[2].eq_ignore_ascii_case("attacks") {
                            json!({ "kind": "declaredAttacker", "object": self_ref() })
                        } else if captures[2].eq_ignore_ascii_case("leaves the battlefield") {
                            json!({ "kind": "permanentLeftBattlefield", "object": self_ref() })
                        } else {
                            json!({ "kind": "permanentDied", "object": self_ref() })
                        },
                    ],
                }),
                captures.get(3)?.as_str(),
            ));
        }
    }

    if !face_name.is_empty() {
        for trigger_word in ["When", "Whenever"] {
            for verb in ["die", "dies"] {
                let prefix = format!("{trigger_word} {face_name} {verb}, ");
                if let Some(instruction) = text.strip_prefix(&prefix) {
                    return Some((
                        json!({ "kind": "permanentDied", "object": self_ref() }),
                        instruction,
                    ));
                }
            }
        }
    }
    let source_dies_re = Regex::new(r"(?i)^When(?:ever)? (.+?) (?:die|dies), (.+)$")
        .expect("source death trigger regex compiles");
    if let Some(captures) = source_dies_re.captures(text) {
        let subject = captures.get(1)?.as_str();
        if subject.eq_ignore_ascii_case("this creature")
            || subject.eq_ignore_ascii_case("this permanent")
            || source_reference_matches(subject, face_name)
        {
            return Some((
                json!({ "kind": "permanentDied", "object": self_ref() }),
                captures.get(2)?.as_str(),
            ));
        }
    }

    let graveyard_entry_re = Regex::new(
        r"(?i)^Whenever one or more (.+?) cards are put into your graveyard from anywhere, (.+)$",
    )
    .expect("graveyard-entry trigger regex compiles");
    if let Some(captures) = graveyard_entry_re.captures(text) {
        return Some((
            json!({
                "kind": "cardsEnteredGraveyard",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let graveyard_leave_re =
        Regex::new(r"(?i)^Whenever (?:an?|one or more) (.+?) cards? leaves? your graveyard, (.+)$")
            .expect("graveyard-leave trigger regex compiles");
    if let Some(captures) = graveyard_leave_re.captures(text) {
        return Some((
            json!({
                "kind": "cardsLeftGraveyard",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let token_created_re =
        Regex::new(r"(?i)^Whenever you create (?:a|an|one or more) (.+?) tokens?, (.+)$")
            .expect("controlled token creation trigger regex compiles");
    if let Some(captures) = token_created_re.captures(text) {
        return Some((
            json!({
                "kind": "tokenCreated",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever you complete a dungeon, ") {
        return Some((
            json!({ "kind": "dungeonCompleted", "player": controller() }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("When you unlock this door, ") {
        return Some((
            json!({ "kind": "doorUnlocked", "object": self_ref() }),
            instruction,
        ));
    }

    if let Some(instruction) = text.strip_prefix("Whenever a land you control enters, ") {
        return Some((
            json!({ "kind": "permanentEntered", "player": controller(), "where": card_type("Land") }),
            instruction,
        ));
    }
    let another_controlled_entry_re =
        Regex::new(r"(?i)^Whenever another (.+?) you control enters, (.+)$")
            .expect("another controlled permanent entry trigger regex compiles");
    if let Some(captures) = another_controlled_entry_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(1)?.as_str()),
                    face_name,
                )?,
                "excludeSource": true,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let controlled_entry_re = Regex::new(r"(?i)^Whenever (?:a|an) (.+?) you control enters, (.+)$")
        .expect("controlled permanent entry trigger regex compiles");
    if let Some(captures) = controlled_entry_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": parse_permanent_criteria(&singular_card_term(&captures[1]), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let opponent_entry_re =
        Regex::new(r"(?i)^Whenever (?:a|an) (.+?) an opponent controls enters, (.+)$")
            .expect("opponent permanent entry trigger regex compiles");
    if let Some(captures) = opponent_entry_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": { "kind": "opponentsOf", "player": controller() },
                "where": parse_permanent_criteria(&singular_card_term(&captures[1]), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let named_opponent_target_re = Regex::new(
        r"(?i)^Whenever (.+?) becomes the target of a spell or ability an opponent controls, (.+)$",
    )
    .expect("named source opponent targeting trigger regex compiles");
    if let Some(captures) = named_opponent_target_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            json!({
                "kind": "becameTarget",
                "object": self_ref(),
                "controlledByOpponent": true,
                "stackObjectKind": "spellOrAbility",
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) = text
        .strip_prefix("Whenever this creature or another nontoken creature you control enters, ")
    {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Creature"),
                "nontoken": true,
            }),
            instruction,
        ));
    }
    let source_or_other_controlled_entry_re =
        Regex::new(r"(?i)^Whenever (.+?) or another (.+?) you control enters, (.+)$")
            .expect("source-or-other controlled permanent entry regex compiles");
    if let Some(captures) = source_or_other_controlled_entry_re.captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(2)?.as_str()),
                    face_name,
                )?,
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let source_or_subtype_enters_re =
        Regex::new(r"(?i)^Whenever (?:.+?) or another (.+?)( you control)? enters, (.+)$")
            .expect("source-or-subtype entry trigger regex compiles");
    if let Some(captures) = source_or_subtype_enters_re.captures(text) {
        let mut event = json!({
            "kind": "permanentEntered",
            "where": parse_permanent_criteria(&singular_card_term(&captures[1]), face_name)?,
        });
        if captures.get(2).is_some() {
            event["player"] = controller();
        } else {
            event["anyController"] = Value::Bool(true);
        }
        return Some((event, captures.get(3)?.as_str()));
    }

    for (prefix, criteria) in [
        (
            "Whenever an enchantment you control enters, ",
            "enchantment",
        ),
        (
            "Whenever this enchantment or another enchantment you control enters, ",
            "enchantment",
        ),
    ] {
        if let Some(instruction) = text.strip_prefix(prefix) {
            return Some((
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": parse_permanent_criteria(criteria, face_name)?,
                }),
                instruction,
            ));
        }
    }

    for subject in [
        "artifact",
        "creature",
        "enchantment",
        "Equipment",
        "permanent",
    ] {
        let prefix = format!("When this {subject} dies, ");
        if let Some(instruction) = text.strip_prefix(&prefix) {
            return Some((
                json!({ "kind": "permanentDied", "object": self_ref() }),
                instruction,
            ));
        }
        let prefix = format!("When this {subject} is put into a graveyard from the battlefield, ");
        if let Some(instruction) = text.strip_prefix(&prefix) {
            return Some((
                json!({ "kind": "permanentDied", "object": self_ref() }),
                instruction,
            ));
        }
    }
    for subject in [
        "artifact",
        "creature",
        "enchantment",
        "Equipment",
        "permanent",
    ] {
        let prefix = format!("When this {subject} leaves the battlefield, ");
        if let Some(instruction) = text.strip_prefix(&prefix) {
            return Some((
                json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
                instruction,
            ));
        }
    }

    for subject in ["creature", "token", "Spacecraft"] {
        let prefix = format!("Whenever this {subject} attacks, ");
        if let Some(instruction) = text.strip_prefix(&prefix) {
            return Some((
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                instruction,
            ));
        }
    }
    if let Some(instruction) = text.strip_prefix("Whenever a creature you control attacks, ") {
        return Some((
            json!({
                "kind": "controlledCreatureDeclaredAttacker",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            instruction,
        ));
    }
    let attached_attacks_alone_re =
        Regex::new(r"(?i)^Whenever (?:equipped|enchanted) creature attacks alone, (.+)$")
            .expect("attached permanent attacks alone regex compiles");
    if let Some(captures) = attached_attacks_alone_re.captures(text) {
        return Some((
            json!({
                "kind": "attachedPermanentDeclaredAttacker",
                "attackingAlone": true,
            }),
            captures.get(1)?.as_str(),
        ));
    }
    if !face_name.is_empty()
        && let Some(captures) = Regex::new(r"(?i)^Whenever (.+?) attacks?, (.+)$")
            .expect("named source attack trigger regex compiles")
            .captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        if let Some(instruction) = captures.get(2).map(|value| value.as_str()) {
            return Some((
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                instruction,
            ));
        }
    }

    if let Some(instruction) = text.strip_prefix("Whenever you attack with two or more creatures, ")
    {
        return Some((
            json!({ "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": 2 }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever another creature you control enters, ") {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Creature"),
                "excludeSource": true,
            }),
            instruction,
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever a creature an opponent controls enters, ")
    {
        return Some((
            json!({
                "kind": "permanentEntered",
                "player": { "kind": "opponentsOf", "player": controller() },
                "where": card_type("Creature"),
            }),
            instruction,
        ));
    }
    let attack_with_filtered_creature_re =
        Regex::new(r"(?i)^Whenever you attack with one or more (.+?), (.+)$")
            .expect("attack with filtered creature regex compiles");
    if let Some(captures) = attack_with_filtered_creature_re.captures(text) {
        return Some((
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": integer(1),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(1)?.as_str()),
                    face_name,
                )?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever you attack, ") {
        return Some((
            json!({ "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": 1 }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever you attack a player, ") {
        return Some((
            json!({ "kind": "controlledCreaturesAttacked", "player": controller(), "minimum": 1 }),
            instruction,
        ));
    }
    let other_dies_re = Regex::new(
        r"(?i)^Whenever (?:one or more )?(?:other|another) (nontoken )?(.+?) (?:die|dies), (.+)$",
    )
    .expect("other-permanent death trigger regex compiles");
    if let Some(captures) = other_dies_re.captures(text) {
        let criteria_text = captures.get(2)?.as_str();
        if !criteria_text.to_ascii_lowercase().ends_with(" you control") {
            let where_filter =
                card_qualifier_list_filter(criteria_text, face_name).or_else(|| {
                    parse_permanent_criteria(&singular_card_term(criteria_text), face_name)
                })?;
            return Some((
                json!({
                    "kind": "permanentDied",
                    "where": where_filter,
                    "excludeSource": true,
                    "nontoken": captures.get(1).is_some(),
                }),
                captures.get(3)?.as_str(),
            ));
        }
    }
    let another_controlled_dies_re = Regex::new(
        r"(?i)^Whenever (?:(?:one or more )?other|another) (nontoken )?(.+?) you control (?:die|dies|is put into a graveyard from the battlefield), (.+)$",
    )
            .expect("another-controlled-permanent death trigger regex compiles");
    if let Some(captures) = another_controlled_dies_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_qualifier_list_filter(captures.get(2)?.as_str(), face_name)
                    .or_else(|| parse_permanent_criteria(
                        &singular_card_term(captures.get(2)?.as_str()),
                        face_name,
                    ))?,
                "excludeSource": true,
                "nontoken": captures.get(1).is_some(),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let controlled_dies_re =
        Regex::new(r"(?i)^Whenever (?:a|an) (nontoken )?(.+?) you control dies, (.+)$")
            .expect("controlled permanent death trigger regex compiles");
    if let Some(captures) = controlled_dies_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": parse_permanent_criteria(&singular_card_term(&captures[2]), face_name)?,
                "nontoken": captures.get(1).is_some(),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let opponent_controlled_dies_re =
        Regex::new(r"(?i)^Whenever (?:a|an) (.+?) an opponent controls dies, (.+)$")
            .expect("opponent-controlled-permanent death trigger regex compiles");
    if let Some(captures) = opponent_controlled_dies_re.captures(text)
        && parse_permanent_criteria(&singular_card_term(&captures[1]), face_name)?
            == card_type("Creature")
    {
        return Some((
            json!({ "kind": "opponentCreatureDied", "player": controller() }),
            captures.get(2)?.as_str(),
        ));
    }
    let chosen_type_dies_re =
        Regex::new(r"(?i)^Whenever (?:a|an) (.+?) of the chosen type dies, (.+)$")
            .expect("chosen creature-type death trigger regex compiles");
    if let Some(captures) = chosen_type_dies_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentDied",
                "where": and(vec![
                    parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
                    chosen_creature_type(),
                ]),
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever you activate an ability that isn't a mana ability, ")
    {
        return Some((
            json!({
                "kind": "nonManaAbilityActivated",
                "player": controller(),
            }),
            instruction,
        ));
    }
    let activated_ability_source_re =
        Regex::new(r"(?i)^Whenever you activate an ability of (?:a|an) (.+?), (.+)$")
            .expect("activated-ability source criteria regex compiles");
    if let Some(captures) = activated_ability_source_re.captures(text) {
        return Some((
            json!({
                "kind": "abilityActivated",
                "player": controller(),
                "where": parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let controlled_sacrifice_re =
        Regex::new(r"(?i)^Whenever you sacrifice (another )?(.+?), (.+)$")
            .expect("controlled permanent sacrifice trigger regex compiles");
    if let Some(captures) = controlled_sacrifice_re.captures(text) {
        return Some((
            json!({
                "kind": "permanentSacrificed",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(2)?.as_str()),
                    face_name,
                )?,
                "excludeSource": captures.get(1).is_some(),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let source_or_subtype_dies_re = Regex::new(
        r"^Whenever (?:.+?) or another ([A-Za-z][A-Za-z '-]+?)( you control)? dies, (.+)$",
    )
    .expect("source-or-subtype death trigger regex compiles");
    if let Some(captures) = source_or_subtype_dies_re.captures(text) {
        let mut event = json!({
            "kind": "permanentDied",
            "where": parse_permanent_criteria(
                &singular_card_term(captures.get(1)?.as_str()),
                face_name,
            )?,
        });
        if captures.get(2).is_some() {
            event["player"] = controller();
        }
        return Some((event, captures.get(3)?.as_str()));
    }
    let colored_spell_re =
        Regex::new(r"^Whenever you cast an? (white|blue|black|red|green) spell, (.+)$")
            .expect("colored spell trigger regex compiles");
    if let Some(captures) = colored_spell_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": color_filter(&captures[1])?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let ordinal_spell_re = Regex::new(&format!(
        r"(?i)^Whenever you cast your ({}) (?:(.+?) )?spell each turn, (.+)$",
        ordinal_word_pattern(),
    ))
    .expect("ordinal spell-cast trigger regex compiles");
    if let Some(captures) = ordinal_spell_re.captures(text) {
        let where_filter = if let Some(criteria) = captures.get(2) {
            parse_permanent_criteria(criteria.as_str(), face_name)?
        } else {
            Value::Null
        };
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": where_filter,
                "spellCastOrdinal": integer(parse_ordinal_word(&captures[1])?),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let any_player_ordinal_spell_re = Regex::new(&format!(
        r"(?i)^Whenever a player casts their ({}) (?:(.+?) )?spell each turn, (.+)$",
        ordinal_word_pattern(),
    ))
    .expect("any-player ordinal spell-cast trigger regex compiles");
    if let Some(captures) = any_player_ordinal_spell_re.captures(text) {
        let where_filter = if let Some(criteria) = captures.get(2) {
            parse_permanent_criteria(criteria.as_str(), face_name)?
        } else {
            Value::Null
        };
        return Some((
            json!({
                "kind": "spellCast",
                "anyPlayer": true,
                "where": where_filter,
                "spellCastOrdinal": integer(parse_ordinal_word(&captures[1])?),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    let mana_value_spell_re = Regex::new(&format!(
        r"(?i)^Whenever you cast (?:a|an) (.+?) spell with mana value ({}) or (greater|less), (.+)$",
        count_word_pattern(),
    ))
    .expect("mana-value-qualified spell trigger regex compiles");
    if let Some(captures) = mana_value_spell_re.captures(text) {
        let criteria = format!(
            "{} with mana value {} or {}",
            captures.get(1)?.as_str(),
            captures.get(2)?.as_str(),
            captures.get(3)?.as_str(),
        );
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": parse_permanent_criteria(&criteria, face_name)?,
            }),
            captures.get(4)?.as_str(),
        ));
    }
    let controlled_cast_or_copy_re =
        Regex::new(r"(?i)^Whenever you cast or copy (?:a|an) (.+?) spell, (.+)$")
            .expect("controlled cast-or-copy trigger regex compiles");
    if let Some(captures) = controlled_cast_or_copy_re.captures(text) {
        let where_filter = parse_permanent_criteria(&captures[1], face_name)?;
        return Some((
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": where_filter.clone(),
                    },
                    {
                        "kind": "spellCopied",
                        "player": controller(),
                        "where": where_filter,
                    },
                ],
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let controlled_spell_re = Regex::new(r"(?i)^Whenever you cast (?:a|an) (.+?) spell, (.+)$")
        .expect("controlled filtered spell trigger regex compiles");
    if let Some(captures) = controlled_spell_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever you cast a spell from anywhere other than your hand, ")
    {
        return Some((
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "fromZoneNot": "hand",
            }),
            instruction,
        ));
    }
    let opponent_spell_mana_value_re = Regex::new(&format!(
        r"(?i)^Whenever an opponent casts a spell with mana value ({}) or less, (.+)$",
        count_word_pattern(),
    ))
    .expect("opponent spell mana-value trigger regex compiles");
    if let Some(captures) = opponent_spell_mana_value_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": compare(
                    "<=",
                    json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                    integer(parse_number_word(captures.get(1)?.as_str())?),
                ),
            }),
            captures.get(2)?.as_str(),
        ));
    }
    let opponent_spell_re =
        Regex::new(r"(?i)^Whenever an opponent casts (?:a|an) (.+?) spell, (.+)$")
            .expect("opponent filtered spell trigger regex compiles");
    let opponent_ordinal_spell_re = Regex::new(&format!(
        r"(?i)^Whenever an opponent casts their ({}) (.+?) spell each turn, (.+)$",
        ordinal_word_pattern(),
    ))
    .expect("opponent ordinal filtered spell trigger regex compiles");
    if let Some(captures) = opponent_ordinal_spell_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": parse_permanent_criteria(&captures[2], face_name)?,
                "spellCastOrdinal": integer(parse_ordinal_word(&captures[1])?),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    if let Some(captures) = opponent_spell_re.captures(text) {
        return Some((
            json!({
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": parse_permanent_criteria(&captures[1], face_name)?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever an opponent casts a spell, ") {
        return Some((
            json!({
                "kind": "spellCast",
                "opponentOfSourceController": true,
                "where": Value::Null,
            }),
            instruction,
        ));
    }
    let ordinal_draw_re = Regex::new(&format!(
        r"(?i)^When(?:ever)? (you draw your|an opponent draws their) ({}) card (?:each turn|in a turn), (.+)$",
        ordinal_word_pattern(),
    ))
    .expect("ordinal card-draw trigger regex compiles");
    if let Some(captures) = ordinal_draw_re.captures(text) {
        let opponent_draw = captures[1].eq_ignore_ascii_case("an opponent draws their");
        return Some((
            json!({
                "kind": "cardDrawn",
                "player": if opponent_draw { Value::Null } else { controller() },
                "opponentOfSourceController": opponent_draw,
                "drawOrdinal": integer(parse_ordinal_word(captures.get(2)?.as_str())?),
            }),
            captures.get(3)?.as_str(),
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever you cast a creature spell with power 5 or greater, ")
    {
        return Some((
            json!({ "kind": "spellCast", "player": controller(), "where": card_type("Creature") }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("When you cast this spell, ") {
        return Some((
            json!({ "kind": "spellCast", "player": controller(), "where": Value::Null, "sourceIsSelf": true }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of your upkeep, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of each upkeep, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "upkeep", "player": { "kind": "eachPlayer" } }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of each opponent's draw step, ")
    {
        return Some((
            json!({
                "kind": "stepBegan",
                "step": "draw",
                "player": { "kind": "opponentsOf", "player": controller() },
            }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of combat on your turn, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of each combat, ") {
        return Some((
            json!({
                "kind": "stepBegan",
                "step": "beginCombat",
                "player": { "kind": "eachPlayer" },
            }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of each end step, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "endStep", "player": { "kind": "eachPlayer" } }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of the end step, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "endStep", "player": { "kind": "eachPlayer" } }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("At the beginning of your end step, ") {
        return Some((
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("When enchanted creature dies, ") {
        return Some((
            json!({ "kind": "attachedPermanentDied", "attachment": self_ref() }),
            instruction,
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("When this Aura is put into a graveyard from the battlefield, ")
    {
        return Some((
            json!({ "kind": "permanentDied", "object": self_ref() }),
            instruction,
        ));
    }
    if let Some(instruction) = text
        .strip_prefix("Whenever this creature deals combat damage to a player, ")
        .or_else(|| text.strip_prefix("Whenever this token deals combat damage to a player, "))
    {
        return Some((
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            instruction,
        ));
    }
    let controlled_filtered_combat_damage_re = Regex::new(
        r"(?i)^Whenever (?:a|an|one or more) (.+?) you control deals? combat damage to a player, (.+)$",
    )
    .expect("controlled filtered combat-damage trigger regex compiles");
    if let Some(captures) = controlled_filtered_combat_damage_re.captures(text) {
        return Some((
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
                "where": parse_permanent_criteria(
                    &singular_card_term(captures.get(1)?.as_str()),
                    face_name,
                )?,
            }),
            captures.get(2)?.as_str(),
        ));
    }
    if !face_name.is_empty()
        && let Some(captures) =
            Regex::new(r"(?i)^Whenever (.+?) deals combat damage to a player, (.+)$")
                .expect("named source combat-damage trigger regex compiles")
                .captures(text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
    {
        return Some((
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            captures.get(2)?.as_str(),
        ));
    }
    if let Some(instruction) =
        text.strip_prefix("Whenever equipped creature deals combat damage to a player, ")
    {
        return Some((
            json!({
                "kind": "attachedPermanentCombatDamageToPlayer",
                "attachment": self_ref(),
            }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever equipped creature attacks, ") {
        return Some((
            json!({
                "kind": "attachedPermanentDeclaredAttacker",
                "attachment": self_ref(),
            }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever the Ring tempts you, ") {
        return Some((
            json!({ "kind": "ringTempted", "player": controller() }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix("Whenever an opponent gains life, ") {
        return Some((
            json!({ "kind": "lifeGained", "player": { "kind": "opponentsOf", "player": controller() } }),
            instruction,
        ));
    }
    if let Some(instruction) = text.strip_prefix(
        "Whenever a player or permanent becomes the target of an ability you control, ",
    ) {
        return Some((
            json!({
                "kind": "objectBecameTargetOfControlledAbility",
                "player": controller(),
            }),
            instruction,
        ));
    }
    if let Some(instruction) = text
        .strip_prefix(
            "Whenever this creature becomes the target of a spell or ability an opponent controls, ",
        )
        .or_else(|| {
            text.strip_prefix(
                "Whenever this creature becomes the target of a spell an opponent controls, ",
            )
        })
    {
        let spell_only = text.contains("target of a spell an opponent controls");
        return Some((
            json!({
                "kind": "becameTarget",
                "object": self_ref(),
                "controlledByOpponent": true,
                "stackObjectKind": if spell_only { "spell" } else { "spellOrAbility" },
            }),
            instruction,
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn creature_candidates() -> Value {
    json!({ "kind": "permanents", "where": card_type("Creature") })
}

pub(in crate::oracle::canonical) fn parse_expansion_counter_instruction(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let instruction = instruction.trim();
    let counters_on_each_controlled_re = Regex::new(&format!(
        r"(?i)^put ({}) ([A-Za-z0-9+/-]+) counters? on each (.+?) you control\.$",
        count_word_pattern(),
    ))
    .expect("counters on each controlled permanent regex compiles");
    if let Some(captures) = counters_on_each_controlled_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": parse_permanent_criteria(captures.get(3)?.as_str(), face_name)?,
                },
                "counter": captures.get(2)?.as_str(),
                "count": integer(parse_number_word(captures.get(1)?.as_str())?),
            })],
            Vec::new(),
        ));
    }
    if instruction == "put a +1/+1 counter on it."
        || instruction == "put a +1/+1 counter on this creature."
    {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
            Vec::new(),
        ));
    }
    if instruction == "put a +1/+1 counter on target creature." {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetCreature"),
                "counter": "+1/+1",
                "count": integer(1),
            })],
            vec![target_decision(
                "targetCreature",
                creature_candidates(),
                1,
                1,
            )],
        ));
    }
    if instruction == "you may put a +1/+1 counter on target creature." {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetCreature"),
                "counter": "+1/+1",
                "count": integer(1),
            })],
            vec![target_decision(
                "targetCreature",
                creature_candidates(),
                0,
                1,
            )],
        ));
    }
    let put_then_double_re = Regex::new(
        r"(?i)^put (?:a|an) ([^ ]+) counter on target (creature(?: you control)?), then double the number of ([^ ]+) counters on (?:that creature|it)\.$",
    )
    .expect("put then double counters regex compiles");
    if let Some(captures) = put_then_double_re.captures(instruction)
        && captures[1].eq_ignore_ascii_case(&captures[3])
    {
        let counter = captures[1].to_string();
        let candidates = if captures[2].eq_ignore_ascii_case("creature you control") {
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            })
        } else {
            creature_candidates()
        };
        let target = chosen_target("targetCreature");
        return Some((
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": target.clone(),
                    "counter": counter,
                    "count": integer(1),
                }),
                json!({
                    "kind": "doubleCounters",
                    "permanent": target,
                    "counter": captures[1].to_string(),
                }),
            ],
            vec![target_decision("targetCreature", candidates, 1, 1)],
        ));
    }
    let counter_on_target_re =
        Regex::new(r"(?i)^put (?:a|an|one) ([A-Za-z0-9+/-]+) counter on target (.+?)\.$")
            .expect("counter on filtered target regex compiles");
    if let Some(captures) = counter_on_target_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetPermanent"),
                "counter": captures.get(1)?.as_str().to_ascii_lowercase(),
                "count": integer(1),
            })],
            vec![target_decision(
                "targetPermanent",
                permanent_target_candidates(captures.get(2)?.as_str(), face_name)?,
                1,
                1,
            )],
        ));
    }
    let support_re = Regex::new(r"(?i)^support (\d+)\.$").expect("support regex compiles");
    if let Some(captures) = support_re.captures(instruction) {
        let count = captures[1].parse::<i64>().ok()?;
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": { "kind": "chosenTargets", "id": "supportedCreatures" },
                "counter": "+1/+1",
                "count": integer(1),
            })],
            vec![target_decision(
                "supportedCreatures",
                creature_candidates(),
                0,
                count,
            )],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_expansion_instruction(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let normalized;
    let instruction = if let Some((main, _)) = instruction.split_once(". (") {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        instruction
    };
    if instruction.eq_ignore_ascii_case(
        "add two mana of any one color. Spend this mana only to cast spells with mana value 4 or greater.",
    ) {
        return Some((
            vec![json!({
                "kind": "addMana",
                "player": controller(),
                "mana": { "kind": "chooseColor", "amount": integer(2) },
                "spendRestriction": {
                    "kind": "castSpell",
                    "where": compare(
                        ">=",
                        json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                        integer(4),
                    ),
                },
            })],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("exile the top two cards of target opponent's library.") {
        return Some((
            vec![
                json!({
                    "kind": "exileTopCards",
                    "zone": library(chosen_target("targetOpponent")),
                    "count": integer(2),
                    "faceDown": false,
                    "bind": "maralenExiledCards",
                    "linkToSource": true,
                }),
                json!({
                    "kind": "grantMaralenPermissions",
                    "player": controller(),
                    "cards": bound_objects("maralenExiledCards"),
                }),
            ],
            vec![target_decision(
                "targetOpponent",
                json!({
                    "kind": "players",
                    "where": { "kind": "isOpponentOf", "player": controller() },
                }),
                1,
                1,
            )],
        ));
    }
    if let Some(effect) = parse_persistent_unused_modal_instruction(instruction, face_name) {
        return Some((vec![effect], Vec::new()));
    }
    let incomplete_dungeon_re = Regex::new(
        r"(?i)^if you haven't completed (.+?), return (?:this creature|this permanent|[A-Z][A-Za-z0-9 ',.-]+) to its owner's hand and venture into the dungeon\.$",
    )
    .expect("incomplete dungeon return and venture regex compiles");
    if let Some(captures) = incomplete_dungeon_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "conditionalEffect",
                "condition": not(json!({
                    "kind": "completedDungeon",
                    "player": controller(),
                    "name": captures[1].trim(),
                })),
                "then": [
                    { "kind": "returnToOwnersHand", "object": self_ref() },
                    { "kind": "ventureDungeon", "player": controller() },
                ],
                "else": [],
            })],
            Vec::new(),
        ));
    }
    if let Some(modal) = parse_general_modal_spell(instruction) {
        let effects = modal.rule["effects"].as_array()?.clone();
        let decisions = modal.rule["declaration"]["decisions"].as_array()?.clone();
        return Some((effects, decisions));
    }
    if let Some(effect_text) = instruction.strip_suffix(" if you've completed a dungeon.") {
        let effect_text = format!("{}.", effect_text.trim_end_matches('.'));
        let (effects, decisions) = parse_general_effect_instruction(&effect_text, face_name)?;
        return Some((
            vec![json!({
                "kind": "conditional",
                "condition": {
                    "kind": "completedDungeon",
                    "player": controller(),
                },
                "then": effects,
                "else": [],
            })],
            decisions,
        ));
    }
    let opponent_hand_control_re = Regex::new(
        r"(?i)^choose target opponent\. This turn, that player can't cast spells or activate abilities and plays with their hand revealed\. You may play lands and cast spells from that player's hand this turn\.$",
    )
    .expect("opponent hand-control instruction regex compiles");
    if opponent_hand_control_re.is_match(instruction) {
        let target = chosen_target("targetOpponent");
        return Some((
            vec![
                json!({
                    "kind": "restrictPlayerActions",
                    "player": target.clone(),
                    "castSpells": true,
                    "activateAbilities": true,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "revealHand",
                    "player": target.clone(),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                    "continuous": true,
                }),
                json!({
                    "kind": "grantHandPlayPermission",
                    "player": controller(),
                    "handOwner": target,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
            vec![target_decision(
                "targetOpponent",
                json!({
                    "kind": "players",
                    "where": { "kind": "isOpponentOf", "player": controller() },
                }),
                1,
                1,
            )],
        ));
    }
    if let Some(parsed) = parse_general_effect_instruction(instruction, face_name) {
        return Some(parsed);
    }
    if let Some(parsed) = parse_general_effect_sequence(instruction, face_name) {
        return Some(parsed);
    }
    if instruction == "copy it. You may choose new targets for the copy." {
        return Some((
            vec![json!({
                "kind": "copyStackItem",
                "object": { "kind": "triggeringStackObject" },
                "controller": controller(),
                "mayChooseNewTargets": true,
            })],
            Vec::new(),
        ));
    }
    let sacrifice_fetch_re = Regex::new(
        r"(?i)^sacrifice it\. When you do, search your library for a basic (.+) card, put it onto the battlefield tapped, then shuffle and you gain (\d+) life\.$",
    )
    .expect("sacrifice-and-fetch trigger regex compiles");
    if let Some(captures) = sacrifice_fetch_re.captures(instruction) {
        let land_types = captures[1]
            .replace(", or ", ", ")
            .replace(" or ", ", ")
            .split(", ")
            .map(|land_type| subtype(land_type.trim()))
            .collect::<Vec<_>>();
        return Some((
            vec![json!({
                "kind": "sacrificeSourceThenSearchLibrary",
                "player": controller(),
                "where": and(vec![
                    json!({ "kind": "typeLineContains", "value": "Basic" }),
                    or(land_types),
                ]),
                "destination": "battlefield",
                "tapped": true,
                "gainLife": integer(captures[2].parse::<i64>().ok()?),
            })],
            Vec::new(),
        ));
    }
    let lower_instruction = instruction.to_ascii_lowercase();
    if instruction == "create a token that's a copy of this creature." {
        return Some((
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": self_ref(),
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
            Vec::new(),
        ));
    }
    let source_copy_tokens_without_legendary_re = Regex::new(
        r"(?i)^create ([A-Za-z0-9]+) tokens? that are copies of (?:it|him|her|them|this creature|this permanent), except (?:the|those) tokens? (?:isn't|aren't|is not|are not) legendary\.$",
    )
    .expect("source token copies without legendary regex compiles");
    if let Some(captures) = source_copy_tokens_without_legendary_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": self_ref(),
                "quantity": integer(parse_number_word(captures.get(1)?.as_str())?),
                "removeLegendary": true,
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
            Vec::new(),
        ));
    }
    if lower_instruction.starts_with("create ")
        && let Some(effect) = create_token_effect(instruction)
    {
        return Some((vec![effect], Vec::new()));
    }
    if instruction == "return this card to its owner's hand."
        || instruction == "return it to its owner's hand."
    {
        return Some((
            vec![json!({ "kind": "returnToOwnersHand", "object": self_ref() })],
            Vec::new(),
        ));
    }
    if instruction.eq_ignore_ascii_case("put it on the bottom of its owner's library.") {
        return Some((
            vec![json!({
                "kind": "moveTriggeringCardFromGraveyard",
                "to": "libraryBottom",
            })],
            Vec::new(),
        ));
    }
    if instruction == "draw a card." {
        return Some((
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
            Vec::new(),
        ));
    }
    if instruction == "you may mill a card." || instruction == "mill a card." {
        return Some((
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [],
            })],
            Vec::new(),
        ));
    }
    let gain_life_re =
        Regex::new(r"(?i)^you gain (\d+) life\.$").expect("gain life regex compiles");
    if let Some(captures) = gain_life_re.captures(instruction) {
        return Some((
            vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(captures[1].parse::<i64>().ok()?),
            })],
            Vec::new(),
        ));
    }
    if instruction == "put that many +1/+1 counters on this creature." {
        return Some((
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": { "kind": "decisionResult", "decisionId": "lifeGainedAmount" },
            })],
            Vec::new(),
        ));
    }
    let library_search_re = Regex::new(
        r"(?i)^(?:you may )?search your library for (.+), reveal it, put it into your hand, then shuffle\.$",
    )
    .expect("triggered library search regex compiles");
    if let Some(captures) = library_search_re.captures(instruction) {
        let mut effects =
            search_library_effects(entry_search_filter(&captures[1])?, 1, "hand", false);
        effects.insert(
            1,
            json!({ "kind": "revealCards", "cards": decision_result("searchedCards") }),
        );
        return Some((effects, Vec::new()));
    }
    if instruction == "you may return target creature card from your graveyard to the battlefield."
    {
        return Some((
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetCreatureCard"),
                "to": "battlefield",
                "tapped": false,
                "controller": controller(),
            })],
            vec![target_decision(
                "targetCreatureCard",
                json!({
                    "kind": "cards",
                    "zone": graveyard(controller()),
                    "where": card_type("Creature"),
                }),
                0,
                1,
            )],
        ));
    }
    if instruction == "double the power of target creature you control until end of turn." {
        let target = chosen_target("targetCreature");
        return Some((
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": target.clone(),
                "power": { "kind": "powerOf", "object": target },
                "toughness": integer(0),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
            vec![target_decision(
                "targetCreature",
                json!({
                    "kind": "permanents",
                    "controller": controller(),
                    "where": card_type("Creature"),
                }),
                1,
                1,
            )],
        ));
    }
    parse_general_effect_sequence(instruction, face_name)
        .or_else(|| parse_general_effect_instruction(instruction, face_name))
        .or_else(|| parse_expansion_counter_instruction(instruction, face_name))
}

pub(in crate::oracle::canonical) fn parse_ordinal_resolution_branches(
    instruction: &str,
    face_name: &str,
) -> Option<Vec<Value>> {
    let ordinal_re = Regex::new(
        r"(?i)^(.+?) if this is the first time this ability has resolved this turn\. If it's the second time, (.+?)\. If it's the third time, (.+)$",
    )
    .expect("ordinal resolution sequence regex compiles");
    let captures = ordinal_re.captures(instruction)?;
    let mut branches = Vec::new();
    for (ordinal, capture_index) in [(1, 1), (2, 2), (3, 3)] {
        let sentence = format!(
            "{}.",
            captures
                .get(capture_index)?
                .as_str()
                .trim()
                .trim_end_matches('.')
        );
        let (effects, decisions) = parse_expansion_instruction(&sentence, face_name)?;
        if !decisions.is_empty() {
            return None;
        }
        branches.push(json!({
            "ordinal": ordinal,
            "effects": effects,
        }));
    }
    Some(branches)
}

pub(in crate::oracle::canonical) fn parse_second_resolution_bonus(
    instruction: &str,
    face_name: &str,
) -> Option<(Vec<Value>, Vec<Value>)> {
    let second_re = Regex::new(
        r"(?is)^(.+?)\. (?:Then )?If this is the second time this ability has resolved this turn, (.+)$",
    )
    .expect("second resolution bonus regex compiles");
    let captures = second_re.captures(instruction)?;
    let base_instruction = format!("{}.", captures.get(1)?.as_str().trim_end_matches('.'));
    let bonus_instruction = format!("{}.", captures.get(2)?.as_str().trim_end_matches('.'));
    let (mut effects, decisions) = parse_expansion_instruction(&base_instruction, face_name)?;
    let (bonus_effects, bonus_decisions) =
        parse_expansion_instruction(&bonus_instruction, face_name)?;
    if !bonus_decisions.is_empty() {
        return None;
    }
    effects.push(json!({
        "kind": "resolveOrdinalTriggeredAbility",
        "id": stable_rule_id("secondResolution", &format!("{face_name}\n{instruction}")),
        "branches": [{ "ordinal": integer(2), "effects": bonus_effects }],
    }));
    Some((effects, decisions))
}

pub(in crate::oracle::canonical) fn parse_expansion_triggered(
    text: &str,
    face_name: &str,
) -> Option<CanonicalRuleDraft> {
    let normalized;
    let text = if let Some((main, _)) = text.split_once(". (") {
        normalized = format!("{main}.");
        normalized.as_str()
    } else {
        text
    };
    let text = strip_short_oracle_label(text);

    let cast_specific_kicker_re = Regex::new(
        r"(?i)^When you cast this spell, if it was kicked with its ((?:\{[^}]+\})+) kicker, (.+)$",
    )
    .expect("cast-specific kicker trigger regex compiles");
    if let Some(captures) = cast_specific_kicker_re.captures(text) {
        let instruction = captures.get(2)?.as_str();
        let direct_target_action = || {
            let zone_change_re = Regex::new(r"(?i)^(Exile|Destroy) target (.+?)\.$")
                .expect("kicker target zone-change regex compiles");
            if let Some(action) = zone_change_re.captures(instruction) {
                return Some((
                    vec![json!({
                        "kind": if action[1].eq_ignore_ascii_case("exile") {
                            "exilePermanent"
                        } else {
                            "destroyPermanent"
                        },
                        "permanent": chosen_target("targetPermanent"),
                    })],
                    vec![target_decision(
                        "targetPermanent",
                        permanent_target_candidates(action.get(2)?.as_str(), face_name)?,
                        1,
                        1,
                    )],
                ));
            }
            let return_re = Regex::new(
                r"(?i)^Return target (.+?) to (?:its owner's hand|their owners' hands)\.$",
            )
            .expect("kicker target return regex compiles");
            let action = return_re.captures(instruction)?;
            Some((
                vec![json!({
                    "kind": "returnToOwnersHand",
                    "object": chosen_target("targetPermanent"),
                })],
                vec![target_decision(
                    "targetPermanent",
                    permanent_target_candidates(action.get(1)?.as_str(), face_name)?,
                    1,
                    1,
                )],
            ))
        };
        let (effects, decisions) = parse_general_effect_sequence(instruction, face_name)
            .or_else(|| parse_general_effect_instruction(instruction, face_name))
            .or_else(direct_target_action)
            .or_else(|| parse_expansion_instruction(instruction, face_name))?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "sourceIsSelf": true,
            },
            "condition": {
                "kind": "kickerCostWasPaid",
                "spell": self_ref(),
                "cost": captures.get(1)?.as_str(),
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Observe this spell being cast",
                "Check the exact kicker cost recorded during casting",
                "Resolve the shared targeted instruction",
            ],
        ));
    }

    if text
        == "Whenever this creature enters or transforms into Sygg, Wanderwine Wisdom, target creature gains \"Whenever this creature deals combat damage to a player or planeswalker, draw a card\" until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "permanentTransformed", "object": self_ref() },
                    ],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "syggWisdomCreature",
                        permanent_target_candidates("creature", face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "installControlledCombatDamageDrawUntilEndOfTurn",
                    "player": {
                        "kind": "controllerOf",
                        "object": chosen_target("syggWisdomCreature"),
                    },
                    "permanent": chosen_target("syggWisdomCreature"),
                    "count": integer(1),
                }],
            }),
            &["Grant the targeted creature its temporary combat-damage draw trigger"],
        ));
    }

    if text
        == "Whenever this creature transforms into Sygg, Wanderbrine Shield, target creature you control gains protection from each color until your next turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentTransformed", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "syggShieldCreature",
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
                    "kind": "grantProtection",
                    "object": chosen_target("syggShieldCreature"),
                    "from": ["white", "blue", "black", "red", "green"],
                    "duration": { "kind": "untilNextTurn", "player": controller() },
                }],
            }),
            &["Grant protection from every color until the controller's next turn"],
        ));
    }

    if text
        == "Whenever Grub attacks, you may blight 1. If you do, create a tapped and attacking token that's a copy of the blighted creature, except it has \"At the beginning of the end step, sacrifice this token.\""
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "grubBlightCopyAttacker",
                }],
            }),
            &["Optionally blight a creature, then copy it tapped and attacking"],
        ));
    }

    if text
        == "Whenever one or more other creatures you control enter, if they entered or were cast from a graveyard, create a token that's a copy of one of them. This ability triggers only once each turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                    "fromZone": "graveyard",
                },
                "triggerLimit": { "kind": "onceEachTurn", "id": "twilightDivinerCopy" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "twilightCopyGraveyardEntrant",
                }],
            }),
            &["Copy one qualifying creature that entered from a graveyard"],
        ));
    }

    if text
        == "Whenever you cast an instant or sorcery spell from your hand, exile that card with a dream counter on it instead of putting it into your graveyard as it resolves."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    "fromZone": "hand",
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "goliathInstallDreamExile",
                }],
            }),
            &["Replace the triggering spell's graveyard destination with dream exile"],
        ));
    }

    if text
        == "Whenever this creature attacks, you may cast a spell from among cards you own in exile with dream counters on them without paying its mana cost."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "goliathCastDream",
                }],
            }),
            &["Optionally cast one owned dream-counter card from exile for free"],
        ));
    }

    if text
        == "When this creature enters, target opponent reveals X cards from their hand, where X is the number of Goblins you control. You choose one of those cards. That player exiles it. If an instant or sorcery card is exiled this way, you may cast it for as long as you control this creature, and mana of any type can be spent to cast that spell."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "tasterOpponent",
                        json!({
                            "kind": "players",
                            "where": { "kind": "isOpponentOf", "player": controller() },
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "tasterOfWaresEnter",
                }],
            }),
            &["Reveal Goblin-count cards, exile one, and link its casting permission"],
        ));
    }

    let counter_trigger = |counter: Option<&str>, operation_name: &str| {
        let mut event = json!({
            "kind": "countersPlaced",
            "player": controller(),
            "where": card_type("Creature"),
            "anyController": true,
        });
        if let Some(counter) = counter {
            event["counter"] = Value::String(counter.to_string());
        }
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": operation_name,
                }],
            }),
            &["Resolve the counter-placement trigger"],
        )
    };
    if text
        == "Whenever one or more -1/-1 counters are put on a creature, draw a card if you control that creature. If you don't control it, its controller loses 1 life."
    {
        return Some(counter_trigger(Some("-1/-1"), "auntieOolCounterPlaced"));
    }
    if text
        == "Whenever one or more -1/-1 counters are put on a creature, put a charge counter on this artifact."
    {
        return Some(counter_trigger(Some("-1/-1"), "wickersmithCounterPlaced"));
    }
    if text
        == "Whenever a -1/-1 counter is put on a creature, you may create a 1/1 green Elf Warrior creature token."
    {
        return Some(counter_trigger(
            Some("-1/-1"),
            "flourishingDefensesCounterPlaced",
        ));
    }

    if text
        == "When this creature dies, if it had one or more -1/-1 counters on it, its owner draws that many cards and each other player loses that much life."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "object": self_ref(),
                    "hadCounter": "-1/-1",
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "oftNabbedGoatDeath",
                }],
            }),
            &["Use the dead creature's -1/-1 counter count for cards and life loss"],
        ));
    }
    if text
        == "Whenever a creature an opponent controls with a counter on it dies, you create a tapped Treasure token."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": { "kind": "opponentsOf", "player": controller() },
                    "where": card_type("Creature"),
                    "hadAnyCounters": true,
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "villagePillagersTreasure",
                }],
            }),
            &["Create one tapped Treasure for the qualifying opposing death"],
        ));
    }
    if text == "Whenever a creature with a -1/-1 counter on it dies, draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                    "hadCounter": "-1/-1",
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &["Draw for a creature dying with a -1/-1 counter"],
        ));
    }
    if text == "When this creature dies, draw a card for each -1/-1 counter on it." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "object": self_ref(),
                    "hadCounter": "-1/-1",
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": { "kind": "triggeringPermanentCounterCount" },
                }],
            }),
            &["Draw according to the dead creature's counter count"],
        ));
    }
    if text
        == "Whenever a creature dies, if it had a -1/-1 counter on it, put a -1/-1 counter on target creature."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                    "hadCounter": "-1/-1",
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "blowflyCreature",
                        permanent_target_candidates("creature", face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": chosen_target("blowflyCreature"),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Move the -1/-1 infestation to a targeted creature"],
        ));
    }
    if text
        == "Whenever a creature an opponent controls with a -1/-1 counter on it dies, you may return that card to the battlefield under your control."
        || text
            == "Whenever a creature an opponent controls with a -1/-1 counter on it dies, you may put that card onto the battlefield under your control. Do this only once each turn."
    {
        let limited = text.contains("only once each turn");
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "permanentDied",
                "player": { "kind": "opponentsOf", "player": controller() },
                "where": card_type("Creature"),
                "hadCounter": "-1/-1",
            },
            "effects": [{
                "kind": "resolveTriggeredInstruction",
                "operation": "returnDeadCreatureUnderControl",
            }],
        });
        if limited {
            rule["triggerLimit"] =
                json!({ "kind": "onceEachTurn", "id": "reaperCounteredCreature" });
        }
        return Some(draft(
            rule,
            &["Optionally return the qualifying opposing creature under your control"],
        ));
    }

    if text
        == "At the beginning of combat on your turn, another target Elemental you control gains myriad until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "player": controller(),
                    "step": "beginCombat",
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "massMysteriesElemental",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": subtype("Elemental"),
                            "excludeSource": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "grantKeyword",
                    "object": chosen_target("massMysteriesElemental"),
                    "keyword": "myriad",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &["Grant myriad to another controlled Elemental for the turn"],
        ));
    }

    if text
        == "When this creature enters, target creature gains flying and \"Whenever this creature deals combat damage to a player, draw that many cards\" until end of turn."
    {
        let target = chosen_target("subterfugeCreature");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "subterfugeCreature",
                        permanent_target_candidates("creature", face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": "flying",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "installControlledCombatDamageDrawUntilEndOfTurn",
                        "player": { "kind": "controllerOf", "object": target.clone() },
                        "permanent": target,
                        "count": integer(1),
                        "useDamageAmount": true,
                    },
                ],
            }),
            &["Grant flying and a damage-sized draw trigger for the turn"],
        ));
    }

    if text == "When The Reaper enters, put a -1/-1 counter on each of up to two target creatures."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "reaperCreatures",
                        permanent_target_candidates("creature", face_name)?,
                        0,
                        2,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "reaperEnterCounters",
                }],
            }),
            &["Put a -1/-1 counter on each selected creature"],
        ));
    }

    if text == "Whenever this creature attacks or blocks, put a -1/-1 counter on it." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "declaredAttacker", "object": self_ref() },
                        { "kind": "declaredBlocker", "object": self_ref() },
                    ],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Put a -1/-1 counter on the attacking or blocking source"],
        ));
    }

    if text
        == "Whenever a creature an opponent controls dies, if its toughness was less than 1, draw a card."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": { "kind": "opponentsOf", "player": controller() },
                    "where": card_type("Creature"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "massacreGirlKnownKillerDeath",
                }],
            }),
            &["Draw only when the opposing creature died below one toughness"],
        ));
    }

    if text
        == "When this creature enters, put target creature card from an opponent's graveyard onto the battlefield under your control. It gains haste. At the beginning of your next end step, exile it."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "puppeteerCreature",
                        json!({
                            "kind": "cards",
                            "zone": { "kind": "anyGraveyard" },
                            "owner": { "kind": "opponentsOf", "player": controller() },
                            "where": card_type("Creature"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "puppeteerCliqueEnter",
                }],
            }),
            &["Reanimate the opposing creature with haste and delayed exile"],
        ));
    }

    if text
        == "When The Scorpion God dies, return it to its owner's hand at the beginning of the next end step."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "scorpionGodDelayedReturn",
                }],
            }),
            &["Schedule the dead source to return to its owner's hand"],
        ));
    }

    if text == "Whenever you draw a card, you may put a hoofprint counter on this enchantment." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "cardDrawn", "player": controller() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "hoofprintsDraw",
                }],
            }),
            &["Optionally put a hoofprint counter on the source"],
        ));
    }

    if text == "At the beginning of your upkeep, put a -1/-1 counter on each nonblack creature."
        || text == "When this creature enters, put a -1/-1 counter on each creature."
    {
        let event = if text.starts_with("At the beginning") {
            json!({ "kind": "stepBegan", "player": controller(), "step": "upkeep" })
        } else {
            json!({ "kind": "enterBattlefield", "object": self_ref() })
        };
        let where_filter = if text.contains("nonblack") {
            and(vec![
                card_type("Creature"),
                not(json!({ "kind": "colorContains", "value": "black" })),
            ])
        } else {
            card_type("Creature")
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "where": where_filter,
                    },
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &["Put a -1/-1 counter on every matching creature"],
        ));
    }

    if text == "When this creature enters, you may destroy target artifact or enchantment." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "foundationBreakerTarget",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![
                                card_type("Artifact"),
                                card_type("Enchantment"),
                            ]),
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "destroyPermanent",
                    "permanent": chosen_target("foundationBreakerTarget"),
                }],
            }),
            &["Optionally destroy the targeted artifact or enchantment"],
        ));
    }

    if text
        == "Whenever Hapatra deals combat damage to a player, you may put a -1/-1 counter on target creature."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "combatDamageToPlayer", "source": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "hapatraCreature",
                        permanent_target_candidates("creature", face_name)?,
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "hapatraCombatCounter",
                }],
            }),
            &["Optionally put a -1/-1 counter on the targeted creature"],
        ));
    }

    if text
        == "When this creature enters, create a 0/1 green Plant creature token for each land you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "createTokens",
                    "quantity": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                    },
                    "token": {
                        "name": "Plant Token",
                        "colors": ["green"],
                        "types": ["Creature"],
                        "subtypes": ["Plant"],
                        "power": 0,
                        "toughness": 1,
                        "abilities": [],
                    },
                }],
            }),
            &["Create a Plant for each controlled land"],
        ));
    }

    if text
        == "Whenever this creature or another Elemental you control enters, look at the top card of your library. If it's a land card, you may put it onto the battlefield tapped. If you don't put the card onto the battlefield, put it into your hand."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": subtype("Elemental"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "risenReefEnter",
                }],
            }),
            &["Inspect the top card, optionally put a land in tapped, otherwise take it"],
        ));
    }

    if text
        == "When Ferrafor enters, create a number of 1/1 green Saproling creature tokens equal to the number of counters among creatures target player controls."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "ferraforPlayer",
                        json!({ "kind": "players" }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "ferraforEnterSaprolings",
                }],
            }),
            &["Count counters on the targeted player's creatures and create Saprolings"],
        ));
    }

    if text
        == "At the beginning of your end step, if you're the monarch, return up to one target creature card from your graveyard to your hand."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "player": controller(), "step": "endStep" },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "graveVenerationsCreature",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": card_type("Creature"),
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "graveVenerationsEndStep",
                }],
            }),
            &["If the controller is monarch, return the targeted creature card"],
        ));
    }

    if text
        == "Whenever a creature with a -1/-1 counter on it dies, you may put a land card from your hand or graveyard onto the battlefield tapped."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                    "hadCounter": "-1/-1",
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "dreadTillerLand",
                }],
            }),
            &["Optionally put a land from hand or graveyard onto the battlefield tapped"],
        ));
    }

    if text
        == "When this creature leaves the battlefield, choose an opponent. If that player has more cards in hand than you, draw cards equal to the difference."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentLeftBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "slithermuseLeaves",
                }],
            }),
            &["Choose an opponent and draw the positive hand-size difference"],
        ));
    }

    if text
        == "When this creature dies, you may exile it. If you do, return target card from your graveyard to your hand."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "greenwardenCard",
                        json!({
                            "kind": "cards",
                            "zone": graveyard(controller()),
                            "where": Value::Null,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "greenwardenDeath",
                }],
            }),
            &["Optionally exile the dead source to return another graveyard card"],
        ));
    }

    if text
        == "Whenever this creature enters or attacks, create a green and white Elemental creature token with \"This token's power and toughness are each equal to the number of creatures you control.\""
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "declaredAttacker", "object": self_ref() },
                    ],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "vernalSovereignToken",
                }],
            }),
            &["Create an Elemental whose power and toughness track controlled creatures"],
        ));
    }

    if text == "At the beginning of your end step, discover 5." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "player": controller(), "step": "endStep" },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "chimilDiscoverFive",
                }],
            }),
            &["Discover five and randomize the skipped cards onto the bottom"],
        ));
    }
    let countered_creature_dies_impulse_re = Regex::new(
        r"(?i)^Whenever a creature you control with one or more counters on it dies, exile that many cards from the top of your library\. Until your next end step, you may play those cards\.$",
    )
    .expect("countered-creature death impulse regex compiles");
    if countered_creature_dies_impulse_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "hadAnyCounters": true,
                },
                "effects": [
                    {
                        "kind": "exileTopCards",
                        "zone": library(controller()),
                        "count": { "kind": "triggeringPermanentCounterCount" },
                        "faceDown": false,
                        "bind": "cardsExiledForCounteredCreature",
                    },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "play",
                            "card": {
                                "kind": "boundObject",
                                "binding": "cardsExiledForCounteredCreature",
                            },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "untilNextEndStep" },
                    },
                ],
            }),
            &[
                "Observe a controlled creature dying with counters",
                "Exile one card per counter it had",
                "Permit those cards through the controller's next end step",
            ],
        ));
    }
    if text
        == "At the beginning of your end step, if you gained life this turn, exile cards from the top of your library until you exile a nonland card. You may cast that card without paying its mana cost if the spell's mana value is less than or equal to the amount of life you gained this turn. Otherwise, put it into your hand."
    {
        let stopped_card = json!({
            "kind": "boundObject",
            "binding": "breExiledNonlandCard",
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "condition": compare(
                    ">",
                    json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
                    integer(0),
                ),
                "effects": [
                    {
                        "kind": "exileFromTopUntil",
                        "zone": library(controller()),
                        "bind": "breExiledCards",
                        "stopCardBind": "breExiledNonlandCard",
                        "faceDown": false,
                        "stopWhere": {
                            "kind": "not",
                            "operand": card_type("Land"),
                        },
                        "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": compare(
                            "<=",
                            json!({ "kind": "manaValueOf", "object": stopped_card }),
                            json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
                        ),
                        "then": [{
                            "kind": "castAnyNumber",
                            "player": controller(),
                            "cards": bound_objects("breExiledNonlandCard"),
                            "where": { "kind": "canBeCastAsSpell" },
                            "timing": { "kind": "duringResolution" },
                            "withoutPayingManaCost": true,
                            "alternativeCostsAllowed": false,
                            "additionalCostsApply": true,
                            "variableManaValue": 0,
                            "maximum": integer(1),
                        }],
                        "else": [{
                            "kind": "moveCards",
                            "cards": bound_objects("breExiledNonlandCard"),
                            "to": hand(controller()),
                        }],
                    },
                ],
            }),
            &[
                "Check life gained during the turn",
                "Exile through the first nonland card",
                "Cast it for free when affordable by life gained, otherwise put it into hand",
            ],
        ));
    }
    if text
        == "When this creature enters, reveal cards from the top of your library until you reveal X permanent cards, where X is the number of colors among permanents you control. Put any number of those permanent cards onto the battlefield, then put the rest of the revealed cards on the bottom of your library in a random order."
    {
        let revealed = bound_objects("vividRevealedCards");
        let chosen = decision_result("vividPermanentsForBattlefield");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [
                    {
                        "kind": "revealFromTopUntilCount",
                        "zone": library(controller()),
                        "count": {
                            "kind": "countDistinctColors",
                            "player": controller(),
                            "where": Value::Null,
                        },
                        "where": { "kind": "isPermanentCard" },
                        "bind": "vividRevealedCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "vividPermanentsForBattlefield",
                        "player": controller(),
                        "from": revealed.clone(),
                        "where": { "kind": "isPermanentCard" },
                        "minimum": integer(0),
                        "maximum": count_bound_objects("vividRevealedCards"),
                    },
                    {
                        "kind": "moveCards",
                        "cards": chosen.clone(),
                        "to": {
                            "kind": "battlefield",
                            "player": controller(),
                            "tapped": false,
                        },
                    },
                    {
                        "kind": "moveCards",
                        "cards": {
                            "kind": "setDifference",
                            "left": revealed,
                            "right": chosen,
                        },
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "bottom",
                        },
                        "order": { "kind": "random" },
                    },
                ],
            }),
            &[
                "Count colors among controlled permanents",
                "Reveal through that many permanent cards",
                "Choose any of those permanents for the battlefield",
                "Randomize the remainder onto the library bottom",
            ],
        ));
    }
    if text
        == "At the beginning of your first main phase, reveal cards from the top of your library until you reveal X nonland cards, where X is the number of colors among permanents you control. For each of those colors, you may exile a card of that color from among the revealed cards. Then shuffle. You may cast the exiled cards this turn."
    {
        let color_count = json!({
            "kind": "countDistinctColors",
            "player": controller(),
            "where": Value::Null,
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "precombatMain",
                    "player": controller(),
                },
                "effects": [
                    {
                        "kind": "revealFromTopUntilCount",
                        "zone": library(controller()),
                        "count": color_count.clone(),
                        "where": {
                            "kind": "not",
                            "operand": card_type("Land"),
                        },
                        "bind": "sanarRevealedCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "sanarCardsForExile",
                        "player": controller(),
                        "from": bound_objects("sanarRevealedCards"),
                        "minimum": integer(0),
                        "maximum": color_count,
                        "onePerControlledColor": true,
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("sanarCardsForExile"),
                        "to": {
                            "kind": "exile",
                            "player": controller(),
                            "faceDown": false,
                        },
                    },
                    { "kind": "shuffleZone", "zone": library(controller()) },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "cast",
                            "card": {
                                "kind": "decisionResult",
                                "decisionId": "sanarCardsForExile",
                            },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Count colors among controlled permanents",
                "Reveal through that many nonland cards",
                "Choose at most one revealed card for each controlled color",
                "Exile the choices, shuffle, and permit casting them this turn",
            ],
        ));
    }
    if text
        == "When this artifact enters and at the beginning of your upkeep, look at the top card of your library. If it's a card of the chosen type, you may reveal it and put it into your hand. If you don't put the card into your hand, you may put it into your graveyard."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "stepBegan", "step": "upkeep", "player": controller() },
                    ],
                },
                "effects": [{
                    "kind": "inspectTopCardForChosenCreatureType",
                    "player": controller(),
                    "decisionId": "chosenCreatureType",
                }],
            }),
            &[
                "Inspect the top card on entry and each upkeep",
                "Offer a matching chosen-type card for reveal and hand",
                "Otherwise optionally move the inspected card to the graveyard",
            ],
        ));
    }
    if text
        == "Whenever this creature enters or transforms into Ashling, Rekindled, you may discard a card. If you do, draw a card."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "permanentTransformed", "object": self_ref() },
                    ],
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "discardCards",
                        "player": controller(),
                        "count": integer(1),
                    },
                    "onPerformed": [{
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    }],
                }],
            }),
            &[
                "Offer one optional discard",
                "Draw only when the discard is performed",
            ],
        ));
    }
    if text
        == "Whenever this creature attacks, you may put a creature card with mana value X or less from your hand onto the battlefield tapped and attacking, where X is the number of attacking creatures you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "effects": [{
                    "kind": "putCreatureFromHandOntoBattlefieldAttacking",
                    "player": controller(),
                    "maximumManaValue": {
                        "kind": "countAttackingCreatures",
                        "player": controller(),
                    },
                }],
            }),
            &[
                "Count the attacking creatures controlled by the source controller",
                "Optionally deploy an eligible creature tapped and attacking the same defender",
            ],
        ));
    }
    if text
        == "Whenever this creature attacks, you may tap another untapped creature you control. If you do, this creature can't be blocked this turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "tap",
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    },
                    "effects": [{
                        "kind": "grantKeyword",
                        "object": self_ref(),
                        "keyword": "cantBeBlocked",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                }],
            }),
            &[
                "Trigger when the source attacks",
                "Offer tapping another controlled creature",
                "Make the source unblockable when paid",
            ],
        ));
    }
    if text
        == "When this Aura enters, enchanted creature fights up to one target creature an opponent controls."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "fightTarget",
                        json!({
                            "kind": "permanents",
                            "controller": { "kind": "opponentsOf", "player": controller() },
                            "where": card_type("Creature"),
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "fightPermanents",
                    "first": { "kind": "attachedPermanent", "attachment": self_ref() },
                    "second": chosen_target("fightTarget"),
                }],
            }),
            &[
                "Choose up to one opposing creature",
                "Have the enchanted creature fight it",
            ],
        ));
    }
    if text
        == "When this creature enters, up to X target creatures you control get +X/+X until end of turn, where X is the number of colors among permanents you control."
    {
        let x = json!({
            "kind": "countDistinctColors",
            "player": controller(),
            "where": Value::Null,
        });
        let mut decision = target_decision(
            "targetCreatures",
            json!({
                "kind": "permanents",
                "controller": controller(),
                "where": card_type("Creature"),
            }),
            0,
            0,
        );
        decision["maximum"] = x.clone();
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": { "kind": "castingDeclaration", "decisions": [decision] },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": { "kind": "chosenTargets", "id": "targetCreatures" },
                    "power": x.clone(),
                    "toughness": x,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Count controlled permanent colors",
                "Choose up to that many creatures",
                "Give them the vivid bonus",
            ],
        ));
    }
    if text
        == "When this enchantment enters, search your library for up to X basic land cards, where X is the number of colors among permanents you control. Reveal those cards, put them into your hand, then shuffle."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "searchLibrary",
                    "player": controller(),
                    "where": and(vec![
                        json!({ "kind": "typeLineContains", "value": "Basic" }),
                        card_type("Land"),
                    ]),
                    "maximum": {
                        "kind": "countDistinctColors",
                        "player": controller(),
                        "where": Value::Null,
                    },
                    "destination": "hand",
                    "tapped": false,
                }],
            }),
            &[
                "Count controlled permanent colors",
                "Find up to that many basic lands",
                "Put them into hand and shuffle",
            ],
        ));
    }
    if text
        == "At the beginning of combat on your turn, another target creature you control gets +X/+X until end of turn, where X is the number of colors among permanents you control."
    {
        let x = json!({
            "kind": "countDistinctColors",
            "player": controller(),
            "where": Value::Null,
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "beginCombat", "player": controller() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Creature"),
                            "excludeSource": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": x.clone(),
                    "toughness": x,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Trigger at beginning of combat",
                "Count controlled permanent colors",
                "Give another creature the vivid bonus",
            ],
        ));
    }
    if text == "When this creature enters, create a tapped Mutavault token." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "tapped": true,
                    "token": {
                        "name": "Mutavault",
                        "colors": [],
                        "types": ["Land"],
                        "subtypes": [],
                        "power": integer(0),
                        "toughness": integer(0),
                        "abilities": [
                            {
                                "kind": "activatedAbility",
                                "source": self_ref(),
                                "costs": [{ "kind": "tap", "object": self_ref() }],
                                "effects": [{
                                    "kind": "addMana",
                                    "player": controller(),
                                    "mana": "{C}",
                                }],
                            },
                            {
                                "kind": "activatedAbility",
                                "source": self_ref(),
                                "costs": [{ "kind": "payMana", "manaCost": "{1}" }],
                                "effects": [
                                    {
                                        "kind": "becomeCreature",
                                        "object": self_ref(),
                                        "addTypes": ["Creature"],
                                        "addSubtypes": [],
                                        "addColors": [],
                                        "basePower": integer(2),
                                        "baseToughness": integer(2),
                                        "retainExistingTypes": true,
                                        "duration": { "kind": "untilEndOfCurrentTurn" },
                                    },
                                    {
                                        "kind": "grantKeyword",
                                        "object": self_ref(),
                                        "keyword": "changeling",
                                        "duration": { "kind": "untilEndOfCurrentTurn" },
                                    },
                                ],
                            },
                        ],
                    },
                }],
            }),
            &[
                "Create Mutavault tapped",
                "Give it its mana ability",
                "Give it its animation ability",
            ],
        ));
    }
    if text
        == "When this artifact enters, draw a card, then choose a color. This artifact becomes the chosen color."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    },
                    { "kind": "chooseAndSetSourceColor" },
                ],
            }),
            &[
                "Draw a card",
                "Choose a color",
                "Set the artifact to the chosen color",
            ],
        ));
    }
    if text
        == "When this creature dies, if it had a -1/-1 counter on it, return it to the battlefield under its owner's control and it loses all abilities."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "object": self_ref(),
                    "hadCounter": "-1/-1",
                },
                "effects": [
                    {
                        "kind": "moveTriggeringCardFromGraveyard",
                        "to": "battlefield",
                    },
                    {
                        "kind": "loseAllAbilitiesPermanently",
                        "object": { "kind": "abilitySource" },
                    },
                ],
            }),
            &[
                "Check the dying creature for a -1/-1 counter",
                "Return it to its owner's battlefield",
                "Remove its abilities",
            ],
        ));
    }
    if text
        == "At the beginning of your first main phase, you may blight 2. If you don't, you lose 3 life."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "precombatMain", "player": controller() },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "putCounters",
                        "where": card_type("Creature"),
                        "counter": "-1/-1",
                        "count": integer(2),
                    },
                    "effects": [],
                    "declineEffects": [{
                        "kind": "loseLife",
                        "player": controller(),
                        "amount": integer(3),
                    }],
                }],
            }),
            &["Offer blight 2", "Lose three life if blight is declined"],
        ));
    }
    if text == "When this creature enters, you may pay {2}. If you don't, blight 2." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": { "kind": "payMana", "manaCost": "{2}" },
                    "effects": [],
                    "declineEffects": [{
                        "kind": "blightPlayer",
                        "player": controller(),
                        "count": integer(2),
                    }],
                }],
            }),
            &[
                "Offer payment of two mana",
                "Blight 2 if payment is declined",
            ],
        ));
    }
    if text
        == "Whenever this creature transforms into Ashling, Rimebound and at the beginning of your first main phase, add two mana of any one color. Spend this mana only to cast spells with mana value 4 or greater."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "permanentTransformed", "object": self_ref() },
                        { "kind": "stepBegan", "step": "precombatMain", "player": controller() },
                    ],
                },
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": { "kind": "chooseColor", "amount": integer(2) },
                    "spendRestriction": {
                        "kind": "castSpell",
                        "where": compare(
                            ">=",
                            json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                            integer(4),
                        ),
                    },
                }],
            }),
            &[
                "Trigger on transformation or the first main phase",
                "Add two same-color mana",
                "Restrict it to large spells",
            ],
        ));
    }
    if text
        == "When this creature enters, target creature you control gets +X/+0 until end of turn and up to one target creature an opponent controls gets -0/-X until end of turn, where X is the number of Elves you control plus the number of Elf cards in your graveyard."
    {
        let x = json!({
            "kind": "add",
            "left": {
                "kind": "countPermanents",
                "player": controller(),
                "where": subtype("Elf"),
            },
            "right": {
                "kind": "countCards",
                "zone": graveyard(controller()),
                "where": subtype("Elf"),
            },
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision(
                            "friendlyCreature",
                            json!({
                                "kind": "permanents",
                                "controller": controller(),
                                "where": card_type("Creature"),
                            }),
                            1,
                            1,
                        ),
                        target_decision(
                            "opposingCreature",
                            json!({
                                "kind": "permanents",
                                "controller": { "kind": "opponentsOf", "player": controller() },
                                "where": card_type("Creature"),
                            }),
                            0,
                            1,
                        ),
                    ],
                },
                "effects": [
                    {
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("friendlyCreature"),
                        "power": x.clone(),
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("opposingCreature"),
                        "power": integer(0),
                        "toughness": { "kind": "multiply", "value": x, "factor": integer(-1) },
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Count Elves on the battlefield and in the graveyard",
                "Increase a controlled creature's power",
                "Optionally reduce an opposing creature's toughness",
            ],
        ));
    }
    if text
        == "Whenever a creature you control attacks alone, it gets +X/+X until end of turn, where X is the number of Kithkin you control."
    {
        let x = json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": subtype("Kithkin"),
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreatureDeclaredAttacker",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "condition": {
                    "kind": "controlledCreatureAttackingAlone",
                    "player": controller(),
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": { "kind": "triggeringPermanent" },
                    "power": x.clone(),
                    "toughness": x,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Trigger for the lone controlled attacker",
                "Count controlled Kithkin",
                "Apply the matching power and toughness bonus",
            ],
        ));
    }
    if text
        == "Whenever a creature you control attacks or blocks, it gets +X/+X until end of turn, where X is the difference between its power and toughness."
    {
        let triggering = json!({ "kind": "triggeringPermanent" });
        let difference = json!({
            "kind": "absoluteDifference",
            "left": { "kind": "powerOf", "object": triggering.clone() },
            "right": { "kind": "toughnessOf", "object": triggering.clone() },
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "controlledCreatureDeclaredAttacker",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                        {
                            "kind": "controlledCreatureDeclaredBlocker",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                    ],
                },
                "effects": [{
                    "kind": "modifyPowerToughness",
                    "object": triggering,
                    "power": difference.clone(),
                    "toughness": difference,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Trigger when a controlled creature attacks or blocks",
                "Take the absolute difference between its power and toughness",
                "Apply that amount as a temporary bonus",
            ],
        ));
    }
    if text
        == "At the beginning of your end step, you may remove a counter from this creature. When you do, return target creature card with power 2 or less from your graveyard to the battlefield."
    {
        let (effects, decisions) = parse_general_effect_instruction(
            "Return target creature card with power 2 or less from your graveyard to the battlefield.",
            face_name,
        )?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "endStep",
                    "player": controller(),
                },
                "effects": [{
                    "kind": "optionalPayCostCreateReflexiveTrigger",
                    "player": controller(),
                    "cost": {
                        "kind": "removeCounters",
                        "permanent": self_ref(),
                        "counter": "any",
                        "count": integer(1),
                    },
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": decisions,
                        },
                        "effects": effects,
                    },
                }],
            }),
            &[
                "Offer removal of any counter from the source",
                "Create the reflexive reanimation trigger when paid",
                "Return a creature card with power two or less",
            ],
        ));
    }
    if text.eq_ignore_ascii_case(
        "Whenever this creature becomes tapped while it has a -1/-1 counter on it, remove a -1/-1 counter from it.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentTapped", "object": self_ref() },
                "condition": {
                    "kind": "compare",
                    "operator": ">",
                    "left": {
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": "-1/-1",
                    },
                    "right": integer(0),
                },
                "effects": [{
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &[
                "Trigger when the source becomes tapped",
                "Require a -1/-1 counter on the source",
                "Remove one such counter",
            ],
        ));
    }
    if text
        == "When this creature enters, each creature target opponent controls loses all abilities, becomes a Coward in addition to its other types, and has base power and toughness 1/1."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
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
                "effects": [{
                    "kind": "makeCreaturesOfPlayerCowardsPermanently",
                    "player": chosen_target("targetOpponent"),
                }],
            }),
            &[
                "Target an opponent",
                "Remove abilities from that player's creatures",
                "Make those creatures 1/1 Cowards",
            ],
        ));
    }
    if text
        == "When this creature enters, choose up to one other target creature. Until end of turn, that creature has base power and toughness 4/4 and gains all creature types."
    {
        let target = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "where": card_type("Creature"),
                            "excludeSource": true,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "setBasePowerToughness",
                        "object": target.clone(),
                        "power": integer(4),
                        "toughness": integer(4),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "grantKeyword",
                        "object": target,
                        "keyword": "changeling",
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Choose up to one other creature",
                "Set its base power and toughness",
                "Grant all creature types until end of turn",
            ],
        ));
    }
    if text
        == "At the beginning of each player's draw step, that player loses 3 life, searches their library for a card, puts it into their hand, then shuffles."
    {
        let triggering_player = json!({ "kind": "triggeringPlayer" });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "draw",
                    "player": { "kind": "eachPlayer" },
                },
                "effects": [
                    {
                        "kind": "loseLife",
                        "player": triggering_player.clone(),
                        "amount": integer(3),
                    },
                    {
                        "kind": "searchLibrary",
                        "player": triggering_player,
                        "where": Value::Null,
                        "maximum": integer(1),
                        "destination": "hand",
                        "tapped": false,
                    },
                ],
            }),
            &[
                "Observe each player's draw step",
                "Make that player lose 3 life",
                "Let that player search for a card and shuffle",
            ],
        ));
    }
    if text
        == "When Abigale enters, up to one other target creature loses all abilities. Put a flying counter, a first strike counter, and a lifelink counter on that creature."
    {
        let target = chosen_target("targetCreature");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({
                            "kind": "permanents",
                            "where": card_type("Creature"),
                            "excludeSource": true,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "loseAllAbilitiesPermanently",
                        "object": target.clone(),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": target.clone(),
                        "counter": "flying",
                        "count": integer(1),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": target.clone(),
                        "counter": "first strike",
                        "count": integer(1),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": target,
                        "counter": "lifelink",
                        "count": integer(1),
                    },
                ],
            }),
            &[
                "Observe Abigale entering",
                "Choose up to one other creature",
                "Remove its abilities and put the three keyword counters on it",
            ],
        ));
    }
    if text
        == "When this creature enters, if you cast it, return all non-Elemental creatures to their owners' hands."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "condition": { "kind": "wasCast", "object": self_ref() },
                "effects": [{
                    "kind": "returnPermanentsToOwnersHands",
                    "where": and(vec![
                        card_type("Creature"),
                        not(subtype("Elemental")),
                    ]),
                }],
            }),
            &[
                "Observe the cast creature entering",
                "Require that the source was cast",
                "Return every non-Elemental creature to its owner's hand",
            ],
        ));
    }
    if text
        == "Whenever this creature enters or becomes tapped, tap up to one target creature and put a stun counter on it."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "permanentTapped", "object": self_ref() },
                    ],
                },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetCreature",
                        json!({ "kind": "permanents", "where": card_type("Creature") }),
                        0,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "tapPermanent",
                        "permanent": chosen_target("targetCreature"),
                    },
                    {
                        "kind": "putCounters",
                        "permanent": chosen_target("targetCreature"),
                        "counter": "stun",
                        "count": integer(1),
                    },
                ],
            }),
            &[
                "Observe the source entering or becoming tapped",
                "Choose up to one target creature",
                "Tap it and put a stun counter on it",
            ],
        ));
    }
    if matches!(
        text,
        "When this creature enters, create a Treasure token."
            | "When this creature enters or dies, create a Treasure token."
    ) {
        let event = if text.contains("enters or dies") {
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    { "kind": "permanentDied", "object": self_ref() },
                ],
            })
        } else {
            json!({ "kind": "enterBattlefield", "object": self_ref() })
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [create_token_effect("Create a Treasure token.")?],
            }),
            &[
                "Observe the source entering or dying",
                "Create a Treasure token",
            ],
        ));
    }
    if text
        == "At the beginning of your first main phase, you may blight 1. If you do, create a Treasure token."
    {
        let treasure = create_token_effect("Create a Treasure token.")?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "precombatMain",
                    "player": controller(),
                },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "putCounters",
                        "controller": controller(),
                        "where": card_type("Creature"),
                        "counter": "-1/-1",
                        "count": integer(1),
                    },
                    "effects": [treasure],
                }],
            }),
            &[
                "Observe the controller's first main phase",
                "Offer blight 1 as an optional resolution cost",
                "Create a Treasure when that cost is paid",
            ],
        ));
    }
    let enter_optional_blight_tokens_re = Regex::new(
        r"(?i)^When this (?:creature|enchantment) enters, you may blight (\d+)\. If you do, create (two|\d+) 1/1 black and red Goblin creature tokens\.$",
    )
    .expect("enter optional blight Goblin tokens regex compiles");
    if let Some(captures) = enter_optional_blight_tokens_re.captures(text) {
        let token_count = if captures[2].eq_ignore_ascii_case("two") {
            2
        } else {
            captures[2].parse::<i64>().ok()?
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "putCounters",
                        "controller": controller(),
                        "where": card_type("Creature"),
                        "counter": "-1/-1",
                        "count": integer(captures[1].parse::<i64>().ok()?),
                    },
                    "effects": [{
                        "kind": "createTokens",
                        "controller": controller(),
                        "quantity": integer(token_count),
                        "token": {
                            "colors": ["black", "red"],
                            "types": ["Creature"],
                            "subtypes": ["Goblin"],
                            "power": 1,
                            "toughness": 1,
                            "abilities": [],
                        },
                    }],
                }],
            }),
            &[
                "Offer blight as an optional resolution cost",
                "Create Goblin tokens when paid",
            ],
        ));
    }
    if text
        == "When this creature enters, you may blight 1. If you do, each opponent discards a card."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "putCounters",
                        "controller": controller(),
                        "where": card_type("Creature"),
                        "counter": "-1/-1",
                        "count": integer(1),
                    },
                    "effects": [{
                        "kind": "discardCards",
                        "player": { "kind": "opponentsOf", "player": controller() },
                        "count": integer(1),
                    }],
                }],
            }),
            &[
                "Offer blight 1 as an optional resolution cost",
                "Each opponent discards when paid",
            ],
        ));
    }
    if text
        == "Whenever this creature enters or attacks, you may blight 2. If you do, you draw a card and lose 1 life."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "declaredAttacker", "object": self_ref() },
                    ],
                },
                "effects": [{
                    "kind": "optionalPayCostPerformEffects",
                    "player": controller(),
                    "cost": {
                        "kind": "putCounters",
                        "controller": controller(),
                        "where": card_type("Creature"),
                        "counter": "-1/-1",
                        "count": integer(2),
                    },
                    "effects": [
                        {
                            "kind": "drawCards",
                            "player": controller(),
                            "count": integer(1),
                        },
                        {
                            "kind": "loseLife",
                            "player": controller(),
                            "amount": integer(1),
                        },
                    ],
                }],
            }),
            &[
                "Trigger on entering or attacking",
                "Offer blight 2",
                "Draw a card and lose one life when paid",
            ],
        ));
    }
    if text
        == "Whenever another Elemental you control enters, it deals damage equal to its power to each opponent."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": subtype("Elemental"),
                    "excludeSource": true,
                },
                "effects": [{
                    "kind": "dealDamageToEachOpponent",
                    "amount": {
                        "kind": "powerOf",
                        "object": { "kind": "triggeringPermanent" },
                    },
                    "source": { "kind": "triggeringPermanent" },
                }],
            }),
            &[
                "Observe another controlled Elemental entering",
                "Read that Elemental's current power",
                "Deal that much damage to each opponent",
            ],
        ));
    }
    if text
        == "Whenever you cast a spell during an opponent's turn, you may pay 1 life. If you do, draw a card."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": Value::Null,
                    "duringOpponentTurn": true,
                },
                "effects": [{
                    "kind": "optionalPayLife",
                    "player": controller(),
                    "amount": integer(1),
                    "effects": [{
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    }],
                }],
            }),
            &[
                "Observe a spell cast during an opponent's turn",
                "Offer the optional life payment",
                "Draw a card when the payment is made",
            ],
        ));
    }
    if text
        == "When Lluwen enters, mill four cards, then you may put a creature or land card from among the milled cards on top of your library."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [
                    {
                        "kind": "mill",
                        "player": controller(),
                        "count": integer(4),
                        "bind": "milledCards",
                    },
                    {
                        "kind": "chooseCards",
                        "id": "milledCardForLibrary",
                        "player": controller(),
                        "from": bound_objects("milledCards"),
                        "where": or(vec![card_type("Creature"), card_type("Land")]),
                        "minimum": integer(0),
                        "maximum": integer(1),
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("milledCardForLibrary"),
                        "to": {
                            "kind": "library",
                            "player": controller(),
                            "position": "top",
                        },
                    },
                ],
            }),
            &[
                "Observe Lluwen entering",
                "Mill four cards and bind them",
                "Optionally choose a creature or land among them",
                "Put the chosen card on top of its controller's library",
            ],
        ));
    }
    let source_counter_condition_re = Regex::new(
        r"(?i)^(Whenever .+?) while (?:this (?:creature|permanent)|it) has a ([^ ]+) counter on it, (.+)$",
    )
    .expect("source-counter trigger condition regex compiles");
    if let Some(captures) = source_counter_condition_re.captures(text) {
        let normalized_trigger = format!(
            "{}, {}",
            captures.get(1)?.as_str(),
            captures.get(3)?.as_str()
        );
        let (event, instruction) = parse_expansion_trigger_event(&normalized_trigger, face_name)?;
        let (effects, decisions) = parse_expansion_instruction(instruction, face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "condition": {
                "kind": "objectMatchesFilter",
                "object": self_ref(),
                "where": {
                    "kind": "hasCounter",
                    "counter": captures.get(2)?.as_str().to_ascii_lowercase(),
                },
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Parse the underlying trigger event",
                "Require the source counter at trigger resolution",
                "Resolve the shared instruction leaves",
            ],
        ));
    }
    let chosen_color_cast_re =
        Regex::new(r"(?i)^Whenever you cast a spell of the chosen color, (.+)$")
            .expect("chosen-color spell-cast trigger regex compiles");
    if let Some(captures) = chosen_color_cast_re.captures(text) {
        let (effects, decisions) =
            parse_general_effect_instruction(captures.get(1)?.as_str(), face_name)?;
        if !decisions.is_empty() {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": Value::Null,
                    "colorDecisionId": "chosenColor",
                },
                "effects": effects,
            }),
            &[
                "Observe a spell cast by the source controller",
                "Compare its colors with the stored source color",
                "Resolve the shared effect instruction",
            ],
        ));
    }
    let shared_effect_alternative_events_re =
        Regex::new(r"(?i)^Whenever (.+?) and whenever (.+?), (.+)$")
            .expect("alternative trigger events with shared effect regex compiles");
    if let Some(captures) = shared_effect_alternative_events_re.captures(text) {
        let instruction = captures.get(3)?.as_str();
        let first_text = format!("Whenever {}, {instruction}", captures.get(1)?.as_str());
        let second_text = format!("Whenever {}, {instruction}", captures.get(2)?.as_str());
        let (first_event, first_instruction) =
            parse_expansion_trigger_event(&first_text, face_name)?;
        let (second_event, second_instruction) =
            parse_expansion_trigger_event(&second_text, face_name)?;
        if first_instruction != instruction || second_instruction != instruction {
            return None;
        }
        let (effects, decisions) = parse_expansion_instruction(instruction, face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "oneOf", "events": [first_event, second_event] },
            "effects": effects,
        });
        if rule["effects"].as_array().is_some_and(|effects| {
            effects.iter().any(|effect| {
                matches!(
                    effect["kind"].as_str(),
                    Some("moveAbilitySourceToHand" | "moveAbilitySourceToBattlefield")
                )
            })
        }) {
            rule["triggerZone"] = Value::String("graveyard".to_string());
        }
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Parse each alternative trigger event independently",
                "Share one declaration across both events",
                "Resolve the common effect instruction",
            ],
        ));
    }
    let entry_and_repeat_event_re = Regex::new(r"(?i)^When (.+? enters) and whenever (.+?), (.+)$")
        .expect("entry and repeated alternative trigger events regex compiles");
    if let Some(captures) = entry_and_repeat_event_re.captures(text) {
        let instruction = captures.get(3)?.as_str();
        let entry_text = format!("When {}, {instruction}", captures.get(1)?.as_str());
        let repeat_text = format!("Whenever {}, {instruction}", captures.get(2)?.as_str());
        let (entry_event, entry_instruction) =
            parse_expansion_trigger_event(&entry_text, face_name)?;
        let (repeat_event, repeat_instruction) =
            parse_expansion_trigger_event(&repeat_text, face_name)?;
        if entry_instruction != instruction || repeat_instruction != instruction {
            return None;
        }
        let (effects, decisions) = parse_expansion_instruction(instruction, face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "oneOf", "events": [entry_event, repeat_event] },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Parse the source entry and repeat event independently",
                "Share the effect instruction across both events",
            ],
        ));
    }
    let while_condition_re = Regex::new(r"(?is)^(Whenever .+?) while (.+?), (.+)$")
        .expect("trigger with while condition regex compiles");
    if let Some(captures) = while_condition_re.captures(text) {
        let base_text = format!(
            "{}, {}",
            captures.get(1)?.as_str(),
            captures.get(3)?.as_str()
        );
        let mut parsed = parse_expansion_triggered(&base_text, face_name)?;
        let condition = parse_condition_text(captures.get(2)?.as_str()).or_else(|| {
            parse_controlled_permanent_condition(captures.get(2)?.as_str(), face_name)
        })?;
        parsed.rule["condition"] = if let Some(existing) = parsed.rule.get("condition") {
            and(vec![condition, existing.clone()])
        } else {
            condition
        };
        parsed
            .operations
            .insert(1, "Evaluate the intervening trigger condition".to_string());
        return Some(parsed);
    }
    let additional_draw_life_or_top_re = Regex::new(&format!(
        r"(?i)^At the beginning of your draw step, you may draw ({0}) additional cards?\. If you do, choose ({0}) cards? in your hand drawn this turn\. For each of those cards, pay ({0}) life or put the card on top of your library\.$",
        count_word_pattern(),
    ))
    .expect("additional draw with per-card life-or-top choice regex compiles");
    if let Some(captures) = additional_draw_life_or_top_re.captures(text) {
        let draw_count = parse_number_word(captures.get(1)?.as_str())?;
        let choose_count = parse_number_word(captures.get(2)?.as_str())?;
        let life_per_card = parse_number_word(captures.get(3)?.as_str())?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "draw", "player": controller() },
                "effects": [{
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": [
                        {
                            "kind": "drawCards",
                            "player": controller(),
                            "count": integer(draw_count),
                        },
                        {
                            "kind": "chooseCards",
                            "id": "drawnCardsToPayOrReturn",
                            "player": controller(),
                            "from": {
                                "kind": "cardsDrawnThisTurn",
                                "player": controller(),
                                "zone": hand(controller()),
                            },
                            "count": integer(choose_count),
                        },
                        {
                            "kind": "payLifeOrMoveCards",
                            "player": controller(),
                            "cards": decision_result("drawnCardsToPayOrReturn"),
                            "lifePerCard": integer(life_per_card),
                            "to": {
                                "kind": "library",
                                "player": controller(),
                                "position": "top",
                            },
                        },
                    ],
                }],
            }),
            &[
                "Trigger at the beginning of the controller's draw step",
                "Offer the additional draw as one optional action",
                "Select cards still in hand that were drawn this turn",
                "For each selected card, pay life or move it to the library top",
            ],
        ));
    }
    let optional_sacrifice_shared_type_re = Regex::new(&format!(
        r"(?i)^At the beginning of your end step, you may sacrifice (.+?)\. If you do, each opponent may sacrifice a permanent of their choice that shares a card type with it\. For each opponent who doesn't, that player loses ({}) life and you draw a card\.$",
        count_word_pattern(),
    ))
    .expect("optional sacrifice and shared-type opponent choice regex compiles");
    if let Some(captures) = optional_sacrifice_shared_type_re.captures(text) {
        let sacrifice_where = card_qualifier_list_filter(captures.get(1)?.as_str(), face_name)
            .or_else(|| parse_permanent_criteria(captures.get(1)?.as_str(), face_name))?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "sacrificePermanents",
                        "player": controller(),
                        "where": sacrifice_where,
                        "count": integer(1),
                        "excludeSource": false,
                        "bind": "sacrificedPermanent",
                    },
                    "onPerformed": [{
                        "kind": "forEachOpponentPaysCostOrControllerEffect",
                        "bindOpponentAs": "currentOpponent",
                        "cost": {
                            "kind": "sacrificePermanent",
                            "where": {
                                "kind": "sharesCardTypeWithBoundObject",
                                "binding": "sacrificedPermanent",
                            },
                        },
                        "otherwise": [
                            {
                                "kind": "loseLife",
                                "player": { "kind": "boundValue", "id": "currentOpponent" },
                                "amount": integer(parse_number_word(captures.get(2)?.as_str())?),
                            },
                            {
                                "kind": "drawCards",
                                "player": controller(),
                                "count": integer(1),
                            },
                        ],
                    }],
                }],
            }),
            &[
                "Offer a reusable optional sacrifice",
                "Bind the sacrificed permanent for later type comparison",
                "Let each opponent sacrifice a permanent sharing a card type",
                "Apply the controller effects once for every opponent who declines",
            ],
        ));
    }
    let (trigger_text, trigger_limit) = text
        .strip_suffix(" This ability triggers only once each turn.")
        .map(|text| {
            (
                text,
                Some(json!({
                    "kind": "onceEachTurn",
                    "id": format!("onceEachTurn:{text}"),
                })),
            )
        })
        .unwrap_or((text, None));
    let trigger_text = strip_short_oracle_label(trigger_text);
    let cast_source_enter_re = Regex::new(
        r"(?i)^When (.+?) enters, if (?:you cast (?:it|that (?:spell|card|permanent))|it was cast), (.+)$",
    )
    .expect("cast source enter trigger regex compiles");
    if let Some(captures) = cast_source_enter_re.captures(trigger_text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let (effects, decisions) =
                parse_expansion_instruction(captures.get(2)?.as_str(), face_name)?;
            let mut rule = json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "condition": {
                    "kind": "wasCast",
                    "object": self_ref(),
                },
                "effects": effects,
            });
            if !decisions.is_empty() {
                rule["declaration"] = json!({
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                });
            }
            if let Some(trigger_limit) = trigger_limit {
                rule["triggerLimit"] = trigger_limit;
            }
            return Some(draft(
                rule,
                &[
                    "Detect the source entering after it was cast",
                    "Reuse the shared was-cast condition",
                    "Resolve the shared effect instruction",
                ],
            ));
        }
    }
    let final_saga_chapter_resolved_re = Regex::new(
        r"(?i)^Whenever the final chapter ability of (?:a|an) (.+?) you control resolves, (.+)$",
    )
    .expect("final Saga chapter resolution trigger regex compiles");
    if let Some(captures) = final_saga_chapter_resolved_re.captures(trigger_text) {
        let where_filter = parse_permanent_criteria(captures.get(1)?.as_str(), face_name)?;
        let (effects, decisions) =
            parse_expansion_instruction(captures.get(2)?.as_str(), face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "sagaFinalChapterAbilityResolved",
                "player": controller(),
                "where": where_filter,
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Detect the resolution of a controlled permanent's final Saga chapter",
                "Reuse the permanent criteria grammar for the resolved Saga",
                "Resolve the shared effect instruction",
            ],
        ));
    }
    if let Some((subject, instruction)) = trigger_text
        .strip_prefix("When ")
        .and_then(|text| {
            text.split_once(" enters, if ")
                .or_else(|| text.split_once(" enter, if "))
        })
        .and_then(|(subject, conditional)| {
            [
                "it's not a token, ",
                "it is not a token, ",
                "he's not a token, ",
                "he is not a token, ",
                "she's not a token, ",
                "she is not a token, ",
                "they're not a token, ",
                "they are not a token, ",
            ]
            .into_iter()
            .find_map(|prefix| conditional.strip_prefix(prefix))
            .map(|instruction| (subject, instruction))
        })
        && (source_reference_matches(subject, face_name) || subject.eq_ignore_ascii_case(face_name))
    {
        let (effects, decisions) = parse_expansion_instruction(instruction, face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "enterBattlefield",
                "object": self_ref(),
                "nontoken": true,
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Recognize a named source entering",
                "Require the entering source to be nontoken",
                "Resolve the shared effect instruction",
            ],
        ));
    }
    let attack_total_power_first_each_turn_re = Regex::new(&format!(
        r"(?i)^Whenever you attack with creatures with total power ({}) or greater for the first time each turn, (.+)$",
        count_word_pattern(),
    ))
    .expect("attack total-power first-each-turn trigger regex compiles");
    if let Some(captures) = attack_total_power_first_each_turn_re.captures(trigger_text) {
        let (effects, decisions) =
            parse_expansion_instruction(captures.get(2)?.as_str(), face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": 1,
                "totalPowerAtLeast": integer(parse_number_word(captures.get(1)?.as_str())?),
            },
            "triggerLimit": {
                "kind": "onceEachTurn",
                "id": format!("onceEachTurn:{trigger_text}"),
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Measure the controlled attackers' total power",
                "Limit the trigger to its first qualifying attack each turn",
                "Resolve the shared untap and additional-combat effects",
            ],
        ));
    }
    let source_attacks_while_re = Regex::new(
        r"(?i)^Whenever (this (?:creature|permanent)|[A-Z][A-Za-z0-9 ',.-]+) attacks while (.+?), (.+)$",
    )
    .expect("source attacks with event condition regex compiles");
    if let Some(captures) = source_attacks_while_re.captures(trigger_text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let condition_text = captures.get(2)?.as_str();
            let condition = parse_condition_text(condition_text)
                .or_else(|| parse_controlled_permanent_condition(condition_text, face_name))?;
            let (effects, decisions) =
                parse_expansion_instruction(captures.get(3)?.as_str(), face_name)?;
            let mut rule = json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "declaredAttacker", "object": self_ref() },
                "condition": condition,
                "effects": effects,
            });
            if !decisions.is_empty() {
                rule["declaration"] = json!({
                    "kind": "castingDeclaration",
                    "decisions": decisions,
                });
            }
            if let Some(trigger_limit) = trigger_limit {
                rule["triggerLimit"] = trigger_limit;
            }
            return Some(draft(
                rule,
                &[
                    "Parse the source attack event",
                    "Evaluate the reusable while-condition at trigger time",
                    "Resolve the shared effect instruction",
                ],
            ));
        }
    }
    let cast_from_hand_energy_cascade_re = Regex::new(
        r"(?i)^When (.+?) enters, you get ((?:\{E\})+) \(.+?\)\. Then if you cast it from your hand, exile cards from the top of your library until you exile a nonland card\. You may cast that card by paying an amount of \{E\} equal to its mana value rather than paying its mana cost\.$",
    )
    .expect("cast-from-hand energy cascade regex compiles");
    if let Some(captures) = cast_from_hand_energy_cascade_re.captures(trigger_text) {
        let subject = captures.get(1)?.as_str();
        if subject.to_ascii_lowercase().starts_with("this ")
            || source_reference_matches(subject, face_name)
        {
            let energy_count = captures[2].matches("{E}").count() as i64;
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": { "kind": "enterBattlefield", "object": self_ref() },
                    "effects": [
                        {
                            "kind": "addPlayerCounters",
                            "player": controller(),
                            "counter": "energy",
                            "count": integer(energy_count),
                        },
                        {
                            "kind": "conditionalEffect",
                            "condition": {
                                "kind": "wasCastFromHand",
                                "object": self_ref(),
                            },
                            "then": [
                                {
                                    "kind": "exileFromTopUntil",
                                    "zone": library(controller()),
                                    "bind": "exiledCards",
                                    "faceDown": false,
                                    "stopWhere": not(card_type("Land")),
                                    "alsoStopsWhen": { "kind": "sourceZoneEmpty" },
                                },
                                {
                                    "kind": "castAnyNumber",
                                    "player": controller(),
                                    "cards": bound_objects("exiledCards"),
                                    "where": { "kind": "canBeCastAsSpell" },
                                    "timing": { "kind": "duringResolution" },
                                    "withoutPayingManaCost": false,
                                    "alternativeCost": {
                                        "kind": "payPlayerCounters",
                                        "counter": "energy",
                                        "count": { "kind": "manaValueOfCastCard" },
                                    },
                                    "alternativeCostsAllowed": false,
                                    "additionalCostsApply": true,
                                    "variableManaValue": 0,
                                    "maximum": integer(1),
                                },
                            ],
                            "else": [],
                        },
                    ],
                }),
                &[
                    "Parse the energy quantity",
                    "Gate the top-card search on casting from hand",
                    "Exile until the shared nonland criterion matches",
                    "Offer the matching card with a player-counter alternative cost",
                ],
            ));
        }
    }
    if trigger_text.eq_ignore_ascii_case(
        "Whenever this creature becomes tapped while it has a -1/-1 counter on it, remove a -1/-1 counter from it.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentTapped", "object": self_ref() },
                "condition": {
                    "kind": "compare",
                    "operator": ">",
                    "left": {
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": "-1/-1",
                    },
                    "right": integer(0),
                },
                "effects": [{
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": "-1/-1",
                    "count": integer(1),
                }],
            }),
            &[
                "Trigger when the source becomes tapped",
                "Require a -1/-1 counter on the source",
                "Remove one such counter",
            ],
        ));
    }

    let source_tapped_re = Regex::new(r"(?i)^Whenever (.+?) becomes tapped, (.+)$")
        .expect("source-tapped trigger regex compiles");
    if let Some(captures) = source_tapped_re.captures(trigger_text)
        && source_reference_matches(&captures[1], face_name)
    {
        let (effects, decisions) =
            parse_expansion_instruction(captures.get(2)?.as_str(), face_name)?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "permanentTapped", "object": self_ref() },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit.clone() {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Recognize a source becoming tapped",
                "Parse its effect through the shared instruction grammar",
            ],
        ));
    }
    let self_death_face_down_pile_re = Regex::new(&format!(
        r"(?i)^When (?:this creature|this permanent) is put into your graveyard from the battlefield, exile it and the top ({}) cards? of your library in a face-down pile\. If you do, shuffle that pile and put it back on top of your library\.$",
        count_word_pattern(),
    ))
    .expect("self-death face-down library pile regex compiles");
    if let Some(captures) = self_death_face_down_pile_re.captures(trigger_text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "effects": [{
                    "kind": "exileDiedSourceAndTopCardsAsShuffledLibraryPile",
                    "player": controller(),
                    "count": integer(parse_number_word(&captures[1])?),
                    "faceDown": true,
                    "destination": "libraryTop",
                }],
            }),
            &[
                "Observe the source moving from the battlefield to its graveyard",
                "Exile the source and the parsed number of top library cards as one pile",
                "Randomize that pile and return it to the top of the library",
            ],
        ));
    }
    let linked_hand_or_permanent_exile_re = Regex::new(
        r"(?i)^When (.+?) enters?, choose target opponent and up to one target creature they control\. They reveal their hand\. You may exile a (.+?) card from their hand or the chosen creature until (.+?) leaves? the battlefield\.$",
    )
    .expect("linked hand-card or permanent exile trigger regex compiles");
    if let Some(captures) = linked_hand_or_permanent_exile_re.captures(trigger_text)
        && source_reference_matches(captures.get(1)?.as_str(), face_name)
        && source_reference_matches(captures.get(3)?.as_str(), face_name)
    {
        let opponent = target_decision(
            "targetOpponent",
            json!({
                "kind": "players",
                "where": { "kind": "isOpponentOf", "player": controller() },
            }),
            1,
            1,
        );
        let mut creature = target_decision(
            "targetCreature",
            json!({
                "kind": "permanents",
                "where": card_type("Creature"),
            }),
            0,
            1,
        );
        creature["selectionConstraint"] = json!({
            "kind": "targetControllerMatchesTargetPlayer",
            "targetId": "targetOpponent",
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [opponent, creature],
                },
                "effects": [{
                    "kind": "chooseExileFromHandOrPermanentUntilSourceLeaves",
                    "player": chosen_target("targetOpponent"),
                    "permanent": chosen_target("targetCreature"),
                    "handWhere": parse_permanent_criteria(&captures[2], face_name)?,
                    "optional": true,
                }],
            }),
            &[
                "Target an opponent and optionally one creature they control",
                "Reveal that player's hand while resolving the linked choice",
                "Exile either a matching hand card or the chosen creature",
                "Return the selected object to its appropriate zone when the source leaves",
            ],
        ));
    }
    let each_main_phase_mana_re = Regex::new(
        r"(?i)^At the beginning of each of your main phases, if you haven't added mana with this ability this turn, you may add X mana of any one color, where X is the number of (.+?) target opponent controls\.$",
    )
    .expect("each-main-phase conditional opponent-permanent mana regex compiles");
    if let Some(captures) = each_main_phase_mana_re.captures(trigger_text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [
                        { "kind": "stepBegan", "step": "precombatMain", "player": controller() },
                        { "kind": "stepBegan", "step": "postcombatMain", "player": controller() },
                    ],
                },
                "condition": { "kind": "sourceDidNotAddManaThisTurn" },
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
                "effects": [{
                    "kind": "optionalEffects",
                    "player": controller(),
                    "effects": [{
                        "kind": "addMana",
                        "player": controller(),
                        "mana": {
                            "kind": "chooseColor",
                            "amount": {
                                "kind": "countPermanents",
                                "player": chosen_target("targetOpponent"),
                                "where": parse_permanent_criteria(&captures[1], face_name)?,
                            },
                        },
                    }],
                }],
            }),
            &[
                "Observe both controller main phases",
                "Limit mana production to once per turn for this source",
                "Count matching permanents controlled by the targeted opponent",
                "Choose one mana color for the computed amount",
            ],
        ));
    }
    let spell_cast_counter_threshold_re = Regex::new(
        r"(?i)^Whenever a player casts a spell with mana value equal to the number of ([A-Za-z0-9+/-]+) counters on (.+?), (.+)$",
    )
    .expect("spell-cast counter-threshold trigger regex compiles");
    if let Some(captures) = spell_cast_counter_threshold_re.captures(trigger_text)
        && source_reference_matches(captures.get(2)?.as_str(), face_name)
    {
        let (effects, decisions) =
            parse_general_effect_instruction(captures.get(3)?.as_str(), face_name)?;
        if !decisions.is_empty() {
            return None;
        }
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "anyPlayer": true,
                },
                "condition": compare(
                    "==",
                    json!({ "kind": "triggeringSpellManaValue" }),
                    json!({
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": captures.get(1)?.as_str(),
                    }),
                ),
                "effects": effects,
            }),
            &[
                "Parse the spell-cast event independently",
                "Compare its mana value with a source counter count",
                "Resolve the linked-spell effect",
            ],
        ));
    }
    let filtered_attacker_keyword_re = Regex::new(
        r"(?i)^Whenever (?:a|an) (.+?) you control with power (\d+) or greater attacks, it gains (flying|deathtouch|double strike|first strike|haste|lifelink|reach|trample|vigilance) until end of turn\.$",
    )
    .expect("filtered controlled attacker keyword regex compiles");
    if let Some(captures) = filtered_attacker_keyword_re.captures(trigger_text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "minimum": integer(1),
                },
                "effects": [{
                    "kind": "grantKeywordToAttackingPermanents",
                    "player": controller(),
                    "where": parse_permanent_criteria(
                        &singular_card_term(&captures[1]),
                        face_name,
                    )?,
                    "minimumPower": integer(captures[2].parse::<i64>().ok()?),
                    "keyword": oracle_keyword_kind(&captures[3])?,
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Recognize a controlled attacker by reusable criteria",
                "Evaluate its current power",
                "Grant the temporary keyword to every matching attacker",
            ],
        ));
    }
    if let Some((subject, instruction)) = trigger_text
        .strip_prefix("When ")
        .or_else(|| trigger_text.strip_prefix("Whenever "))
        .and_then(|text| {
            text.rsplit_once(" die, ")
                .or_else(|| text.rsplit_once(" dies, "))
        })
        && source_reference_matches(subject, face_name)
    {
        let return_as_type_re = Regex::new(
            r"(?i)^if (?:it|he|she|they) (?:was|were) (?:a|an) (.+?), return (?:it|him|her|them) to the battlefield(?: under (?:its|their) owner's control)?\. (?:It's|He's|She's|They're) (?:a|an) (.+?)\.?$",
        )
        .expect("source return with replacement card type regex compiles");
        if let Some(captures) = return_as_type_re.captures(instruction) {
            if known_card_type_filter(captures.get(1)?.as_str())? != card_type("Creature") {
                return None;
            }
            let new_type_filter = known_card_type_filter(captures.get(2)?.as_str())?;
            let new_type = new_type_filter["value"].as_str()?;
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": { "kind": "permanentDied", "object": self_ref() },
                    "condition": {
                        "kind": "decisionWas",
                        "decisionId": "triggeringPermanentWasCreature",
                        "value": true,
                    },
                    "effects": [
                        {
                            "kind": "moveAbilitySourceToBattlefield",
                            "from": "graveyard",
                            "tapped": false,
                        },
                        {
                            "kind": "setPermanentCardTypes",
                            "object": self_ref(),
                            "cardTypes": [new_type],
                            "removeSubtypes": true,
                        },
                    ],
                }),
                &[
                    "Resolve a named source even when its name contains punctuation",
                    "Observe that source dying",
                    "Check its last-known card type",
                    "Return it with replacement card types",
                ],
            ));
        }
    }
    let (mut event, mut instruction) = parse_expansion_trigger_event(trigger_text, face_name)?;
    // Reminder text is non-normative. Removing a final parenthetical here lets
    // the shared effect leaves parse token, keyword, blight, and surveil text
    // without each mechanic having to duplicate the reminder wording.
    if let Some((rules_text, reminder)) = instruction.rsplit_once(" (")
        && reminder.ends_with(')')
    {
        instruction = rules_text;
    }
    let nontoken_source_condition_re = Regex::new(
        r"(?i)^if (?:it(?:'s| is)|he(?:'s| is)|she(?:'s| is)|they(?:'re| are)) not a token, (.+)$",
    )
    .expect("nontoken source trigger condition regex compiles");
    if let Some(captures) = nontoken_source_condition_re.captures(instruction)
        && event["kind"].as_str() == Some("enterBattlefield")
    {
        event["nontoken"] = Value::Bool(true);
        instruction = captures.get(1)?.as_str();
    }
    let trigger_condition_re =
        Regex::new(r"(?i)^if (.+?), (.+)$").expect("generic trigger condition regex compiles");
    if let Some(captures) = trigger_condition_re.captures(instruction)
        && let Some(condition) = parse_condition_text(captures.get(1)?.as_str())
        && let Some((effects, decisions)) =
            parse_expansion_instruction(captures.get(2)?.as_str(), face_name)
    {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "condition": condition,
            "effects": effects,
        });
        if rule["effects"].as_array().is_some_and(|effects| {
            effects.iter().any(|effect| {
                matches!(
                    effect["kind"].as_str(),
                    Some("moveAbilitySourceToHand" | "moveAbilitySourceToBattlefield")
                )
            })
        }) {
            rule["triggerZone"] = Value::String("graveyard".to_string());
        }
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Parse the shared trigger event",
                "Evaluate the reusable intervening condition",
                "Resolve the shared effect instruction",
            ],
        ));
    }
    let damage_quantity_token_re = Regex::new(r"(?i)^create that many (.+? tokens?(?:\..*)?)$")
        .expect("damage-quantity token regex compiles");
    if matches!(
        event["kind"].as_str(),
        Some("combatDamageToPlayer" | "permanentDealtDamage")
    ) && let Some(captures) = damage_quantity_token_re.captures(instruction)
    {
        let token_instruction = format!("Create one {}", captures.get(1)?.as_str());
        let mut create = create_token_effect(&token_instruction)?;
        create["quantity"] = decision_result("damageAmount");
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [create],
        });
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Observe a damage event that records its amount",
                "Reuse the recorded damage amount as token quantity",
                "Parse the token specification through the shared token grammar",
            ],
        ));
    }
    let optional_payment_immediate_re = Regex::new(
        r"(?i)^you may (?:pay (.+?)|(tap .+?)|(remove .+?)|(blight .+?))\. If you do, (.+)$",
    )
    .expect("optional payment followed by immediate effects regex compiles");
    if let Some(captures) = optional_payment_immediate_re.captures(instruction) {
        let cost_text = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3))
            .or_else(|| captures.get(4))?;
        let cost = parse_resolution_cost_text(cost_text.as_str())?;
        let (effects, decisions) =
            parse_general_effect_sequence(captures.get(5)?.as_str(), face_name).or_else(|| {
                parse_general_effect_instruction(captures.get(5)?.as_str(), face_name)
            })?;
        let activates_from_graveyard = effects.iter().any(|effect| {
            matches!(
                effect["kind"].as_str(),
                Some("moveAbilitySourceToHand" | "moveAbilitySourceToBattlefield")
            )
        });
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [{
                "kind": "optionalPayCostPerformEffects",
                "player": controller(),
                "cost": cost,
                "effects": effects,
            }],
        });
        if activates_from_graveyard {
            rule["triggerZone"] = Value::String("graveyard".to_string());
        }
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Observe the outer trigger event",
                "Offer the parsed reusable cost",
                "Perform the following effects immediately only after payment",
            ],
        ));
    }
    let optional_payment_reflexive_re = Regex::new(
        r"(?i)^you may (?:pay (.+?)|(tap .+?)|(remove .+?)|(blight .+?))\. When you do, (.+)$",
    )
    .expect("optional payment followed by reflexive trigger regex compiles");
    if let Some(captures) = optional_payment_reflexive_re.captures(instruction) {
        let cost_text = captures
            .get(1)
            .or_else(|| captures.get(2))
            .or_else(|| captures.get(3))
            .or_else(|| captures.get(4))?;
        let cost = parse_resolution_cost_text(cost_text.as_str())?;
        let (effects, decisions) =
            parse_general_effect_sequence(captures.get(5)?.as_str(), face_name).or_else(|| {
                parse_general_effect_instruction(captures.get(5)?.as_str(), face_name)
            })?;
        let mut reflexive = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
            "effects": effects,
        });
        if !decisions.is_empty() {
            reflexive["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [{
                "kind": "optionalPayCostCreateReflexiveTrigger",
                "player": controller(),
                "cost": cost,
                "ability": reflexive,
            }],
        });
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Observe the outer trigger event",
                "Offer the parsed reusable cost",
                "Create the reflexive trigger only after payment",
                "Declare reflexive targets when that trigger is created",
            ],
        ));
    }
    let return_source_as_type_re = Regex::new(
        r"(?i)^if (?:it|he|she|they) (?:was|were) (?:a|an) (.+?), return (?:it|him|her|them) to the battlefield(?: under (?:its|their) owner's control)?\. (?:It's|He's|She's|They're) (?:a|an) (.+?)\.?$",
    )
    .expect("conditional source return with new card type regex compiles");
    if let Some(captures) = return_source_as_type_re.captures(instruction) {
        if known_card_type_filter(captures.get(1)?.as_str())? != card_type("Creature") {
            return None;
        }
        let new_type_filter = known_card_type_filter(captures.get(2)?.as_str())?;
        let new_type = new_type_filter["value"].as_str()?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "condition": {
                "kind": "decisionWas",
                "decisionId": "triggeringPermanentWasCreature",
                "value": true,
            },
            "effects": [
                {
                    "kind": "moveAbilitySourceToBattlefield",
                    "from": "graveyard",
                    "tapped": false,
                },
                {
                    "kind": "setPermanentCardTypes",
                    "object": self_ref(),
                    "cardTypes": [new_type],
                    "removeSubtypes": true,
                },
            ],
        });
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Observe the source leaving the battlefield",
                "Check its last-known card type",
                "Return the source from its owner's graveyard",
                "Replace its card types while preserving supertypes",
            ],
        ));
    }
    let intervening_condition_re =
        Regex::new(r"(?i)^if (.+?), (.+)$").expect("generic triggered condition regex compiles");
    if let Some(captures) = intervening_condition_re.captures(instruction)
        && !captures
            .get(1)?
            .as_str()
            .to_ascii_lowercase()
            .starts_with("it was kicked with its ")
        && let Some(condition) = parse_condition_text(captures.get(1)?.as_str())
        && let Some((effects, decisions)) =
            parse_expansion_instruction(captures.get(2)?.as_str(), face_name)
    {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "condition": condition,
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Parse the reusable trigger event",
                "Evaluate the intervening condition",
                "Resolve the shared effect instruction",
            ],
        ));
    }
    let specific_kicker_trigger_re =
        Regex::new(r"(?i)^if it was kicked with its ((?:\{[^}]+\})+) kicker, (.+)$")
            .expect("specific kicker trigger condition regex compiles");
    if let Some(captures) = specific_kicker_trigger_re.captures(instruction) {
        let (effects, decisions) =
            parse_general_effect_sequence(captures.get(2)?.as_str(), face_name)
                .or_else(|| parse_general_effect_instruction(captures.get(2)?.as_str(), face_name))
                .or_else(|| parse_expansion_instruction(captures.get(2)?.as_str(), face_name))?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "condition": {
                "kind": "kickerCostWasPaid",
                "spell": self_ref(),
                "cost": captures.get(1)?.as_str(),
            },
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Observe the reusable spell-cast event",
                "Check the specific kicker cost recorded during casting",
                "Parse and resolve the conditional instruction",
            ],
        ));
    }
    let ordinal_landfall_re = Regex::new(
        r"(?i)^you gain (\d+) life if this is the first time this ability has resolved this turn\. If it's the second time, add ((?:\{[WUBRGC]\})+)\. If it's the third time, (.+) deals (\d+) damage to each opponent and each planeswalker you don't control\.$",
    )
    .expect("ordinal landfall instruction regex compiles");
    if let Some(captures) = ordinal_landfall_re.captures(instruction) {
        let symbols = Regex::new(r"\{([WUBRGC])\}")
            .expect("fixed mana symbol regex compiles")
            .captures_iter(&captures[2])
            .map(|capture| capture[1].to_string())
            .collect::<Vec<_>>();
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "resolveOrdinalTriggeredAbility",
                    "id": "landEntryOrdinal",
                    "branches": [
                        {
                            "ordinal": 1,
                            "effects": [{
                                "kind": "gainLife",
                                "player": controller(),
                                "amount": integer(captures[1].parse::<i64>().ok()?),
                            }],
                        },
                        {
                            "ordinal": 2,
                            "effects": [{
                                "kind": "addFixedManaSymbols",
                                "player": controller(),
                                "symbols": symbols,
                            }],
                        },
                        {
                            "ordinal": 3,
                            "effects": [{
                                "kind": "dealDamageToEachOpponentAndPlaneswalker",
                                "amount": integer(captures[4].parse::<i64>().ok()?),
                            }],
                        },
                    ],
                }],
            }),
            &[
                "Resolve the recurring trigger",
                "Count resolutions of this ability this turn",
                "Apply only the matching ordinal branch",
            ],
        ));
    }
    if let Some((effects, decisions)) = parse_second_resolution_bonus(instruction, face_name) {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        return Some(draft(
            rule,
            &[
                "Resolve the reusable base instruction",
                "Count this ability's resolutions during the current turn",
                "Apply the additional instruction only on the second resolution",
            ],
        ));
    }
    if let Some(branches) = parse_ordinal_resolution_branches(instruction, face_name) {
        let ordinal_id = stable_rule_id("ordinalAbility", &format!("{face_name}\n{trigger_text}"));
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "resolveOrdinalTriggeredAbility",
                    "id": ordinal_id,
                    "branches": branches,
                }],
            }),
            &[
                "Observe the reusable trigger event",
                "Count this ability's resolutions during the current turn",
                "Parse each ordinal branch through the shared effect grammar",
                "Apply only the branch matching the current ordinal",
            ],
        ));
    }
    if let Some((effects, decisions)) = parse_expansion_instruction(instruction, face_name) {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        if let Some(trigger_limit) = trigger_limit {
            rule["triggerLimit"] = trigger_limit;
        }
        return Some(draft(
            rule,
            &[
                "Parse generic trigger event",
                "Parse generic triggered instruction",
            ],
        ));
    }

    const SUPPORTED: &[&str] = &[
        "Whenever an enchantment you control enters, create a 2/2 white Cat creature token. If that enchantment is an Aura, you may attach it to the token.",
        "When this Aura enters, you may have enchanted creature fight target creature an opponent controls. (Each deals damage equal to its power to the other.)",
        "When this Aura enters, enchanted creature fights up to one target creature an opponent controls. (Each deals damage equal to its power to the other.)",
        "When this Aura enters, draw a card for each Aura you control that's attached to a creature.",
        "At the beginning of each end step, each player gains control of all nontoken permanents they own.",
        "When this creature enters, search your library and/or graveyard for a card named Command Tower, reveal it, and put it into your hand. If you search your library this way, shuffle.",
        "When this creature enters, reveal cards from the top of your library until you reveal a land card. Put that card onto the battlefield tapped and the rest on the bottom of your library in a random order.",
        "Whenever you cast a creature spell with power 5 or greater, put a +1/+1 counter on Gwenna and untap it.",
        "Whenever this Spacecraft attacks, you may put a creature card from your hand onto the battlefield.",
        "At the beginning of your upkeep, look at the top card of your library. If it's a land card, you may put it onto the battlefield.",
        "Landfall — Whenever a land you control enters, choose one —\n• Put a +1/+1 counter on target creature.\n• You gain 2 life.",
    ];
    if !SUPPORTED.contains(&text) {
        return None;
    }
    Some(draft(
        json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": [{ "kind": "resolveExpansionTriggered", "instruction": text }],
        }),
        &[
            "Bind the Oracle trigger event",
            "Resolve the reusable expansion instruction",
        ],
    ))
}

pub(in crate::oracle::canonical) fn parse_remaining_kellan_ability(
    text: &str,
    ability_kind: &str,
) -> Option<CanonicalRuleDraft> {
    if ability_kind == "spellAbility" && text.eq_ignore_ascii_case("Gift a card") {
        return Some(draft(
            json!({
                "kind": "keywordAbility",
                "source": self_ref(),
                "ability": {
                    "kind": "gift",
                    "optional": true,
                    "effects": [{
                        "kind": "drawCards",
                        "player": { "kind": "boundValue", "id": "giftRecipient" },
                        "count": integer(1),
                    }],
                },
            }),
            &["Offer the promised opponent a card draw"],
        ));
    }
    if ability_kind == "spellAbility" && text.to_ascii_lowercase().starts_with("gift a ") {
        return parse_keyword_ability(text, "");
    }
    if text.starts_with(
        "Flurry — Whenever you cast your second spell each turn, copy it, then exile the spell you cast",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "spellCastOrdinal": 2,
                },
                "effects": [
                    {
                        "kind": "copyStackItem",
                        "object": { "kind": "triggeringStackObject" },
                        "controller": controller(),
                    },
                    {
                        "kind": "suspendStackItem",
                        "object": { "kind": "triggeringStackObject" },
                        "timeCounters": integer(4),
                    },
                ],
            }),
            &[
                "Trigger only on the controller's second spell each turn",
                "Copy the triggering spell",
                "Exile the original spell with four time counters and suspend",
            ],
        ));
    }
    const SUPPORTED: &[&str] = &[
        "{T}: Draw a card, then exile a card from your hand and put a number of time counters on it equal to its mana value. It gains \"When the last time counter is removed from this card, if it's exiled, you may cast it without paying its mana cost. If you cast a creature spell this way, it gains haste until end of turn.\" Then remove a time counter from each other card you own in exile.",
        "Flurry — Whenever you cast your second spell each turn, put a rally counter on this enchantment. Then create a 1/1 white Monk creature token with prowess for each rally counter on it. (Whenever you cast a noncreature spell, the token gets +1/+1 until end of turn.)",
        "This land enters tapped unless you control two or fewer other lands.",
        "To solve — You control seven or more lands. (If unsolved, solve at the beginning of your end step.)",
        "Solved — You may look at the top card of your library any time, and you may play lands and cast creature and enchantment spells from the top of your library.",
        "{4}, {T}: Target opponent exiles cards from the top of their library until they exile an instant or sorcery card. You may cast that card without paying its mana cost. Then put the exiled cards that weren't cast this way on the bottom of that library in a random order.",
        "Until your next turn, creatures can't attack you. Exile Chronomantic Escape with three time counters on it.",
        "Hideaway 4 (When this land enters, look at the top four cards of your library, exile one face down, then put the rest on the bottom in a random order.)",
        "{2}, {T}: You may play the exiled card without paying its mana cost if you control four or more legendary creatures.",
        "This creature can't be blocked.",
        "Whenever an opponent casts a spell, if this card is suspended, remove a time counter from it.",
        "Each player chooses a creature they control. Destroy the rest.",
        "If you control five or more untapped lands, this creature enters with two +1/+1 counters and a lifelink counter on it.",
        "{G}, {T}, Tap two untapped creatures you control: Reveal a card from your hand or the top card of your library. If you reveal a creature card this way, put it onto the battlefield. Activate only during your turn.",
        "This creature enters with three +1/+1 counters on it if you didn't cast it from your hand.",
        "When this creature dies, exile it with three time counters on it and it gains suspend. (At the beginning of its owner's upkeep, they remove a time counter. When the last is removed, they may cast this card without paying its mana cost. It has haste.)",
        "The top card of your library has plot. The plot cost is equal to its mana cost.",
        "You may plot nonland cards from the top of your library.",
        "Shadow (This creature can block or be blocked by only creatures with shadow.)",
        "Draw two cards. Exile Inspiring Refrain with three time counters on it.",
        "{T}: Untap target attacking creature. Prevent all combat damage that would be dealt to and dealt by that creature this turn.",
        "When this creature enters, tap all nonwhite creatures.",
        "+1: Draw a card, then discard a card.",
        "+1: You may exile a nonland card with mana value 3 or less from your hand. If you do, it becomes plotted.",
        "−6: Until end of turn, whenever you cast a spell, copy it. You may choose new targets for the copy.",
        "Whenever Kellan attacks, reveal the top card of your library. If it's a creature card with mana value 3 or less, put it into your hand. Otherwise, you may put it into your graveyard.",
        "Create X Map tokens, where X is one plus the number of opponents who control an artifact. (Then exile this card. You may cast the creature later from exile.)",
        "Imprint — When this artifact enters, each player exiles the top three cards of their library.",
        "Whenever a player casts a spell from their hand, that player exiles it. If the player does, they may cast a spell from among other cards exiled with this artifact without paying its mana cost.",
        "Whenever an opponent casts their first spell each turn, that player exiles the top card of their library. If it's a nonland card, you may cast it without paying its mana cost.",
        "{G}, {T}: You may play the exiled card without paying its mana cost if creatures you control have total power 10 or greater.",
        "Birds you control get +1/+1 and have vigilance.",
        "At the beginning of your end step, for each spell you've cast this turn, create a 1/2 blue Bird creature token with flying named Storm Crow.",
        "Exalted (Whenever a creature you control attacks alone, that creature gets +1/+1 until end of turn.)",
        "When this creature enters, create a 2/2 blue and black Zombie Rogue creature token, then put two +1/+1 counters on that token for each spell you've cast this turn other than the first.",
        "Create a token that's a copy of target creature you control, except it isn't legendary.",
        "Whenever another creature you control enters, put X +1/+1 counters on it, where X is its power.",
        "Whenever you cast your first spell each turn, reveal the top card of your library. You may cast it without paying its mana cost if it's a spell with lesser mana value. If you don't cast it, put it into your hand.",
        "This creature enters with a +1/+1 counter on it plus an additional +1/+1 counter on it for each other creature you control.",
        "When this creature dies, you may exile it and put three time counters on it. If you do, exile up to one target creature and put three time counters on it. Each card exiled this way that doesn't have suspend gains suspend. (For each card with suspend, its owner removes a time counter from it at the beginning of their upkeep. When the last is removed, they may cast it without paying its mana cost. Those creature spells have haste.)",
        "Paradox — Whenever you cast a spell from anywhere other than your hand, double the number of +1/+1 counters on this creature.",
        "When this creature enters, target instant or sorcery card in your graveyard gains flashback until end of turn. The flashback cost is equal to its mana cost. (You may cast that card from your graveyard for its flashback cost. Then exile it.)",
        "Create a token that's a copy of target artifact or creature.",
        "Cipher (Then you may exile this spell card encoded on a creature you control. Whenever that creature deals combat damage to a player, its controller may cast a copy of the encoded card without paying its mana cost.)",
        "Whenever a player attacks you, if that player has another opponent who isn't being attacked, prevent all combat damage that would be dealt to you this combat.",
        "Exile target creature and put two time counters on it. If it doesn't have suspend, it gains suspend. (At the beginning of its owner's upkeep, they remove a time counter. When the last is removed, they may play it without paying its mana cost. If it's a creature, it has haste.)",
        "Enchanted creature gets +1/+1 for each enchantment you control.",
        "This creature must be blocked if able.",
        "This creature can't be blocked by more than one creature.",
        "Whenever this creature deals damage, exile it face down. It becomes foretold.",
        "Exile cards from the top of your library until you exile a land card. Put that card onto the battlefield and the rest on the bottom of your library in a random order. Exile Venture Forth with three time counters on it.",
        "After an Adventure resolves, you can place the exiled card here. You may cast the creature from exile.",
    ];
    if !SUPPORTED.contains(&text) {
        return None;
    }

    let operation = json!({
        "kind": "resolveKellanAbility",
        "instruction": text,
    });
    if text.starts_with("Flurry — Whenever you cast your second spell each turn") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "spellCastOrdinal": 2,
                },
                "effects": [operation],
            }),
            &["Trigger only on the controller's second spell each turn"],
        ));
    }
    if text.starts_with("Hideaway 4 ") || text.starts_with("Imprint — When this artifact enters")
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [operation],
            }),
            &["Resolve the enter-the-battlefield instruction"],
        ));
    }
    if text.starts_with("To solve — ") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": controller() },
                "effects": [operation],
            }),
            &["Check the Case solve condition at the controller's end step"],
        ));
    }
    if text.starts_with("+1:") || text.starts_with("−6:") {
        let loyalty = if text.starts_with("+1:") { 1 } else { -6 };
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "startingLoyalty": integer(3),
                "costs": [{ "kind": "payLoyalty", "object": self_ref(), "amount": integer(loyalty) }],
                "activationLimit": { "kind": "oncePerTurn", "id": "loyaltyAbility" },
                "activationCondition": { "kind": "sorceryTiming" },
                "effects": [operation],
            }),
            &["Parse Jace's loyalty cost and resolve its instruction"],
        ));
    }
    if text.starts_with("{G}, {T}, Tap two untapped creatures you control:") {
        let creature_candidates = json!({
            "kind": "permanents",
            "controller": controller(),
            "where": card_type("Creature"),
            "excludeSource": true,
        });
        return Some(draft(
            json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": [
                    { "kind": "payMana", "manaCost": "{G}" },
                    { "kind": "tap", "object": self_ref() },
                    { "kind": "tap", "object": chosen_target("eladamriTapOne") },
                    { "kind": "tap", "object": chosen_target("eladamriTapTwo") }
                ],
                "activationCondition": { "kind": "duringControllerTurn", "player": controller() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [
                        target_decision("eladamriTapOne", creature_candidates.clone(), 1, 1),
                        target_decision("eladamriTapTwo", creature_candidates, 1, 1),
                    ],
                },
                "distinctTargets": [["eladamriTapOne", "eladamriTapTwo"]],
                "effects": [operation],
            }),
            &["Pay green mana, tap Eladamri and two other creatures"],
        ));
    }
    if text.starts_with("After an Adventure resolves") {
        return Some(draft(
            json!({ "kind": "rulesMarker", "text": "Adventure exile cast permission" }),
            &["Represent the Adventure helper zone as a rules marker"],
        ));
    }
    let rule = match ability_kind {
        "activatedAbility" | "staticAbility" if text.contains(':') => {
            let (cost_text, _) = text.split_once(':')?;
            let (costs, _) = parse_activation_costs(cost_text)?;
            let mut rule = json!({
                "kind": "activatedAbility",
                "source": self_ref(),
                "costs": costs,
                "effects": [operation],
            });
            if text.contains("Activate only during your turn") {
                rule["activationCondition"] = json!({ "kind": "controllerTurn" });
            }
            rule
        }
        "triggeredAbility" => {
            let event = if text.contains("enters") || text.starts_with("Imprint") {
                json!({ "kind": "enterBattlefield", "object": self_ref() })
            } else if text.contains("dies") {
                json!({ "kind": "permanentDied", "object": self_ref() })
            } else if text.contains("attacks") {
                json!({ "kind": "declaredAttacker", "object": self_ref() })
            } else if text.contains("deals combat damage") || text.contains("deals damage") {
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() })
            } else if text.contains("end step") {
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() })
            } else if text.contains("another creature you control enters") {
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                })
            } else {
                json!({
                    "kind": "spellCast",
                    "anyPlayer": true,
                    "where": Value::Null,
                })
            };
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [operation],
            })
        }
        "spellAbility" => json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [operation],
        }),
        "keywordAbility" | "keywordAbilityGroup" if text.starts_with("Hideaway ") => json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "effects": [operation],
        }),
        "keywordAbility" | "keywordAbilityGroup" => json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [operation],
        }),
        _ => json!({
            "kind": "staticAbility",
            "source": self_ref(),
            "activeWhile": active_while_battlefield(),
            "modifiers": [{
                "kind": "kellanStatic",
                "instruction": text,
            }],
        }),
    };
    Some(draft(
        rule,
        &[
            "Recognize the remaining Kellan Oracle instruction",
            "Bind its timing and source",
            "Delegate to the authoritative Rust operation",
        ],
    ))
}
