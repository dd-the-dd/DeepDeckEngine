use super::super::*;

pub(in crate::oracle::canonical) fn parse_azula_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let triggered = |event: Value, effects: Vec<Value>| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": effects,
            }),
            &[
                "Recognize the reusable trigger event",
                "Compose canonical effects in Oracle order",
            ],
        )
    };
    let operation = |event: Value, operation: &str| {
        triggered(
            event,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": operation,
            })],
        )
    };
    let enter_self = || json!({ "kind": "enterBattlefield", "object": self_ref() });
    let attacks = || json!({ "kind": "declaredAttacker", "object": self_ref() });
    let end_step =
        |player: Value| json!({ "kind": "stepBegan", "step": "endStep", "player": player });

    if text.starts_with("Magecraft")
        && text.contains("Whenever you cast or copy an instant or sorcery spell")
        && text.contains("discard a card, then draw a card")
    {
        return Some(operation(
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    },
                    {
                        "kind": "spellCopied",
                        "player": controller(),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    },
                ],
            }),
            "ashlingMagecraft",
        ));
    }
    if text
        .starts_with("When Azula enters, target opponent exiles a nontoken creature they control")
    {
        return Some(operation(enter_self(), "azulaCunningEnter"));
    }
    if text
        == "When this creature enters, copy target instant or sorcery spell. You may choose new targets for the copy."
    {
        return Some(operation(enter_self(), "dualcasterMageCopy"));
    }
    if text.starts_with("When Electro leaves the battlefield, you may pay {X}.") {
        return Some(operation(
            json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
            "electroLeaves",
        ));
    }
    if text.starts_with("Whenever Fire Lord Ozai attacks, you may sacrifice another creature.") {
        return Some(operation(attacks(), "fireLordOzaiAttacks"));
    }
    if text.starts_with(
        "At the beginning of combat on your turn, up to one target creature gets +2/+0",
    ) {
        return Some(operation(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            "fireNationTurretCombat",
        ));
    }
    if text.starts_with("At the beginning of your end step, if an opponent lost life this turn") {
        return Some(operation(end_step(controller()), "lionVultureEndStep"));
    }
    if text.starts_with("Whenever Ty Lee attacks, you may pay {1}.") {
        return Some(operation(attacks(), "tyLeeAttackPayment"));
    }
    if text.starts_with(
        "Whenever a Mountain you control enters, if you control at least five other Mountains",
    ) {
        return Some(operation(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": subtype("Mountain"),
            }),
            "volcanoOfRokuMountain",
        ));
    }
    if text.starts_with(
        "Whenever an opponent searches their library, put a +1/+1 counter on Wan Shi Tong",
    ) {
        return Some(operation(
            json!({ "kind": "librarySearched", "anyPlayer": true }),
            "wanShiTongSearch",
        ));
    }
    if text.starts_with("Whenever Ragavan deals combat damage to a player, create a Treasure token")
    {
        return Some(operation(
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            "ragavanCombatDamage",
        ));
    }
    if text.starts_with(
        "At the beginning of each end step, if an opponent lost 2 or more life this turn",
    ) {
        return Some(operation(
            end_step(json!({ "kind": "eachPlayer" })),
            "bloodbendersRiseEndStep",
        ));
    }
    if text.starts_with("Whenever a card is put into an opponent's graveyard from anywhere") {
        return Some(operation(
            json!({ "kind": "opponentCardEnteredGraveyard", "player": controller() }),
            "bloodbendersRiseGraveyard",
        ));
    }
    if text.starts_with("Whenever this creature attacks, you may cast an Ally spell from among cards you own exiled with this creature") {
        return Some(operation(attacks(), "boilingRockRioterAttack"));
    }
    if text
        == "When this creature enters, put a +1/+1 counter on target creature or Vehicle you control."
    {
        return Some(operation(enter_self(), "fireNationSalvagersEnter"));
    }
    if text.starts_with("Whenever one or more creatures you control with counters on them deal combat damage to a player") {
        return Some(operation(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
            }),
            "fireNationSalvagersCombat",
        ));
    }
    if text.starts_with(
        "When this creature enters and whenever an opponent draws a card except the first one",
    ) {
        return Some(operation(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    {
                        "kind": "cardDrawn",
                        "opponentOfSourceController": true,
                        "exceptFirstInDrawStep": true,
                    },
                ],
            }),
            "orcishBowmastersTrigger",
        ));
    }
    if text.starts_with(
        "Whenever you cast a spell while Fire Lord Azula is attacking, copy that spell",
    ) {
        let mut rule = operation(
            json!({ "kind": "spellCast", "player": controller(), "where": Value::Null }),
            "fireLordAzulaCopy",
        );
        rule.rule["condition"] = json!({ "kind": "isAttacking", "object": self_ref() });
        return Some(rule);
    }
    if text.starts_with("Magecraft")
        && text.contains(
            "Whenever you cast or copy an instant or sorcery spell, create a Treasure token",
        )
    {
        return Some(operation(
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    },
                    {
                        "kind": "spellCopied",
                        "player": controller(),
                        "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                    },
                ],
            }),
            "stormKilnMagecraft",
        ));
    }
    if text.starts_with("Whenever Smellerbee attacks, you may discard your hand.") {
        return Some(operation(attacks(), "smellerbeeAttack"));
    }
    if text
        .starts_with("Whenever Fire Lord Sozin deals combat damage to a player, you may pay {X}.")
    {
        return Some(operation(
            json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
            "fireLordSozinCombat",
        ));
    }
    if text.starts_with("When this Equipment enters, attach it to target creature you control.") {
        return Some(operation(enter_self(), "twinBladesEnter"));
    }

    if text.starts_with("Whenever Azula attacks, you lose 1 life and create a Clue token.") {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            vec![
                json!({ "kind": "loseLife", "player": controller(), "amount": integer(1) }),
                create_token_effect("Create a Clue token.")?,
            ],
        ));
    }
    if text.starts_with("When this creature dies, it deals 1 damage to you. Create a Clue token.") {
        return Some(triggered(
            json!({ "kind": "permanentDied", "object": self_ref() }),
            vec![
                json!({
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(1),
                    "recipient": controller(),
                }),
                create_token_effect("Create a Clue token.")?,
            ],
        ));
    }
    if text == "Whenever you cast an instant or sorcery spell, add {R}." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
            }),
            vec![json!({ "kind": "addMana", "player": controller(), "mana": "{R}" })],
        ));
    }
    if text == "Whenever you cast a noncreature spell, Longshot deals 2 damage to each opponent." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": not(card_type("Creature")),
            }),
            vec![json!({ "kind": "dealDamageToEachOpponent", "amount": integer(2) })],
        ));
    }
    let second_draw_re = Regex::new(&format!(
        r"^Whenever an opponent draws their second card each turn, you draw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("opponent second-draw regex compiles");
    if let Some(captures) = second_draw_re.captures(text) {
        return Some(triggered(
            json!({
                "kind": "cardDrawn",
                "opponentOfSourceController": true,
                "drawOrdinal": integer(2),
            }),
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
        ));
    }
    if text
        == "Whenever a nontoken creature an opponent controls dies, put a +1/+1 counter on each creature you control."
    {
        return Some(triggered(
            json!({
                "kind": "opponentCreatureDied",
                "player": controller(),
                "nontoken": true,
            }),
            vec![json!({
                "kind": "putCounters",
                "permanent": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_special_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if let Some(rule) = parse_azula_triggered_ability(text) {
        return Some(rule);
    }
    if let Some(rule) = parse_avatar_triggered_ability(text) {
        return Some(rule);
    }
    if let Some(rule) = parse_simple_triggered_ability(text) {
        return Some(rule);
    }
    if let Some(rule) = parse_common_triggered_ability(text) {
        return Some(rule);
    }
    if let Some(rule) = parse_avatar_deck_trigger(text) {
        return Some(rule);
    }
    let turned_face_up_operation = match text {
        "When this creature is turned face up, counter target instant or sorcery spell." => {
            Some("stratusDancerCounter")
        }
        "When this creature is turned face up, counter target spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard. You may cast that card without paying its mana cost for as long as it remains exiled." => {
            Some("kheruSpellsnatcherCounter")
        }
        _ => None,
    };
    if let Some(operation) = turned_face_up_operation {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "turnedFaceUp", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": operation,
                }],
            }),
            &[
                "Resolve the source turning face up",
                "Choose and counter the matching spell",
            ],
        ));
    }
    if text.starts_with("Whenever Daxos deals combat damage to a player,") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "combatDamageToPlayer", "source": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "daxosCombatExile",
                }],
            }),
            &[
                "Resolve Daxos combat damage",
                "Exile the damaged player's top card",
                "Grant its temporary casting permission",
            ],
        ));
    }
    if text.starts_with("When Ao dies, choose one") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "permanentDied", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "aoDeathChoice",
                }],
            }),
            &["Resolve Ao dying", "Choose and execute one death mode"],
        ));
    }
    if text.starts_with("Whenever Odric and at least three other creatures attack,") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "minimum": integer(4),
                    "sourceMustAttack": true,
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "odricChooseBlocks",
                }],
            }),
            &[
                "Require Odric and three other attackers",
                "Let the attacking player choose legal blocks this combat",
            ],
        ));
    }
    let cast_draw_filter = match text {
        "Whenever you cast an enchantment spell, draw a card." => Some(card_type("Enchantment")),
        "Whenever you cast an Aura, Equipment, or Vehicle spell, draw a card." => Some(or(vec![
            subtype("Aura"),
            subtype("Equipment"),
            subtype("Vehicle"),
        ])),
        _ => None,
    };
    if let Some(where_filter) = cast_draw_filter {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": where_filter,
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &["Resolve matching controlled spell cast", "Draw one card"],
        ));
    }
    if text == "Whenever you cast an Aura spell, you may draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": subtype("Aura"),
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    },
                    "onPerformed": [],
                }],
            }),
            &["Resolve controlled Aura cast", "Offer one card draw"],
        ));
    }
    let constellation_token = match text {
        value if value.ends_with(
            "Whenever an enchantment you control enters, create a 2/2 white Pegasus creature token with flying.",
        ) => Some("Create a 2/2 white Pegasus creature token with flying."),
        _ => None,
    };
    if let Some(token_text) = constellation_token {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Enchantment"),
                },
                "effects": [create_token_effect(token_text)?],
            }),
            &[
                "Resolve controlled enchantment entry",
                "Create the specified token",
            ],
        ));
    }
    if text
        == "Whenever an opponent casts an instant or sorcery spell, create a 1/2 green Spider creature token with reach."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "opponentOfSourceController": true,
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                },
                "effects": [create_token_effect(
                    "Create a 1/2 green Spider creature token with reach.",
                )?],
            }),
            &[
                "Resolve opponent instant or sorcery cast",
                "Create a Spider token",
            ],
        ));
    }
    if text.ends_with(
        "Whenever an enchantment you control enters, put a +1/+1 counter on this creature and draw a card.",
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Enchantment"),
                },
                "effects": [
                    {
                        "kind": "putCounters",
                        "permanent": self_ref(),
                        "counter": "+1/+1",
                        "count": integer(1),
                    },
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    },
                ],
            }),
            &["Resolve controlled enchantment entry", "Add a counter", "Draw one card"],
        ));
    }
    let aura_entry_life_re = Regex::new(r"^When this Aura enters, you gain (\d+) life\.$")
        .expect("Aura entry life regex compiles");
    if let Some(captures) = aura_entry_life_re.captures(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(captures[1].parse::<i64>().ok()?),
                }],
            }),
            &["Resolve Aura entry", "Gain the printed life amount"],
        ));
    }
    if text
        == "Whenever another legendary creature you control enters, put a +1/+1 counter on Legolas."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), json!({ "kind": "isLegendary" })]),
                    "excludeSource": true,
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Resolve another controlled legendary creature entry",
                "Put a counter on Legolas",
            ],
        ));
    }
    if text == "Whenever Legolas deals combat damage to a player, draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "combatDamageToPlayer", "source": self_ref() },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &["Resolve source combat damage to player", "Draw one card"],
        ));
    }
    if text == "Whenever you cast a noncreature spell, put a lore counter on this enchantment." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": not(card_type("Creature")),
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "lore",
                    "count": integer(1),
                }],
            }),
            &[
                "Resolve controlled noncreature spell cast",
                "Put a lore counter on the source",
            ],
        ));
    }
    if text == "Whenever you cast a creature spell, draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &["Resolve controlled creature spell cast", "Draw one card"],
        ));
    }
    if text == "Whenever a player plays a land, that player draws a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "anyController": true,
                    "where": card_type("Land"),
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "enteringLandControllerDraws",
                }],
            }),
            &[
                "Resolve any player's land entry",
                "Draw for that land's controller",
            ],
        ));
    }
    if text.ends_with("Whenever a land you control enters, you gain 1 life and draw a card.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                },
                "effects": [
                    { "kind": "gainLife", "player": controller(), "amount": integer(1) },
                    { "kind": "drawCards", "player": controller(), "count": integer(1) },
                ],
            }),
            &[
                "Resolve controlled land entry",
                "Gain one life",
                "Draw one card",
            ],
        ));
    }
    if text == "When this creature enters, add one mana of any color." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "effects": [{
                    "kind": "addMana",
                    "player": controller(),
                    "mana": { "kind": "chooseColor", "amount": 1 },
                }],
            }),
            &[
                "Resolve this permanent entering",
                "Choose and add one colored mana",
            ],
        ));
    }
    if text == "When this creature enters, return target permanent to its owner's hand." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        json!({ "kind": "permanents" }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "returnToOwnersHand",
                    "object": chosen_target("targetPermanent"),
                }],
            }),
            &[
                "Declare any target permanent",
                "Return it to its owner's hand",
            ],
        ));
    }
    let operation = |event: Value, operation: &str| {
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
            &[
                "Resolve reusable trigger event",
                "Apply complete ordered instruction",
            ],
        )
    };
    match text {
        "When this creature enters, if you control a creature with power 4 or greater, draw a card." =>
        {
            return Some(operation(
                json!({ "kind": "enterBattlefield", "object": self_ref() }),
                "drawIfControlPowerFour",
            ));
        }
        "Whenever another creature you control with power 4 or greater enters, draw a card." => {
            return Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "drawIfEnteringPowerFour",
            ));
        }
        "Whenever another creature you control enters, put X +1/+1 counters on it, where X is its power." =>
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
                    },
                    "effects": [{
                        "kind": "putCounters",
                        "permanent": { "kind": "triggeringPermanent" },
                        "counter": "+1/+1",
                        "count": {
                            "kind": "powerOf",
                            "object": { "kind": "triggeringPermanent" },
                        },
                    }],
                }),
                &[
                    "Resolve the entering creature",
                    "Read its power",
                    "Put that many counters",
                ],
            ));
        }
        "Whenever you cast a creature spell, draw a card, then you may put a land card from your hand onto the battlefield." =>
        {
            return Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "chulaneDrawAndLand",
            ));
        }
        "When this card becomes plotted, target creature gets +3/+2 and gains trample until end of turn." =>
        {
            return Some(operation(
                json!({ "kind": "cardPlotted", "object": self_ref() }),
                "aloeAlchemistPlotBoost",
            ));
        }
        "Whenever Kellan attacks, reveal the top card of your library. If it's a creature card with mana value 3 or less, put it into your hand. Otherwise, you may put it into your graveyard." =>
        {
            return Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "kellanDaringAttack",
            ));
        }
        "When this creature enters, tap all nonwhite creatures." => {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": { "kind": "enterBattlefield", "object": self_ref() },
                    "effects": [{
                        "kind": "tapPermanents",
                        "where": and(vec![
                            card_type("Creature"),
                            json!({ "kind": "colorDoesNotContain", "value": "White" }),
                        ]),
                    }],
                }),
                &["Resolve all nonwhite creatures", "Tap them simultaneously"],
            ));
        }
        "Paradox — Whenever you cast a spell from anywhere other than your hand, double the number of +1/+1 counters on this creature." => {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": Value::Null,
                        "fromZoneNot": "hand",
                    },
                    "effects": [{
                        "kind": "doubleCounters",
                        "permanent": self_ref(),
                        "counter": "+1/+1",
                    }],
                }),
                &["Resolve an out-of-hand cast", "Double source +1/+1 counters"],
            ));
        }
        "When this creature enters, create a 2/2 blue and black Zombie Rogue creature token, then put two +1/+1 counters on that token for each spell you've cast this turn other than the first." => {
            return Some(operation(
                json!({ "kind": "enterBattlefield", "object": self_ref() }),
                "outlawStitcherToken",
            ));
        }
        "Whenever you cast your first spell each turn, reveal the top card of your library. You may cast it without paying its mana cost if it's a spell with lesser mana value. If you don't cast it, put it into your hand." => {
            return Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": Value::Null,
                }),
                "rashmiFirstSpell",
            ));
        }
        value if value.starts_with("When this creature dies, exile it with three time counters on it and it gains suspend.") => {
            return Some(operation(
                json!({ "kind": "permanentDied", "object": self_ref() }),
                "suspendDeadSource",
            ));
        }
        "Whenever an opponent casts a spell, if this card is suspended, remove a time counter from it." => {
            return Some(operation(
                json!({
                    "kind": "spellCast",
                    "opponentOfSourceController": true,
                    "where": Value::Null,
                }),
                "suspendRemoveTimeCounter",
            ));
        }
        _ => {}
    }
    if text
        == "Whenever you cast a spell from anywhere other than your hand, you may cast a permanent spell with equal or lesser mana value from your hand without paying its mana cost. If you don't, you may put a land card from your hand onto the battlefield."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": Value::Null,
                    "fromZoneNot": "hand",
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "kellanCastPermanentOrPutLand",
                }],
            }),
            &[
                "Resolve a spell cast outside its controller's hand",
                "Offer a free permanent spell bounded by mana value",
                "Offer a land from hand only when the spell is declined",
            ],
        ));
    }
    if text
        == "At the beginning of each player's upkeep, that player gains control of Alexios, untaps it, and puts a +1/+1 counter on it. It gains haste until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "upkeep",
                    "player": { "kind": "eachPlayer" },
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "alexiosUpkeepControl",
                }],
            }),
            &[
                "Resolve each player's upkeep",
                "Transfer control without changing ownership",
                "Untap, add counter, and grant temporary haste",
            ],
        ));
    }
    if let Some(rule) = parse_remaining_deck_trigger(text) {
        return Some(rule);
    }

    let step_trigger = |step: &str, condition: Option<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": {
                "kind": "stepBegan",
                "step": step,
                "player": controller(),
            },
            "effects": effects,
        });
        if let Some(condition) = condition {
            rule["condition"] = condition;
        }
        draft(
            rule,
            &[
                "Resolve active-player step event",
                "Evaluate trigger condition",
                "Resolve ordered trigger effects",
            ],
        )
    };

    if matches!(
        text,
        "Whenever this land becomes tapped, it deals 1 damage to you."
            | "Whenever City of Brass becomes tapped, it deals 1 damage to you."
    ) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentTapped",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "dealDamage",
                    "source": self_ref(),
                    "amount": integer(1),
                    "recipient": controller(),
                }],
            }),
            &[
                "Resolve self-tap event",
                "Resolve controller as damage recipient",
                "Resolve damage vocabulary",
            ],
        ));
    }
    let enchanted_land_mana_re = Regex::new(&format!(
        r"^Whenever enchanted land is tapped for mana, its controller adds an additional ({}) mana (of any color|in any combination of colors|of the chosen color)\.$",
        count_word_pattern(),
    ))
    .expect("enchanted-land mana trigger regex compiles");
    if let Some(captures) = enchanted_land_mana_re.captures(text) {
        let amount = parse_number_word(&captures[1])?;
        let mana = match &captures[2] {
            "of any color" => json!({ "kind": "chooseColor", "amount": integer(amount) }),
            "in any combination of colors" => {
                json!({ "kind": "chooseColors", "amount": integer(amount) })
            }
            "of the chosen color" => {
                json!({ "kind": "storedColor", "decisionId": "chosenColor", "amount": integer(amount) })
            }
            _ => return None,
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "attachedPermanentManaAbilityActivated",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "addMana",
                    "player": {
                        "kind": "controllerOf",
                        "object": { "kind": "triggeringPermanent" },
                    },
                    "mana": mana,
                }],
            }),
            &[
                "Observe the enchanted land's mana ability",
                "Resolve the enchanted land's controller",
                "Add the selected additional mana",
            ],
        ));
    }
    if text == "At the beginning of your upkeep, sacrifice a creature." {
        return Some(step_trigger(
            "upkeep",
            None,
            vec![json!({
                "kind": "sacrificePermanents",
                "player": controller(),
                "where": card_type("Creature"),
                "count": integer(1),
            })],
        ));
    }

    let enter_card_selection_re = Regex::new(
        r"^When this (?:land|creature|artifact|enchantment) enters, (scry|surveil) (\d+)\.(?: .+)?$",
    )
    .expect("enter card-selection regex compiles");
    if let Some(captures) = enter_card_selection_re.captures(text) {
        let effect_kind = captures[1].to_ascii_lowercase();
        let count = captures[2].parse::<i64>().ok()?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": effect_kind,
                    "player": controller(),
                    "count": integer(count),
                }],
            }),
            &[
                "Resolve source enter event",
                "Resolve library-selection action",
                "Resolve inspected-card count",
            ],
        ));
    }

    if text
        == "At the beginning of your upkeep, put a muster counter on this enchantment. Then create a 1/1 red and white Soldier creature token with haste for each muster counter on this enchantment."
    {
        return Some(step_trigger(
            "upkeep",
            None,
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "muster",
                    "count": integer(1),
                }),
                json!({
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": {
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": "muster",
                    },
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Soldier"],
                        "colors": ["Red", "White"],
                        "power": 1,
                        "toughness": 1,
                        "abilities": [{ "kind": "haste" }],
                    },
                }),
            ],
        ));
    }

    if text
        == "At the beginning of your end step, create a Treasure token for each creature that died this turn. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")"
    {
        return Some(step_trigger(
            "endStep",
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "countEventsThisTurn",
                    "event": "permanentDied",
                    "where": card_type("Creature"),
                },
                "token": {
                    "kind": "namedToken",
                    "name": "Treasure",
                },
            })],
        ));
    }
    if text
        == "At the beginning of your end step, for each spell you've cast this turn, create a 1/2 blue Bird creature token with flying named Storm Crow."
    {
        return Some(step_trigger(
            "endStep",
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "countEventsThisTurn",
                    "event": "spellCast",
                    "player": controller(),
                },
                "token": {
                    "name": "Storm Crow",
                    "types": ["Creature"],
                    "subtypes": ["Bird"],
                    "colors": ["Blue"],
                    "power": 1,
                    "toughness": 2,
                    "abilities": [{ "kind": "flying" }],
                },
            })],
        ));
    }

    if text
        == "At the beginning of your end step, draw a card if you've gained 3 or more life this turn."
    {
        return Some(step_trigger(
            "endStep",
            Some(compare(
                ">=",
                json!({
                    "kind": "lifeGainedThisTurn",
                    "player": controller(),
                }),
                integer(3),
            )),
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }

    let void_end_step = match text {
        "Void — At the beginning of your end step, if a nonland permanent left the battlefield this turn or a spell was warped this turn, create a 2/2 colorless Robot artifact creature token." => {
            Some(vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": {
                    "types": ["Artifact", "Creature"],
                    "subtypes": ["Robot"],
                    "power": 2,
                    "toughness": 2,
                    "abilities": [],
                },
            })])
        }
        "Void — At the beginning of your end step, if a nonland permanent left the battlefield this turn or a spell was warped this turn, you draw a card and lose 1 life." => {
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
        }
        _ => None,
    };
    if let Some(effects) = void_end_step {
        return Some(step_trigger(
            "endStep",
            Some(compare(
                ">=",
                json!({
                    "kind": "countEventsThisTurn",
                    "event": "permanentLeftBattlefield",
                    "where": not(card_type("Land")),
                }),
                integer(1),
            )),
            effects,
        ));
    }

    if text == "Whenever this creature attacks, you gain 1 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "declaredAttacker",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(1),
                }],
            }),
            &["Resolve source attack", "Gain life"],
        ));
    }

    if text
        == "When this creature dies, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "searchLibrary",
                        "player": controller(),
                        "where": {
                            "kind": "typeLineContains",
                            "value": "Basic Land",
                        },
                        "maximum": 1,
                        "destination": "battlefield",
                        "tapped": true,
                    },
                    "onPerformed": [],
                }],
            }),
            &["Resolve source death", "Offer basic-land search"],
        ));
    }

    let death_token = match text {
        "Whenever another nontoken creature you control dies, create a 3/1 black and red Graveborn creature token with haste." => {
            Some((
                "anotherNontokenCreatureYouControl",
                1,
                "Graveborn",
                3,
                1,
                vec!["Black", "Red"],
                vec!["haste"],
            ))
        }
        "Whenever another creature you control dies, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" => {
            Some((
                "anotherCreatureYouControl",
                1,
                "Treasure",
                0,
                0,
                vec![],
                vec![],
            ))
        }
        "When this creature dies, create a 1/1 black and green Insect creature token with flying." => {
            Some((
                "thisCreature",
                1,
                "Insect",
                1,
                1,
                vec!["Black", "Green"],
                vec!["flying"],
            ))
        }
        "When this creature dies, create three 1/1 green Saproling creature tokens." => {
            Some(("thisCreature", 3, "Saproling", 1, 1, vec!["Green"], vec![]))
        }
        _ => None,
    };
    if let Some((scope, quantity, subtype, power, toughness, colors, abilities)) = death_token {
        let event = match scope {
            "thisCreature" => json!({
                "kind": "permanentDied",
                "object": self_ref(),
            }),
            "anotherNontokenCreatureYouControl" => json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Creature"),
                "excludeSource": true,
                "nontoken": true,
            }),
            _ => json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Creature"),
                "excludeSource": true,
            }),
        };
        let token = if subtype == "Treasure" {
            json!({
                "kind": "namedToken",
                "name": "Treasure",
            })
        } else {
            json!({
                "types": ["Creature"],
                "subtypes": [subtype],
                "colors": colors,
                "power": power,
                "toughness": toughness,
                "abilities": abilities.into_iter().map(|kind| json!({ "kind": kind })).collect::<Vec<_>>(),
            })
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(quantity),
                    "token": token,
                }],
            }),
            &[
                "Resolve permanent-died event",
                "Constrain dying permanent",
                "Resolve token characteristics",
                "Create tokens under source controller",
            ],
        ));
    }
    let death_life = match text {
        "Whenever another creature you control dies, each opponent loses 1 life." => {
            Some(("controlled", true, false))
        }
        "Whenever this creature or another creature or planeswalker you control dies, each opponent loses 1 life and you gain 1 life." => {
            Some(("controlledCreatureOrPlaneswalker", false, true))
        }
        "Whenever this creature or another creature you control dies, each opponent loses 1 life and you gain 1 life." => {
            Some(("controlled", false, true))
        }
        _ => None,
    };
    if let Some((scope, exclude_source, gain_life)) = death_life {
        let where_filter = if scope == "controlledCreatureOrPlaneswalker" {
            or(vec![card_type("Creature"), card_type("Planeswalker")])
        } else {
            card_type("Creature")
        };
        let mut effects = vec![json!({
            "kind": "loseLifeEachOpponent",
            "amount": integer(1),
        })];
        if gain_life {
            effects.push(json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(1),
            }));
        }
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": where_filter,
                    "excludeSource": exclude_source,
                },
                "effects": effects,
            }),
            &[
                "Resolve controlled permanent death",
                "Apply opponent life loss",
                "Apply controller life gain",
            ],
        ));
    }

    if text == "Whenever another creature dies, you may gain 1 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                    "excludeSource": true,
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(1),
                    },
                    "onPerformed": [],
                }],
            }),
            &["Resolve another creature death", "Offer optional life gain"],
        ));
    }

    let death_counter = match text {
        "Whenever a creature dies, put a charge counter on this enchantment." => {
            Some(("charge", false, false))
        }
        "Whenever another creature you control dies, put a +1/+1 counter on this creature." => {
            Some(("+1/+1", true, true))
        }
        _ => None,
    };
    if let Some((counter, controlled, exclude_source)) = death_counter {
        let mut event = json!({
            "kind": "permanentDied",
            "where": card_type("Creature"),
            "excludeSource": exclude_source,
        });
        if controlled {
            event["player"] = controller();
        }
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": [{
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": counter,
                    "count": integer(1),
                }],
            }),
            &["Resolve creature death", "Put counter on source"],
        ));
    }

    if text == "Whenever you sacrifice a creature, draw a card." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "reason": "sacrificed",
                },
                "effects": [{
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                }],
            }),
            &["Resolve controlled creature sacrifice", "Draw a card"],
        ));
    }

    let life_gain_trigger = match text {
        "Whenever you gain life, each opponent loses 1 life." => Some(("loseOpponents", "")),
        "Whenever you gain life, put a charge counter on Excalibur II." => {
            Some(("counter", "charge"))
        }
        _ => None,
    };
    if let Some((effect_kind, counter)) = life_gain_trigger {
        let effects = if effect_kind == "counter" {
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": counter,
                "count": integer(1),
            })]
        } else {
            vec![json!({
                "kind": "loseLifeEachOpponent",
                "amount": integer(1),
            })]
        };
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "lifeGained",
                    "player": controller(),
                },
                "effects": effects,
            }),
            &["Resolve controller life gain", "Resolve life-gain trigger"],
        ));
    }

    if text == "Whenever another creature enters, you may gain 1 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "where": card_type("Creature"),
                    "excludeSource": true,
                    "anyController": true,
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(1),
                    },
                    "onPerformed": [],
                }],
            }),
            &["Resolve any creature entry", "Offer one life"],
        ));
    }

    if text
        == "Whenever you cast an instant or sorcery spell, create a 1/1 red Elemental creature token."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Elemental"],
                        "colors": ["Red"],
                        "power": 1,
                        "toughness": 1,
                    },
                }],
            }),
            &[
                "Resolve controller spell-cast event",
                "Constrain instant or sorcery",
                "Create Elemental token",
            ],
        ));
    }

    if text
        == "Whenever you cast a creature spell, create X 1/1 black Thrull creature tokens, where X is that spell's mana value."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "spellCast",
                    "player": controller(),
                    "where": card_type("Creature"),
                },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": {
                        "kind": "triggeringSpellManaValue",
                    },
                    "token": {
                        "types": ["Creature"],
                        "subtypes": ["Thrull"],
                        "colors": ["Black"],
                        "power": 1,
                        "toughness": 1,
                    },
                }],
            }),
            &[
                "Resolve controller creature-spell cast",
                "Bind triggering spell mana value",
                "Create that many Thrull tokens",
            ],
        ));
    }

    let endrek_threshold_re = Regex::new(
        r"^When you control seven or more Thrulls, sacrifice Endrek Sahr(?:, Master Breeder)?\.$",
    )
    .expect("Endrek threshold regex compiles");
    if endrek_threshold_re.is_match(text) {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stateConditionMet",
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countPermanents",
                            "player": controller(),
                            "where": subtype("Thrull"),
                        }),
                        integer(7),
                    ),
                },
                "effects": [{
                    "kind": "sacrificePermanent",
                    "permanent": self_ref(),
                }],
            }),
            &[
                "Count controlled Thrulls",
                "Trigger at seven or more",
                "Sacrifice ability source",
            ],
        ));
    }

    if text == "When this land enters, you gain 1 life." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(1),
                }],
            }),
            &[
                "Resolve land enter event",
                "Resolve source controller",
                "Apply one life gain",
            ],
        ));
    }

    let entered_life_scope = match text {
        "Whenever this creature or another creature you control enters, you gain 1 life." => {
            Some((card_type("Creature"), false))
        }
        "Whenever another creature you control enters, you gain 1 life." => {
            Some((card_type("Creature"), true))
        }
        "Whenever Haliya or another creature or artifact you control enters, you gain 1 life." => {
            Some((
                or(vec![card_type("Creature"), card_type("Artifact")]),
                false,
            ))
        }
        _ => None,
    };
    if let Some((where_filter, exclude_source)) = entered_life_scope {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": where_filter,
                    "excludeSource": exclude_source,
                },
                "effects": [{
                    "kind": "gainLife",
                    "player": controller(),
                    "amount": integer(1),
                }],
            }),
            &[
                "Resolve permanent-entered event",
                "Constrain entering permanent",
                "Apply controller life gain",
            ],
        ));
    }

    if text
        == "Whenever another creature you control enters, this creature deals 1 damage to each opponent."
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
                },
                "effects": [{
                    "kind": "dealDamageToEachOpponent",
                    "source": self_ref(),
                    "amount": integer(1),
                }],
            }),
            &[
                "Resolve another-creature-entered event",
                "Select each opponent",
                "Apply source damage",
            ],
        ));
    }

    if text
        == "Whenever a creature you control enters, put a +1/+1 counter on each creature you control."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": false,
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                    },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            }),
            &[
                "Resolve creature-entered event",
                "Select controlled creatures",
                "Put one counter on each",
            ],
        ));
    }

    let entered_token = match text {
        "When Moseo enters, create a 1/1 black and green Pest creature token with \"Whenever this token attacks, you gain 1 life.\"" => {
            Some((
                1,
                "Pest",
                1,
                1,
                vec!["Black", "Green"],
                vec![json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "declaredAttacker",
                        "object": self_ref(),
                    },
                    "effects": [{
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(1),
                    }],
                })],
            ))
        }
        "When this creature enters, create a 1/1 black Rat creature token with \"This token can't block.\"" => {
            Some((
                1,
                "Rat",
                1,
                1,
                vec!["Black"],
                vec![json!({ "kind": "cantBlock" })],
            ))
        }
        "When Wort enters, create two 1/1 red and green Goblin Warrior creature tokens." => {
            Some((2, "Goblin Warrior", 1, 1, vec!["Red", "Green"], vec![]))
        }
        _ => None,
    };
    if let Some((quantity, subtype, power, toughness, colors, abilities)) = entered_token {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(quantity),
                    "token": {
                        "types": ["Creature"],
                        "subtypes": [subtype],
                        "colors": colors,
                        "power": power,
                        "toughness": toughness,
                        "abilities": abilities,
                    },
                }],
            }),
            &[
                "Resolve source enter event",
                "Resolve token characteristics",
                "Create tokens",
            ],
        ));
    }

    if text
        == "When this creature enters, mill three cards and you gain 3 life. (To mill three cards, put the top three cards of your library into your graveyard.)"
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [
                    {
                        "kind": "mill",
                        "player": controller(),
                        "count": integer(3),
                    },
                    {
                        "kind": "gainLife",
                        "player": controller(),
                        "amount": integer(3),
                    },
                ],
            }),
            &[
                "Resolve source enter event",
                "Mill three cards",
                "Gain three life",
            ],
        ));
    }

    if text.starts_with("When this artifact enters, mill a card.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [
                    {
                        "kind": "mill",
                        "player": controller(),
                        "count": 1,
                        "bind": "milledCards",
                    },
                    {
                        "kind": "grantPermission",
                        "player": controller(),
                        "action": {
                            "kind": "play",
                            "card": {
                                "kind": "singleBoundObject",
                                "binding": "milledCards",
                            },
                            "normalTimingApplies": true,
                            "normalCostsApply": true,
                        },
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                ],
            }),
            &[
                "Resolve enter-battlefield event",
                "Resolve mill vocabulary and bind result",
                "Install same-turn play permission",
            ],
        ));
    }

    if text
        == "When this creature enters, if you cast it, you may put a card you own from outside the game into your hand."
    {
        return Some(draft(
            json!({
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
                "effects": [
                    {
                        "kind": "chooseCards",
                        "id": "outsideCard",
                        "player": controller(),
                        "minimum": 0,
                        "maximum": 1,
                        "candidates": {
                            "kind": "cards",
                            "zone": { "kind": "outsideGame" },
                            "where": {
                                "kind": "ownedBy",
                                "player": controller(),
                            },
                        },
                    },
                    {
                        "kind": "moveCards",
                        "cards": decision_result("outsideCard"),
                        "to": {
                            "kind": "hand",
                            "player": controller(),
                        },
                    },
                ],
            }),
            &[
                "Resolve enter-battlefield event",
                "Attach intervening cast condition",
                "Resolve optional outside-game card choice",
                "Move chosen card to hand",
            ],
        ));
    }

    if text
        == "Whenever this creature attacks, you may exile eight cards from your graveyard. If you do, this creature becomes prepared."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "declaredAttacker",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "optionalAction",
                    "player": controller(),
                    "action": {
                        "kind": "exileCards",
                        "count": 8,
                        "from": graveyard(controller()),
                    },
                    "onPerformed": [{
                        "kind": "setPrepared",
                        "object": self_ref(),
                        "value": true,
                    }],
                }],
            }),
            &[
                "Resolve declared-attacker event",
                "Resolve optional exact-eight-card exile",
                "Attach if-performed branch",
                "Resolve prepared vocabulary",
            ],
        ));
    }

    if text == "When this land enters, choose a land card name." {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "enterBattlefield",
                    "object": self_ref(),
                },
                "effects": [{
                    "kind": "chooseCardName",
                    "id": "chosenLandName",
                    "player": controller(),
                    "where": card_type("Land"),
                    "persistOn": self_ref(),
                }],
            }),
            &[
                "Resolve enter-battlefield event",
                "Constrain card-name choice to lands",
                "Persist chosen name on source",
            ],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_avatar_deck_trigger(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let operation = |event: Value, operation: &str| {
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
            &[
                "Resolve an Avatar deck trigger event",
                "Preserve triggering object context",
                "Apply the complete ordered instruction",
            ],
        )
    };
    let enter_self = || json!({ "kind": "enterBattlefield", "object": self_ref() });
    let upkeep = |player: Value| {
        json!({
            "kind": "stepBegan",
            "step": "upkeep",
            "player": player,
        })
    };

    match text {
        "Whenever equipped creature attacks, look at the top six cards of your library. You may reveal an artifact card from among them and put it into your hand. Put the rest on the bottom of your library in a random order." => {
            Some(operation(
                json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
                "adaptiveOmnitoolAttack",
            ))
        }
        "Whenever a goaded attacking or blocking creature dies, you create a Treasure token." => {
            Some(operation(
                json!({ "kind": "permanentDied", "anyPlayer": true, "where": card_type("Creature") }),
                "baelothTreasure",
            ))
        }
        "Whenever equipped creature deals combat damage to a player or battle, create a Treasure token." => {
            Some(operation(
                json!({ "kind": "controlledCreaturesCombatDamageToPlayer", "player": controller() }),
                "beamtownTreasure",
            ))
        }
        value if value.starts_with("When this Class enters, create a colorless Equipment artifact token named Sword") => {
            Some(operation(enter_self(), "blacksmithCreateSword"))
        }
        "At the beginning of combat on your turn, attach target Equipment you control to up to one target creature you control." => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
                "blacksmithAttachEquipment",
            ))
        }
        "When this creature enters, you take the initiative." => {
            Some(operation(enter_self(), "takeInitiative"))
        }
        "Whenever you cast a noncreature spell, goad target creature an opponent controls. (Until your next turn, that creature attacks each combat if able and attacks a player other than you if able.)" => {
            Some(operation(
                json!({ "kind": "spellCast", "player": controller(), "where": not(card_type("Creature")) }),
                "quasitGoadCreature",
            ))
        }
        "Whenever equipped creature attacks, if it's the first combat phase of the turn, untap it. After this phase, there is an additional combat phase." => {
            Some(operation(
                json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
                "genjiGloveCombat",
            ))
        }
        "Whenever equipped creature becomes blocked by a creature, you may draw two cards." => {
            Some(operation(
                json!({ "kind": "attachedCreatureBecameBlocked", "attachment": self_ref() }),
                "infiltrationLensDraw",
            ))
        }
        "Whenever this creature or equipped creature deals combat damage to a player, goad each creature that player controls." => {
            Some(operation(
                json!({ "kind": "controlledCreaturesCombatDamageToPlayer", "player": controller() }),
                "komainuGoadDefender",
            ))
        }
        value if value.starts_with("Whenever enchanted creature becomes blocked, you may have it deal damage equal to its power") => {
            Some(operation(
                json!({ "kind": "attachedCreatureBecameBlocked", "attachment": self_ref() }),
                "laccolithRigDamage",
            ))
        }
        "When this Aura enters, enchanted creature deals damage equal to its power to any other target." => {
            Some(operation(enter_self(), "painForAllEntryDamage"))
        }
        "Whenever enchanted creature is dealt damage, it deals that much damage to each opponent." => {
            Some(operation(
                json!({ "kind": "attachedPermanentDealtDamage", "attachment": self_ref() }),
                "painForAllRetaliate",
            ))
        }
        value if value.starts_with("Whenever an opponent casts a spell, you may reveal the top card of your library") => {
            Some(operation(
                json!({ "kind": "spellCast", "anyPlayer": true, "where": Value::Null }),
                "powerbalanceCast",
            ))
        }
        "At the beginning of your upkeep, sacrifice this artifact. When you do, target creature you control can't be blocked this turn." => {
            Some(operation(upkeep(controller()), "smokeBombSacrifice"))
        }
        "Whenever you play a land or cast a spell, draw a card." => {
            Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        { "kind": "permanentEntered", "player": controller(), "where": card_type("Land") },
                        { "kind": "spellCast", "player": controller(), "where": Value::Null },
                    ],
                }),
                "endstoneDraw",
            ))
        }
        "At the beginning of your end step, your life total becomes half your starting life total, rounded up." => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "endstoneSetLife",
            ))
        }
        "Whenever an opponent taps an artifact for mana, gain control of that artifact until the end of your next turn." => {
            Some(operation(
                json!({ "kind": "controlledPermanentManaAbilityActivated", "anyPlayer": true, "where": card_type("Artifact") }),
                "treasureNabberControl",
            ))
        }
        "Whenever a Mountain enters the battlefield under your control, this emblem deals 4 damage to any target." => {
            Some(operation(
                json!({ "kind": "permanentEntered", "player": controller(), "where": subtype("Mountain") }),
                "kothEmblemDamage",
            ))
        }
        "Whenever one or more creatures a player controls deal combat damage to you, that player takes the initiative." => {
            Some(operation(
                json!({ "kind": "combatDamageReceived", "player": controller() }),
                "initiativeTakenByAttacker",
            ))
        }
        value if value.starts_with("Whenever you take the initiative and at the beginning of your upkeep, venture into Undercity") => {
            Some(operation(upkeep(controller()), "ventureUndercity"))
        }
        "When this enchantment enters, exile up to one other target nonland permanent until this enchantment leaves the battlefield." => {
            Some(operation(enter_self(), "aangsIcebergExile"))
        }
        "At the beginning of your end step, if this enchantment has four or more quest counters on it, exile up to one target creature you control, then return it to the battlefield under its owner's control." => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "airbenderAscensionBlink",
            ))
        }
        "When this creature dies, create X 1/1 blue Squid creature tokens with islandwalk, where X is the number of +1/+1 counters on this creature. (They can't be blocked as long as defending player controls an Island.)" => {
            Some(operation(
                json!({ "kind": "permanentDied", "object": self_ref() }),
                "chasmSkulkerSquids",
            ))
        }
        "At the beginning of combat on your turn, each creature you control with a counter on it gains firebending 2 until end of turn. (Whenever it attacks, add {R}{R}. This mana lasts until end of combat.)" => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
                "irohGrantFirebending",
            ))
        }
        "When Katara enters, draw a card, then discard a card unless her additional cost was paid." => {
            Some(operation(enter_self(), "kataraSeekingRevengeLoot"))
        }
        "Whenever another creature you control becomes the target of a spell or ability, you may airbend that creature. (Exile it. While it's exiled, its owner may cast it for {2} rather than its mana cost.)" => {
            Some(operation(
                json!({
                    "kind": "controlledObjectBecameTarget",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "monkGyatsoAirbend",
            ))
        }
        "At the beginning of combat on your turn, if you've drawn more than one card this turn, put X +1/+1 counters on target creature you control, where X is the number of cards you've drawn this turn minus one." => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
                "proftsMemoryCounters",
            ))
        }
        "Whenever you cast a spell, create a 1/1 colorless Spirit creature token with \"This token can't block or be blocked by non-Spirit creatures.\"" => {
            Some(operation(
                json!({ "kind": "spellCast", "player": controller(), "where": Value::Null }),
                "legendOfKurukSpirit",
            ))
        }
        "Whenever a Tentacle you control dies, untap up to one target Kraken and put a stun counter on up to one target nonland permanent." => {
            Some(operation(
                json!({ "kind": "permanentDied", "player": controller(), "where": subtype("Tentacle") }),
                "watcherTentacleDied",
            ))
        }
        "Whenever you cast a spell during combat, you get an experience counter." => {
            Some(operation(
                json!({ "kind": "spellCast", "player": controller(), "where": Value::Null }),
                "zukoCombatExperience",
            ))
        }
        "At the beginning of each upkeep, you may transform Aang, Master of Elements. If you do, you gain 4 life, draw four cards, put four +1/+1 counters on him, and he deals 4 damage to each opponent." => {
            Some(operation(
                json!({ "kind": "stepBegan", "step": "upkeep", "player": { "kind": "eachPlayer" } }),
                "avatarAangUpkeep",
            ))
        }
        "Whenever a permanent you control enters tapped, untap it." => Some(operation(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": Value::Null,
            }),
            "untapEnteredPermanent",
        )),
        "Whenever you tap a creature for mana, add an additional {G}." => Some(operation(
            json!({
                "kind": "controlledPermanentManaAbilityActivated",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            "badgermoleAdditionalGreen",
        )),
        "Whenever you attack a player with one or more creatures with power 4 or greater, draw a card." => {
            Some(operation(
                json!({
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                }),
                "drawForLargeAttacker",
            ))
        }
        "Whenever Bumi deals combat damage to a player, untap all lands you control. After this phase, there is an additional combat phase. Only land creatures can attack during that combat phase." => {
            Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "bumiAdditionalCombat",
            ))
        }
        "When Lumra enters, mill four cards. Then return all land cards from your graveyard to the battlefield tapped." => {
            Some(operation(enter_self(), "lumraReturnsLands"))
        }
        "When this artifact enters, each opponent sacrifices three creatures of their choice." => {
            Some(operation(enter_self(), "portalSacrificeThree"))
        }
        "At the beginning of your upkeep, put target creature card from a graveyard onto the battlefield under your control. It's a Phyrexian in addition to its other types." => {
            Some(operation(upkeep(controller()), "portalReanimateCreature"))
        }
        "At the beginning of each upkeep, untap all creatures and lands." => Some(operation(
            upkeep(json!({ "kind": "eachPlayer" })),
            "awakeningUntapAll",
        )),
        "When Dark Depths has no ice counters on it, sacrifice it. If you do, create Marit Lage, a legendary 20/20 black Avatar creature token with flying and indestructible." =>
        {
            let mut rule = operation(
                json!({
                    "kind": "stateConditionMet",
                    "condition": compare(
                        "==",
                        json!({
                            "kind": "countCounters",
                            "object": self_ref(),
                            "counter": "ice",
                        }),
                        integer(0),
                    ),
                }),
                "createMaritLage",
            );
            rule.rule["condition"] = compare(
                "==",
                json!({
                    "kind": "countCounters",
                    "object": self_ref(),
                    "counter": "ice",
                }),
                integer(0),
            );
            Some(rule)
        }
        "Whenever an opponent plays a land, you may put a land card from your hand onto the battlefield." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "anyController": true,
                    "where": card_type("Land"),
                }),
                "burgeoningLand",
            ))
        }
        "Whenever you tap a land for mana, add one mana of any type that land produced." => {
            Some(operation(
                json!({
                    "kind": "controlledPermanentManaAbilityActivated",
                    "player": controller(),
                    "where": card_type("Land"),
                }),
                "mirarisWakeAdditionalMana",
            ))
        }
        "When this enchantment enters, create X 1/1 colorless Shapeshifter creature tokens with changeling. (They're every creature type.)" => {
            Some(operation(enter_self(), "springleafParadeTokens"))
        }
        "At the beginning of your first main phase, look at the top card of your library. You may reveal that card if it has three or more colored mana symbols in its mana cost. If you do, add three mana in any combination of its colors and put it into your hand. If you don't reveal it, put it into your hand." => {
            Some(operation(
                json!({
                    "kind": "stepBegan",
                    "step": "precombatMain",
                    "player": controller(),
                }),
                "omnathTopCardMana",
            ))
        }
        value
            if value.contains(
                "Whenever a land you control enters, put a +1/+1 counter on target creature",
            ) =>
        {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                }),
                "bristlyBillLandfall",
            ))
        }
        "When this artifact enters or leaves the battlefield, exile the top card of your library. Until end of turn, you may play that card." => {
            Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        enter_self(),
                        {
                            "kind": "permanentLeftBattlefield",
                            "object": self_ref(),
                        },
                    ],
                }),
                "experimentalSynthesizerImpulse",
            ))
        }
        "Whenever a creature enters, if there are two or more other creatures on the battlefield, exile that creature. Return that card to the battlefield under its owner's control when this artifact leaves the battlefield." => {
            Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "permanentEntered",
                            "anyController": true,
                            "where": card_type("Creature"),
                        },
                        {
                            "kind": "permanentLeftBattlefield",
                            "object": self_ref(),
                        },
                    ],
                }),
                "portcullisExileOrReturn",
            ))
        }
        "When this Equipment enters, attach it to target creature you control. That creature gains shroud until end of turn. (It can't be the target of spells or abilities.)" => {
            Some(operation(enter_self(), "silverShroudAttach"))
        }
        "Whenever you draw a card, target opponent mills two cards. If two nonland cards that share a color were milled this way, repeat this process." => {
            Some(operation(
                json!({ "kind": "cardDrawn", "player": controller() }),
                "sphinxsTutelageMill",
            ))
        }
        "When you next cast an instant or sorcery spell this turn, copy that spell X times. You may choose new targets for the copies." => {
            Some(draft(
                json!({
                    "kind": "spellAbility",
                    "source": self_ref(),
                    "declaration": {
                        "kind": "castingDeclaration",
                        "decisions": [{
                            "id": "xValue",
                            "kind": "chooseNumber",
                            "minimum": 0,
                        }],
                    },
                    "effects": [{
                        "kind": "resolveSpellInstruction",
                        "operation": "installStormKingsThunder",
                    }],
                }),
                &[
                    "Declare the spell's X value",
                    "Install a one-shot instant-or-sorcery cast trigger",
                    "Copy the next qualifying spell X times",
                ],
            ))
        }
        "Whenever you cast a permanent spell with a mana cost that contains {X}, double the value of X." => {
            Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": not(or(vec![card_type("Instant"), card_type("Sorcery")])),
                }),
                "unboundFlourishingDoubleX",
            ))
        }
        "Whenever you cast an instant or sorcery spell or activate an ability, if that spell's mana cost or that ability's activation cost contains {X}, copy that spell or ability. You may choose new targets for the copy." => {
            Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
                }),
                "unboundFlourishingCopyX",
            ))
        }
        "Whenever you cast a spell, earthbend 1. If that spell is a Lesson, put an additional +1/+1 counter on that land. (Target land you control becomes a 0/0 creature with haste that's still a land. Put a +1/+1 counter on it. When it dies or is exiled, return it to the battlefield tapped.)" => {
            Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": Value::Null,
                }),
                "tophTeacherEarthbend",
            ))
        }
        "At the beginning of each opponent's upkeep, you may have that player gain control of equipped creature until end of turn. If you do, untap it." => {
            Some(operation(
                upkeep(json!({ "kind": "eachPlayer" })),
                "assaultSuitUpkeepControl",
            ))
        }
        "At the beginning of your upkeep, sacrifice this enchantment unless you discard a card." => {
            Some(operation(
                upkeep(controller()),
                "solitaryConfinementUpkeep",
            ))
        }
        "Whenever you put one or more +1/+1 counters on a creature, you may gain that much life. Do this only once each turn." => {
            let mut rule = operation(
                json!({
                    "kind": "countersPlaced",
                    "player": controller(),
                    "counter": "+1/+1",
                    "where": card_type("Creature"),
                }),
                "earthKingdomGeneralLife",
            );
            rule.rule["triggerLimit"] = json!({
                "kind": "onceEachTurn",
                "id": "earthKingdomGeneralLife",
            });
            Some(rule)
        }
        "Whenever an opponent casts their first noncreature spell each turn, draw a card unless that player pays {X}, where X is this creature's power." => {
            Some(operation(
                json!({
                    "kind": "spellCast",
                    "anyPlayer": true,
                    "where": not(card_type("Creature")),
                }),
                "esperSentinelTaxDraw",
            ))
        }
        "When this Vehicle enters, exile target player's graveyard." => {
            Some(operation(enter_self(), "nautiloidExileGraveyard"))
        }
        "Whenever this Vehicle deals combat damage to a player, you may put a creature card exiled with this Vehicle onto the battlefield under your control." => {
            Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "nautiloidReanimateExiledCreature",
            ))
        }
        "When another creature you control leaves the battlefield, transform Aang at the beginning of the next upkeep." => {
            Some(operation(
                json!({
                    "kind": "controlledPermanentLeftBattlefield",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "delayAangTransform",
            ))
        }
        "When this land enters, sacrifice it. When you do, search your library for a basic Forest, Plains, or Island card, put it onto the battlefield tapped, then shuffle and you gain 1 life." => {
            Some(operation(enter_self(), "brokersHideoutFetch"))
        }
        "When this creature enters, exile up to one target artifact, creature, or enchantment an opponent controls with mana value 3 or greater until this creature leaves the battlefield." => {
            Some(operation(enter_self(), "earthKingdomJailerExile"))
        }
        "Whenever Jet attacks, look at the top five cards of your library. You may put a creature card with mana value 3 or less from among them onto the battlefield tapped and attacking. Put the rest on the bottom of your library in a random order." => {
            Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "jetAttackDeploy",
            ))
        }
        "Whenever you cast a spell during an opponent's turn, you get an experience counter." => {
            Some(operation(
                json!({ "kind": "spellCast", "player": controller(), "where": Value::Null }),
                "kataraOpponentTurnExperience",
            ))
        }
        "Whenever Katara attacks, you may draw a card for each experience counter you have. If you do, discard a card." => {
            Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "kataraAttackDrawDiscard",
            ))
        }
        "Whenever a creature you control of the chosen type enters or attacks, draw a card." => {
            Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "permanentEntered",
                            "player": controller(),
                            "where": card_type("Creature"),
                        },
                        { "kind": "controlledCreaturesAttacked", "player": controller() },
                    ],
                }),
                "kindredDiscoveryDraw",
            ))
        }
        "Whenever this creature or another Ally you control enters, you gain 1 life. If this is the second time this ability has resolved this turn, draw a card." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "southPoleVoyagerAlly",
            ))
        }
        "Whenever this creature or another Ally you control enters, you may have this creature deal damage to target creature with flying equal to the number of Allies you control." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "tajuruArcherAlly",
            ))
        }
        "Whenever a nontoken creature you control enters, put a +1/+1 counter on it and draw a card." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "banyanTreeGrowth",
            ))
        }
        "Whenever one or more creatures you control with power 4 or greater attack, search your library for up to that many basic land cards, put them onto the battlefield tapped, then shuffle." => {
            Some(operation(
                json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
                "earthKingAttackRamp",
            ))
        }
        "Whenever this creature or another Ally you control enters, you may create a 2/2 green Wolf creature token. If you do, put a +1/+1 counter on this creature." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "turntimberRangerAlly",
            ))
        }
        "When Ty Lee enters, tap up to one target creature. It doesn't untap during its controller's untap step for as long as you control Ty Lee." => {
            Some(operation(enter_self(), "tyLeeFreezeCreature"))
        }
        "Whenever you or a permanent you control becomes the target of a spell or ability an opponent controls, counter that spell or ability unless its controller pays {1}." => {
            Some(operation(
                json!({ "kind": "controlledObjectBecameTarget", "player": controller() }),
                "unsettledMarinerTax",
            ))
        }
        "At the beginning of your next upkeep, pay {1}{W}{W}. If you don't, you lose the game." => {
            Some(draft(
                json!({
                    "kind": "spellAbility",
                    "source": self_ref(),
                    "effects": [{
                        "kind": "resolveSpellInstruction",
                        "operation": "installInterventionPactPayment",
                    }],
                }),
                &["Install the next-upkeep Intervention Pact payment"],
            ))
        }
        "At the beginning of your next upkeep, pay {3}{U}{U}. If you don't, you lose the game." => {
            Some(draft(
                json!({
                    "kind": "spellAbility",
                    "source": self_ref(),
                    "effects": [{
                        "kind": "resolveSpellInstruction",
                        "operation": "installNegationPactPayment",
                    }],
                }),
                &["Install the next-upkeep Pact of Negation payment"],
            ))
        }
        _ => None,
    }
}

