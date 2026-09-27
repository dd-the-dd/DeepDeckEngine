use mtg_engine::engine::rule_is_executable;
use mtg_engine::model::{PlayableCardInput, compile_playable_card};
use mtg_engine::oracle::{OracleCardFace, OracleCardParseRequest, parse_oracle_card};

fn card_parse_error(request: OracleCardParseRequest) -> Option<String> {
    let name = request.card_name.clone();
    let parsed = parse_oracle_card(request.clone());
    let unsupported = parsed
        .abilities
        .iter()
        .filter(|ability| ability.status != "canonical")
        .map(|ability| ability.source.text.as_str())
        .collect::<Vec<_>>();

    if parsed.status != "canonical" {
        return Some(format!(
            "{name} has unsupported Oracle abilities: {unsupported:#?}"
        ));
    }
    if !parsed
        .abilities
        .iter()
        .all(|ability| ability.rule.as_ref().is_some_and(rule_is_executable))
    {
        return Some(format!(
            "{name} produced a rule the engine cannot execute: {:#?}",
            parsed.abilities
        ));
    }
    let compilation = compile_playable_card(PlayableCardInput {
        id: format!("test:{}", name.to_ascii_lowercase().replace(' ', "-")),
        face_id: request.faces.first().map(|face| face.id.clone()),
        is_token: false,
        is_game_piece: false,
        is_sideboard: false,
        power: request.faces.first().and_then(|face| face.power.clone()),
        toughness: request
            .faces
            .first()
            .and_then(|face| face.toughness.clone()),
        oracle: request,
    })
    .unwrap_or_else(|error| panic!("{name} could not compile as a playable card: {error}"));
    if compilation.parser_status != "canonical" || compilation.engine_status != "executable" {
        return Some(format!(
            "{name} did not compile as playable: parser={}, engine={}",
            compilation.parser_status, compilation.engine_status
        ));
    }
    match name.as_str() {
        "Bruvac the Grandiloquent" => {
            let modifier = &parsed.abilities[0].rule.as_ref().unwrap()["modifiers"][0];
            assert_eq!(modifier["kind"], "multiplyMill");
            assert_eq!(modifier["factor"]["value"], 2);
        }
        "Ghost Vacuum" => {
            let effect = &parsed.abilities[1].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "moveCards");
            assert_eq!(effect["to"]["basePower"]["value"], 1);
            assert_eq!(effect["to"]["baseToughness"]["value"], 1);
            assert_eq!(effect["to"]["enterWithCounters"][0]["counter"], "flying");
            assert_eq!(effect["to"]["addSubtypes"][0], "Spirit");
        }
        "High Tide" => {
            let effect = &parsed.abilities[0].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "installAdditionalManaOnLandTap");
            assert_eq!(effect["where"]["value"], "Island");
            assert_eq!(effect["mana"], "U");
        }
        "Restless Reef" => {
            let effect = &parsed.abilities[2].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "becomeCreature");
            assert_eq!(effect["addColors"], serde_json::json!(["U", "B"]));
            assert_eq!(effect["addSubtypes"], serde_json::json!(["Shark"]));
        }
        "Jidoor, Aristocratic Capital // Overture" => {
            let effect = &parsed.abilities[2].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "mill");
            assert_eq!(effect["count"]["kind"], "divide");
            assert_eq!(effect["count"]["right"]["value"], 2);
            assert_eq!(effect["count"]["round"], "down");
        }
        "Doomsday Excruciator" => {
            let rule = parsed.abilities[1].rule.as_ref().unwrap();
            assert_eq!(rule["condition"]["kind"], "wasCast");
            assert_eq!(rule["effects"][0]["kind"], "exileLibrariesExceptBottom");
            assert_eq!(rule["effects"][0]["retainBottom"]["value"], 6);
        }
        "Extirpate" | "Surgical Extraction" => {
            let rule = parsed.abilities.last().unwrap().rule.as_ref().unwrap();
            assert_eq!(rule["effects"][0]["kind"], "exileNamedCardsFromZones");
            assert_eq!(rule["effects"][0]["all"], true);
        }
        "Flood of Recollection" => {
            let rule = parsed.abilities[0].rule.as_ref().unwrap();
            assert_eq!(rule["effects"][0]["kind"], "moveTargetCard");
            assert_eq!(rule["effects"][0]["to"], "hand");
            assert_eq!(rule["exileAfterResolution"], true);
        }
        "Founding the Third Path" => {
            assert_eq!(
                parsed.abilities[0].rule.as_ref().unwrap()["ability"]["kind"],
                "readAhead"
            );
            assert_eq!(
                parsed.abilities[1].rule.as_ref().unwrap()["effects"][0]["kind"],
                "castAnyNumber"
            );
            assert_eq!(
                parsed.abilities[2].rule.as_ref().unwrap()["effects"][0]["kind"],
                "mill"
            );
            let final_effects = parsed.abilities[3].rule.as_ref().unwrap()["effects"]
                .as_array()
                .unwrap();
            assert_eq!(final_effects[0]["kind"], "moveTargetCard");
            assert_eq!(final_effects[1]["kind"], "createCardCopy");
            assert_eq!(final_effects[2]["kind"], "castAnyNumber");
        }
        "Infinite Obliteration" => {
            let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
                .as_array()
                .unwrap();
            assert_eq!(effects[0]["kind"], "chooseCardName");
            assert_eq!(effects[0]["where"]["value"], "Creature");
            assert_eq!(effects[1]["kind"], "exileNamedCardsFromZones");
        }
        "Nature's Claim" => {
            let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
                .as_array()
                .unwrap();
            assert_eq!(effects[1]["kind"], "destroyPermanent");
            assert_eq!(effects[2]["kind"], "gainLife");
            assert_eq!(effects[2]["amount"]["value"], 4);
        }
        "Nix" => {
            let effect = &parsed.abilities[0].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "conditionalEffect");
            assert_eq!(effect["condition"]["kind"], "noManaSpentToCastTargetSpell");
            assert_eq!(effect["then"][0]["kind"], "counterSpell");
        }
        "Pongify" => {
            let effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
                .as_array()
                .unwrap();
            assert_eq!(effects[1]["kind"], "destroyPermanent");
            assert_eq!(effects[1]["cannotRegenerate"], true);
            assert_eq!(effects[2]["kind"], "createTokens");
            assert_eq!(effects[2]["token"]["subtypes"][0], "Ape");
        }
        "Ravenous Trap" => {
            let ability = &parsed.abilities[0].rule.as_ref().unwrap()["ability"];
            assert_eq!(ability["kind"], "alternativeCost");
            assert_eq!(
                ability["condition"]["kind"],
                "opponentCardsEnteredGraveyardThisTurn"
            );
            assert_eq!(ability["condition"]["minimum"]["value"], 3);
        }
        "Scheming Symmetry" => {
            let rule = parsed.abilities[0].rule.as_ref().unwrap();
            assert_eq!(
                rule["effects"][0]["kind"],
                "chosenPlayersSearchLibrariesToTop"
            );
            assert_eq!(rule["declaration"]["decisions"][0]["minimum"], 2);
            assert_eq!(rule["declaration"]["decisions"][0]["maximum"], 2);
        }
        "Summoning Trap" => {
            let ability = &parsed.abilities[0].rule.as_ref().unwrap()["ability"];
            assert_eq!(
                ability["condition"]["kind"],
                "controlledSpellCounteredByOpponentThisTurn"
            );
            let effects = parsed.abilities[1].rule.as_ref().unwrap()["effects"]
                .as_array()
                .unwrap();
            assert_eq!(effects[0]["kind"], "lookAtTopCards");
            assert_eq!(effects[0]["count"]["value"], 7);
            assert_eq!(effects[2]["to"]["kind"], "battlefield");
            assert_eq!(effects.last().unwrap()["to"]["position"], "bottom");
        }
        "Superior Spider-Man" => {
            let rule = parsed.abilities[0].rule.as_ref().unwrap();
            assert_eq!(rule["kind"], "replacementEffect");
            assert_eq!(rule["decisions"][0]["kind"], "chooseGraveyardCard");
            assert_eq!(rule["replacement"][0]["kind"], "copyEnteringGraveyardCard");
            assert_eq!(rule["replacement"][0]["basePower"]["value"], 4);
            assert_eq!(rule["replacement"][0]["baseToughness"]["value"], 4);
        }
        "Split Decision" => {
            let effect = &parsed.abilities[0].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "willOfCouncilCounterOrCopy");
            assert_eq!(effect["tiesCopy"], true);
        }
        "Twincast" => {
            let effect = &parsed.abilities[0].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(effect["kind"], "copyStackItem");
            assert_eq!(effect["mayChooseNewTargets"], true);
        }
        "Whiplash Trap" => {
            let alternative = &parsed.abilities[0].rule.as_ref().unwrap()["ability"];
            assert_eq!(
                alternative["condition"]["kind"],
                "opponentPermanentsEnteredThisTurn"
            );
            assert_eq!(alternative["condition"]["minimum"]["value"], 2);
            let return_effect = &parsed.abilities[1].rule.as_ref().unwrap()["effects"][0];
            assert_eq!(return_effect["kind"], "returnToOwnersHand");
            assert_eq!(return_effect["object"]["kind"], "chosenTargets");
        }
        _ => {}
    }
    None
}

