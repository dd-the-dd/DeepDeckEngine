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

    let attack_with_power = |minimum: i64| {
        json!({
            "kind": "controlledCreaturesAttacked",
            "player": controller(),
            "totalPowerAtLeast": integer(minimum),
        })
    };
    let single_line_text = text.replace('\n', " ");
    match single_line_text.as_str() {
        "Whenever Thorin or another Dwarf you control enters, create a Treasure token." => {
            return Some(operation(
                json!({ "kind": "permanentEntered", "player": controller(), "where": subtype("Dwarf") }),
                "createTreasure",
            ));
        }
        "When this artifact enters, create three Food tokens." => {
            return Some(operation(enter_self(), "bagEndBanquetEnter"));
        }
        "Whenever Smaug is dealt noncombat damage, create that many Treasure tokens." => {
            return Some(operation(
                json!({ "kind": "permanentDealtDamage", "object": self_ref(), "noncombatOnly": true }),
                "smaugImpenetrableDamage",
            ));
        }
        "Whenever this creature deals combat damage to a player, you create a Treasure token for each artifact that player controls." =>
        {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "cavernHoardDragonDamage",
            ));
        }
        "Alliance — Whenever another creature you control enters, choose one that hasn't been chosen this turn — • Add {G}{G}{G}. • Put a +1/+1 counter on each creature you control. • Scry 2, then draw a card." =>
        {
            return Some(operation(
                json!({ "kind": "permanentEntered", "player": controller(), "where": card_type("Creature"), "excludeSource": true }),
                "galadrielAlliance",
            ));
        }
        "Whenever a player casts their second spell each turn, you lose 1 life and create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(
                json!({ "kind": "spellCast", "anyPlayer": true, "spellCastOrdinal": integer(2), "where": Value::Null }),
                "lothoSecondSpell",
            ));
        }
        "Whenever equipped creature deals combat damage to a player, you may cast an instant or sorcery spell from your hand with mana value less than or equal to that damage without paying its mana cost." =>
        {
            return Some(operation(
                json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
                "glamdringCombatDamage",
            ));
        }
        "At the beginning of your end step, put an influence counter on Palantír of Orthanc and scry 2. Then target opponent may have you draw a card. If that player doesn't, you mill X cards, where X is the number of influence counters on Palantír of Orthanc, and that player loses life equal to the total mana value of those cards." =>
        {
            return Some(operation(end_step(controller()), "palantirEndStep"));
        }
        "Whenever Bilbo deals combat damage to a player or battle, put up to one target nonland permanent card with mana value 3 or less from a graveyard onto the battlefield under its owner's control." =>
        {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "bilboUnexpectedDamage",
            ));
        }
        "When Gandalf enters, exile up to three target lands you control, then return them to the battlefield tapped under their owner's control." =>
        {
            return Some(operation(enter_self(), "gandalfShadowEnter"));
        }
        "Whenever a Dwarf you control deals combat damage to a player or battle, create two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(
                json!({ "kind": "combatDamageReceived", "player": { "kind": "opponentsOf", "player": controller() }, "where": subtype("Dwarf") }),
                "thorinCompanyDamage",
            ));
        }
        "When this creature enters, draw a card. Then if you don't control a legendary creature, put a card from your hand on the bottom of your library." =>
        {
            return Some(operation(enter_self(), "errandRiderEnter"));
        }
        "When this creature enters, you may exile another target creature." => {
            return Some(operation(enter_self(), "fiendHunterEnter"));
        }
        "When this creature leaves the battlefield, return the exiled card to the battlefield under its owner's control." =>
        {
            return Some(operation(
                json!({ "kind": "permanentLeftBattlefield", "object": self_ref() }),
                "fiendHunterLeave",
            ));
        }
        "Whenever two or more creatures you control attack a player, target attacking creature without flying gains flying until end of turn." =>
        {
            return Some(operation(
                json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
                "landrovalAttack",
            ));
        }
        "Whenever another creature you control with power 2 or less enters, you may pay {1}. If you do, draw a card." =>
        {
            return Some(operation(
                json!({ "kind": "permanentEntered", "player": controller(), "where": and(vec![card_type("Creature"), compare("<=", json!({ "kind": "powerOf", "object": { "kind": "candidate" } }), integer(2))]), "excludeSource": true }),
                "mentorMeekEnter",
            ));
        }
        "Whenever this creature attacks, you may exile target creature defending player controls until this creature leaves the battlefield. (That creature returns under its owner's control.)" =>
        {
            return Some(operation(attacks(), "colossalWhaleAttack"));
        }
        "Whenever this creature attacks, you may tap any number of untapped Humans you control. Draw a card for each Human tapped this way." =>
        {
            return Some(operation(attacks(), "minasTirithGarrisonAttack"));
        }
        "Whenever you scry, this creature gets +1/+0 until end of turn and can't be blocked this turn. This ability triggers only once each turn." =>
        {
            return Some(operation(
                json!({ "kind": "scried", "player": controller() }),
                "nimrodelWatcherScry",
            ));
        }
        "Whenever this creature becomes blocked, it deals 1 damage to each creature blocking it." =>
        {
            return Some(operation(
                json!({ "kind": "controlledCreatureBecameBlocked", "player": controller() }),
                "battleScarredGoblinBlocked",
            ));
        }
        "Whenever you scry, Celeborn gets +1/+1 until end of turn for each card looked at while scrying this way." =>
        {
            return Some(operation(
                json!({ "kind": "scried", "player": controller() }),
                "celebornScry",
            ));
        }
        "Whenever this creature enters or attacks, return target Elf card from your graveyard to your hand. You gain life equal to that card's power." =>
        {
            return Some(operation(
                json!({ "kind": "oneOf", "events": [enter_self(), attacks()] }),
                "mirkwoodElkEnterAttack",
            ));
        }
        "When this creature enters, create a tapped Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(enter_self(), "createTappedTreasure"));
        }
        "Whenever an opponent draws their second card each turn, you create a Treasure token." => {
            return Some(operation(
                json!({
                    "kind": "cardDrawn", "opponentOfSourceController": true, "drawOrdinal": integer(2),
                }),
                "createTreasure",
            ));
        }
        "When The Sackville-Bagginses enter, you may sacrifice another creature or artifact. If you do, draw a card and create a Treasure token." =>
        {
            return Some(operation(enter_self(), "sackvilleBagginsesEnter"));
        }
        "When Dori enters, create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(enter_self(), "createTreasure"));
        }
        "I, II, III, IV — Create a Treasure token. Then if you control four or more Treasures, sacrifice this Saga. If you do, create a 6/6 red Dragon creature token with flying. (A Treasure token is an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(
                json!({
                    "kind": "sagaChapterReached", "object": self_ref(),
                    "chapters": [integer(1), integer(2), integer(3), integer(4)],
                }),
                "mistyMountainsColdChapter",
            ));
        }
        "At the beginning of your upkeep, create a Treasure token." => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
                "createTreasure",
            ));
        }
        "Whenever this creature deals combat damage to a player, choose one — • Put a +1/+1 counter on target Wolf you control. • Create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")" =>
        {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "bejeweledWargDamage",
            ));
        }
        "Whenever you put one or more counters on a Goblin, Orc, or Army you control, The Great Goblin deals 2 damage to target opponent." =>
        {
            return Some(operation(
                json!({
                    "kind": "countersPlaced", "player": controller(),
                    "where": { "kind": "or", "operands": [subtype("Goblin"), subtype("Orc"), subtype("Army")] },
                }),
                "greatGoblinCounters",
            ));
        }
        "When Smaug enters, create X tapped Treasure tokens, where X is the number of artifacts your opponents control." =>
        {
            return Some(operation(enter_self(), "smaugWickedEnter"));
        }
        "Whenever equipped creature deals combat damage to a player, choose a creature type. Create a Treasure token for each creature you control of that type." =>
        {
            return Some(operation(
                json!({ "kind": "attachedPermanentCombatDamageToPlayer" }),
                "orcristCombatDamage",
            ));
        }
        "Strange new worlds — When Christine Chapel enters, you gain life equal to the number of differently named lands you control." =>
        {
            return Some(operation(enter_self(), "christineChapelEnter"));
        }
        "Whenever Captain Kirk enters or attacks, search your library and/or graveyard for a basic land card or a card named Starship Enterprise, reveal it, and put it into your hand. If you search your library this way, shuffle." =>
        {
            return Some(operation(
                json!({ "kind": "oneOf", "events": [enter_self(), attacks()] }),
                "captainKirkBoldSearch",
            ));
        }
        "When Will Riker enters, put a +1/+1 counter on another target creature you control." => {
            return Some(operation(enter_self(), "willRikerEnter"));
        }
        "When Pelia enters, return target creature an opponent controls to its owner's hand." => {
            return Some(operation(enter_self(), "peliaEnter"));
        }
        "Whenever Picard enters or attacks, put two +1/+1 counters on another target creature you control." =>
        {
            return Some(operation(
                json!({ "kind": "oneOf", "events": [enter_self(), attacks()] }),
                "picardSteadfastCounters",
            ));
        }
        "When Borg Queen enters, assimilate target creature card from an opponent's graveyard. (Put it onto the battlefield under your control with a +1/+1 counter. It's a Borg artifact creature and loses all other creature types.)" =>
        {
            return Some(operation(enter_self(), "borgQueenAssimilate"));
        }
        "Whenever Kirk attacks while creatures you control have total power 8 or greater, exile the top card of your library. You may play that card this turn." =>
        {
            return Some(operation(
                json!({
                    "kind": "controlledCreaturesAttacked",
                    "player": controller(),
                    "sourceMustAttack": true,
                    "totalPowerAtLeast": integer(8),
                }),
                "kirkEnterprisingAttack",
            ));
        }
        "Whenever Brad Boimler becomes tapped, until end of turn, if one or more counters would be put on a permanent you control, that many plus one of each of those kinds of counters are put on that permanent instead." =>
        {
            return Some(operation(
                json!({ "kind": "permanentTapped", "object": self_ref() }),
                "bradBoimlerTapped",
            ));
        }
        "Whenever you gain life, put a +1/+1 counter on Dr. Crusher." => {
            return Some(operation(
                json!({ "kind": "lifeGained", "player": controller() }),
                "beverlyCrusherLife",
            ));
        }
        "When this creature enters, choose one — • Put a +1/+1 counter on target creature. It gains vigilance until end of turn. • Draw a card if you control a permanent with a counter on it." =>
        {
            return Some(operation(enter_self(), "federationFieldMedicEnter"));
        }
        "Federation — When Saavik enters, she deals X damage to target attacking or blocking creature an opponent controls, where X is the number of creature types among non-Borg creatures you control." =>
        {
            return Some(operation(enter_self(), "saavikFederationEnter"));
        }
        "Whenever you attack while creatures you control have total power 8 or greater, put a +1/+1 counter on this creature. Untap it." =>
        {
            return Some(operation(attack_with_power(8), "tacticalOfficerAttack"));
        }
        "Federation — When Hoshi Sato enters, look at the top X cards of your library, where X is the number of creature types among non-Borg creatures you control. Put one of them into your hand and the rest on the bottom of your library in a random order." =>
        {
            return Some(operation(enter_self(), "hoshiSatoFederationEnter"));
        }
        "Whenever V'Ger enters or attacks, choose one — • Look at target opponent's hand, then draw a card. • Creatures your opponents control get -1/-0 until your next turn." =>
        {
            return Some(operation(
                json!({ "kind": "oneOf", "events": [enter_self(), attacks()] }),
                "vgerEnterOrAttack",
            ));
        }
        "Reckless Behavior — Whenever Beckett Mariner becomes tapped, put a promotion counter on her. Then if there are three or more promotion counters on her, remove those counters and she deals 2 damage to each opponent." =>
        {
            return Some(operation(
                json!({ "kind": "permanentTapped", "object": self_ref() }),
                "beckettMarinerTapped",
            ));
        }
        "At the beginning of combat on your turn, target creature you control gets +1/+0 until end of turn. Then that creature gains menace until end of turn if creatures you control have total power 8 or greater. (It can't be blocked except by two or more creatures.)" =>
        {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
                "cantankerousCaptainCombat",
            ));
        }
        "Whenever Captain Kirk enters or attacks, choose one. If you have no cards in hand, choose one or more instead. • Discard a card, then draw a card. • Create a 1/1 red Officer creature token. • Creatures you control get +1/+0 until end of turn." =>
        {
            return Some(operation(
                json!({ "kind": "oneOf", "events": [enter_self(), attacks()] }),
                "captainKirkEnterOrAttack",
            ));
        }
        "Whenever you attack while creatures you control have total power 8 or greater, target creature can't block this turn." =>
        {
            return Some(operation(attack_with_power(8), "laanNoonienSinghAttack"));
        }
        "Whenever Captain Janeway or another creature you control enters, that creature explores. (Reveal the top card of your library. Put that card into your hand if it's a land. Otherwise, put a +1/+1 counter on the creature, then put the card back or put it into your graveyard.)" =>
        {
            return Some(operation(
                json!({ "kind": "permanentEntered", "player": controller(), "where": card_type("Creature") }),
                "janewayExplore",
            ));
        }
        "When this creature enters, look at target opponent's hand. You may choose a nonland card from it. If you do, that player exiles that card, then draws a card." =>
        {
            return Some(operation(enter_self(), "saltVampireEnter"));
        }
        "Whenever you face a dilemma, draw a card. (You face a dilemma as you choose one or more modes for a spell or ability.)" =>
        {
            return Some(operation(
                json!({ "kind": "dilemmaFaced", "player": controller() }),
                "sevenOfNineDilemma",
            ));
        }
        "Whenever one or more charge counters are put on U.S.S. Enterprise-D for the first time each turn, exile the top card of your library. You may play that card this turn." =>
        {
            return Some(draft(
                json!({
                    "kind": "triggeredAbility",
                    "source": self_ref(),
                    "event": {
                        "kind": "countersPlaced",
                        "player": controller(),
                        "counter": "charge",
                        "where": { "kind": "nameEquals", "value": "U.S.S. Enterprise-D, Galaxy-Class" },
                    },
                    "triggerLimit": { "kind": "onceEachTurn", "id": "enterpriseDCharge" },
                    "effects": [{ "kind": "resolveTriggeredInstruction", "operation": "enterpriseDCharge" }],
                }),
                &[
                    "Watch charge counters placed on U.S.S. Enterprise-D",
                    "Trigger only once each turn",
                    "Exile the top card and grant a turn-limited play permission",
                ],
            ));
        }
        "II — Investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")" =>
        {
            return Some(operation(
                json!({ "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(2)] }),
                "inPaleMoonlightInvestigate",
            ));
        }
        "I — All artifacts and creatures phase out. Create a 0/1 white Human creature token with \"Permanents can't phase in.\" (Treat phased-out permanents and anything attached to them as though they don't exist.)" =>
        {
            return Some(operation(
                json!({ "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(1)] }),
                "cityEdgeChapterOne",
            ));
        }
        "III — Sacrifice a creature token. If you do, you gain 5 life." => {
            return Some(operation(
                json!({ "kind": "sagaChapterReached", "object": self_ref(), "chapters": [integer(3)] }),
                "cityEdgeChapterThree",
            ));
        }
        _ => {}
    }

    if text
        == "When this creature enters, draw two cards, then discard two cards. When you discard one or more nonland cards this way, tap up to that many target creatures and put a stun counter on each of them."
    {
        return Some(operation(enter_self(), "seasonedCryomancerEnter"));
    }
    if text
        == "When this creature enters, if you cast it, target opponent reveals their hand. You choose a nonland card from it. Exile that card."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "condition": { "kind": "wasCast", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetOpponent",
                        json!({
                            "kind": "players",
                            "where": {
                                "kind": "isOpponentOf",
                                "player": controller(),
                            },
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "nullSummonerExile",
                }],
            }),
            &[
                "Trigger only when Null Summoner was cast",
                "Choose a nonland card from the target opponent's revealed hand",
                "Exile it and link its threshold casting permission to Null Summoner",
            ],
        ));
    }
    if text
        == "When one or more of your opponents are dealt combat damage during your turn, draw two cards. If your library has no cards in it, you win the game. Fblthp's owner shuffles him into their library. (If you draw from an empty library this way, you still win the game.)"
    {
        return Some(operation(
            json!({ "kind": "opponentCombatDamageDuringControllerTurn" }),
            "fblthpImpossiblyLost",
        ));
    }
    if text
        == "When Uldaros Theorix enters, if you cast him, exile up to one target nonland card of each card type from your graveyard. Copy those cards. You may cast any number of spells with total mana value 6 or less from among the copies without paying their mana costs. (Permanent spells cast this way become tokens.)"
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "condition": { "kind": "wasCast", "object": self_ref() },
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "uldarosTheorixEnter",
                }],
            }),
            &[
                "Trigger only when Uldaros was cast",
                "Choose nonland graveyard cards across card types and exile them",
                "Copy and cast the copies for total mana value six or less",
            ],
        ));
    }
    if text
        == "When Ob Nixilis enters, destroy all tapped creatures your opponents control. You gain 1 life for each creature destroyed this way."
    {
        return Some(operation(enter_self(), "obNixilisAscendedEnter"));
    }
    if text == "Whenever Memnarch attacks, draw a card for each artifact you control." {
        return Some(operation(attacks(), "memnarchWardenAttack"));
    }
    if text == "Whenever this land attacks, create a Map token." {
        return Some(operation(attacks(), "createMapToken"));
    }
    if text
        == "Whenever you gain life, you may pay that much life. If you do, draw that many cards."
    {
        return Some(operation(
            json!({ "kind": "lifeGained", "player": controller() }),
            "nivMizzetGhostCounsel",
        ));
    }
    if text
        == "At the beginning of each end step, each opponent loses life equal to the life that player lost this turn. (Damage causes loss of life.)"
    {
        return Some(operation(
            end_step(json!({ "kind": "eachPlayer" })),
            "archfiendDespairEndStep",
        ));
    }
    if text.starts_with("Landfall — Whenever a land you control enters, draw a card. Then if this is the first time this ability has resolved this turn,") {
        return Some(operation(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Land"),
            }),
            "nissaLeylineTamerLandfall",
        ));
    }
    if text
        == "Whenever Avacyn or another nontoken creature you control dies, return that card to the battlefield under your control at the beginning of the next end step."
    {
        return Some(operation(
            json!({
                "kind": "permanentDied",
                "player": controller(),
                "where": card_type("Creature"),
                "nontoken": true,
            }),
            "avacynAngelOfHorrorDeath",
        ));
    }
    if text.starts_with("Whenever Jhoira enters or attacks, target opponent reveals cards from the top of their library until they reveal a historic permanent card.") {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "oneOf",
                    "events": [enter_self(), attacks()],
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
                "effects": [{
                    "kind": "resolveTriggeredInstruction",
                    "operation": "jhoiraWeatherlightCorsair",
                }],
            }),
            &[
                "Trigger when Jhoira enters or attacks",
                "Reveal through the target opponent's library to a historic permanent",
                "Put it under your control and bottom the other revealed cards",
            ],
        ));
    }
    if text.starts_with("When ") && text.ends_with(" enters, you become the monarch.") {
        return Some(operation(enter_self(), "becomeMonarch"));
    }
    if text
        == "Whenever one or more creatures deal combat damage to you while you're the monarch, tap those creatures and put a stun counter on each of them."
    {
        return Some(operation(
            json!({ "kind": "combatDamageReceived", "player": controller() }),
            "tamiyoMonarchDamage",
        ));
    }
    if text.starts_with(
        "At the beginning of each upkeep, if you're the monarch, create a Gingerbrute token.",
    ) {
        return Some(operation(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": { "kind": "eachPlayer" } }),
            "gingerMonarchUpkeep",
        ));
    }
    if text
        == "Whenever you cast a noncreature spell, create an X/X blue Shark creature token with flying, where X is that spell's mana value."
    {
        return Some(operation(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": not(card_type("Creature")),
            }),
            "sharkTyphoonSpell",
        ));
    }
    if text == "When you cycle this card, create an X/X blue Shark creature token with flying." {
        return Some(operation(
            json!({ "kind": "sourceCycled", "object": self_ref() }),
            "sharkTyphoonCycled",
        ));
    }
    if text
        == "At the beginning of combat on each opponent's turn, they may pay {2}. If they don't, creatures they control can't attack Jaces you control this turn."
    {
        return Some(operation(
            json!({
                "kind": "stepBegan",
                "step": "beginCombat",
                "player": { "kind": "opponentsOf", "player": controller() },
            }),
            "jaceMultiverseCombatTax",
        ));
    }
    if text.starts_with("At the beginning of your upkeep, you lose 1 life and create a 1/1 colorless Phyrexian Mite artifact creature token with toxic 1") {
        return Some(operation(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            "skrelvsHiveUpkeep",
        ));
    }
    if text.starts_with("Whenever one or more Sphinxes you control attack, each player mills that many cards. For each player, you may cast a card that player milled this way") {
        return Some(operation(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "where": subtype("Sphinx"),
            }),
            "urSphinxAttack",
        ));
    }
    if text == "Whenever you discard a card, you may exile that card from your graveyard." {
        return Some(operation(
            json!({ "kind": "playerDiscardedCard", "player": controller() }),
            "currencyConverterDiscard",
        ));
    }
    if text.starts_with("When Dack Fayden enters, reveal cards from the top of your library until you reveal X creature cards, where X is the number of opponents you have.") {
        return Some(operation(enter_self(), "dackFaydenHelpingHand"));
    }
    if text.starts_with("When Venser enters, choose one —") {
        return Some(operation(enter_self(), "venserFerventForger"));
    }
    if text
        == "Whenever a creature you control with shadow deals combat damage to a player, draw a card."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesCombatDamageToPlayer",
                "player": controller(),
                "where": { "kind": "hasKeyword", "value": "shadow" },
            }),
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": integer(1),
            })],
        ));
    }
    if text
        == "Whenever you cast a noncreature spell, counter that spell. Create a number of 1/1 blue Merfolk creature tokens equal to the amount of mana spent to cast that spell."
    {
        return Some(operation(
            json!({ "kind": "spellCast", "player": controller(), "where": not(card_type("Creature")) }),
            "grandmotherGobySpell",
        ));
    }
    if text
        == "Whenever you cast your first spell during each opponent's turn, draw three cards, then put two cards from your hand on top of your library in any order."
    {
        return Some(operation(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "duringOpponentTurn": true,
                "spellCastOrdinal": integer(1),
            }),
            "gusthaFirstOpponentTurnSpell",
        ));
    }
    if text
        == "Whenever an artifact is put into a graveyard from the battlefield, put a number of +1/+1 counters equal to that artifact's mana value on Joven and Chandler."
    {
        return Some(operation(
            json!({ "kind": "permanentDied", "where": card_type("Artifact") }),
            "jovenChandlerArtifactDied",
        ));
    }
    if text
        == "At the beginning of your upkeep, create a 2/1 red Goblin creature token with haste. Then if you're the monarch, for each creature token you control, create a token that's a copy of it."
    {
        return Some(operation(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            "chiefMagistrateUpkeep",
        ));
    }
    if text
        == "At the beginning of your end step, each opponent loses life equal to the number of tapped Cats and/or Zombies you control."
    {
        return Some(operation(
            json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
            "olagMiauEndStep",
        ));
    }

    if text
        == "When Fblthp enters, search your library for up to X basic land cards with different names, reveal them, put them into your hand, then shuffle."
    {
        return Some(triggered(
            enter_self(),
            vec![json!({
                "kind": "searchLibrary",
                "player": controller(),
                "where": and(vec![
                    json!({ "kind": "typeLineContains", "value": "Basic" }),
                    card_type("Land"),
                ]),
                "maximum": { "kind": "sourceCastXValue" },
                "destination": "hand",
                "tapped": false,
                "differentNames": true,
            })],
        ));
    }
    if text
        == "When Hapatra enters, for each opponent, tap up to one target creature that player controls. Put a stun counter on each of those creatures. (If a permanent with a stun counter would become untapped, remove one from it instead.)"
    {
        let mut decision = target_decision(
            "targetCreatures",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": card_type("Creature"),
            }),
            0,
            0,
        );
        decision["maximum"] = json!({ "kind": "countOpponents", "player": controller() });
        decision["selectionConstraint"] = json!({ "kind": "distinctPermanentControllers" });
        let targets = json!({ "kind": "chosenTargets", "id": "targetCreatures" });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [decision],
                },
                "effects": [
                    { "kind": "tapPermanent", "permanent": targets.clone() },
                    {
                        "kind": "putCounters",
                        "permanent": targets,
                        "counter": "stun",
                        "count": integer(1),
                    },
                ],
            }),
            &[
                "Choose at most one creature controlled by each opponent",
                "Tap the chosen creatures",
                "Put a stun counter on each chosen creature",
            ],
        ));
    }
    if text
        == "When Hapatra enters, for each opponent, put X -1/-1 counters on up to one target creature that player controls, where X is the greatest mana value among cards in your graveyard."
    {
        let mut decision = target_decision(
            "targetCreatures",
            json!({
                "kind": "permanents",
                "controller": { "kind": "opponentsOf", "player": controller() },
                "where": card_type("Creature"),
            }),
            0,
            0,
        );
        decision["maximum"] = json!({ "kind": "countOpponents", "player": controller() });
        decision["selectionConstraint"] = json!({ "kind": "distinctPermanentControllers" });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [decision],
                },
                "effects": [{
                    "kind": "putCounters",
                    "permanent": { "kind": "chosenTargets", "id": "targetCreatures" },
                    "counter": "-1/-1",
                    "count": {
                        "kind": "greatestManaValue",
                        "zone": graveyard(controller()),
                        "where": Value::Null,
                    },
                }],
            }),
            &[
                "Choose at most one creature controlled by each opponent",
                "Find the greatest mana value in the controller's graveyard",
                "Put that many -1/-1 counters on each chosen creature",
            ],
        ));
    }
    if text
        == "Whenever you cast a prepared spell, copy it. You may choose new targets for the copy."
    {
        return Some(triggered(
            json!({
                "kind": "spellCast",
                "player": controller(),
                "where": Value::Null,
                "preparedOnly": true,
            }),
            vec![json!({
                "kind": "copyStackItem",
                "object": { "kind": "triggeringStackObject" },
                "controller": controller(),
                "mayChooseNewTargets": true,
            })],
        ));
    }
    if text
        == "When this enchantment enters, the owner of up to one other target nonland permanent puts it on their choice of the top or bottom of their library."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        json!({
                            "kind": "permanents",
                            "where": not(card_type("Land")),
                            "excludeSource": true,
                        }),
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "putPermanentOnOwnerLibrary",
                    "permanent": chosen_target("targetPermanent"),
                    "position": "ownerChoiceTopOrBottom",
                }],
            }),
            &[
                "Optionally target another nonland permanent",
                "Let its owner choose the top or bottom of their library",
            ],
        ));
    }
    if text
        == "At the beginning of your end step, if you gained life this turn, surveil 1. If you put a card with mana value less than or equal to the amount of life you gained this turn into your graveyard this way, put that card into your hand."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "stepBegan",
                    "step": "endStep",
                    "player": controller(),
                },
                "condition": compare(
                    ">=",
                    json!({ "kind": "lifeGainedThisTurn", "player": controller() }),
                    integer(1),
                ),
                "effects": [
                    {
                        "kind": "surveil",
                        "player": controller(),
                        "count": integer(1),
                        "bindMovedAs": "surveilledCards",
                    },
                    {
                        "kind": "returnBoundCardsWithinLifeGained",
                        "player": controller(),
                        "binding": "surveilledCards",
                    },
                ],
            }),
            &[
                "Require life gained during the current turn",
                "Surveil one and retain the card moved to the graveyard",
                "Return it when its mana value does not exceed life gained",
            ],
        ));
    }
    if text
        == "Whenever a creature an opponent controls with power or toughness 1 or less blocks, Tetsuko Umezawa deals 1 damage to that creature's controller."
    {
        return Some(triggered(
            json!({
                "kind": "opponentSmallCreatureBlocks",
                "player": controller(),
            }),
            vec![json!({
                "kind": "dealDamage",
                "source": self_ref(),
                "recipient": { "kind": "triggeringPlayer" },
                "amount": integer(1),
            })],
        ));
    }
    if text
        == "Whenever this creature is dealt damage, you may search your library for up to that many land cards, put them onto the battlefield tapped, then shuffle."
    {
        return Some(triggered(
            json!({ "kind": "selfDealtDamage", "object": self_ref() }),
            vec![json!({
                "kind": "invigoratorMaySearchLands",
                "player": controller(),
            })],
        ));
    }

    if text
        == "Whenever you cast a creature spell, exile up to one other target creature you control, then return that card to the battlefield under its owner's control."
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
                        0,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "blinkPermanent",
                    "permanent": chosen_target("targetCreature"),
                    "controller": { "kind": "ownerOf", "object": chosen_target("targetCreature") },
                }],
            }),
            &[
                "Trigger when the controller casts a creature spell",
                "Optionally target another controlled creature",
                "Exile it and return it under its owner's control",
            ],
        ));
    }

    if text
        == "At the beginning of combat on your turn, you may remove a +1/+1 counter from this creature. If you do, put a +1/+1 counter on each other creature you control."
    {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "beginCombat", "player": controller() }),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "removeCounters",
                    "permanent": self_ref(),
                    "counter": "+1/+1",
                    "count": integer(1),
                },
                "onPerformed": [{
                    "kind": "putCounters",
                    "permanent": {
                        "kind": "eachPermanent",
                        "player": controller(),
                        "where": card_type("Creature"),
                        "excludeSource": true,
                    },
                    "counter": "+1/+1",
                    "count": integer(1),
                }],
            })],
        ));
    }
    if text.starts_with(
        "When Vraska enters, if you control six or more lands, destroy target permanent an opponent controls. They create a Treasure token.",
    ) {
        let target = chosen_target("targetPermanent");
        let treasure = json!({
            "kind": "createTokens",
            "controller": { "kind": "boundValue", "id": "targetController" },
            "quantity": integer(1),
            "token": {
                "name": "Treasure",
                "colors": [],
                "types": ["Artifact"],
                "subtypes": ["Treasure"],
                "power": 0,
                "toughness": 0,
                "abilities": [{
                    "kind": "manaAbility",
                    "source": self_ref(),
                    "costs": [
                        { "kind": "tap", "object": self_ref() },
                        { "kind": "sacrificePermanent", "permanent": self_ref() },
                    ],
                    "effects": [{
                        "kind": "addMana",
                        "player": controller(),
                        "mana": { "kind": "chooseColor", "amount": 1 },
                    }],
                }],
            },
        });
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Land"),
                    }),
                    integer(6),
                ),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        json!({
                            "kind": "permanents",
                            "controller": { "kind": "opponentsOf", "player": controller() },
                            "where": Value::Null,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "bind",
                        "id": "targetController",
                        "value": { "kind": "controllerOf", "object": target.clone() },
                    },
                    { "kind": "destroyPermanent", "permanent": target },
                    treasure,
                ],
            }),
            &[
                "Require six controlled lands",
                "Target an opponent's permanent",
                "Preserve its controller before destruction",
                "Create the Treasure for that player",
            ],
        ));
    }

    if text
        == "When this creature enters, creatures you control gain trample and get +X/+0 until end of turn, where X is the number of artifacts you control."
    {
        let creatures = json!({
            "kind": "eachPermanent",
            "player": controller(),
            "where": card_type("Creature"),
        });
        return Some(triggered(
            enter_self(),
            vec![
                json!({
                    "kind": "grantKeyword",
                    "object": creatures.clone(),
                    "keyword": "trample",
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
                json!({
                    "kind": "modifyPowerToughness",
                    "object": creatures,
                    "power": {
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": card_type("Artifact"),
                    },
                    "toughness": integer(0),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }),
            ],
        ));
    }
    if text
        == "When this creature enters, search your library for a card, put it into your hand, shuffle, then discard a card at random."
    {
        return Some(operation(enter_self(), "searchAnyThenDiscardRandom"));
    }
    if text == "When you discard this card, you gain 3 life." {
        let mut parsed = triggered(
            json!({ "kind": "selfDiscarded", "object": self_ref() }),
            vec![json!({
                "kind": "gainLife",
                "player": controller(),
                "amount": integer(3),
            })],
        );
        parsed.rule["triggerZone"] = Value::String("hand".to_string());
        return Some(parsed);
    }
    if text.starts_with("At the beginning of each player's upkeep, you create a 3/3 green Forest Tentacle land creature token.") {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": { "kind": "eachPlayer" } }),
            vec![json!({
                "kind": "createTokens",
                "controller": controller(),
                "quantity": integer(1),
                "token": {
                    "name": "Forest Tentacle Token",
                    "colors": ["green"],
                    "types": ["Land", "Creature"],
                    "subtypes": ["Forest", "Tentacle"],
                    "power": 3,
                    "toughness": 3,
                    "abilities": [{
                        "kind": "manaAbility",
                        "source": self_ref(),
                        "costs": [{ "kind": "tap", "object": self_ref() }],
                        "effects": [{
                            "kind": "addMana",
                            "player": controller(),
                            "mana": "{G}",
                        }],
                    }],
                },
            })],
        ));
    }
    if text.starts_with("At the beginning of your upkeep, surveil 1. Then if there are seven or more cards in your graveyard, sacrifice this artifact") {
        return Some(triggered(
            json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
            vec![
                json!({ "kind": "surveil", "player": controller(), "count": integer(1) }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": compare(
                        ">=",
                        json!({
                            "kind": "countCards",
                            "zone": graveyard(controller()),
                            "where": Value::Null,
                        }),
                        integer(7),
                    ),
                    "then": [
                        { "kind": "sacrificePermanent", "permanent": self_ref() },
                        { "kind": "dealDamageToEachOpponent", "amount": integer(2) },
                        { "kind": "gainLife", "player": controller(), "amount": integer(2) },
                    ],
                    "else": [],
                }),
            ],
        ));
    }
    if text.starts_with("Landfall")
        && text
            .contains("Whenever a land you control enters, Koth deals 1 damage to each opponent.")
        && text.contains("If that land is a Mountain, add {R}.")
    {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Land"),
            }),
            vec![
                json!({
                    "kind": "dealDamageToEachOpponent",
                    "amount": integer(1),
                }),
                json!({
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "objectMatchesFilter",
                        "object": { "kind": "triggeringPermanent" },
                        "where": subtype("Mountain"),
                    },
                    "then": [{
                        "kind": "addMana",
                        "player": controller(),
                        "mana": "{R}",
                    }],
                    "else": [],
                }),
            ],
        ));
    }
    if text == "When this Aura enters, tap enchanted creature. It becomes unprepared." {
        let attached = json!({ "kind": "attachedPermanent", "attachment": self_ref() });
        return Some(triggered(
            enter_self(),
            vec![
                json!({ "kind": "tapPermanent", "permanent": attached.clone() }),
                json!({ "kind": "setPrepared", "object": attached, "value": false }),
            ],
        ));
    }
    if text == "When this creature dies, if it isn't a token, create a token that's a copy of it." {
        return Some(triggered(
            json!({
                "kind": "permanentDied",
                "object": self_ref(),
                "nontoken": true,
            }),
            vec![json!({
                "kind": "createTokenCopyOfPermanent",
                "object": self_ref(),
                "grantKeywords": [],
                "exileAtNextEndStep": false,
            })],
        ));
    }
    if text
        == "At the beginning of combat on your turn, another target creature you control gets +1/+1 until end of turn. If you've scried or surveilled this turn, put a +1/+1 counter on that creature instead."
    {
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
                    "kind": "conditionalEffect",
                    "condition": {
                        "kind": "scriedOrSurveilledThisTurn",
                        "player": controller(),
                    },
                    "then": [{
                        "kind": "putCounters",
                        "permanent": chosen_target("targetCreature"),
                        "counter": "+1/+1",
                        "count": integer(1),
                    }],
                    "else": [{
                        "kind": "modifyPowerToughness",
                        "object": chosen_target("targetCreature"),
                        "power": integer(1),
                        "toughness": integer(1),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                }],
            }),
            &[
                "Resolve beginning-of-combat trigger",
                "Choose the replacement branch",
            ],
        ));
    }
    if text
        == "Whenever a Forest you control enters, if you control at least five other Forests, target creature you control gets +3/+3 until end of turn."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": {
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": subtype("Forest"),
                },
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countPermanents",
                        "player": controller(),
                        "where": subtype("Forest"),
                        "excludeSource": true,
                    }),
                    integer(5),
                ),
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
                    "kind": "modifyPowerToughness",
                    "object": chosen_target("targetCreature"),
                    "power": integer(3),
                    "toughness": integer(3),
                    "duration": { "kind": "untilEndOfCurrentTurn" },
                }],
            }),
            &[
                "Observe a Forest entering",
                "Require five other Forests",
                "Apply the bonus",
            ],
        ));
    }
    let entry_type_counter_re = Regex::new(
        r"(?i)^When (.+?) enters, target permanent you control gains (hexproof|deathtouch) until end of turn\. Put a \+1/\+1 counter on it if it's a creature\. Put a loyalty counter on it if it's a planeswalker\.(?: \(.+\))?$",
    )
    .expect("entry keyword with type-appropriate counter regex compiles");
    if let Some(captures) = entry_type_counter_re.captures(text) {
        let target = chosen_target("targetPermanent");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        json!({ "kind": "permanents", "controller": controller(), "where": Value::Null }),
                        1,
                        1,
                    )],
                },
                "effects": [
                    {
                        "kind": "grantKeyword",
                        "object": target.clone(),
                        "keyword": captures.get(2)?.as_str().to_ascii_lowercase(),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": {
                            "kind": "objectMatchesFilter",
                            "object": target.clone(),
                            "where": card_type("Creature"),
                        },
                        "then": [{
                            "kind": "putCounters",
                            "permanent": target.clone(),
                            "counter": "+1/+1",
                            "count": integer(1),
                        }],
                        "else": [],
                    },
                    {
                        "kind": "conditionalEffect",
                        "condition": {
                            "kind": "objectMatchesFilter",
                            "object": target.clone(),
                            "where": card_type("Planeswalker"),
                        },
                        "then": [{
                            "kind": "putCounters",
                            "permanent": target,
                            "counter": "loyalty",
                            "count": integer(1),
                        }],
                        "else": [],
                    },
                ],
            }),
            &[
                "Target a controlled permanent",
                "Grant the keyword",
                "Add counters by card type",
            ],
        ));
    }
    if text == "When Karn enters, draw a card for each color among other artifacts you control." {
        return Some(triggered(
            enter_self(),
            vec![json!({
                "kind": "drawCards",
                "player": controller(),
                "count": {
                    "kind": "countDistinctColors",
                    "player": controller(),
                    "where": card_type("Artifact"),
                    "excludeSource": true,
                },
            })],
        ));
    }
    if text
        == "When Mabel enters, remove up to three counters from another target creature or planeswalker."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "enterBattlefield", "object": self_ref() },
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [target_decision(
                        "targetPermanent",
                        json!({
                            "kind": "permanents",
                            "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                            "excludeSource": true,
                        }),
                        1,
                        1,
                    )],
                },
                "effects": [{
                    "kind": "removeUpToCounters",
                    "permanent": chosen_target("targetPermanent"),
                    "player": controller(),
                    "maximum": integer(3),
                }],
            }),
            &[
                "Target another creature or planeswalker",
                "Choose and remove up to three counters",
            ],
        ));
    }
    if text.starts_with("When this creature enters, mill four cards. When you do, return target land card from your graveyard to the battlefield tapped.") {
        return Some(triggered(
            enter_self(),
            vec![
                json!({
                    "kind": "mill",
                    "player": controller(),
                    "count": integer(4),
                    "bind": "milledCards",
                }),
                json!({
                    "kind": "createReflexiveTrigger",
                    "source": self_ref(),
                    "controller": controller(),
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": [target_decision(
                                "targetLandCard",
                                json!({
                                    "kind": "cards",
                                    "zone": graveyard(controller()),
                                    "where": card_type("Land"),
                                }),
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "moveTargetCard",
                            "card": chosen_target("targetLandCard"),
                            "to": "battlefield",
                            "tapped": true,
                        }],
                    },
                }),
            ],
        ));
    }
    if text
        == "Whenever a player discards one or more cards, Tinybones deals 1 damage to each opponent."
    {
        return Some(triggered(
            json!({ "kind": "playerDiscardedCard" }),
            vec![json!({ "kind": "dealDamageToEachOpponent", "amount": integer(1) })],
        ));
    }
    if text
        == "At the beginning of each end step, if you've drawn three or more cards this turn, create a 3/3 blue Angel creature token with flying."
    {
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": { "kind": "stepBegan", "step": "endStep", "player": { "kind": "eachPlayer" } },
                "condition": compare(
                    ">=",
                    json!({
                        "kind": "countEventsThisTurn",
                        "event": "cardDrawn",
                        "player": controller(),
                    }),
                    integer(3),
                ),
                "effects": [{
                    "kind": "createTokens",
                    "controller": controller(),
                    "quantity": integer(1),
                    "token": {
                        "name": "Angel Token",
                        "colors": ["blue"],
                        "types": ["Creature"],
                        "subtypes": ["Angel"],
                        "power": 3,
                        "toughness": 3,
                        "abilities": [{
                            "kind": "keywordAbility",
                            "source": self_ref(),
                            "ability": { "kind": "flying" },
                        }],
                    },
                }],
            }),
            &[
                "Check cards drawn at each end step",
                "Create the flying Angel token",
            ],
        ));
    }
    if text.starts_with("When Yuriko enters, choose one —") {
        let target = chosen_target("targetCreature");
        let mut target_choice = target_decision(
            "targetCreature",
            json!({ "kind": "permanents", "where": card_type("Creature") }),
            1,
            1,
        );
        target_choice["condition"] = selection("spellMode", "weaken");
        return Some(draft(
            json!({
                "kind": "triggeredAbility",
                "source": self_ref(),
                "event": enter_self(),
                "declaration": {
                    "kind": "castingDeclaration",
                    "decisions": [{
                        "id": "spellMode",
                        "kind": "chooseModes",
                        "minimum": 1,
                        "maximum": 1,
                        "options": ["weaken", "surveil"],
                    }, target_choice],
                },
                "effects": [{
                    "kind": "conditionalEffect",
                    "condition": selection("spellMode", "weaken"),
                    "then": [{
                        "kind": "modifyPowerToughness",
                        "object": target,
                        "power": {
                            "kind": "negate",
                            "operand": {
                                "kind": "countCards",
                                "zone": graveyard(controller()),
                                "where": Value::Null,
                            },
                        },
                        "toughness": integer(0),
                        "duration": { "kind": "untilEndOfCurrentTurn" },
                    }],
                    "else": [{
                        "kind": "surveil",
                        "player": controller(),
                        "count": integer(2),
                    }],
                }],
            }),
            &["Choose the entry mode", "Resolve the selected branch"],
        ));
    }
    if text
        == "Whenever a creature you control attacks a player alone, it gains double strike until end of turn."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": 1,
                "maximum": 1,
            }),
            vec![json!({
                "kind": "grantKeyword",
                "object": { "kind": "triggeringPermanent" },
                "keyword": "doubleStrike",
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text
        == "Whenever a creature you control attacks a player alone, discard a card, then draw a card. Then put a +1/+1 counter on that creature for each card you've discarded this turn."
    {
        return Some(triggered(
            json!({
                "kind": "controlledCreaturesAttacked",
                "player": controller(),
                "minimum": 1,
                "maximum": 1,
            }),
            vec![
                json!({ "kind": "discardCards", "player": controller(), "count": integer(1) }),
                json!({ "kind": "drawCards", "player": controller(), "count": integer(1) }),
                json!({
                    "kind": "putCounters",
                    "permanent": { "kind": "triggeringPermanent" },
                    "counter": "+1/+1",
                    "count": {
                        "kind": "countEventsThisTurn",
                        "event": "cardDiscarded",
                        "player": controller(),
                    },
                }),
            ],
        ));
    }
    if text
        == "Whenever you activate a loyalty ability, if you removed two or more loyalty counters to activate it, draw a card."
    {
        return Some(triggered(
            json!({
                "kind": "nonManaAbilityActivated",
                "player": controller(),
                "loyaltyOnly": true,
                "minimumLoyaltyRemoved": integer(2),
            }),
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
        ));
    }
    if text
        == "Whenever you cast an Equipment spell or a spell that targets a creature you control, draw a card. This ability triggers only once each turn."
    {
        let mut result = triggered(
            json!({
                "kind": "oneOf",
                "events": [
                    {
                        "kind": "spellCast",
                        "player": controller(),
                        "where": card_type("Equipment"),
                    },
                    {
                        "kind": "spellCastTargetingControlledCreature",
                        "player": controller(),
                    },
                ],
            }),
            vec![json!({ "kind": "drawCards", "player": controller(), "count": integer(1) })],
        );
        result.rule["triggerLimit"] = json!({ "kind": "onceEachTurn", "id": "danithaSwordOfHope" });
        return Some(result);
    }
    if text
        == "Whenever you cast a spell that targets an opponent or a creature an opponent controls, put a +1/+1 counter on Danitha."
    {
        return Some(triggered(
            json!({
                "kind": "spellCastTargetingOpponentOrCreature",
                "player": controller(),
            }),
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }

    if text
        == "Whenever an opponent is dealt noncombat damage, put a +1/+1 counter on Massacre Girl."
    {
        return Some(triggered(
            json!({
                "kind": "opponentDealtDamage",
                "player": controller(),
                "noncombatOnly": true,
            }),
            vec![json!({
                "kind": "putCounters",
                "permanent": self_ref(),
                "counter": "+1/+1",
                "count": integer(1),
            })],
        ));
    }
    if text
        == "Whenever another creature you control enters, Arni gets +X/+0 until end of turn, where X is that creature's power."
    {
        return Some(triggered(
            json!({
                "kind": "permanentEntered",
                "player": controller(),
                "where": card_type("Creature"),
                "excludeSource": true,
            }),
            vec![json!({
                "kind": "modifyPowerToughness",
                "object": self_ref(),
                "power": { "kind": "powerOf", "object": { "kind": "triggeringPermanent" } },
                "toughness": integer(0),
                "duration": { "kind": "untilEndOfCurrentTurn" },
            })],
        ));
    }
    if text
        == "When Winter enters, you may sacrifice a creature or planeswalker. When you do, each opponent sacrifices a creature of their choice."
    {
        return Some(triggered(
            enter_self(),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "sacrificePermanents",
                    "player": controller(),
                    "where": or(vec![card_type("Creature"), card_type("Planeswalker")]),
                    "count": integer(1),
                },
                "onPerformed": [{
                    "kind": "createReflexiveTrigger",
                    "source": self_ref(),
                    "controller": controller(),
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                        "effects": [{
                            "kind": "sacrificePermanentsEachPlayer",
                            "controller": controller(),
                            "scope": "opponents",
                            "where": card_type("Creature"),
                            "count": integer(1),
                        }],
                    },
                }],
            })],
        ));
    }

    if text
        == "When this creature enters, you may discard a card. When you do, this creature deals 2 damage to any target."
    {
        return Some(triggered(
            enter_self(),
            vec![json!({
                "kind": "optionalAction",
                "player": controller(),
                "action": {
                    "kind": "discardCards",
                    "player": controller(),
                    "count": integer(1),
                },
                "onPerformed": [{
                    "kind": "createReflexiveTrigger",
                    "source": self_ref(),
                    "controller": controller(),
                    "ability": {
                        "kind": "triggeredAbility",
                        "source": self_ref(),
                        "event": { "kind": "reflexiveTriggerCreated", "object": self_ref() },
                        "declaration": {
                            "kind": "castingDeclaration",
                            "decisions": [target_decision(
                                "targetDamageable",
                                json!({ "kind": "anyTarget" }),
                                1,
                                1,
                            )],
                        },
                        "effects": [{
                            "kind": "dealDamage",
                            "source": self_ref(),
                            "amount": integer(2),
                            "recipient": chosen_target("targetDamageable"),
                        }],
                    },
                }],
            })],
        ));
    }

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
    let once_operation = |event: Value, operation_name: &str| {
        let mut parsed = operation(event, operation_name);
        parsed.rule["triggerLimit"] = json!({
            "kind": "oncePerGameObject",
            "id": operation_name,
        });
        parsed
    };
    let enter_self = || json!({ "kind": "enterBattlefield", "object": self_ref() });
    match text {
        "At the beginning of your end step, target opponent may draw four cards. If they do, look at that player's hand and you may cast a spell from their hand without paying its mana cost. If they don't, put two +1/+1 counters on Kuroki." => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "kurokiEndStep",
            ));
        }
        "At the beginning of your upkeep, exile the top three cards of your library. Each opponent secretly chooses a number 0 or greater. Then those numbers are revealed. Choose an opponent with the highest number. Itazura deals that much damage to them, then they may cast a spell from among those cards without paying its mana cost. You put a card from among them that wasn't cast this way into your hand." => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
                "itazuraUpkeep",
            ));
        }
        "Whenever a source you control deals damage, if at least one permanent or player was dealt exactly 2 of that damage, exile the top card of your library. You may play it until the end of your next turn." => {
            return Some(operation(
                json!({
                    "kind": "controlledSourceDealtExactDamage",
                    "player": controller(),
                    "amount": 2,
                }),
                "matocExileTop",
            ));
        }
        "Whenever you cast a permanent spell with {X} in its mana cost or activate an ability with {X} in its activation cost that isn't a mana ability, you may have the value of X become 5. Do this only once each turn." => {
            let mut parsed = operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "spellCast",
                            "player": controller(),
                            "where": and(vec![
                                json!({ "kind": "manaCostContainsX" }),
                                or(vec![
                                    card_type("Artifact"),
                                    card_type("Creature"),
                                    card_type("Enchantment"),
                                    card_type("Planeswalker"),
                                    card_type("Battle"),
                                ]),
                            ]),
                        },
                        {
                            "kind": "nonManaAbilityActivatedWithX",
                            "player": controller(),
                        },
                    ],
                }),
                "glavaSetXFive",
            );
            parsed.rule["triggerLimit"] = json!({
                "kind": "onceEachTurn",
                "id": "glavaSetXFive",
            });
            return Some(parsed);
        }
        "Whenever Massimo or another creature you control enters, exile up to one target instant or sorcery card with mana value 1 from your graveyard. If a card is exiled this way, that creature gains \"Whenever this creature deals combat damage to a player, copy the exiled card. You may cast the copy without paying its mana cost.\"" => {
            return Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Creature"),
                }),
                "massimoGrantMagicTrick",
            ));
        }
        "Whenever Miss Highwater deals combat damage to a player who doesn't have a contract counter, they may discard their hand. If they do, they draw seven cards and get a contract counter. For as long as they have a contract counter, when they lose the game, for each artifact and creature they controlled, create a token that's a copy of it." => {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "missHighwaterContract",
            ));
        }
        "Whenever The Madcap Jester attacks, each opponent exiles the top card of their library." => {
            return Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "madcapJesterAttack",
            ));
        }
        "Whenever a spell or ability you control causes one or more cards to be exiled from an opponent's library, you may play those cards until the end of your next turn. Mana of any type can be spent to cast spells this way." => {
            return Some(operation(
                json!({
                    "kind": "opponentLibraryCardsExiledByControlledEffect",
                    "player": controller(),
                }),
                "madcapJesterPermission",
            ));
        }
        "Whenever a saddled creature you control attacks, it gets +2/+2 until end of turn. Draw a card. Create a Food token." => {
            return Some(operation(
                json!({
                    "kind": "controlledCreatureDeclaredAttacker",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "saddledOnly": true,
                }),
                "jandorSaddledAttack",
            ));
        }
        "Whenever Dragon's Smile deals combat damage to a player, that player exiles that many cards from the top of their library. You may play those cards this turn." => {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "dragonSmileCombatExile",
            ));
        }
        "Eminence — At the beginning of your upkeep, Sahir deals 1 damage to you. This ability triggers only if Sahir is in the command zone or on the battlefield." => {
            let mut parsed = operation(
                json!({ "kind": "stepBegan", "step": "upkeep", "player": controller() }),
                "sahirEminenceDamage",
            );
            parsed.rule["triggerZone"] = Value::String("commandZoneOrBattlefield".to_string());
            return Some(parsed);
        }
        "Whenever a source you control deals exactly 1 damage to you and whenever a spell or ability you control causes you to lose exactly 1 life, put a +1/+1 counter on Sahir. If this is the second time this ability has resolved this turn, draw a card." => {
            return Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        {
                            "kind": "controlledSourceDealtExactDamageToController",
                            "player": controller(),
                            "amount": 1,
                        },
                        {
                            "kind": "controlledEffectCausedExactLifeLossToController",
                            "player": controller(),
                            "amount": 1,
                        },
                    ],
                }),
                "sahirDarknessCounter",
            ));
        }
        "Whenever you discard one or more cards at random, select one or more targets at random, choose one or more modes at random, flip one or more coins, or roll one or more dice, put a +1/+1 counter on Chira and create a tapped Treasure token." => {
            return Some(operation(
                json!({ "kind": "randomChoicePerformed", "player": controller() }),
                "chiraRandomReward",
            ));
        }
        "Whenever a face-down creature attacks one of your opponents, it can't be blocked this combat." => {
            return Some(operation(
                json!({
                    "kind": "controlledCreatureDeclaredAttacker",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "faceDownOnly": true,
                }),
                "sashFaceDownUnblockable",
            ));
        }
        "The first spell you cast during each of your turns has assist. When you cast that spell, if another player spent mana to cast it, you and that player each draw a card. (As you cast a spell with assist, you may choose another player. They may pay some or all of the generic mana in the spell's cost.)" => {
            return Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "spellCastOrdinal": 1,
                    "duringControllerTurn": true,
                }),
                "yumeAssistDraw",
            ));
        }
        "The first nonlegendary creature spell you cast each turn has demonstrate. (When you cast that spell, you may copy it. If you do, choose an opponent to also copy it. Each copy becomes a token.)" => {
            return Some(operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": and(vec![
                        card_type("Creature"),
                        not(json!({ "kind": "typeLineContains", "value": "Legendary" })),
                    ]),
                    "spellCastOrdinal": 1,
                }),
                "eldaDemonstrate",
            ));
        }
        "Whenever you cast an artifact creature or Equipment spell, you may copy that spell. Do this only once each turn. (The copy becomes a token.)" => {
            let mut parsed = operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": or(vec![
                        and(vec![card_type("Artifact"), card_type("Creature")]),
                        subtype("Equipment"),
                    ]),
                }),
                "everforgerCopySpell",
            );
            parsed.rule["triggerLimit"] = json!({
                "kind": "onceEachTurn",
                "id": "everforgerCopySpell",
            });
            return Some(parsed);
        }
        "Whenever Tsagan attacks, creatures you control get +1/+0 until end of turn for each creature you control with first strike or double strike." => {
            return Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "tsaganAttack",
            ));
        }
        "Whenever The Weaver King deals combat damage to a player, they mill that many cards. Then for each opponent, put a creature card from that player's graveyard that was put there from their library this turn onto the battlefield under your control." => {
            return Some(operation(
                json!({ "kind": "combatDamageToPlayer", "source": self_ref() }),
                "weaverKingCombatDamage",
            ));
        }
        "At the beginning of your end step, each player may create a tapped land token named Sanctum with \"{T}: Add one mana of any color.\" Each opponent who does can't attack you during their next turn." => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "zagorkaSanctums",
            ));
        }
        "Whenever another nontoken Ooze creature you control dies, create two tokens that are copies of that creature, except they're 2/2 and they aren't legendary." => {
            return Some(operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": subtype("Ooze"),
                    "excludeSource": true,
                    "nontoken": true,
                }),
                "uugguuCopyOoze",
            ));
        }
        "When Tresserhorn's Lord enters, sacrifice him unless you sacrifice three creatures, pay 3 life, and have target opponent draw three cards." => {
            return Some(operation(enter_self(), "tresserhornEntryPayment"));
        }
        "Whenever you sacrifice a creature, draw a card." => {
            return Some(operation(
                json!({
                    "kind": "permanentDied",
                    "player": controller(),
                    "where": card_type("Creature"),
                    "reason": "sacrificed",
                }),
                "tresserhornSacrificeDraw",
            ));
        }
        "Whenever a creature you control with mana value 7 or greater attacks, double its power and toughness until end of turn." => {
            return Some(operation(
                json!({
                    "kind": "controlledCreatureDeclaredAttacker",
                    "player": controller(),
                    "where": {
                        "kind": "compare",
                        "operator": ">=",
                        "left": { "kind": "manaValueOf", "object": { "kind": "candidate" } },
                        "right": integer(7),
                    },
                }),
                "maularDoubleAttacker",
            ));
        }
        "When Nephilim Epochal enters, search your library for a Nephilim card, reveal it, put it into your hand, then shuffle." => {
            return Some(operation(enter_self(), "nephilimEpochalSearch"));
        }
        value if value.starts_with("Landfall")
            && value.contains("twice the number of Crabs")
            && value.contains("Trilobites") =>
        {
            return Some(operation(
                json!({
                    "kind": "permanentEntered",
                    "player": controller(),
                    "where": card_type("Land"),
                }),
                "homerLandfall",
            ));
        }
        "Whenever Grizzlegom attacks, create a 1/1 white Soldier creature token for each Plains you control. Draw a card for each Island you control. Each opponent loses 1 life for each Swamp you control. Put a +1/+1 counter on Grizzlegom for each Mountain you control. You gain 1 life for each Forest you control." => {
            return Some(operation(
                json!({ "kind": "declaredAttacker", "object": self_ref() }),
                "grizzlegomAttack",
            ));
        }
        "When Selenia dies, create a legendary black Aura Curse enchantment token named Selenia's Curse attached to target opponent. The token has enchant player and \"If enchanted player would lose life, they lose twice that much life instead.\"" => {
            return Some(operation(
                json!({ "kind": "permanentDied", "object": self_ref() }),
                "seleniaCurse",
            ));
        }
        "When Balefang enters, target opponent creates a tapped Baneslayer Angel token. The token is goaded for the rest of the game. (It's a {3}{W}{W} 5/5 Angel creature with flying, first strike, lifelink, and protection from Demons and from Dragons.)" => {
            return Some(operation(enter_self(), "balefangBaneslayer"));
        }
        "Whenever a creature you control becomes blocked, you may untap it and remove it from combat." => {
            return Some(operation(
                json!({
                    "kind": "controlledCreatureBecameBlocked",
                    "player": controller(),
                }),
                "niveaUnblockCreature",
            ));
        }
        "Whenever you cast an Aura spell that targets a creature an opponent controls, draw a card. This ability triggers only once each turn." => {
            let mut parsed = operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": subtype("Aura"),
                }),
                "grakkAuraDraw",
            );
            parsed.rule["triggerLimit"] = json!({
                "kind": "onceEachTurn",
                "id": "grakkAuraDraw",
            });
            return Some(parsed);
        }
        "Whenever Olinda enters or attacks, for each opponent, put an odor counter on up to one target creature that player controls without an odor counter on it. Each of those creatures has \"At the beginning of your upkeep, you lose 2 life\" for as long as it has an odor counter on it. (They continue to smell after Olinda has left the battlefield.)" => {
            return Some(operation(
                json!({
                    "kind": "oneOf",
                    "events": [
                        { "kind": "enterBattlefield", "object": self_ref() },
                        { "kind": "declaredAttacker", "object": self_ref() },
                    ],
                }),
                "olindaOdor",
            ));
        }
        "When this Case enters, conjure four cards named Fblthp, the Lost into your library, then shuffle. Draw a card." => {
            return Some(operation(enter_self(), "lostWitnessEnter"));
        }
        value if value.starts_with("To solve") && value.contains("legendary Homunculus") => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "solveLostWitness",
            ));
        }
        "When Perforator Crocodile enters, for each creature your opponents control, conjure a card named Stab Wound onto the battlefield attached to that creature." => {
            return Some(operation(enter_self(), "perforatorStabWounds"));
        }
        "When Euru enters, you may forage. When you do, conjure a card named Chitterspitter onto the battlefield. (To forage, exile three cards from your graveyard or sacrifice a Food.)" => {
            return Some(operation(enter_self(), "euruForageEnter"));
        }
        "Whenever one or more Squirrels you control deal combat damage to a player, you may sacrifice a token. If you do, put an acorn counter on each permanent you control named Chitterspitter." => {
            return Some(operation(
                json!({
                    "kind": "controlledCreaturesCombatDamageToPlayer",
                    "player": controller(),
                    "where": subtype("Squirrel"),
                }),
                "euruSquirrelDamage",
            ));
        }
        "When you attack with three or more creatures, conjure a card named Mox Ruby into your hand. This ability triggers only once." => {
            return Some(once_operation(
                json!({ "kind": "controlledCreaturesAttacked", "player": controller() }),
                "rubyCollectorConjure",
            ))
        }
        "When you draw your third card in a turn, conjure a card named Mox Emerald into your hand. This ability triggers only once." => {
            return Some(once_operation(
                json!({ "kind": "cardDrawn", "player": controller(), "drawOrdinal": 3 }),
                "emeraldCollectorConjure",
            ))
        }
        "When Oracle of the Alpha enters the battlefield, conjure the Power Nine into your library, then shuffle." => {
            return Some(operation(enter_self(), "oracleAlphaConjurePowerNine"));
        }
        "At the beginning of your second main phase, if you gained 4 or more life this turn, conjure a card named Mox Pearl into your hand. This ability triggers only once." => {
            return Some(once_operation(
                json!({ "kind": "stepBegan", "step": "postcombatMain", "player": controller() }),
                "pearlCollectorConjure",
            ))
        }
        value if value.starts_with("Celebration") && value.contains("Food Fight") => {
            return Some(operation(
                json!({ "kind": "stepBegan", "step": "endStep", "player": controller() }),
                "overcookedCelebration",
            ))
        }
        "When you cast your second noncreature spell in a turn, conjure a card named Mox Sapphire into your hand. This ability triggers only once." => {
            return Some(once_operation(
                json!({
                    "kind": "spellCast",
                    "player": controller(),
                    "where": not(card_type("Creature")),
                    "spellCastOrdinal": 2,
                }),
                "sapphireCollectorConjure",
            ))
        }
        "At the beginning of your second main phase, if there are four or more cards in your graveyard, conjure a card named Mox Jet into your hand. This ability triggers only once." => {
            return Some(once_operation(
                json!({ "kind": "stepBegan", "step": "postcombatMain", "player": controller() }),
                "jetCollectorConjure",
            ))
        }
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
        "Whenever you tap a land creature for mana, add an additional {G}." => Some(operation(
            json!({
                "kind": "controlledPermanentManaAbilityActivated",
                "player": controller(),
                "where": and(vec![card_type("Land"), card_type("Creature")]),
            }),
            "badgermoleAdditionalGreen",
        )),
        "At the beginning of your upkeep, put a tide counter on Rikala. Then if there are four or more tide counters on Rikala, remove them." => {
            Some(operation(upkeep(controller()), "rikalaTideUpkeep"))
        }
        "Whenever you tap an Island for mana, if there are three or more tide counters on Rikala, add an additional {U}." => {
            Some(operation(
                json!({
                    "kind": "controlledPermanentManaAbilityActivated",
                    "player": controller(),
                    "where": subtype("Island"),
                }),
                "rikalaAdditionalBlue",
            ))
        }
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
        "Whenever a player taps a basic land for mana, that player adds one mana of any type that land produced." => {
            Some(operation(
                json!({
                    "kind": "controlledPermanentManaAbilityActivated",
                    "anyPlayer": true,
                    "where": {
                        "kind": "and",
                        "operands": [
                            card_type("Land"),
                            { "kind": "typeLineContains", "value": "Basic" },
                        ],
                    },
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
        == "When enchanted creature dies, return that card to the battlefield tapped under its owner's control."
    {
        return Some(triggered(
            json!({ "kind": "attachedPermanentDied", "attachment": self_ref() }),
            None,
            vec![json!({
                "kind": "moveTriggeringCardFromGraveyard",
                "to": "battlefield",
                "tapped": true,
            })],
        ));
    }
    if text == "When this creature dies, put its counters on up to one target creature you control."
    {
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
                    0,
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
        "Whenever another creature dies, create a Food token. This ability triggers only once each turn. (It's an artifact with \"{2}, {T}, Sacrifice this token: You gain 3 life.\")" => {
            Some(limited_operation(
                json!({
                    "kind": "permanentDied",
                    "anyPlayer": true,
                    "where": card_type("Creature"),
                    "excludeSource": true,
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