pub(in crate::oracle::canonical) fn parse_avatar_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    if text
        == "Whenever you waterbend, earthbend, firebend, or airbend, draw a card. Then if you've done all four this turn, transform Avatar Aang."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "bendingPerformed",
                    "player": controller(),
                    "forms": ["waterbend", "earthbend", "firebend", "airbend"],
                },
                "effects": [
                    {
                        "kind": "drawCards",
                        "player": controller(),
                        "count": integer(1),
                    },
                    {
                        "kind": "resolveTriggeredInstruction",
                        "operation": "transformIfAllBendingForms",
                    },
                ],
            }),
            &[
                "Resolve any bending event controlled by the source controller",
                "Draw one card",
                "Transform after all four bending forms in the same turn",
            ],
        ));
    }

    let (event, instruction) = if let Some(captures) = Regex::new(r"^When .+ enters, (.+)$")
        .expect("avatar enter trigger regex compiles")
        .captures(text)
    {
        (
            json!({ "kind": "enterBattlefield", "object": self_ref() }),
            captures[1].to_string(),
        )
    } else if let Some(captures) =
        Regex::new(r"^At the beginning of (combat on your turn|your end step), (.+)$")
            .expect("avatar step trigger regex compiles")
            .captures(text)
    {
        let step = if &captures[1] == "combat on your turn" {
            "beginCombat"
        } else {
            "endStep"
        };
        (
            json!({
                "kind": "stepBegan",
                "step": step,
                "player": controller(),
            }),
            captures[2].to_string(),
        )
    } else if let Some(captures) = Regex::new(r"^Whenever another Ally you control enters, (.+)$")
        .expect("avatar Ally trigger regex compiles")
        .captures(text)
    {
        (
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": subtype("Ally"),
                "excludeSource": true,
            }),
            captures[1].to_string(),
        )
    } else if let Some(captures) =
        Regex::new(r"^Whenever you cast your second spell each turn, (.+)$")
            .expect("avatar second-spell trigger regex compiles")
            .captures(text)
    {
        (
            json!({
                "kind": "spellCastOrdinal",
                "player": controller(),
                "ordinal": 2,
            }),
            captures[1].to_string(),
        )
    } else {
        return None;
    };

    let instruction_without_reminder = instruction
        .split_once(". (")
        .map(|(instruction, _)| format!("{instruction}."))
        .unwrap_or(instruction);
    let earthbend_re =
        Regex::new(r"(?i)^earthbend ([^.]+)\.?$").expect("triggered earthbend regex compiles");
    if let Some(captures) = earthbend_re.captures(&instruction_without_reminder) {
        let quantity = avatar_quantity(&captures[1])?;
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "earthbendLand",
                        json!({
                            "kind": "permanents",
                            "controller": controller(),
                            "where": card_type("Land"),
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [earthbend_effect("earthbendLand", quantity)],
            }),
            &[
                "Resolve Avatar trigger event",
                "Declare controlled land target",
                "Resolve earthbend quantity and effect",
            ],
        ));
    }

    if instruction_without_reminder
        .to_ascii_lowercase()
        .starts_with("airbend ")
    {
        let decision = airbend_target_decision(&instruction_without_reminder)?;
        let candidates = decision["candidates"].clone();
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [decision],
                },
                "effects": [{
                    "kind": "airbend",
                    "object": chosen_target("airbendTarget"),
                    "candidates": candidates,
                    "alternativeManaCost": "{2}",
                }],
            }),
            &[
                "Resolve Avatar trigger event",
                "Declare airbend target",
                "Exile target and grant alternative casting cost",
            ],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_simple_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let triggered = |event: Value, effects: Vec<Value>| {
        draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": event,
                "effects": effects,
            }),
            &["Resolve trigger event", "Resolve ordered semantic effects"],
        )
    };
    let enter_event = json!({ "kind": "enterBattlefield", "object": self_ref() });

    let entered_untapped_re = Regex::new(
        r"(?i)^When (?:this (?:artifact|creature|enchantment|land|permanent)|[A-Z][^,]+) enters untapped, (.+)$",
    )
    .expect("entered-untapped trigger regex compiles");
    if let Some(captures) = entered_untapped_re.captures(text) {
        let (effects, decisions) = parse_general_effect_sequence(&captures[1], "")
            .or_else(|| parse_general_effect_instruction(&captures[1], ""))?;
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": enter_event.clone(),
            "condition": not(json!({ "kind": "isTapped", "object": self_ref() })),
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
                "Recognize an enters-untapped event",
                "Check the source's live tapped state",
                "Delegate the instruction to reusable effect grammar",
            ],
        ));
    }

    let attached_tapped_re = Regex::new(r"(?i)^Whenever equipped creature becomes tapped, (.+)$")
        .expect("equipped-creature tapped trigger regex compiles");
    if let Some(captures) = attached_tapped_re.captures(text) {
        let (mut effects, decisions) = parse_general_effect_sequence(&captures[1], "")
            .or_else(|| parse_general_effect_instruction(&captures[1], ""))?;
        for effect in &mut effects {
            if effect["kind"] == "dealDamageToEachOpponent" {
                effect["source"] = json!({ "kind": "triggeringPermanent" });
            }
        }
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "attachedPermanentTapped", "attachment": self_ref() },
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
                "Resolve the equipped permanent through the attachment relation",
                "Observe its tap event",
                "Delegate the instruction to reusable effect grammar",
            ],
        ));
    }

    if text.contains(
        "At the beginning of your end step, put a +1/+1 counter on each creature you control that didn't attack or enter this turn. Untap those creatures.",
    ) {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "endStep",
                "player": controller(),
            }),
            vec![json!({ "kind": "advancePeacefulCreatures" })],
        ));
    }

    if text.starts_with("Whenever this creature attacks, other modified creatures you control get +X/+X until end of turn, where X is this creature's power.")
    {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            vec![json!({ "kind": "boostModifiedCreaturesBySourcePower" })],
        ));
    }
    let self_died_event = json!({ "kind": "permanentDied", "object": self_ref() });

    if let Some(captures) = Regex::new(r"(?i)^When .+ enters, (create .+ token[s]?\.)$")
        .expect("simple enter token trigger regex compiles")
        .captures(text)
    {
        return Some(triggered(
            enter_event,
            vec![create_token_effect(&captures[1])?],
        ));
    }
    if let Some(captures) = Regex::new(&format!(
        r"^When .+ enters, draw ({}) cards?\.$",
        count_word_pattern(),
    ))
    .expect("simple enter draw trigger regex compiles")
    .captures(text)
    {
        return Some(triggered(
            enter_event,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(parse_number_word(&captures[1])?),
            })],
        ));
    }
    if text
        == "Whenever this creature or another Ally you control enters, you may put a +1/+1 counter on this creature."
    {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": subtype("Ally"),
            }),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                },
                "onPerformed": [],
            })],
        ));
    }
    if text == "At the beginning of your end step, if Katara is tapped, put a +1/+1 counter on her."
    {
        let mut rule = triggered(
            json!({
                "kind": "stepBegan",
                "step": "endStep",
                "player": controller(),
            }),
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        );
        rule.rule["condition"] = json!({ "kind": "isTapped", "object": self_ref() });
        return Some(rule);
    }
    if text
        == "At the beginning of your upkeep, create a 1/1 white Ally creature token for each experience counter you have."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "upkeep",
                "player": controller(),
            }),
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "countPlayerCounters",
                    "player": controller(),
                    "counter": "experience",
                },
                "token": {
                    "colors": ["white"],
                    "types": ["Creature"],
                    "subtypes": ["Ally"],
                    "power": 1,
                    "toughness": 1,
                },
            })],
        ));
    }
    if text
        == "When this enchantment enters, earthbend 2. Then search your library for a basic land card, put it onto the battlefield tapped, then shuffle."
    {
        let mut effects = vec![earthbend_effect("earthbendLand", integer(2))];
        effects.extend(search_library_effects(
            json!({ "kind": "typeLineContains", "value": "Basic Land" }),
            1,
            "battlefield",
            true,
        ));
        return Some(triggered(enter_event, effects));
    }
    if text
        == "Whenever equipped creature deals combat damage to a player, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")"
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
            vec![create_token_effect("Create a Treasure token.")?],
        ));
    }
    if text
        == "When this artifact enters or is put into a graveyard from the battlefield, draw a card."
    {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    enter_event.clone(),
                    {
                        "kind": "permanentLeftBattlefield",
                        "object": self_ref(),
                        "destination": "graveyard",
                    },
                ],
            }),
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever equipped creature deals combat damage to a player, you may draw a card." {
        return Some(triggered(
            json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [],
            })],
        ));
    }
    if text
        == "Whenever equipped creature deals combat damage to a player, you may draw two cards. If you do, discard a card."
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "drawThenDiscard",
                    "player": controller(),
                    "drawCount": integer(2),
                    "discardCount": integer(1),
                },
                "onPerformed": [],
            })],
        ));
    }
    if text
        == "Whenever equipped creature attacks, create a 4/4 white Angel creature token with flying."
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDeclaredAttacker" }),
            vec![create_token_effect(
                "Create a 4/4 white Angel creature token with flying.",
            )?],
        ));
    }
    if text
        == "Whenever enchanted creature attacks, you create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")"
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDeclaredAttacker" }),
            vec![create_token_effect("Create a Treasure token.")?],
        ));
    }
    if text
        == "Whenever equipped creature attacks, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle."
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDeclaredAttacker" }),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "mayRampBasicTapped",
            })],
        ));
    }
    if text == "When this creature dies, you may draw a card." {
        return Some(triggered(
            self_died_event,
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "drawCards",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [],
            })],
        ));
    }
    if text
        == "Whenever one or more creatures you control leave the battlefield without dying, you get an experience counter."
    {
        return Some(triggered(
            json!({
                "kind": "permanentLeftBattlefield",
                "player": controller(),
                "where": card_type("Creature"),
                "withoutDying": true,
            }),
            vec![json!({
                "kind": "addPlayerCounters",
                "player": controller(),
                "counter": "experience",
                "count": integer(1),
            })],
        ));
    }
    None
}