fn assert_card_is_canonical_and_executable(request: OracleCardParseRequest) {
    if let Some(error) = card_parse_error(request) {
        panic!("{error}");
    }
}

#[test]
fn mill_deck_cards_are_canonical_and_executable() {
    let cards = [
        (
            "Bruvac the Grandiloquent",
            "Legendary Creature — Human Advisor",
            Some("{2}{U}"),
            "If an opponent would mill one or more cards, they mill twice that many cards instead. (To mill a card, a player puts the top card of their library into their graveyard.)",
        ),
        (
            "Chancellor of the Spires",
            "Creature — Phyrexian Sphinx",
            Some("{4}{U}{U}{U}"),
            "You may reveal this card from your opening hand. If you do, at the beginning of the first upkeep, each opponent mills seven cards.\nFlying\nWhen this creature enters, you may cast target instant or sorcery card from an opponent's graveyard without paying its mana cost.",
        ),
        (
            "Dusk Rose Reliquary",
            "Artifact",
            Some("{W}"),
            "As an additional cost to cast this spell, sacrifice an artifact or creature.\nWard {2}\nWhen this artifact enters, exile target artifact or creature an opponent controls until this artifact leaves the battlefield.",
        ),
        (
            "Doomsday Excruciator",
            "Creature — Demon",
            Some("{B}{B}{B}{B}{B}{B}"),
            "Flying\nWhen this creature enters, if it was cast, each player exiles all but the bottom six cards of their library face down.\nAt the beginning of your upkeep, draw a card.",
        ),
        (
            "Extirpate",
            "Instant",
            Some("{B}"),
            "Split second (As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities.)\nChoose target card in a graveyard other than a basic land card. Search its owner's graveyard, hand, and library for all cards with the same name as that card and exile them. Then that player shuffles.",
        ),
        (
            "Surgical Extraction",
            "Instant",
            Some("{B/P}"),
            "Choose target card in a graveyard other than a basic land card. Search its owner's graveyard, hand, and library for all cards with the same name as that card and exile them. Then that player shuffles.",
        ),
        (
            "Founding the Third Path",
            "Enchantment — Saga",
            Some("{1}{U}"),
            "Read ahead (Choose a chapter and start with that many lore counters. Add one after your draw step. Skipped chapters don't trigger. Sacrifice after III.)\nI — You may cast an instant or sorcery spell with mana value 1 or 2 from your hand without paying its mana cost.\nII — Target player mills four cards.\nIII — Exile target instant or sorcery card from your graveyard. Copy it. You may cast the copy.",
        ),
        (
            "Fugitive Droid",
            "Artifact Creature — Robot Scientist",
            Some("{U}"),
            "This creature can't be blocked if an artifact entered the battlefield under your control this turn.\n{U}, Sacrifice this creature: Counter target spell that targets an artifact or creature you control.",
        ),
        (
            "Flood of Recollection",
            "Sorcery",
            Some("{U}{U}"),
            "Return target instant or sorcery card from your graveyard to your hand. Exile Flood of Recollection.",
        ),
        (
            "Ghost Vacuum",
            "Artifact",
            Some("{1}"),
            "{T}: Exile target card from a graveyard.\n{6}, {T}, Sacrifice this artifact: Put each creature card exiled with this artifact onto the battlefield under your control with a flying counter on it. Each of them is a 1/1 Spirit in addition to its other types. Activate only as a sorcery.",
        ),
        (
            "Glasses of Urza",
            "Artifact",
            Some("{1}"),
            "{T}: Look at target player's hand.",
        ),
        (
            "Infinite Obliteration",
            "Sorcery",
            Some("{1}{B}{B}"),
            "Choose a creature card name. Search target opponent's graveyard, hand, and library for any number of cards with that name and exile them. Then that player shuffles.",
        ),
        (
            "Nature's Claim",
            "Instant",
            Some("{G}"),
            "Destroy target artifact or enchantment. Its controller gains 4 life.",
        ),
        (
            "Nix",
            "Instant",
            Some("{U}"),
            "Counter target spell if no mana was spent to cast it.",
        ),
        (
            "Peek",
            "Instant",
            Some("{U}"),
            "Look at target player's hand.\nDraw a card.",
        ),
        (
            "Pongify",
            "Instant",
            Some("{U}"),
            "Destroy target creature. It can't be regenerated. Its controller creates a 3/3 green Ape creature token.",
        ),
        (
            "High Tide",
            "Instant",
            Some("{U}"),
            "Until end of turn, whenever a player taps an Island for mana, that player adds an additional {U}.",
        ),
        (
            "Profane Memento",
            "Artifact",
            Some("{1}"),
            "Whenever a creature card is put into an opponent's graveyard from anywhere, you gain 1 life.",
        ),
        (
            "Ravenous Trap",
            "Instant — Trap",
            Some("{2}{B}{B}"),
            "If an opponent had three or more cards put into their graveyard from anywhere this turn, you may pay {0} rather than pay this spell's mana cost.\nExile target player's graveyard.",
        ),
        (
            "Restless Reef",
            "Land",
            None,
            "This land enters tapped.\n{T}: Add {U} or {B}.\n{2}{U}{B}: Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.\nWhenever this land attacks, target player mills four cards.",
        ),
        (
            "Scheming Symmetry",
            "Sorcery",
            Some("{B}"),
            "Choose two target players. Each of them searches their library for a card, then shuffles and puts that card on top.",
        ),
        (
            "Standstill",
            "Enchantment",
            Some("{1}{U}"),
            "When a player casts a spell, sacrifice this enchantment. If you do, each of that player's opponents draws three cards.",
        ),
        (
            "Split Decision",
            "Instant",
            Some("{1}{U}"),
            "Will of the council — Choose target instant or sorcery spell. Starting with you, each player votes for denial or duplication. If denial gets more votes, counter the spell. If duplication gets more votes or the vote is tied, copy the spell. You may choose new targets for the copy.",
        ),
        (
            "Summoning Trap",
            "Instant — Trap",
            Some("{4}{G}{G}"),
            "If a creature spell you cast this turn was countered by a spell or ability an opponent controlled, you may pay {0} rather than pay this spell's mana cost.\nLook at the top seven cards of your library. You may put a creature card from among them onto the battlefield. Put the rest on the bottom of your library in any order.",
        ),
        (
            "Superior Spider-Man",
            "Legendary Creature — Spider Human Hero",
            Some("{2}{U}{B}"),
            "Mind Swap — You may have Superior Spider-Man enter as a copy of any creature card in a graveyard, except his name is Superior Spider-Man and he's a 4/4 Spider Human Hero in addition to his other types. When you do, exile that card.",
        ),
        (
            "Twincast",
            "Instant",
            Some("{U}{U}"),
            "Copy target instant or sorcery spell. You may choose new targets for the copy.",
        ),
        (
            "Whiplash Trap",
            "Instant — Trap",
            Some("{3}{U}{U}"),
            "If an opponent had two or more creatures enter the battlefield under their control this turn, you may pay {U} rather than pay this spell's mana cost.\nReturn two target creatures to their owners' hands.",
        ),
    ];

    let errors = cards
        .into_iter()
        .filter_map(|(card_name, type_line, mana_cost, oracle_text)| {
            card_parse_error(OracleCardParseRequest {
                card_name: card_name.to_string(),
                type_line: type_line.to_string(),
                mana_cost: mana_cost.map(str::to_string),
                oracle_text: Some(oracle_text.to_string()),
                layout: None,
                faces: Vec::new(),
            })
        })
        .collect::<Vec<_>>();

    assert!(errors.is_empty(), "{}", errors.join("\n\n"));
}

#[test]
fn jidoor_adventure_faces_are_canonical_and_executable() {
    assert_card_is_canonical_and_executable(OracleCardParseRequest {
        card_name: "Jidoor, Aristocratic Capital // Overture".to_string(),
        type_line: "Land — Town // Sorcery — Adventure".to_string(),
        mana_cost: Some("{4}{U}{U}".to_string()),
        oracle_text: None,
        layout: Some("adventure".to_string()),
        faces: vec![
            OracleCardFace {
                id: "jidoor".to_string(),
                name: "Jidoor, Aristocratic Capital".to_string(),
                type_line: "Land — Town".to_string(),
                mana_cost: None,
                oracle_text: "This land enters tapped.\n{T}: Add {U}.".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
            OracleCardFace {
                id: "overture".to_string(),
                name: "Overture".to_string(),
                type_line: "Sorcery — Adventure".to_string(),
                mana_cost: Some("{4}{U}{U}".to_string()),
                oracle_text: "Target opponent mills half their library, rounded down. (Then exile this card. You may play the land later from exile.)".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    });
}

#[test]
fn sea_gate_restoration_faces_are_canonical_and_executable() {
    let request = OracleCardParseRequest {
        card_name: "Sea Gate Restoration // Sea Gate, Reborn".to_string(),
        type_line: "Sorcery // Land".to_string(),
        mana_cost: None,
        oracle_text: None,
        layout: Some("modal_dfc".to_string()),
        faces: vec![
            OracleCardFace {
                id: "sea-gate-restoration".to_string(),
                name: "Sea Gate Restoration".to_string(),
                type_line: "Sorcery".to_string(),
                mana_cost: Some("{4}{U}{U}{U}".to_string()),
                oracle_text: "Draw cards equal to the number of cards in your hand plus one. You have no maximum hand size for the rest of the game.".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
            OracleCardFace {
                id: "sea-gate-reborn".to_string(),
                name: "Sea Gate, Reborn".to_string(),
                type_line: "Land".to_string(),
                mana_cost: Some(String::new()),
                oracle_text: "As this land enters, you may pay 3 life. If you don't, it enters tapped.\n{T}: Add {U}.".to_string(),
                power: None,
                toughness: None,
                loyalty: None,
            },
        ],
    };
    let parsed = parse_oracle_card(request.clone());
    let spell_effects = parsed.abilities[0].rule.as_ref().unwrap()["effects"]
        .as_array()
        .unwrap();
    assert_eq!(spell_effects[0]["kind"], "drawCards");
    assert_eq!(spell_effects[0]["count"]["kind"], "add");
    assert_eq!(spell_effects[1]["kind"], "grantNoMaximumHandSize");
    assert_card_is_canonical_and_executable(request);
}