pub(in crate::oracle::canonical) fn parse_common_triggered_ability(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let text = text
        .strip_prefix("Imprint — ")
        .or_else(|| text.strip_prefix("Imprint â€” "))
        .or_else(|| text.strip_prefix("Imprint Ã¢â‚¬â€ "))
        .or_else(|| text.strip_prefix("Opus — "))
        .or_else(|| text.strip_prefix("Opus â€” "))
        .or_else(|| text.strip_prefix("Opus Ã¢â‚¬â€ "))
        .unwrap_or(text);
    let triggered = |event: Value, declaration: Option<Value>, effects: Vec<Value>| {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "effects": effects,
        });
        if let Some(declaration) = declaration {
            rule["declaration"] = declaration;
        }
        draft(
            rule,
            &[
                "Resolve the reusable trigger pattern",
                "Declare any required targets",
                "Apply ordered semantic effects",
            ],
        )
    };
    let enter_event = json!({ "kind": "enterBattlefield", "object": self_ref() });

    if text
        == "Whenever you cast an instant or sorcery spell, target player mills three cards. If five or more mana was spent to cast that spell, that player mills ten cards instead."
    {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": or(vec![card_type("Instant"), card_type("Sorcery")]),
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "conditionalEffect",
                "condition": compare(
                    ">=",
                    decision_result("triggeringSpellManaSpent"),
                    integer(5),
                ),
                "then": [{
                    "kind": "mill",
                    "player": chosen_target("targetPlayer"),
                    "count": integer(10),
                }],
                "else": [{
                    "kind": "mill",
                    "player": chosen_target("targetPlayer"),
                    "count": integer(3),
                }],
            })],
        ));
    }

    if text
        == "Whenever a creature dealt damage by this creature this turn dies, put a +1/+1 counter on this creature."
    {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "where": card_type("Creature"),
                "damagedBySourceThisTurn": true,
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": { "kind": "abilitySource" },
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "At the beginning of combat on your turn, you may pay {G}{U}. When you do, put a +1/+1 counter on another target creature you control, and that creature gains flying until end of turn."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "beginCombat",
                "player": controller(),
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "skyriderPatrolCombat",
            })],
        ));
    }

    if text
        == "At the beginning of each upkeep, you may exile target creature card from your graveyard. If you do, create a token that's a copy of that card, except it's a Spirit in addition to its other types. Exile it at the beginning of the next end step."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "upkeep",
                "player": { "kind": "eachPlayer" },
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "seanceCreatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": card_type("Creature"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "seanceCreateSpirit",
            })],
        ));
    }

    if text
        == "Whenever a nontoken creature dies, you may exile that card. If you do, return each other card exiled with this artifact to its owner's graveyard."
    {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "where": card_type("Creature"),
                "nontoken": true,
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "mimicVatImprint",
            })],
        ));
    }

    if text
        == "Whenever a creature you control dies, you may pay {1}. If you do, reveal cards from the top of your library until you reveal a creature card. Put that card into your hand and the rest into your graveyard."
    {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "fosterRevealCreature",
            })],
        ));
    }

    if text
        == "Whenever enchanted creature deals combat damage to a player, you may sacrifice this Aura. If you do, destroy target enchantment."
    {
        return Some(triggered(
            json!({
                "kind": "attachedPermanentCombatDamageToPlayer",
                "attachment": self_ref(),
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetEnchantment",
                    json!({
                        "kind": "permanents",
                        "where": card_type("Enchantment"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "mortalObstinacySacrifice",
            })],
        ));
    }

    if text
        == "At the beginning of the upkeep of enchanted permanent's controller, that player sacrifices it unless they pay {X}, where X is its mana value."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "upkeep",
                "player": {
                    "kind": "controllerOfAttachedPermanent",
                    "attachment": self_ref(),
                },
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "soulTitheUpkeep",
            })],
        ));
    }

    if text
        == "Whenever enchanted creature blocks or becomes blocked by a non-Wall creature, destroy the other creature at end of combat."
    {
        let non_wall_creature = and(vec![card_type("Creature"), not(subtype("Wall"))]);
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "attachedCreatureBecameBlocked",
                        "attachment": self_ref(),
                        "where": non_wall_creature,
                    },
                    {
                        "kind": "attachedCreatureBlocks",
                        "attachment": self_ref(),
                        "where": non_wall_creature,
                    },
                ],
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "venomDestroyAtEndCombat",
            })],
        ));
    }

    if text
        == "Whenever this Vehicle enters or attacks, exile up to one other target creature until this Vehicle leaves the battlefield. If a creature is put into exile this way, return each other card exiled with this Vehicle to the battlefield under its owner's control."
    {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    { "kind": "declaredAttacker", "object": self_ref() },
                ],
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "exileTarget",
                    json!({
                        "kind": "permanents",
                        "excludeSource": true,
                        "where": card_type("Creature"),
                    }),
                    0,
                    1,
                )],
            })),
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "limousineLinkedExile",
            })],
        ));
    }

    if text.starts_with("Increment ") && text.contains("Whenever you cast a spell") {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "resolveIncrement",
            })],
        ));
    }

    if text
        == "At the beginning of each player's end step, if an artifact entered the battlefield under your control this turn, look at the top two cards of your library. Put one of them into your hand and the other into your graveyard."
    {
        let mut result = triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": { "kind": "eachPlayer" } }),
            None,
            vec![
                json!({
                    "kind": "lookAtTopCards",
                    "zone": library(controller()),
                    "count": integer(2),
                    "bind": "akalLookedCards",
                }),
                json!({
                    "kind": "chooseCards",
                    "id": "akalCardForHand",
                    "player": controller(),
                    "from": bound_objects("akalLookedCards"),
                    "count": minimum(vec![integer(1), count_bound_objects("akalLookedCards")]),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": decision_result("akalCardForHand"),
                    "to": hand(controller()),
                }),
                json!({
                    "kind": "moveCards",
                    "cards": {
                        "kind": "setDifference",
                        "left": bound_objects("akalLookedCards"),
                        "right": decision_result("akalCardForHand"),
                    },
                    "to": graveyard(controller()),
                }),
            ],
        );
        result.rule["condition"] = compare(
            ">=",
            json!({
                "kind": "countEventsThisTurn",
                "event": "permanentEnteredBattlefield",
                "player": controller(),
                "where": card_type("Artifact"),
            }),
            integer(1),
        );
        return Some(result);
    }

    if text == "When this artifact enters or leaves the battlefield, draw a card." {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    { "kind": "permanentLeftBattlefield", "object": self_ref() },
                ],
            }),
            None,
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
        ));
    }

    if text
        == "Whenever equipped creature deals combat damage to a player, scry 1, then draw a card. (To scry 1, look at the top card of your library, then you may put that card on the bottom.)"
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentCombatDamageToPlayer", "attachment": self_ref() }),
            None,
            vec![
                json!({ "kind": "scry", "player": controller(), "count": integer(1) }),
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
            ],
        ));
    }

    if text
        == "Whenever another artifact you control enters, create a 2/2 colorless Robot artifact creature token. This ability triggers only once each turn."
    {
        let mut result = triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Artifact"),
                "excludeSource": true,
            }),
            None,
            vec![create_token_effect(
                "Create a 2/2 colorless Robot artifact creature token.",
            )?],
        );
        result.rule["triggerLimit"] =
            json!({ "kind": "onceEachTurn", "id": "mechanAssemblerArtifact" });
        return Some(result);
    }

    if text == "When this Spacecraft enters, it deals 10 damage to up to one target creature." {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    0,
                    1,
                )],
            })),
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("targetCreature"),
                "amount": integer(10),
            })],
        ));
    }

    if text
        == "When this land enters, target creature gets +1/+1 and gains vigilance until end of turn."
    {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            })),
            vec![
                json!({
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": integer(1),
                    "toughness": integer(1),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "grantKeyword",
                    "object": chosen_target("targetCreature"),
                    "keyword": "vigilance",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }

    if text
        == "When Lena enters, create a 1/1 white Soldier creature token for each nontoken creature you control."
    {
        return Some(triggered(
            enter_event.clone(),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": {
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), not(json!({ "kind": "isToken" }))]),
                },
                "token": {
                    "name": "Soldier Token",
                    "colors": ["white"],
                    "types": ["Creature"],
                    "subtypes": ["Soldier"],
                    "power": 1,
                    "toughness": 1,
                },
            })],
        ));
    }

    if text
        == "When this land enters, target creature an opponent controls doesn't untap during its controller's next untap step."
    {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
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
            })),
            vec![json!({
                "kind": "installUntapRestriction",
                "permanent": chosen_target("targetCreature"),
                "duration": { "kind": "nextUntapStep" },
            })],
        ));
    }

    if text
        == "When this Spacecraft enters, return up to two target non-Spacecraft creatures to their owners' hands."
    {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreatures",
                    json!({
                        "kind": "permanents",
                        "where": and(vec![
                            card_type("Creature"),
                            not(subtype("Spacecraft")),
                        ]),
                    }),
                    0,
                    2,
                )],
            })),
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": { "kind": "chosenTargets", "id": "targetCreatures" },
            })],
        ));
    }

    if text == "Whenever this Spacecraft attacks, defending player mills four cards." {
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "mill",
                "player": { "kind": "triggeringPlayer" },
                "count": integer(4),
            })],
        ));
    }

    if text
        == "Whenever you cast a historic spell, return target creature card with mana value 3 or less from your graveyard to the battlefield. (Artifacts, legendaries, and Sagas are historic.)"
    {
        return Some(triggered(
            json!({ "kind": "spellCast", "player": controller(), "where": { "kind": "historic" } }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreatureCard",
                    json!({
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": and(vec![
                            card_type("Creature"),
                            compare(
                                "<=",
                                json!({ "kind": "manaValueOf", "object": { "kind": "candidate" } }),
                                integer(3),
                            ),
                        ]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "moveTargetCard",
                "card": chosen_target("targetCreatureCard"),
                "to": "battlefield",
                "tapped": false,
                "controller": controller(),
            })],
        ));
    }

    if text
        == "Whenever you cast a historic spell, untap Traxos. (Artifacts, legendaries, and Sagas are historic.)"
    {
        return Some(triggered(
            json!({ "kind": "spellCast", "player": controller(), "where": { "kind": "historic" } }),
            None,
            vec![json!({ "kind": "untapPermanent", "permanent": self_ref() })],
        ));
    }

    if text
        == "When this creature dies, create a 3/3 colorless Golem artifact creature token with flying, a 3/3 colorless Golem artifact creature token with vigilance, and a 3/3 colorless Golem artifact creature token with trample."
    {
        return Some(triggered(
            json!({ "kind": "permanentDied", "object": self_ref() }),
            None,
            vec![
                create_token_effect(
                    "Create a 3/3 colorless Golem artifact creature token with flying.",
                )?,
                create_token_effect(
                    "Create a 3/3 colorless Golem artifact creature token with vigilance.",
                )?,
                create_token_effect(
                    "Create a 3/3 colorless Golem artifact creature token with trample.",
                )?,
            ],
        ));
    }

    if text == "When this Spacecraft enters, draw two cards, then discard a card." {
        return Some(triggered(
            enter_event.clone(),
            None,
            vec![json!({
                "kind": "drawThenDiscard",
                "player": controller(),
                "drawCount": integer(2),
                "discardCount": integer(1),
            })],
        ));
    }

    if text == "At the beginning of your end step, untap target artifact." {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetArtifact",
                    json!({ "kind": "permanents", "where": card_type("Artifact") }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "untapPermanent",
                "permanent": chosen_target("targetArtifact"),
            })],
        ));
    }

    if text
        == "Whenever you cast a historic spell, target player mills two cards. (Artifacts, legendaries, and Sagas are historic.)"
    {
        return Some(triggered(
            json!({ "kind": "spellCast", "player": controller(), "where": { "kind": "historic" } }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetPlayer",
                    json!({ "kind": "players" }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "mill",
                "player": chosen_target("targetPlayer"),
                "count": integer(2),
            })],
        ));
    }

    if text
        == "At the beginning of your end step, if you haven't cast a spell from your hand this turn, draw a card."
    {
        let mut result = triggered(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            None,
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
        );
        result.rule["condition"] = compare(
            "==",
            json!({
                "kind": "countEventsThisTurn",
                "event": "spellCast",
                "player": controller(),
                "fromZone": "hand",
            }),
            integer(0),
        );
        return Some(result);
    }

    if text
        == "Whenever you cast your second spell each turn, investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")"
    {
        return Some(triggered(
            json!({ "kind": "spellCastOrdinal", "player": controller(), "ordinal": integer(2) }),
            None,
            vec![create_token_effect("Create a Clue token.")?],
        ));
    }

    if text == "Whenever you cast your first spell during each opponent's turn, draw a card." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "duringOpponentTurn": true,
                "spellCastOrdinal": integer(1),
            }),
            None,
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
        ));
    }

    if text
        == "Whenever you put one or more +1/+1 counters on a creature you control, you may draw that many cards. Do this only once each turn."
    {
        let mut result = triggered(
            json!({
                "kind": "countersPlaced",
                "player": controller(),
                "counter": "+1/+1",
                "where": card_type("Creature"),
            }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "drawCounterCount",
            })],
        );
        result.rule["triggerLimit"] = json!({
            "kind": "onceEachTurn",
            "id": "terrasymbiosisDraw",
        });
        return Some(result);
    }

    if text
        == "At the beginning of your upkeep, if this enchantment has no charge counters on it, return it to its owner's hand."
    {
        let mut result = triggered(
            json!({
                "kind": "stepBegan",
                "step": "upkeep",
                "player": controller(),
            }),
            None,
            vec![json!({
                "kind": "returnToOwnersHand",
                "object": self_ref(),
            })],
        );
        result.rule["condition"] = compare(
            "==",
            json!({
                "kind": "countCounters",
                "object": self_ref(),
                "counter": "charge",
            }),
            integer(0),
        );
        return Some(result);
    }
    if text == "Whenever Grunn attacks alone, double its power and toughness until end of turn." {
        let mut result = triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": 1,
                "maximum": 1,
            }),
            None,
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": { "kind": "powerOf", "object": self_ref() },
                "toughness": { "kind": "toughnessOf", "object": self_ref() },
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        );
        result.rule["condition"] = json!({
            "kind": "isAttacking",
            "object": self_ref(),
        });
        return Some(result);
    }
    if text.starts_with("Enrage ")
        && text.contains("Whenever this creature is dealt damage, proliferate.")
    {
        return Some(triggered(
            json!({ "kind": "permanentDealtDamage", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "resolveSpellInstruction",
                "operation": "proliferateOnce",
            })],
        ));
    }
    if text == "Whenever a creature you control attacks alone, you may tap target creature." {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": 1,
                "maximum": 1,
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "targetCreature",
                    json!({ "kind": "permanents", "where": card_type("Creature") }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "optionalEffects",
                "player": controller(),
                "effects": [{
                    "kind": "tapPermanent",
                    "permanent": chosen_target("targetCreature"),
                }],
            })],
        ));
    }
    if text
        == "When enchanted creature dies, return that card to the battlefield under your control."
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDied", "attachment": self_ref() }),
            None,
            vec![json!({
                "kind": "moveTriggeringCardFromGraveyard",
                "to": "battlefield",
                "controller": controller(),
            })],
        ));
    }
    if text == "When enchanted land dies, return that card to its owner's hand." {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDied", "attachment": self_ref() }),
            None,
            vec![json!({
                "kind": "moveTriggeringCardFromGraveyard",
                "to": "hand",
            })],
        ));
    }

    if text == "Whenever you gain life, put a +1/+1 counter on this creature." {
        return Some(triggered(
            json!({
                "kind": "lifeGained",
                "player": controller(),
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text == "When this creature dies, put its counters on target creature you control." {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "object": self_ref(),
            }),
            Some(json!({
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
            })),
            vec![json!({
                "kind": "putSameCountersAs",
                "permanent": chosen_target("targetCreature"),
                "source": { "kind": "abilitySource" },
            })],
        ));
    }
    if text
        == "When this creature dies, you may exile it. When you do, return target creature card with mana value less than or equal to this creature's power from your graveyard to the battlefield."
    {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "object": self_ref(),
            }),
            None,
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "exileAbilitySourceFromGraveyard",
                    "object": { "kind": "abilitySource" },
                },
                "onPerformed": [{
                    "kind": "createReflexiveTriggeredAbility",
                    "player": controller(),
                    "decisionId": "targetCreatureCard",
                    "candidates": {
                        "kind": "cards",
                        "zone": graveyard(controller()),
                        "where": card_type("Creature"),
                    },
                    "maximumManaValue": {
                        "kind": "powerOf",
                        "object": { "kind": "abilitySource" },
                    },
                    "effects": [{
                        "kind": "moveTargetCard",
                        "card": chosen_target("targetCreatureCard"),
                        "to": "battlefield",
                        "tapped": false,
                    }],
                }],
            })],
        ));
    }
    if matches!(
        text,
        "Whenever you sacrifice a permanent, put a +1/+1 counter on Juri."
            | "Whenever you sacrifice a permanent, put a +1/+1 counter on this creature."
            | "Whenever you sacrifice another permanent, put a +1/+1 counter on this creature."
    ) {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": Value::Null,
                "reason": "sacrificed",
                "excludeSource": text.contains("another permanent"),
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever you cast a multicolored spell, draw a card." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": compare(
                    ">",
                    json!({
                        "kind": "colorCountOf",
                        "object": { "kind": "candidate" },
                    }),
                    integer(1),
                ),
            }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text
        == "Whenever you sacrifice an artifact, put a +1/+1 counter on this creature and add {R}."
    {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Artifact"),
                "reason": "sacrificed",
            }),
            None,
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                }),
                json!({
                    "kind": "addMana",
                    "player": controller(),
                    "mana": "{R}",
                }),
            ],
        ));
    }
    if text == "When Juri dies, it deals damage equal to its power to any target." {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "object": self_ref(),
            }),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "damageTarget",
                    json!({ "kind": "anyTarget" }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("damageTarget"),
                "amount": {
                    "kind": "powerOf",
                    "object": { "kind": "abilitySource" },
                },
            })],
        ));
    }
    if text
        == "At the beginning of your end step, each opponent loses life equal to the number of tapped creatures you control."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "endStep",
                "player": controller(),
            }),
            None,
            vec![json!({
                "kind": "loseLifeEachOpponent",
                "amount": {
                    "kind": "countPermanents",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), json!({ "kind": "isTapped" })]),
                },
            })],
        ));
    }

    if text
        == "Whenever a creature you control attacks, you may put a quest counter on this enchantment."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
            }),
            None,
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "quest",
                    "count": integer(1),
                },
                "onPerformed": [],
            })],
        ));
    }
    if text
        == "At the beginning of combat on your turn, put a +1/+1 counter on target creature you control."
    {
        return Some(triggered(
            json!({
                "kind": "stepBegan",
                "step": "beginCombat",
                "player": controller(),
            }),
            Some(json!({
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
            })),
            vec![json!({
                "kind": "putCounters",
                "permanent": chosen_target("targetCreature"),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text
        == "When this creature enters, put a +1/+1 counter on each other Ally creature you control."
    {
        return Some(triggered(
            enter_event.clone(),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": {
                    "kind": "eachPermanent",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), subtype("Ally")]),
                    "excludeSource": true,
                },
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    let enter_damage_re = Regex::new(
        r"^When .+ enters, (?:he|she|it) deals (\d+) damage to target tapped creature an opponent controls\.$",
    )
    .expect("enter damage to tapped creature regex compiles");
    if let Some(captures) = enter_damage_re.captures(text) {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "damageTarget",
                    json!({
                        "kind": "permanents",
                        "controller": {
                            "kind": "opponentsOf",
                            "player": controller(),
                        },
                        "where": and(vec![
                            card_type("Creature"),
                            json!({ "kind": "isTapped" }),
                        ]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("damageTarget"),
                "amount": integer(captures[1].parse::<i64>().ok()?),
            })],
        ));
    }
    if text
        == "When Annie Joins Up enters, it deals 5 damage to target creature or planeswalker an opponent controls."
    {
        return Some(triggered(
            enter_event.clone(),
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "damageTarget",
                    json!({
                        "kind": "permanents",
                        "controller": {
                            "kind": "opponentsOf",
                            "player": controller(),
                        },
                        "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                    }),
                    1,
                    1,
                )],
            })),
            vec![json!({
                "kind": "dealDamage",
                "recipient": chosen_target("damageTarget"),
                "amount": integer(5),
            })],
        ));
    }
    if text
        == "When this enchantment enters, it deals 3 damage to each creature and each planeswalker."
    {
        return Some(triggered(
            enter_event.clone(),
            None,
            vec![json!({
                "kind": "dealDamage",
                "recipient": {
                    "kind": "eachPermanent",
                    "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                },
                "amount": integer(3),
            })],
        ));
    }

    if text
        == "When this creature enters, you may search your library for a basic land card, put that card onto the battlefield tapped, then shuffle."
    {
        return Some(triggered(
            enter_event,
            None,
            search_library_effects(
                json!({ "kind": "typeLineContains", "value": "Basic Land" }),
                1,
                "battlefield",
                true,
            ),
        ));
    }
    if text
        == "When this creature dies, put each permanent card exiled with it onto the battlefield under the control of that card's owner."
    {
        return Some(triggered(
            json!({ "kind": "permanentDied", "object": self_ref() }),
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": "returnCardsExiledWithSource",
            })],
        ));
    }
    if text == "Whenever another Ally you control enters, put a +1/+1 counter on this creature." {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": subtype("Ally"),
                "excludeSource": true,
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever a creature you control enters, put a quest counter on this enchantment." {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Creature"),
            }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "quest",
                "count": integer(1),
            })],
        ));
    }
    let enter_power_draw_re = Regex::new(
        r"^Whenever a creature you control with power (\d+) or greater enters, draw a card\.$",
    )
    .expect("power threshold enter draw regex compiles");
    if let Some(captures) = enter_power_draw_re.captures(text) {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": and(vec![
                    card_type("Creature"),
                    compare(
                        ">=",
                        json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                        integer(captures[1].parse::<i64>().ok()?),
                    ),
                ]),
            }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever a nontoken creature you control enters during combat, draw a card." {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": and(vec![card_type("Creature"), not(json!({ "kind": "isToken" }))]),
                "duringCombat": true,
            }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text
        == "Whenever an artifact you control enters, this creature deals 1 damage to each opponent."
    {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Artifact"),
            }),
            None,
            vec![json!({ "kind": "dealDamageToEachOpponent", "amount": integer(1) })],
        ));
    }
    if text == "Whenever you cast a noncreature spell, draw a card." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": not(card_type("Creature")),
            }),
            None,
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever you cast a Lesson spell, Aang gains lifelink until end of turn." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": subtype("Lesson"),
            }),
            None,
            vec![json!({
                "kind": "grantKeyword",
                "object": { "kind": "abilitySource" },
                "keyword": "lifelink",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text == "Whenever you draw a card, put a +1/+1 counter on this creature." {
        return Some(triggered(
            json!({ "kind": "cardDrawn", "player": controller() }),
            None,
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text == "Whenever you draw a card, each opponent loses 1 life." {
        return Some(triggered(
            json!({ "kind": "cardDrawn", "player": controller() }),
            None,
            vec![json!({ "kind": "loseLifeEachOpponent", "amount": integer(1) })],
        ));
    }
    if text
        == "Whenever you draw a card during an opponent's turn, create a 1/1 blue Tentacle creature token."
    {
        return Some(triggered(
            json!({
                "kind": "cardDrawn",
                "player": controller(),
                "duringOpponentTurn": true,
            }),
            None,
            vec![create_token_effect(
                "Create a 1/1 blue Tentacle creature token.",
            )?],
        ));
    }
    if text
        == "At the beginning of your upkeep, put a knowledge counter on The Magic Mirror, then draw a card for each knowledge counter on The Magic Mirror."
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            None,
            vec![
                json!({
                    "kind": "putCounters",
                    "permanent": self_ref(),
                    "counter": "knowledge",
                    "count": integer(1),
                }),
                json!({
                    "kind": "drawCards",
                    "player": controller(),
                    "count": {
                        "kind": "countCounters",
                        "object": self_ref(),
                        "counter": "knowledge",
                    },
                }),
            ],
        ));
    }
    let operation = |event: Value, operation: &str| {
        triggered(
            event,
            None,
            vec![json!({
                "kind": "resolveTriggeredInstruction",
                "operation": operation,
            })],
        )
    };
    if text
        == "Whenever equipped creature deals combat damage to a player, that player loses half their life, rounded up."
    {
        return Some(operation(
            json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
            "quietusSpikeLifeLoss",
        ));
    }
    if text
        == "Whenever a creature you control deals combat damage to a player, put a quest counter on this enchantment. Then if it has four or more quest counters on it, draw a card."
    {
        return Some(operation(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
            }),
            "advanceWaterbenderAscension",
        ));
    }
    if text
        == "Whenever Aang and La attack, put a +1/+1 counter on each tapped creature you control."
    {
        return Some(operation(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            "counterTappedCreatures",
        ));
    }
    if text == "Whenever you cast a spell from exile, create a 1/1 white Ally creature token." {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "fromZone": "exile",
            }),
            None,
            vec![create_token_effect(
                "Create a 1/1 white Ally creature token.",
            )?],
        ));
    }
    if text
        == "When Toph enters, you may discard a card. If you do, return target instant or sorcery card from your graveyard to your hand."
    {
        return Some(operation(enter_event.clone(), "tophDiscardReturnSpell"));
    }
    if text
        == "When Aang enters, look at the top five cards of your library. You may put a creature card with mana value 4 or less from among them onto the battlefield. Put the rest on the bottom of your library in a random order."
    {
        return Some(operation(enter_event, "aangTopFiveCreature"));
    }

    let attack_earthbend_re = Regex::new(&format!(
        r"^Whenever (?:you|.+) attack(?:s)?, earthbend (X, where X is ({}))\.?.*$",
        variable_clause_pattern(),
    ))
    .expect("attack earthbend regex compiles");
    if let Some(captures) = attack_earthbend_re.captures(text) {
        let event = if text.starts_with("Whenever you attack,") {
            json!({ "kind": "controlledCreaturesAttacked", "player": controller() })
        } else {
            json!({ "kind": "declaredAttacker", "object": self_ref() })
        };
        return Some(triggered(
            event,
            Some(json!({
                "kind": "castingDeclaration",
                "decisions": [target_decision(
                    "earthbendLand",
                    json!({
                        "kind": "permanents",
                        "controller": controller(),
                        "where": card_type("Land"),
                    }),
                    1,
                    1,
                )],
            })),
            vec![earthbend_effect(
                "earthbendLand",
                avatar_quantity(&captures[1])?,
            )],
        ));
    }
    if text
        == "Whenever Aang and Katara enter or attack, create X 1/1 white Ally creature tokens, where X is the number of tapped artifacts and/or creatures you control."
    {
        return Some(triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    { "kind": "enterBattlefield", "object": self_ref() },
                    { "kind": "declaredAttacker", "object": self_ref() },
                ],
            }),
            None,
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": avatar_quantity(
                    "X, where X is the number of tapped artifacts and/or creatures you control",
                )?,
                "token": {
                    "colors": ["white"],
                    "types": ["Creature"],
                    "subtypes": ["Ally"],
                    "power": 1,
                    "toughness": 1,
                },
            })],
        ));
    }
    if text
        == "Whenever Suki attacks, create a 1/1 white Ally creature token that's tapped and attacking."
    {
        let mut effect = create_token_effect("Create a 1/1 white Ally creature token.")?;
        effect["tapped"] = Value::Bool(true);
        effect["attacking"] = Value::Bool(true);
        return Some(triggered(
            json!({ "kind": "declaredAttacker", "object": self_ref() }),
            None,
            vec![effect],
        ));
    }

    None
}

pub(in crate::oracle::canonical) fn parse_remaining_deck_trigger(
    text: &str,
) -> Option<CanonicalRuleDraft> {
    let source_event = |kind: &str| {
        json!({
            "kind": kind,
            "object": self_ref(),
        })
    };
    let step_event = |step: &str| {
        json!({
            "kind": "stepBegan",
            "step": step,
            "player": controller(),
        })
    };
    let operation = |event: Value, operation: &str| {
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
            &[
                "Resolve the triggering event and affected objects",
                "Normalize the complete Oracle instruction",
                "Apply choices, targets, and ordered effects",
            ],
        )
    };
    let limited_operation = |event: Value, operation_name: &str, minimum_level: Option<i64>| {
        let mut rule = json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": event,
            "triggerLimit": {
                "kind": "onceEachTurn",
                "id": operation_name,
            },
            "effects": [{
                "kind": "resolveTriggeredInstruction",
                "operation": operation_name,
            }],
        });
        if let Some(level) = minimum_level {
            rule["minimumClassLevel"] = integer(level);
        }
        draft(
            rule,
            &[
                "Resolve the triggering event and affected objects",
                "Enforce the once-each-turn trigger limit",
                "Apply the complete normalized instruction",
            ],
        )
    };
    let one_of = |events: Vec<Value>| {
        json!({
            "kind": "oneOf",
            "events": events,
        })
    };

    match text {
        "Whenever a creature token you control leaves the battlefield, draw a card if it was attacking. Otherwise, each opponent loses 1 life." => {
            Some(operation(
                json!({
                    "kind": "permanentLeftBattlefield",
                    "player": controller(),
                    "where": and(vec![card_type("Creature"), json!({ "kind": "isToken" })]),
                }),
                "drawIfAttackingOtherwiseDrain",
            ))
        }
        "Whenever you attack with this creature and/or your commander, for each opponent, create a 1/1 red Goblin creature token that's tapped and attacking that player." => {
            Some(operation(
                json!({
                    "kind": "attackDeclaredWithSourceOrCommander",
                    "player": controller(),
                    "source": self_ref(),
                }),
                "createGoblinAttackingEachOpponent",
            ))
        }
        "Whenever this creature deals combat damage to a player, draw a card if that player has more cards in hand than each other player. Then you create a Treasure token if that player controls more lands than each other player. Then you gain 3 life if that player has more life than each other player." => {
            Some(operation(
                json!({
                    "kind": "combatDamageToPlayer",
                    "source": self_ref(),
                }),
                "resolveBattleAngelsComparison",
            ))
        }
        "At the beginning of your first main phase, add {B} for each charge counter on this enchantment." => {
            Some(operation(
                step_event("precombatMain"),
                "addBlackForChargeCounters",
            ))
        }
        "Whenever one or more tokens you control enter, draw a card. This ability triggers only once each turn." => {
            Some(limited_operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": json!({ "kind": "isToken" }),
                }),
                "drawForTokensEntering",
                None,
            ))
        }
        "When this Class becomes level 2, create a token that's a copy of target token you control." => {
            Some(operation(
                json!({
                    "kind": "classLeveled",
                    "object": self_ref(),
                    "level": integer(2),
                }),
                "copyControlledToken",
            ))
        }
        "Whenever an artifact, creature, or enchantment enters, its controller chooses target permanent another player controls that shares a card type with it. Exchange control of those permanents." => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "anyController": true,
                    "where": or(vec![
                        card_type("Artifact"),
                        card_type("Creature"),
                        card_type("Enchantment"),
                    ]),
                }),
                "exchangeEnteredPermanent",
            ))
        }
        "Whenever you cast your second spell each turn, choose one —\n• Create two 1/1 white Human Soldier creature tokens.\n• Put a +1/+1 counter on each creature you control." => {
            Some(operation(
                json!({
                    "kind": "spellCastOrdinal",
                    "player": controller(),
                    "ordinal": integer(2),
                }),
                "chooseCosmograndMode",
            ))
        }
        "Whenever one or more other creatures you control with power 2 or less enter, draw a card. This ability triggers only once each turn." => {
            Some(limited_operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        compare(
                            "<=",
                            json!({
                                "kind": "powerOf",
                                "object": { "kind": "candidate" },
                            }),
                            integer(2),
                        ),
                    ]),
                    "excludeSource": true,
                }),
                "drawForSmallCreatureEntering",
                None,
            ))
        }
        "When Enduring Innocence dies, if it was a creature, return it to the battlefield under its owner's control. It's an enchantment. (It's not a creature.)" => {
            Some(operation(
                source_event("permanentDied"),
                "returnEnduringAsEnchantment",
            ))
        }
        "Whenever another creature you control dies, you may pay 2 life. If you do, draw a card." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "payTwoLifeToDraw",
            ))
        }
        "When this enchantment enters, create a 2/2 red Soldier creature token with firebending 1." => {
            Some(operation(
                source_event("enterBattlefield"),
                "createFirebendingSoldier",
            ))
        }
        "Whenever a creature you control attacking causes a triggered ability of that creature to trigger, put a quest counter on this enchantment. Then if it has four or more quest counters on it, you may copy that ability. You may choose new targets for the copy." => {
            Some(operation(
                json!({
                    "kind": "attackingCreatureAbilityTriggered",
                    "player": controller(),
                }),
                "advanceAndCopyAttackTrigger",
            ))
        }
        "Whenever you tap this land for mana, target opponent creates a 1/1 colorless Spirit creature token." => {
            Some(operation(
                source_event("manaAbilityActivated"),
                "opponentCreatesSpirit",
            ))
        }
        "At the beginning of your upkeep, if an opponent controls more lands than you, you may search your library for up to three basic land cards, reveal them, put them into your hand, then shuffle." => {
            Some(operation(step_event("upkeep"), "resolveLandTax"))
        }
        "When this land enters, sacrifice two lands." => Some(operation(
            source_event("enterBattlefield"),
            "sacrificeTwoLands",
        )),
        "Whenever you create or sacrifice a token, each opponent loses 1 life." => Some(operation(
            json!({
                "kind": "tokenCreatedOrSacrificed",
                "player": controller(),
            }),
            "drainOpponentsOne",
        )),
        "When a player casts a spell or a creature attacks, exile Norin. Return it to the battlefield under its owner's control at the beginning of the next end step." => {
            Some(operation(
                one_of(vec![
                    json!({ "kind": "spellCast", "anyPlayer": true }),
                    json!({ "kind": "creatureAttacked", "anyPlayer": true }),
                ]),
                "blinkSourceAtNextEndStep",
            ))
        }
        "When this enchantment enters, target creature an opponent controls gets -3/-3 until end of turn." => {
            Some(operation(
                source_event("enterBattlefield"),
                "weakenOpponentCreatureThree",
            ))
        }
        "At the beginning of your end step, if you gained life this turn, create a 1/1 white Cat creature token. Then if you have the city's blessing, for each token you control that entered this turn, create a token that's a copy of it." => {
            Some(operation(step_event("endStep"), "resolveOcelotPride"))
        }
        "Whenever a source you control deals damage to an opponent, you may put a quest counter on this enchantment." => {
            Some(operation(
                json!({
                    "kind": "damageDealtToOpponent",
                    "sourceController": controller(),
                }),
                "mayAddQuestCounter",
            ))
        }
        "Whenever this creature attacks, for each creature token you control that entered this turn, create a tapped and attacking token that's a copy of that token. At the beginning of the next end step, sacrifice those tokens." => {
            Some(operation(
                source_event("declaredAttacker"),
                "copyEnteredTokensAttacking",
            ))
        }
        "When you have 30 or more life, flip Rune-Tail." => Some(operation(
            json!({
                "kind": "stateConditionMet",
                "condition": compare(
                    ">=",
                    json!({ "kind": "lifeTotal", "player": controller() }),
                    integer(30),
                ),
            }),
            "transformSource",
        )),
        "Whenever one or more creatures you control die, create a Food token. This ability triggers only once each turn." => {
            Some(limited_operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "createFoodForCreatureDeath",
                None,
            ))
        }
        "Whenever you sacrifice a permanent, target player mills two cards." => Some(operation(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": Value::Null,
                "reason": "sacrificed",
            }),
            "targetPlayerMillsTwo",
        )),
        "At the beginning of your end step, you may sacrifice three other nonland permanents. If you do, return a creature card from your graveyard to the battlefield with a finality counter on it." => {
            Some(operation(
                step_event("endStep"),
                "sacrificeThreeToReanimateFinality",
            ))
        }
        "Whenever Sephiroth enters or attacks, you may sacrifice another creature. If you do, draw a card." => {
            Some(operation(
                one_of(vec![
                    source_event("enterBattlefield"),
                    source_event("declaredAttacker"),
                ]),
                "sacrificeAnotherToDraw",
            ))
        }
        "Whenever another creature dies, target opponent loses 1 life and you gain 1 life. If this is the fourth time this ability has resolved this turn, transform Sephiroth." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "sephirothDrainAndTransform",
            ))
        }
        "Whenever Sephiroth attacks, you may sacrifice any number of other creatures. If you do, draw that many cards." => {
            Some(operation(
                source_event("declaredAttacker"),
                "sacrificeAnyCreaturesToDraw",
            ))
        }
        "Whenever equipped creature dies, draw two cards." => Some(operation(
            json!({
                "kind": "attachedPermanentDied",
                "attachment": self_ref(),
            }),
            "drawTwo",
        )),
        "Whenever another creature you control with power 2 or less enters, surveil 1. (Look at the top card of your library. You may put it into your graveyard.)" => {
            Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        compare(
                            "<=",
                            json!({
                                "kind": "powerOf",
                                "object": { "kind": "candidate" },
                            }),
                            integer(2),
                        ),
                    ]),
                    "excludeSource": true,
                }),
                "surveilOne",
            ))
        }
        "Whenever equipped creature deals combat damage to a player, exile up to one target creature you own, then search your library for a basic land card. Put both cards onto the battlefield under your control, then shuffle." => {
            Some(operation(
                json!({
                    "kind": "attachedPermanentCombatDamageToPlayer",
                    "attachment": self_ref(),
                }),
                "resolveHearthAndHome",
            ))
        }
        "Whenever another creature you control dies or is put into exile, put a +1/+1 counter on Syr Vondam and you gain 1 life." => {
            Some(operation(
                json!({
                    "kind": "controlledCreatureDiedOrExiled",
                    "player": controller(),
                    "excludeSource": true,
                }),
                "counterSourceAndGainLife",
            ))
        }
        "When Syr Vondam dies or is put into exile while its power is 4 or greater, destroy up to one target nonland permanent." => {
            Some(operation(
                json!({
                    "kind": "sourceDiedOrExiled",
                    "object": self_ref(),
                    "minimumPower": integer(4),
                }),
                "destroyOptionalNonland",
            ))
        }
        "When this land enters, up to one target creature phases out." => Some(operation(
            source_event("enterBattlefield"),
            "phaseOutOptionalCreature",
        )),
        "At the beginning of combat on your turn, put an oil counter on this artifact, then create an X/1 red Phyrexian Horror creature token with trample and haste, where X is the number of oil counters on this artifact. Sacrifice that token at the beginning of the next end step." => {
            Some(operation(
                step_event("beginCombat"),
                "resolveUrabrasksForge",
            ))
        }
        "Whenever you gain life, target opponent loses that much life." => Some(operation(
            json!({
                "kind": "lifeGained",
                "player": controller(),
            }),
            "targetOpponentLosesLifeGained",
        )),
        "Whenever a creature you control with power or toughness 1 or less dies, target opponent loses 2 life and you gain 2 life." => {
            Some(operation(
                json!({
                    "kind": "smallControlledCreatureDied",
                    "player": controller(),
                }),
                "targetOpponentDrainTwo",
            ))
        }
        "Whenever this creature or another creature dies, target player loses 1 life and you gain 1 life." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                }),
                "targetPlayerDrainOne",
            ))
        }
        "Whenever one or more creatures you control deal combat damage to a player, you draw a card and lose 1 life." => {
            Some(operation(
                json!({
                    "kind": "controlledCreaturesCombatDamageToPlayer",
                    "player": controller(),
                }),
                "drawAndLoseOne",
            ))
        }
        "Whenever a creature dies, that creature's controller may draw a card." => Some(operation(
            json!({
                "kind": "permanentDied",
                "where": card_type("Creature"),
            }),
            "diedCreatureControllerMayDraw",
        )),
        "When this land enters, return a land you control to its owner's hand." => Some(operation(
            source_event("enterBattlefield"),
            "returnControlledLand",
        )),
        "When this artifact enters and when you sacrifice it, you may search your library for a basic land card, put it onto the battlefield tapped, then shuffle." => {
            Some(operation(
                one_of(vec![
                    source_event("enterBattlefield"),
                    json!({
                        "kind": "permanentDied",
                        "object": self_ref(),
                        "reason": "sacrificed",
                    }),
                ]),
                "mayRampBasicTapped",
            ))
        }
        "Whenever a player sacrifices a permanent, this creature deals 1 damage to any target." => {
            Some(operation(
                json!({
                    "kind": "permanentSacrificed",
                    "anyPlayer": true,
                }),
                "dealOneAnyTarget",
            ))
        }
        "Whenever a creature an opponent controls dies, you may gain 3 life." => Some(operation(
            json!({
                "kind": "opponentCreatureDied",
                "player": controller(),
            }),
            "mayGainThree",
        )),
        "Whenever an opponent discards a card, you may gain 3 life." => Some(operation(
            json!({
                "kind": "opponentDiscardedCard",
                "player": controller(),
            }),
            "mayGainThree",
        )),
        "At the beginning of your second main phase, if you gained 2 or more life this turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)" => {
            Some(operation(
                step_event("postcombatMain"),
                "prepareIfGainedTwoLife",
            ))
        }
        "Whenever another creature you control dies, it deals damage equal to its power to target player or planeswalker." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "excludeSource": true,
                }),
                "deadCreatureDealsItsPower",
            ))
        }
        "Whenever The Dawning Archaic attacks, you may cast target instant or sorcery card from your graveyard without paying its mana cost. If that spell would be put into your graveyard, exile it instead." => {
            Some(operation(
                source_event("declaredAttacker"),
                "mayCastInstantSorceryFromGraveyard",
            ))
        }
        "When this creature enters, target opponent chooses a permanent they control at random and sacrifices it. If a nonland permanent is sacrificed this way, repeat this process." => {
            Some(operation(
                source_event("enterBattlefield"),
                "resolveTyrantOfDiscord",
            ))
        }
        "Whenever this creature or another creature you control dies, target opponent loses 1 life and you gain 1 life." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "targetOpponentDrainOne",
            ))
        }
        "Whenever a creature dies, target opponent loses 1 life and you gain 1 life." => {
            Some(operation(
                json!({
                    "kind": "permanentDied",
                    "where": card_type("Creature"),
                }),
                "targetOpponentDrainOne",
            ))
        }
        "When this creature enters, return target instant or sorcery card from your graveyard to your hand." => {
            Some(operation(
                source_event("enterBattlefield"),
                "returnInstantSorceryToHand",
            ))
        }
        "Whenever one or more cards leave your graveyard, create a 2/2 black Horror enchantment creature token. This ability triggers only once each turn." => {
            Some(limited_operation(
                json!({
                    "kind": "cardsLeftGraveyard",
                    "player": controller(),
                }),
                "createHorrorEnchantmentToken",
                None,
            ))
        }
        "When you unlock this door, return target creature card from your graveyard to your hand." => {
            Some(operation(
                source_event("doorUnlocked"),
                "returnCreatureToHand",
            ))
        }
        "Whenever this token attacks, you gain 1 life." => {
            Some(operation(source_event("declaredAttacker"), "gainOneLife"))
        }
        "Whenever this creature deals combat damage to a player, that player loses the game." => {
            Some(operation(
                json!({
                    "kind": "combatDamageToPlayer",
                    "source": self_ref(),
                }),
                "damagedPlayerLosesGame",
            ))
        }
        _ => None,
    }
}
