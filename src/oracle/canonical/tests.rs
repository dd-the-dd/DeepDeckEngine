use super::*;

#[test]
fn linked_hand_exile_composes_card_criteria_and_preserves_the_source_link() {
    for instruction in [
        "You may exile an instant card with mana value 2 or less from your hand.",
        "You may exile a creature card from your hand.",
    ] {
        let (effects, decisions) = parse_general_effect_instruction(instruction, "Test Artifact")
            .expect("linked hand exile should compose from reusable zone and criteria leaves");
        assert_eq!(effects[0]["kind"], "exileTargetCardWithSource");
        assert_eq!(effects[0]["fromZone"], "hand");
        assert_eq!(decisions[0]["candidates"]["zone"]["kind"], "hand");
    }

    assert!(
        parse_general_effect_instruction(
            "You may exile an instant card with mana value 2 or less from your library.",
            "Test Artifact",
        )
        .is_none()
    );
}

#[test]
fn linked_exiled_card_copy_sequence_supports_optional_and_mandatory_copying() {
    for (instruction, outer_kind) in [
        (
            "You may copy the exiled card. If you do, you may cast the copy without paying its mana cost.",
            "optionalEffects",
        ),
        (
            "Copy the exiled card. You may cast the copy without paying its mana cost.",
            "createCardCopy",
        ),
    ] {
        let (effects, decisions) = parse_general_effect_sequence(instruction, "Test Artifact")
            .expect("linked exiled-card copying should compose");
        assert!(decisions.is_empty());
        assert_eq!(effects[0]["kind"], outer_kind);
        let executable_effects = if outer_kind == "optionalEffects" {
            effects[0]["effects"]
                .as_array()
                .expect("optional copy effects")
        } else {
            &effects
        };
        assert_eq!(executable_effects[0]["kind"], "createCardCopy");
        assert_eq!(
            executable_effects[0]["card"]["kind"],
            "cardExiledWithSource"
        );
        assert_eq!(executable_effects[1]["kind"], "castAnyNumber");
        assert_eq!(executable_effects[2]["kind"], "ceaseToExist");
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": executable_effects,
        })));
    }

    assert!(
        parse_general_effect_sequence(
            "Copy target card. You may cast the copy without paying its mana cost.",
            "Test Artifact",
        )
        .is_none()
    );
}

#[test]
fn linked_card_mana_value_can_fix_an_activated_x_cost() {
    let parsed = parse_simple_activated_ability_for_face(
        "{X}, {T}: Copy the exiled card. You may cast the copy without paying its mana cost. X is the mana value of the exiled card.",
        "Test Arcanist",
    )
    .expect("a linked-card mana value should constrain the activation's X");
    assert_eq!(parsed.rule["costs"][0]["manaCost"], "{X}");
    assert_eq!(parsed.rule["activationXValue"]["kind"], "manaValueOf");
    assert_eq!(
        parsed.rule["activationXValue"]["object"]["kind"],
        "cardExiledWithSource"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_simple_activated_ability_for_face(
            "{X}, {T}: Copy the exiled card. X is the power of the exiled card.",
            "Test Arcanist",
        )
        .is_none()
    );
}

#[test]
fn deck_audit_primitives_compose_related_oracle_formulations() {
    for instruction in [
        "Target opponent mills thirteen cards.",
        "Target player mills three cards.",
        "Target player takes an extra turn after this one.",
        "Untap all artifacts you control.",
        "Put target card from a graveyard on top of its owner's library.",
    ] {
        let (effects, _) = parse_general_effect_instruction(instruction, "Test Spell")
            .unwrap_or_else(|| panic!("{instruction} should compose"));
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        })));
    }

    assert!(
        parse_general_effect_instruction("Target team mills thirteen cards.", "Test Spell",)
            .is_none()
    );
}

#[test]
fn bounded_free_cast_from_hand_composes_spell_criteria_and_runtime_arithmetic() {
    assert!(parse_permanent_criteria("instant or sorcery", "").is_some());
    assert!(
        parse_numeric_expression_text("twice the number of legendary Wizards you control")
            .is_some()
    );
    assert!(parse_general_effect_instruction(
        "you may cast an instant or sorcery spell with mana value X or less from your hand without paying its mana cost, where X is twice the number of legendary Wizards you control.",
        "Test Wizard",
    )
    .is_some());
    let parsed = parse_expansion_triggered(
        "At the beginning of combat on your turn, you may cast an instant or sorcery spell with mana value X or less from your hand without paying its mana cost, where X is twice the number of legendary Wizards you control.",
        "Test Wizard",
    )
    .expect("the beginning-of-combat trigger should compose the bounded hand cast");
    assert_eq!(parsed.rule["event"]["kind"], "stepBegan");
    assert_eq!(parsed.rule["event"]["step"], "beginCombat");
    assert_eq!(parsed.rule["effects"][0]["kind"], "castAnyNumber");
    assert_eq!(parsed.rule["effects"][0]["cards"]["zone"]["kind"], "hand");
    assert_eq!(
        parsed.rule["effects"][0]["cards"]["where"]["operands"][1]["right"]["kind"],
        "multiply"
    );
    assert_eq!(
        parsed.rule["effects"][0]["cards"]["where"]["operands"][1]["right"]["left"]["kind"],
        "countPermanents"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let (effects, decisions) = parse_general_effect_instruction(
        "You may cast one creature spell with mana value X or less from your hand without paying its mana cost, where X is the number of artifacts you control.",
        "",
    )
    .expect("an unrelated spell criterion and count expression should use the same leaves");
    assert!(decisions.is_empty());
    assert_eq!(
        effects[0]["cards"]["where"]["operands"][0]["kind"],
        "cardTypeContains"
    );
    assert_eq!(
        effects[0]["cards"]["where"]["operands"][1]["right"]["kind"],
        "countPermanents"
    );

    assert!(parse_general_effect_instruction(
        "You may cast an instant spell with mana value X or less from your graveyard without paying its mana cost, where X is the number of Wizards you control.",
        "",
    )
    .is_none());
}

#[test]
fn controlled_counted_lord_bonus_composes_selectors_criteria_and_scaling() {
    let parsed = parse_common_static_ability(
        "Other Dwarves you control get +1/+0 for each artifact token you control.",
        "Test Dwarf",
    )
    .expect("the counted lord bonus should compose");
    let modifier = &parsed.rule["modifiers"][0];
    assert_eq!(modifier["kind"], "modifyPowerToughness");
    assert_eq!(modifier["objects"]["excludeSource"], true);
    assert_eq!(modifier["objects"]["where"]["kind"], "subtypeContains");
    assert_eq!(modifier["power"]["kind"], "countPermanents");
    assert_eq!(
        modifier["power"]["where"]["kind"], "and",
        "artifact and token remain independent criteria leaves"
    );
    assert_eq!(modifier["toughness"], integer(0));
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let scaled = parse_common_static_ability(
        "Creatures you control get -2/+1 for each enchantment you control.",
        "",
    )
    .expect("signed unrelated stat scaling should use the same grammar");
    assert_eq!(scaled.rule["modifiers"][0]["power"]["kind"], "multiply");
    assert_eq!(scaled.rule["modifiers"][0]["power"]["right"], integer(-2));
    assert_eq!(
        scaled.rule["modifiers"][0]["toughness"]["kind"],
        "countPermanents"
    );

    assert!(
        parse_common_static_ability(
            "Creatures you control get +1/+1 for each artifact an opponent controls.",
            "",
        )
        .is_none()
    );
}

#[test]
fn named_token_creation_replacement_composes_retained_and_additional_tokens() {
    let parsed = parse_common_static_ability(
        "If you would create a Food token, instead create a Food token and a Treasure token.",
        "",
    )
    .expect("the named-token supplement replacement should parse");
    let modifier = &parsed.rule["modifiers"][0];
    assert_eq!(modifier["kind"], "supplementNamedTokenCreation");
    assert_eq!(modifier["token"], "Food");
    assert_eq!(modifier["additionalTokens"], json!(["Treasure"]));
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let clue = parse_common_static_ability(
        "If you would create a Clue token, instead create a Clue token and a Treasure token.",
        "",
    )
    .expect("unrelated named tokens should use the same replacement grammar");
    assert_eq!(clue.rule["modifiers"][0]["token"], "Clue");

    assert!(
        parse_common_static_ability(
            "If you would create a Food token, instead create a Clue token and a Treasure token.",
            "",
        )
        .is_none()
    );
}

#[test]
fn global_destruction_binds_only_destroyed_permanents_for_linked_life_gain() {
    let parsed = parse_expansion_triggered(
        "When Test Source enters, destroy all artifacts and enchantments your opponents control. You gain 1 life for each permanent destroyed this way.",
        "Test Source",
    )
    .expect("the named entry trigger should compose linked global destruction");
    assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(parsed.rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(
        parsed.rule["effects"][0]["permanent"]["player"]["kind"],
        "opponentsOf"
    );
    assert_eq!(parsed.rule["effects"][0]["bind"], "destroyedPermanents");
    assert_eq!(parsed.rule["effects"][1]["amount"]["kind"], "countObjects");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let (effects, _) = parse_general_effect_sequence(
        "Destroy all creatures your opponents control. You gain two life for each permanent destroyed this way.",
        "",
    )
    .expect("an unrelated criterion and scaled life gain should use the same sequence");
    assert_eq!(effects[1]["amount"]["kind"], "multiply");
    assert_eq!(effects[1]["amount"]["right"], integer(2));

    assert!(parse_general_effect_sequence(
        "Destroy all creatures your opponents control. You gain 1 life for each permanent exiled this way.",
        "",
    )
    .is_none());
}

#[test]
fn controller_turn_keywords_compose_controlled_state_criteria() {
    let parsed = parse_common_static_ability(
        "During your turn, creatures you control that are equipped have first strike and vigilance.",
        "",
    )
    .expect("controlled equipped creatures should gain turn-bound keywords");
    assert_eq!(parsed.rule["modifiers"].as_array().map(Vec::len), Some(2));
    for modifier in parsed.rule["modifiers"].as_array().unwrap() {
        assert_eq!(modifier["kind"], "grantKeyword");
        assert_eq!(modifier["objects"]["where"]["kind"], "and");
        assert_eq!(modifier["condition"]["kind"], "duringControllerTurn");
    }
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let tapped = parse_common_static_ability(
        "During your turn, artifacts you control that are tapped have hexproof.",
        "",
    )
    .expect("an unrelated state qualifier should use the same controlled grammar");
    assert_eq!(
        tapped.rule["modifiers"][0]["objects"]["where"]["operands"][0]["kind"],
        "isTapped"
    );

    assert!(
        parse_common_static_ability(
            "During your turn, creatures an opponent controls that are equipped have vigilance.",
            "",
        )
        .is_none()
    );
}

#[test]
fn nontoken_source_entry_can_create_nonlegendary_token_copies() {
    let instruction = parse_expansion_instruction(
        "create two tokens that are copies of them, except the tokens aren't legendary.",
        "The Notary Hobbits",
    )
    .expect("the reusable token-copy leaf should parse");
    assert_eq!(instruction.0[0]["kind"], "createTokenCopyOfPermanent");

    let parsed = parse_expansion_triggered(
        "When The Notary Hobbits enter, if they're not a token, create two tokens that are copies of them, except the tokens aren't legendary.",
        "The Notary Hobbits",
    )
    .expect("the nontoken source entry trigger should compose");
    assert_eq!(parsed.rule["event"]["nontoken"], true);
    assert_eq!(parsed.rule["effects"][0]["quantity"]["value"], 2);
    assert_eq!(parsed.rule["effects"][0]["removeLegendary"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn controlled_permanent_lists_can_gain_ward_conditionally() {
    assert!(parse_activation_costs("{1}").is_some());
    assert!(card_qualifier_list_filter("artifacts and creatures", "").is_some());
    let parsed =
        parse_common_static_ability("artifacts and creatures you control have ward {1}.", "")
            .expect("the controlled permanent list should gain ward");
    assert_eq!(parsed.rule["modifiers"][0]["kind"], "grantWard");

    let conditional = parse_common_static_ability(
        "As long as you have an enduring story, artifacts and creatures you control have ward {1}.",
        "",
    )
    .expect("the ward modifier should compose with a condition");
    assert_eq!(conditional.rule["condition"]["kind"], "hasEnduringStory");
    assert_eq!(
        conditional.rule["condition"]["player"]["kind"],
        "controllerOf"
    );
    assert!(crate::engine::rule_is_executable(&conditional.rule));
}

#[test]
fn fetch_land_subtype_lists_strip_indefinite_articles_from_every_alternative() {
    for (text, expected) in [
        (
            "{T}, Pay 1 life, Sacrifice this land: Search your library for an Island or Swamp card, put it onto the battlefield, then shuffle.",
            ["Island", "Swamp"],
        ),
        (
            "{T}, Sacrifice this land: Search your library for a Plains or Island card, put it onto the battlefield, then shuffle.",
            ["Plains", "Island"],
        ),
    ] {
        let rule = parse_special_activated_ability(text)
            .unwrap_or_else(|| panic!("fetch-land subtype alternatives parse: {text}"))
            .rule;
        let operands = rule["effects"][0]["candidates"]["where"]["operands"]
            .as_array()
            .expect("fetch-land search has subtype alternatives");
        assert_eq!(operands.len(), expected.len());
        for (operand, subtype_name) in operands.iter().zip(expected) {
            assert_eq!(operand["kind"], "subtypeContains");
            assert_eq!(operand["value"], subtype_name);
        }
        assert!(crate::engine::rule_is_executable(&rule));
    }
}

#[test]
fn source_with_commas_returns_as_a_different_permanent_type() {
    let parsed = parse_expansion_triggered(
        "When Tom, Bert, and William die, if they were a creature, return them to the battlefield. They're an artifact. (They're no longer a creature.)",
        "Tom, Bert, and William",
    )
    .expect("a named source containing commas parses through the shared death event");

    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(
        parsed.rule["condition"]["decisionId"],
        "triggeringPermanentWasCreature"
    );
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "moveAbilitySourceToBattlefield"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "setPermanentCardTypes");
    assert_eq!(parsed.rule["effects"][1]["cardTypes"], json!(["Artifact"]));
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn draw_then_return_from_hand_uses_composable_zone_and_order_primitives() {
    for (text, draw_count, put_count) in [
        (
            "Draw three cards, then put two cards from your hand on top of your library in any order.",
            3,
            2,
        ),
        (
            "Draw two cards, then put one card from your hand on top of your library in any order.",
            2,
            1,
        ),
    ] {
        let (effects, decisions) = parse_general_effect_instruction(text, "")
            .unwrap_or_else(|| panic!("draw-and-return family parses: {text}"));
        assert!(decisions.is_empty());
        assert_eq!(effects.len(), 4);
        assert_eq!(effects[0]["kind"], "drawCards");
        assert_eq!(effects[0]["count"], integer(draw_count));
        assert_eq!(effects[1]["kind"], "chooseCards");
        assert_eq!(effects[1]["minimum"], integer(put_count));
        assert_eq!(effects[1]["maximum"], integer(put_count));
        assert_eq!(effects[1]["candidates"]["zone"]["kind"], "hand");
        assert_eq!(effects[2]["kind"], "chooseOrder");
        assert_eq!(effects[3]["kind"], "moveCards");
        assert_eq!(effects[3]["to"]["kind"], "library");
        assert_eq!(effects[3]["to"]["position"], "top");
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        })));
    }

    assert!(parse_general_effect_instruction(
            "Draw three cards, then put two cards from your graveyard on top of your library in any order.",
            "",
        )
        .is_none());
}

#[test]
fn ordinal_draw_transform_and_variable_loyalty_sequences_use_shared_grammars() {
    let investigate = parse_expansion_triggered(
            "Whenever Tamiyo attacks, investigate. (Create a Clue token. It's an artifact with \"{2}, Sacrifice this token: Draw a card.\")",
            "Tamiyo, Inquisitive Student",
        )
        .expect("named attack and investigate parse generically");
    assert_eq!(investigate.rule["event"]["kind"], "declaredAttacker");
    assert_eq!(investigate.rule["effects"][0]["kind"], "createTokens");
    assert!(crate::engine::rule_is_executable(&investigate.rule));

    let transform = parse_expansion_triggered(
            "When you draw your third card in a turn, exile Tamiyo, then return her to the battlefield transformed under her owner's control.",
            "Tamiyo, Inquisitive Student",
        )
        .expect("arbitrary ordinal draw and transformed return parse generically");
    assert_eq!(transform.rule["event"]["drawOrdinal"], integer(3));
    assert_eq!(
        transform.rule["effects"][0]["kind"],
        "exileThenReturnTransformed"
    );
    assert!(crate::engine::rule_is_executable(&transform.rule));

    let return_and_mana = parse_simple_activated_ability(
            "−3: Return target instant or sorcery card from your graveyard to your hand. If it's a green card, add one mana of any color.",
        )
        .expect("graveyard return with color-conditioned mana parses");
    assert_eq!(return_and_mana.rule["effects"][0]["kind"], "moveTargetCard");
    assert_eq!(
        return_and_mana.rule["effects"][1]["kind"],
        "conditionalEffect"
    );
    assert!(crate::engine::rule_is_executable(&return_and_mana.rule));

    let draw_emblem_instruction = "Draw cards equal to half the number of cards in your library, rounded up. You get an emblem with \"You have no maximum hand size.\"";
    assert!(
        parse_numeric_expression_text("half the number of cards in your library, rounded up")
            .is_some()
    );
    assert!(
        parse_general_effect_instruction(
            "Draw cards equal to half the number of cards in your library, rounded up.",
            "",
        )
        .is_some()
    );
    assert!(
        parse_general_effect_instruction(
            "You get an emblem with \"You have no maximum hand size.\".",
            "",
        )
        .is_some()
    );
    assert!(parse_general_effect_sequence(draw_emblem_instruction, "").is_some());
    let draw_and_emblem = parse_simple_activated_ability(&format!("−7: {draw_emblem_instruction}"))
        .expect("rounded library count and player emblem parse");
    assert_eq!(draw_and_emblem.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draw_and_emblem.rule["effects"][0]["count"]["round"], "up");
    assert_eq!(draw_and_emblem.rule["effects"][1]["kind"], "createEmblem");
    assert!(crate::engine::rule_is_executable(&draw_and_emblem.rule));
}

#[test]
fn named_tokens_preserve_their_tapped_entry_state_through_trigger_composition() {
    for (text, token_name) in [
        (
            "When this creature enters, create a tapped Treasure token.",
            "Treasure",
        ),
        (
            "When this artifact enters, create a tapped Food token.",
            "Food",
        ),
    ] {
        let parsed = parse_expansion_triggered(text, "Test Permanent")
            .unwrap_or_else(|| panic!("tapped named-token trigger parses: {text}"));
        assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
        assert_eq!(parsed.rule["effects"][0]["kind"], "createTokens");
        assert_eq!(parsed.rule["effects"][0]["token"]["name"], token_name);
        assert_eq!(parsed.rule["effects"][0]["tapped"], true);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(create_token_effect("Create a tapped imaginary maybe token.").is_none());
}

#[test]
fn optional_library_search_can_shuffle_before_putting_the_selection_on_top() {
    for (criteria, expected_filter_kind) in [
        ("a basic land card", "and"),
        ("an artifact card", "cardTypeContains"),
    ] {
        let text = format!(
            "When this creature enters, you gain 2 life. You may search your library for {criteria}, reveal it, then shuffle and put that card on top."
        );
        let parsed = parse_expansion_triggered(&text, "Test Creature")
            .unwrap_or_else(|| panic!("search-to-library-top trigger parses: {text}"));
        let effects = parsed.rule["effects"]
            .as_array()
            .expect("trigger has ordered effects");
        assert_eq!(effects[0]["kind"], "gainLife");
        assert_eq!(effects[1]["kind"], "chooseCards");
        assert_eq!(effects[1]["minimum"], 0);
        assert_eq!(
            effects[1]["candidates"]["where"]["kind"],
            expected_filter_kind
        );
        assert_eq!(effects[2]["kind"], "revealCards");
        assert_eq!(effects[3]["kind"], "shuffleZone");
        assert_eq!(effects[4]["kind"], "moveCards");
        assert_eq!(effects[4]["to"]["position"], "top");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_general_effect_instruction(
            "Search your library for a land card, shuffle and put it on the bottom.",
            "",
        )
        .is_none()
    );
}

#[test]
fn split_library_search_composes_costs_criteria_and_two_destinations() {
    for reference in ["them", "those cards"] {
        let text = format!(
            "{{2}}, {{T}}, Sacrifice this creature: Search your library for up to two basic land cards, reveal {reference}, put one onto the battlefield tapped and the other into your hand, then shuffle."
        );
        let parsed = parse_simple_activated_ability(&text)
            .unwrap_or_else(|| panic!("split library-search activation parses: {text}"));

        assert_eq!(parsed.rule["costs"][0]["kind"], "payMana");
        assert_eq!(parsed.rule["costs"][1]["kind"], "tap");
        assert_eq!(parsed.rule["costs"][2]["kind"], "sacrificePermanent");
        assert_eq!(parsed.rule["effects"][0]["kind"], "chooseCards");
        assert_eq!(parsed.rule["effects"][0]["maximum"], 2);
        assert_eq!(
            parsed.rule["effects"][0]["candidates"]["where"]["kind"],
            "and"
        );
        assert_eq!(parsed.rule["effects"][2]["kind"], "chooseCards");
        assert_eq!(parsed.rule["effects"][3]["to"]["kind"], "battlefield");
        assert_eq!(parsed.rule["effects"][3]["to"]["tapped"], true);
        assert_eq!(parsed.rule["effects"][4]["cards"]["kind"], "setDifference");
        assert_eq!(parsed.rule["effects"][4]["to"]["kind"], "hand");
        assert_eq!(parsed.rule["effects"][5]["kind"], "shuffleZone");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(split_library_search_between_battlefield_and_hand_effects(
        "Search your library for up to three basic land cards, reveal them, put one onto the battlefield tapped and the other into your hand, then shuffle.",
        "",
    )
    .is_none());
}

#[test]
fn ordinal_triggered_branches_reuse_leaf_effect_parsers() {
    let parsed = parse_expansion_triggered(
        "Whenever a token you control enters, you gain 1 life if this is the first time this ability has resolved this turn. If it's the second time, draw a card. If it's the third time, put a +1/+1 counter on each creature you control.",
        "Test Citizen",
    )
    .expect("ordinal token-entry trigger composes from reusable event and effect leaves");

    assert_eq!(parsed.rule["event"]["kind"], "permanentEntered");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "isToken");
    let ordinal = &parsed.rule["effects"][0];
    assert_eq!(ordinal["kind"], "resolveOrdinalTriggeredAbility");
    assert!(
        ordinal["id"]
            .as_str()
            .is_some_and(|id| id.starts_with("ordinalAbility:"))
    );
    assert_eq!(ordinal["branches"][0]["effects"][0]["kind"], "gainLife");
    assert_eq!(ordinal["branches"][1]["effects"][0]["kind"], "drawCards");
    assert_eq!(ordinal["branches"][2]["effects"][0]["kind"], "putCounters");
    assert_eq!(
        ordinal["branches"][2]["effects"][0]["permanent"]["kind"],
        "eachPermanent"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(parse_ordinal_resolution_branches(
        "You gain 1 life if this is the first time this ability has resolved this turn. If it's the second time, choose target creature. If it's the third time, draw a card.",
        "",
    )
    .is_none());
}

#[test]
fn gift_keyword_binds_generic_effects_and_followup_conditions() {
    let treasure = parse_remaining_kellan_ability(
        "Gift a Treasure (You may promise an opponent a gift as you cast this spell. If you do, they create a Treasure token before its other effects.)",
        "spellAbility",
    )
    .expect("a named-token gift parses through token creation");
    assert_eq!(treasure.rule["ability"]["kind"], "gift");
    assert_eq!(
        treasure.rule["ability"]["effects"][0]["kind"],
        "createTokens"
    );
    assert_eq!(
        treasure.rule["ability"]["effects"][0]["controller"]["kind"],
        "boundValue"
    );
    assert_eq!(
        treasure.rule["ability"]["effects"][0]["token"]["name"],
        "Treasure"
    );
    assert!(crate::engine::rule_is_executable(&treasure.rule));

    let card = parse_remaining_kellan_ability("Gift a card", "spellAbility")
        .expect("a card gift uses the same generic gift container");
    assert_eq!(card.rule["ability"]["kind"], "gift");
    assert_eq!(card.rule["ability"]["effects"][0]["kind"], "drawCards");
    assert!(crate::engine::rule_is_executable(&card.rule));

    let (effects, decisions) = parse_general_effect_sequence(
        "Return target spell to its owner's hand. If the gift was promised, players can't cast spells this turn.",
        "Test Gambit",
    )
    .expect("spell return and promised-gift condition compose");
    assert_eq!(decisions.len(), 1);
    assert_eq!(decisions[0]["candidates"]["where"]["kind"], "isSpell");
    assert_eq!(effects[0]["kind"], "returnToOwnersHand");
    assert_eq!(effects[1]["kind"], "conditionalEffect");
    assert_eq!(effects[1]["condition"]["kind"], "selectionNotEmpty");
    assert_eq!(effects[1]["then"][0]["kind"], "restrictPlayerActions");
    assert_eq!(effects[1]["then"][0]["player"]["kind"], "eachPlayer");
    let rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "declaration": { "kind": "castingDeclaration", "decisions": decisions },
        "effects": effects,
    });
    assert!(crate::engine::rule_is_executable(&rule));
}

#[test]
fn zero_loyalty_ninjutsu_and_conditional_planeswalker_rules_are_generic() {
    let ninjutsu = parse_keyword_ability(
            "Ninjutsu {1}{U}{B} ({1}{U}{B}, Return an unblocked attacker you control to hand: Put this card onto the battlefield from your hand tapped and attacking.)",
            "",
        )
        .expect("ninjutsu parses as a hand activation");
    assert_eq!(ninjutsu.rule["activationZone"], "hand");
    assert_eq!(
        ninjutsu.rule["activationCondition"]["kind"],
        "ninjutsuTiming"
    );
    assert!(crate::engine::rule_is_executable(&ninjutsu.rule));

    let animated = parse_common_static_ability(
            "During your turn, as long as Kaito has one or more loyalty counters on him, he's a 3/4 Ninja creature and has hexproof.",
            "Kaito, Bane of Nightmares",
        )
        .expect("conditional planeswalker animation parses");
    assert!(
        animated.rule["modifiers"]
            .as_array()
            .is_some_and(|modifiers| {
                modifiers
                    .iter()
                    .any(|modifier| modifier["kind"] == "addCardType")
                    && modifiers
                        .iter()
                        .any(|modifier| modifier["kind"] == "setBasePowerToughness")
            })
    );
    assert!(crate::engine::rule_is_executable(&animated.rule));

    for text in [
        "0: Surveil 2. Then draw a card for each opponent who lost life this turn.",
        "0: Create a 2/1 white Cat Warrior creature token. When you do, if you control a red permanent other than Ajani, he deals damage equal to the number of creatures you control to any target.",
        "−4: Each opponent chooses an artifact, a creature, an enchantment, and a planeswalker from among the nonland permanents they control, then sacrifices the rest.",
    ] {
        let parsed = parse_simple_activated_ability(text)
            .unwrap_or_else(|| panic!("generic loyalty sequence parses: {text}"));
        assert_eq!(parsed.rule["kind"], "activatedAbility");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn legacy_rule_families_parse_without_card_specific_branches() {
    let attack = parse_expansion_triggered(
            "Whenever Raph & Mikey attack, reveal cards from the top of your library until you reveal a creature card. Put that card onto the battlefield tapped and attacking and the rest on the bottom of your library in a random order.",
            "Raph & Mikey, Troublemakers",
        )
        .expect("named attack reveal-until sequence parses");
    assert_eq!(
        attack.rule["effects"][0]["kind"],
        "revealUntilAndPutOntoBattlefield"
    );
    assert!(crate::engine::rule_is_executable(&attack.rule));

    let alternative = parse_alternative_cost_ability(
            "You may pay 1 life and exile a blue card from your hand rather than pay this spell's mana cost.",
        )
        .expect("composite alternative cost parses");
    assert_eq!(alternative.rule["ability"]["kind"], "alternativeCost");
    assert!(crate::engine::rule_is_executable(&alternative.rule));

    for text in [
        "Creature cards in graveyards and libraries can't enter the battlefield.",
        "Players can't cast spells from graveyards or libraries.",
    ] {
        let cage = parse_common_static_ability(text, "").expect("zone prohibition parses");
        assert!(crate::engine::rule_is_executable(&cage.rule));
    }

    for text in [
        "Choose one —\n• Counter target spell if it's blue.\n• Destroy target permanent if it's blue.",
        "Choose one —\n• Counter target blue spell.\n• Destroy target blue permanent.",
    ] {
        let modal = parse_general_modal_spell(text)
            .unwrap_or_else(|| panic!("generic two-mode spell parses: {text}"));
        assert!(crate::engine::rule_is_executable(&modal.rule));
    }

    let token_exception = parse_common_static_ability(
        "Gelatinous Hero isn't legendary if it's a token.",
        "Gelatinous Hero",
    )
    .expect("a source-relative token legendary exception parses");
    assert_eq!(
        token_exception.rule["modifiers"][0]["kind"],
        "removeLegendaryFromTokenCopy"
    );
    assert!(crate::engine::rule_is_executable(&token_exception.rule));

    let entering_counters = parse_common_static_ability(
        "Gelatinous Hero enters with a +1/+1 counter on it for each other Slime you control.",
        "Gelatinous Hero",
    )
    .expect("source-relative entering counters use generic subtype criteria");
    assert_eq!(
        entering_counters.rule["replacement"][0]["count"]["where"],
        subtype("Slime")
    );
    assert!(crate::engine::rule_is_executable(&entering_counters.rule));
}

#[test]
fn topiary_lecturer_mana_scales_with_its_power() {
    let parsed = parse_mana_ability("{T}: Add an amount of {G} equal to this creature's power.")
        .map(promote_activated_mana_ability)
        .expect("Topiary Lecturer's mana ability parses");

    assert_eq!(parsed.rule["kind"], "manaAbility");
    assert_eq!(
        parsed.rule["effects"][0]["mana"],
        json!({
            "kind": "fixedMana",
            "symbol": "G",
            "amount": {
                "kind": "powerOf",
                "object": { "kind": "self" },
            },
        })
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn opus_mill_trigger_uses_actual_mana_spent_threshold() {
    let parsed = parse_common_triggered_ability(
            "Opus — Whenever you cast an instant or sorcery spell, target player mills three cards. If five or more mana was spent to cast that spell, that player mills ten cards instead.",
        )
        .expect("Exhibition Tidecaller's Opus ability parses");

    assert_eq!(parsed.rule["event"]["kind"], "spellCast");
    assert_eq!(
        parsed.rule["effects"][0]["condition"]["left"],
        decision_result("triggeringSpellManaSpent")
    );
    assert_eq!(parsed.rule["effects"][0]["then"][0]["count"], integer(10));
    assert_eq!(parsed.rule["effects"][0]["else"][0]["count"], integer(3));
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn warp_parser_reuses_the_generic_cost_parser_without_reminder_text_dependency() {
    for oracle_text in [
        "Warp {X}{G}",
        "Warp {X}{G} (You may cast this card from your hand for its warp cost. Exile this permanent at the beginning of the next end step, then you may cast it from exile on a later turn.)",
    ] {
        let parsed = parse_keyword_ability(oracle_text, "Broodguard Elite")
            .expect("warp keyword parses independently of its reminder text");

        assert_eq!(parsed.rule["kind"], "keywordAbility");
        assert_eq!(parsed.rule["ability"]["kind"], "warp");
        assert_eq!(
            parsed.rule["ability"]["cost"],
            json!({ "kind": "payMana", "manaCost": "{X}{G}" })
        );
    }
}

#[test]
fn dredge_parser_cross_checks_the_keyword_and_reminder_quantities() {
    let parsed = parse_keyword_ability(
            "Dredge 3 (If you would draw a card, you may mill three cards instead. If you do, return this card from your graveyard to your hand.)",
            "Dredge Test",
        )
        .expect("dredge parses through the generic keyword grammar");

    assert_eq!(parsed.rule["kind"], "keywordAbility");
    assert_eq!(parsed.rule["ability"]["kind"], "dredge");
    assert_eq!(parsed.rule["ability"]["count"], integer(3));
    assert!(crate::engine::rule_is_executable(&parsed.rule));
    assert!(parse_keyword_ability(
            "Dredge 3 (If you would draw a card, you may mill two cards instead. If you do, return this card from your graveyard to your hand.)",
            "Dredge Test",
        )
        .is_none());
}

#[test]
fn self_death_face_down_pile_uses_the_generic_zone_sequence() {
    let parsed = parse_expansion_triggered(
            "Memorial Protocol â€” When this creature is put into your graveyard from the battlefield, exile it and the top six cards of your library in a face-down pile. If you do, shuffle that pile and put it back on top of your library.",
            "Test Creature",
        )
        .expect("the labeled self-death pile sequence parses");

    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "exileDiedSourceAndTopCardsAsShuffledLibraryPile"
    );
    assert_eq!(parsed.rule["effects"][0]["count"], integer(6));
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn saga_chapters_grant_parsed_abilities_and_search_by_exact_mana_cost() {
    let mana = parse_generalized_zone_and_combat_ability(
        "I â€” This Saga gains \"{T}: Add {C}.\"",
        "staticAbility",
        "Test Saga",
    )
    .expect("a Saga chapter grants a parsed mana ability");
    assert_eq!(mana.rule["event"]["chapters"][0], integer(1));
    assert_eq!(mana.rule["effects"][0]["ability"]["kind"], "manaAbility");
    assert!(crate::engine::rule_is_executable(&mana.rule));

    let activated = parse_generalized_zone_and_combat_ability(
            "II â€” This Saga gains \"{2}, {T}: Create a 0/0 colorless Construct artifact creature token with 'This token gets +1/+1 for each artifact you control.'\"",
            "staticAbility",
            "Test Saga",
        )
        .expect("a Saga chapter grants a parsed activated ability");
    assert_eq!(
        activated.rule["effects"][0]["ability"]["kind"],
        "activatedAbility"
    );
    assert!(crate::engine::rule_is_executable(&activated.rule));

    let search = parse_generalized_zone_and_combat_ability(
            "III â€” Search your library for an artifact card with mana cost {0} or {1}, put it onto the battlefield, then shuffle.",
            "staticAbility",
            "Test Saga",
        )
        .expect("a Saga chapter searches by exact printed mana cost");
    assert_eq!(search.rule["event"]["chapters"][0], integer(3));
    assert_eq!(
        search.rule["effects"][0]["where"]["operands"][1]["kind"],
        "or"
    );
    assert!(crate::engine::rule_is_executable(&search.rule));
}

#[test]
fn saga_chapters_reuse_the_fixed_mana_effect_leaf() {
    let red = parse_generalized_zone_and_combat_ability(
        "III, IV — Add {R}.",
        "staticAbility",
        "Test Saga",
    )
    .expect("a multi-chapter Saga adds fixed mana through the shared effect grammar");
    assert_eq!(
        red.rule["event"]["chapters"],
        json!([integer(3), integer(4)])
    );
    assert_eq!(red.rule["effects"][0]["kind"], "addMana");
    assert_eq!(red.rule["effects"][0]["mana"], "{R}");
    assert!(crate::engine::rule_is_executable(&red.rule));

    let azorius = parse_general_effect_instruction("Add {W}{U}.", "")
        .expect("multiple fixed symbols use the same effect leaf");
    assert_eq!(azorius.0[0]["mana"], "{W}{U}");

    assert!(parse_general_effect_instruction("Add {1}.", "").is_none());
}

#[test]
fn direct_mana_effects_distinguish_one_color_from_color_combinations() {
    let combination = parse_general_effect_instruction(
        "Add four mana in any combination of colors. Spend this mana only to cast Dragon spells.",
        "",
    )
    .expect("a spell can add independently chosen colors with a cast restriction");
    assert_eq!(combination.0[0]["mana"]["kind"], "chooseColors");
    assert_eq!(combination.0[0]["mana"]["amount"], integer(4));
    assert_eq!(combination.0[0]["spendRestriction"]["kind"], "castSpell");
    assert_eq!(
        combination.0[0]["spendRestriction"]["where"]["kind"],
        "subtypeContains"
    );
    assert!(crate::engine::effect_supported(&combination.0[0]));

    let one_color = parse_general_effect_instruction(
        "Add two mana of any one color. Spend this mana only to cast artifact spells.",
        "",
    )
    .expect("a single chosen color remains distinct");
    assert_eq!(one_color.0[0]["mana"]["kind"], "chooseColor");

    assert!(
        parse_general_effect_instruction("Add four mana of whatever colors you want.", "")
            .is_none()
    );
}

#[test]
fn equipment_targeting_reductions_parse_amount_subject_and_attached_object() {
    let equip = parse_avatar_deck_static(
        "Equip abilities you activate that target this creature cost {2} less to activate.",
    )
    .expect("Equip-only reductions parse generically");
    let modifier = &equip.rule["modifiers"][0];
    assert_eq!(modifier["amount"], integer(2));
    assert_eq!(modifier["abilityKind"], "equip");
    assert_eq!(modifier["object"]["kind"], "self");
    assert!(crate::engine::rule_is_executable(&equip.rule));

    let attached = parse_avatar_deck_static(
        "Activated abilities of Equipment you control that target enchanted creature cost {3} less to activate.",
    )
    .expect("all Equipment activation reductions can select an attached permanent");
    assert!(attached.rule["modifiers"][0].get("abilityKind").is_none());
    assert_eq!(
        attached.rule["modifiers"][0]["object"]["kind"],
        "attachedPermanent"
    );

    assert!(
        parse_avatar_deck_static(
            "Equip abilities you activate that target this creature cost some mana less to activate.",
        )
        .is_none()
    );
}

#[test]
fn multi_kicker_uses_independent_generic_costs_and_cost_specific_triggers() {
    let kicker = parse_keyword_ability("Kicker {G} and/or {1}{U}", "Test Battlemage")
        .expect("independent kicker costs parse");
    assert_eq!(kicker.rule["ability"]["costs"].as_array().unwrap().len(), 2);
    assert_eq!(
        kicker.rule["ability"]["costs"][0],
        json!({ "kind": "payMana", "manaCost": "{G}" })
    );
    assert!(crate::engine::rule_is_executable(&kicker.rule));

    for (text, expected_cost) in [
        (
            "When you cast this spell, if it was kicked with its {G} kicker, exile target artifact or enchantment an opponent controls.",
            "{G}",
        ),
        (
            "When you cast this spell, if it was kicked with its {1}{U} kicker, return target creature an opponent controls to its owner's hand.",
            "{1}{U}",
        ),
    ] {
        let trigger = parse_expansion_triggered(text, "Test Battlemage")
            .expect("a cost-specific kicker trigger parses");
        assert_eq!(trigger.rule["condition"]["kind"], "kickerCostWasPaid");
        assert_eq!(trigger.rule["condition"]["cost"], expected_cost);
        assert!(crate::engine::rule_is_executable(&trigger.rule));
    }
}

#[test]
fn ward_parser_reuses_the_generic_cost_parser_including_keyword_lists() {
    let cases = [
        ("Ward {2}", json!({ "kind": "payMana", "manaCost": "{2}" })),
        (
            "Ward—Pay 3 life.",
            json!({
                "kind": "payLife",
                "player": controller(),
                "amount": integer(3),
            }),
        ),
        (
            "Ward—Waterbend {2}. (You may pay this cost by tapping untapped artifacts and creatures you control, with each one paying for {1}.)",
            json!({ "kind": "payWaterbend", "amount": integer(2) }),
        ),
    ];
    for (oracle_text, expected_cost) in cases {
        let parsed = parse_keyword_ability(oracle_text, "Ward Test")
            .expect("ward with a canonical cost parses");
        assert_eq!(parsed.rule["kind"], "keywordAbility");
        assert_eq!(parsed.rule["ability"]["kind"], "ward");
        assert_eq!(parsed.rule["ability"]["cost"], expected_cost);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let grouped = parse_keyword_ability("Flying, ward {2}", "Ward Test")
        .expect("ward participates in the generic keyword-list parser");
    assert_eq!(grouped.rule["kind"], "keywordAbilityGroup");
    assert_eq!(grouped.rule["abilities"][0]["kind"], "flying");
    assert_eq!(grouped.rule["abilities"][1]["kind"], "ward");
    assert_eq!(
        grouped.rule["abilities"][1]["cost"],
        json!({ "kind": "payMana", "manaCost": "{2}" })
    );
    assert!(crate::engine::rule_is_executable(&grouped.rule));

    for (oracle_text, expected_kind) in [
        ("Ward—Sacrifice a creature.", "sacrificePermanent"),
        ("Ward—Discard a card.", "discardCard"),
    ] {
        let parsed = parse_keyword_ability(oracle_text, "Ward Test")
            .expect("ward accepts a nonmana cost from the generic cost parser");
        assert_eq!(parsed.rule["ability"]["cost"]["kind"], expected_kind);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let qualified_discard = parse_keyword_ability(
        "Ward\u{2014}Discard an enchantment, instant, or sorcery card.",
        "Ward Test",
    )
    .expect("ward delegates a qualified discard list to the shared criteria parser");
    let cost = &qualified_discard.rule["ability"]["cost"];
    assert_eq!(cost["kind"], "discardCard");
    let where_filter = &cost["where"];
    assert_eq!(where_filter["kind"], "or");
    assert_eq!(
        where_filter["operands"]
            .as_array()
            .expect("the discard criteria is an OR list")
            .len(),
        3
    );
    assert!(crate::engine::rule_is_executable(&qualified_discard.rule));

    let (costs, decisions) =
        parse_activation_costs("{1}, Discard an enchantment, instant, or sorcery card")
            .expect("cost delimiters remain distinct from commas inside a criteria list");
    assert_eq!(costs.len(), 2);
    assert_eq!(decisions.len(), 1);
    assert_eq!(costs[0]["kind"], "payMana");
    assert_eq!(costs[1]["kind"], "discardCard");

    assert_eq!(
        split_activation_cost_atoms("Sacrifice an artifact, creature, or land, Pay 2 life"),
        vec!["Sacrifice an artifact, creature, or land", " Pay 2 life"]
    );
}

#[test]
fn blight_reuses_the_root_cost_atom_across_activated_keyword_and_casting_costs() {
    let activated = parse_simple_activated_ability(
        "{T}, Blight 2: Draw a card. (To blight 2, put two -1/-1 counters on a creature you control.)",
    )
    .expect("blight composes with tap in an activated cost list");
    assert_eq!(activated.rule["costs"][0]["kind"], "tap");
    assert_eq!(activated.rule["costs"][1]["kind"], "putCounters");
    assert_eq!(activated.rule["costs"][1]["counter"], "-1/-1");
    assert_eq!(activated.rule["costs"][1]["count"], integer(2));
    assert_eq!(
        activated.rule["declaration"]["decisions"][0]["candidates"]["where"],
        card_type("Creature")
    );
    assert_eq!(
        activated.rule["declaration"]["decisions"][0]["candidates"]["ignoreTargetingRestrictions"],
        true
    );
    assert!(crate::engine::rule_is_executable(&activated.rule));

    let discard_then_draw = parse_simple_activated_ability(
        "{T}, Blight 1: Discard a card. If you do, draw a card. (To blight 1, put a -1/-1 counter on a creature you control.)",
    )
    .expect("blight composes with an independent discard-then-draw effect");
    assert_eq!(discard_then_draw.rule["costs"][1]["kind"], "putCounters");
    assert_eq!(
        discard_then_draw.rule["effects"][0]["kind"],
        "discardThenDraw"
    );
    assert!(crate::engine::rule_is_executable(&discard_then_draw.rule));
    let (scaled_discard_draw, decisions) =
        parse_general_effect_sequence("Discard two cards. If you do, draw three cards.", "")
            .expect("the reusable effect leaf accepts independent positive quantities");
    assert!(decisions.is_empty());
    assert_eq!(scaled_discard_draw[0]["discardCount"], integer(2));
    assert_eq!(scaled_discard_draw[0]["drawCount"], integer(3));
    assert!(
        parse_general_effect_sequence("Discard zero cards. If you do, draw a card.", "").is_none()
    );

    let ward = parse_keyword_ability(
        "Ward\u{2014}Blight 1. (To blight 1, put a -1/-1 counter on a creature you control.)",
        "Ward Test",
    )
    .expect("ward delegates blight to the shared cost grammar");
    let ward_cost = &ward.rule["ability"]["cost"];
    assert_eq!(ward_cost["kind"], "putCounters");
    assert_eq!(ward_cost["where"], card_type("Creature"));
    assert!(ward_cost.get("permanent").is_none());
    assert!(crate::engine::rule_is_executable(&ward.rule));

    let additional =
        parse_simple_spell_ability("As an additional cost to cast this spell, blight one.")
            .expect("additional blight delegates to the shared cost grammar");
    assert_eq!(
        additional.rule["declaration"]["additionalCosts"][0]["kind"],
        "putCounters"
    );
    assert_eq!(
        additional.rule["declaration"]["decisions"][0]["id"],
        "blightCost1"
    );
    assert!(crate::engine::rule_is_executable(&additional.rule));

    let optional_additional = parse_simple_spell_ability(
        "As an additional cost to cast this spell, you may blight 1. (You may put a -1/-1 counter on a creature you control.)",
    )
    .expect("the optional additional-cost wrapper reuses the same cost atom");
    let optional_declaration = &optional_additional.rule["declaration"];
    assert_eq!(
        optional_declaration["decisions"][0]["options"],
        json!(["decline", "pay"])
    );
    assert_eq!(
        optional_declaration["decisions"][1]["condition"],
        selection("additionalCostMode", "pay")
    );
    assert_eq!(
        optional_declaration["additionalCosts"][0]["then"][0]["kind"],
        "putCounters"
    );
    assert!(crate::engine::rule_is_executable(&optional_additional.rule));

    for invalid in ["Blight 0", "Blight X", "Blight a creature"] {
        assert!(
            parse_activation_costs(invalid).is_none(),
            "unsupported or malformed cost must not be accepted: {invalid}"
        );
    }
}

#[test]
fn unspecified_source_counter_cost_defers_the_counter_kind_to_activation() {
    let activated =
        parse_simple_activated_ability("{1}{U}, Remove a counter from this creature: Draw a card.")
            .expect("an unnamed source counter composes with mana and a generic draw effect");
    let remove_cost = &activated.rule["costs"][1];
    assert_eq!(remove_cost["kind"], "removeCounters");
    assert_eq!(remove_cost["counter"]["kind"], "decisionResult");
    assert_eq!(remove_cost["counter"]["decisionId"], "removeCounterCost2");
    assert_eq!(remove_cost["count"], integer(1));
    assert!(crate::engine::rule_is_executable(&activated.rule));

    let (costs, decisions) = parse_activation_costs("Remove two counters from Test Permanent")
        .expect("a named source accepts a positive plural quantity");
    assert!(decisions.is_empty());
    assert_eq!(costs[0]["count"], integer(2));
    assert_eq!(costs[0]["counter"], decision_result("removeCounterCost1"));

    assert!(parse_activation_costs("Remove a counter from target creature").is_none());
    assert!(parse_activation_costs("Remove zero counters from this creature").is_none());
}

#[test]
fn granted_ward_uses_the_same_cost_parser() {
    let parsed =
        parse_special_static_ability("Other creatures you control have \"Ward—Pay 2 life.\"")
            .expect("granted ward parses through the generic ward cost path");

    assert_eq!(parsed.rule["modifiers"][0]["kind"], "grantWard");
    assert_eq!(
        parsed.rule["modifiers"][0]["cost"],
        json!({
            "kind": "payLife",
            "player": controller(),
            "amount": integer(2),
        })
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn flashback_reuses_the_generic_cost_parser() {
    let cases = [
        (
            "Flashback {2}{U} (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
            "payMana",
        ),
        (
            "Flashbackâ€”Sacrifice a creature. (You may cast this card from your graveyard for its flashback cost. Then exile it.)",
            "sacrificePermanent",
        ),
        (
            "Flashbackâ€”Discard a card. (You may cast this card from your graveyard for its flashback cost and any additional costs. Then exile it.)",
            "discardCard",
        ),
    ];
    for (oracle_text, expected_cost_kind) in cases {
        let parsed = parse_keyword_ability(oracle_text, "Flashback Test")
            .expect("flashback accepts a cost from the generic cost grammar");
        assert_eq!(parsed.rule["ability"]["kind"], "flashback");
        assert_eq!(parsed.rule["ability"]["cost"]["kind"], expected_cost_kind);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_keyword_ability(
            "Flashbackâ€”Exchange control of two permanents.",
            "Flashback Test",
        )
        .is_none()
    );
}

#[test]
fn targeted_zone_change_uses_criteria_and_resolution_conditions() {
    let conditional = parse_common_zone_and_value_spell(
        "Destroy target creature if it has mana value 2 or less.",
        "",
    )
    .expect("conditional destroy parses");
    assert_eq!(conditional.rule["effects"][0]["kind"], "conditionalEffect");
    assert_eq!(
        conditional.rule["effects"][0]["condition"]["left"]["kind"],
        "manaValueOf"
    );
    assert!(crate::engine::rule_is_executable(&conditional.rule));

    let no_regeneration = parse_common_zone_and_value_spell(
        "Destroy target nonblack creature. It can't be regenerated.",
        "",
    )
    .expect("destroy with regeneration prohibition parses");
    assert_eq!(no_regeneration.rule["effects"][0]["cannotRegenerate"], true);
    assert!(crate::engine::rule_is_executable(&no_regeneration.rule));

    let exile = parse_common_zone_and_value_spell("Exile target black or red permanent.", "")
        .expect("colored permanent criteria parse for exile");
    assert_eq!(
        exile.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "or"
    );
    assert!(crate::engine::rule_is_executable(&exile.rule));

    assert!(parse_common_zone_and_value_spell("Exile target vaguely useful thing.", "").is_none());

    for oracle_text in [
        "Destroy each artifact with mana value X or less.",
        "Destroy all nonland permanents with mana value 1 or less.",
    ] {
        let global =
            parse_global_destruction(oracle_text).expect("global mana-value destruction parses");
        assert_eq!(
            global.rule["effects"][0]["kind"],
            "destroyPermanentsByManaValue"
        );
        assert!(crate::engine::rule_is_executable(&global.rule));
    }

    for oracle_text in [
        "Revolt â€” Destroy that creature if it has mana value 4 or less instead if a permanent left the battlefield under your control this turn.",
        "Renewal — Destroy that permanent if it has mana value 7 or less instead if a permanent left the battlefield under your control this turn.",
    ] {
        let alternate = parse_common_zone_and_value_spell(oracle_text, "")
            .expect("ability-word alternate threshold parses");
        assert_eq!(alternate.rule["effects"][0]["kind"], "conditionalEffect");
        assert!(crate::engine::rule_is_executable(&alternate.rule));
    }
    assert!(parse_common_zone_and_value_spell(
            "Revolt — Destroy that creature if it has mana value 4 or less instead if a permanent left the battlefield under an opponent's control this turn.",
            "",
        )
        .is_none());
    let inherited_damage = parse_common_zone_and_value_spell(
            "Delirium — Test Flame deals 6 damage instead if there are four or more card types among cards in your graveyard.",
            "Test Flame",
        )
        .expect("a conditional replacement amount inherits the prior effect recipient");
    assert_eq!(
        inherited_damage.rule["effects"][0]["then"][0]["recipient"]["id"],
        "targetPermanent"
    );
    assert!(crate::engine::rule_is_executable(&inherited_damage.rule));
}

#[test]
fn reusable_criteria_stats_and_shared_subject_grammars_cover_legacy_forms() {
    let reduction = parse_common_static_ability(
        "Colorless Eldrazi spells you cast cost {2} less to cast.",
        "",
    )
    .expect("colorless plus subtype criteria compose in a casting reduction");
    assert_eq!(reduction.rule["modifiers"][0]["kind"], "reduceCastingCost");
    assert_eq!(reduction.rule["modifiers"][0]["where"]["kind"], "and");
    assert!(crate::engine::rule_is_executable(&reduction.rule));

    let mana = parse_mana_ability(
            "{T}: Add {C}{C}. Spend this mana only to cast colorless Eldrazi spells or activate abilities of colorless Eldrazi.",
        )
        .expect("mana restriction delegates colorless subtype criteria");
    assert_eq!(
        mana.rule["effects"][0]["spendRestriction"]["where"]["kind"],
        "and"
    );
    assert!(crate::engine::rule_is_executable(&mana.rule));

    let aura = parse_common_static_ability("Enchanted creature gets -1/-0.", "")
        .expect("signed enchanted power and toughness parse generically");
    assert_eq!(aura.rule["modifiers"][0]["power"], integer(-1));
    assert_eq!(aura.rule["modifiers"][0]["toughness"], integer(0));
    assert!(crate::engine::rule_is_executable(&aura.rule));

    let trigger = parse_expansion_triggered(
            "Whenever another creature you control enters, you gain 1 life and get {E} (an energy counter).",
            "Test Creature",
        )
        .expect("a shared controller subject composes life and energy effects");
    assert_eq!(trigger.rule["effects"][0]["kind"], "gainLife");
    assert_eq!(trigger.rule["effects"][1]["kind"], "addPlayerCounters");
    assert!(crate::engine::rule_is_executable(&trigger.rule));

    let colorless_entry = parse_expansion_triggered(
            "Whenever another colorless creature you control enters, this creature deals 1 damage to each opponent.",
            "Test Creature",
        )
        .expect("another-entry events delegate colorless creature criteria");
    assert_eq!(colorless_entry.rule["event"]["where"]["kind"], "and");
    assert!(crate::engine::rule_is_executable(&colorless_entry.rule));

    for text in [
        "Whenever a player casts a spell with mana value equal to the number of charge counters on this artifact, counter that spell.",
        "Whenever a player casts a spell with mana value equal to the number of fate counters on Test Artifact, counter it.",
    ] {
        let counter = parse_expansion_triggered(text, "Test Artifact")
            .expect("spell event, counter-count condition, and linked effect compose");
        assert_eq!(counter.rule["event"]["kind"], "spellCast");
        assert_eq!(counter.rule["condition"]["right"]["kind"], "countCounters");
        assert_eq!(counter.rule["effects"][0]["kind"], "counterStackObject");
        assert!(crate::engine::rule_is_executable(&counter.rule));
    }
    assert!(parse_expansion_triggered(
            "Whenever a player casts a spell with mana value equal to the number of charge counters on Another Artifact, counter that spell.",
            "Test Artifact",
        )
        .is_none());

    let delayed = parse_simple_activated_ability(
            "{T}, Sacrifice this artifact: Look at the top card of target player's library. Draw a card at the beginning of the next turn's upkeep.",
        )
        .expect("targeted library inspection composes with a delayed step trigger");
    assert_eq!(delayed.rule["effects"][0]["kind"], "lookAtTopCards");
    assert_eq!(
        delayed.rule["effects"][1]["kind"],
        "installDelayedStepTrigger"
    );
    assert_eq!(delayed.rule["effects"][1]["step"], "upkeep");
    assert!(crate::engine::rule_is_executable(&delayed.rule));

    let leaves = parse_expansion_triggered(
        "When this creature leaves the battlefield, target opponent draws a card.",
        "Test Creature",
    )
    .expect("source-leaves event composes with opponent target criteria");
    assert_eq!(leaves.rule["event"]["kind"], "permanentLeftBattlefield");
    assert_eq!(
        leaves.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "isOpponentOf"
    );
    assert!(crate::engine::rule_is_executable(&leaves.rule));

    let enters_or_dies = parse_expansion_triggered(
        "When this creature enters or dies, mill three cards.",
        "Test Creature",
    )
    .expect("source event alternatives compose with a general mill effect");
    assert_eq!(enters_or_dies.rule["event"]["kind"], "oneOf");
    assert_eq!(
        enters_or_dies.rule["event"]["events"][1]["kind"],
        "permanentDied"
    );
    assert!(crate::engine::rule_is_executable(&enters_or_dies.rule));

    assert!(parse_permanent_criteria("colorlessly Eldrazi", "").is_none());
}

#[test]
fn create_creature_token_parser_uses_general_quantity_and_color_helpers() {
    let blood =
        create_token_effect("Create a Blood token.").expect("catalog-backed Blood token parses");
    assert_eq!(blood["quantity"], integer(1));
    assert_eq!(blood["token"]["kind"], "namedToken");
    assert_eq!(blood["token"]["name"], "Blood");

    let clues = create_token_effect("Create three Clue tokens.")
        .expect("plural catalog-backed Clue tokens parse");
    assert_eq!(clues["quantity"], integer(3));
    assert_eq!(clues["token"]["name"], "Clue");

    let two_color = create_token_effect(
        "Create twenty-one 2/2 blue and black Zombie Rogue creature tokens with flying.",
    )
    .expect("two-color token parses");
    assert_eq!(two_color["quantity"], integer(21));
    assert_eq!(two_color["token"]["colors"], json!(["blue", "black"]));
    assert_eq!(two_color["token"]["subtypes"], json!(["Zombie", "Rogue"]));

    let variable = create_token_effect(
            "Create X 1/1 white Human Soldier creature tokens, where X is the number of creatures you control.",
        )
        .expect("variable token parses");
    assert_eq!(
        variable["quantity"],
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": card_type("Creature"),
        })
    );

    let artifact =
        create_token_effect("Create twelve 2/2 colorless Robot artifact creature tokens.")
            .expect("colorless artifact creature token parses");
    assert_eq!(artifact["quantity"], integer(12));
    assert_eq!(artifact["token"]["colors"], json!([]));
    assert_eq!(artifact["token"]["types"], json!(["Artifact", "Creature"]));
    assert_eq!(artifact["token"]["subtypes"], json!(["Robot"]));

    let named = create_token_effect(
            "Create a 1/1 green Minion creature token named Moloid with \"Whenever this token attacks, you may mill a card.\"",
        )
        .expect("named token with embedded trigger parses");
    assert_eq!(named["token"]["name"], "Moloid");
    assert_eq!(
        named["token"]["abilities"][0]["effects"][0]["kind"],
        "optionalAction"
    );
}

#[test]
fn expansion_triggers_compose_events_and_effects_without_string_fallbacks() {
    let support = parse_expansion_triggered(
            "When this Aura enters, support 2. (Put a +1/+1 counter on each of up to two target creatures.)",
            "Together Forever",
        )
        .expect("support trigger parses");
    assert_eq!(support.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(
        support.rule["effects"][0]["permanent"],
        json!({ "kind": "chosenTargets", "id": "supportedCreatures" })
    );
    assert!(support.rule["effects"][0]["instruction"].is_null());

    let sazh = parse_expansion_triggered(
            "Whenever Sazh Katzroy attacks, put a +1/+1 counter on target creature, then double the number of +1/+1 counters on that creature.",
            "Sazh Katzroy",
        )
        .expect("named self attack trigger parses");
    assert_eq!(sazh.rule["event"]["kind"], "declaredAttacker");
    assert_eq!(sazh.rule["effects"][1]["kind"], "doubleCounters");
    assert!(sazh.rule["effects"][0]["instruction"].is_null());

    let generic_counter = parse_expansion_triggered(
            "Whenever Counter Tester attacks, put a charge counter on target creature, then double the number of charge counters on that creature.",
            "Counter Tester",
        )
        .expect("generic put-then-double trigger parses");
    assert_eq!(generic_counter.rule["effects"][0]["counter"], "charge");
    assert_eq!(generic_counter.rule["effects"][1]["kind"], "doubleCounters");
    assert_eq!(generic_counter.rule["effects"][1]["counter"], "charge");

    let search = parse_expansion_triggered(
            "When Search Tester enters, you may search your library for a Bird or basic land card, reveal it, put it into your hand, then shuffle.",
            "Search Tester",
        )
        .expect("generic subtype-or-basic-land search parses");
    assert_eq!(search.rule["effects"][0]["kind"], "chooseCards");
    assert_eq!(
        search.rule["effects"][0]["candidates"]["where"]["kind"],
        "or"
    );
    assert_eq!(
        search.rule["effects"][0]["candidates"]["where"]["operands"][0],
        subtype("Bird")
    );
    assert_eq!(search.rule["effects"][1]["kind"], "revealCards");

    let landfall = parse_expansion_triggered(
            "Landfall — Whenever a land you control enters, create a 2/2 colorless Robot artifact creature token.",
            "",
        )
        .expect("landfall token parses");
    assert_eq!(landfall.rule["event"]["where"], card_type("Land"));
    assert_eq!(
        landfall.rule["effects"][0]["token"]["types"],
        json!(["Artifact", "Creature"])
    );

    let life_gain = parse_expansion_triggered(
        "Whenever an opponent gains life, put that many +1/+1 counters on this creature.",
        "",
    )
    .expect("life-gain trigger parses");
    assert_eq!(life_gain.rule["event"]["kind"], "lifeGained");
    assert_eq!(
        life_gain.rule["effects"][0]["count"],
        json!({ "kind": "decisionResult", "decisionId": "lifeGainedAmount" })
    );
}

#[test]
fn landfall_deck_grammar_handles_short_names_replacements_and_single_cast_permissions() {
    let named_entry = parse_expansion_triggered(
            "When Omnath enters, it deals damage to any target equal to the number of Elementals you control.",
            "omnath, locus of the roil",
        )
        .expect("a short printed name identifies the named source");
    assert_eq!(named_entry.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(
        named_entry.rule["effects"][0]["amount"]["kind"],
        "countPermanents"
    );
    assert!(crate::engine::rule_is_executable(&named_entry.rule));

    let threshold_replacement = parse_expansion_triggered(
            "Landfall \u{2014} Whenever a land you control enters, create a 1/1 green Insect creature token. If you control six or more lands, create a token that's a copy of this creature instead.",
            "scute swarm",
        )
        .expect("a threshold replaces the base token instruction");
    assert_eq!(
        threshold_replacement.rule["effects"][0]["kind"],
        "conditionalEffect"
    );
    assert_eq!(
        threshold_replacement.rule["effects"][0]["then"][0]["kind"],
        "createTokenCopyOfPermanent"
    );
    assert!(crate::engine::rule_is_executable(
        &threshold_replacement.rule
    ));

    let single_cast = parse_simple_activated_ability(
            "{T}: Choose target nonland permanent card in your graveyard. If you haven't cast a spell this turn, you may cast that card. If you do, you can't cast additional spells this turn. Activate only as a sorcery.",
        )
        .expect("conditional graveyard cast permission parses");
    assert_eq!(
        single_cast.rule["effects"][0]["kind"],
        "grantSingleCastPermissionIfNoSpellCast"
    );
    assert_eq!(
        single_cast.rule["activationCondition"]["kind"],
        "sorceryTiming"
    );
    assert!(crate::engine::rule_is_executable(&single_cast.rule));
}

#[test]
fn variable_x_clauses_share_one_expression_parser() {
    let variable_re = Regex::new(&format!(r"^where X is ({})$", variable_clause_pattern()))
        .expect("variable clause pattern compiles");
    assert!(
        variable_re
            .captures("where X is the greatest power among Allies you control")
            .is_some()
    );
    assert!(
        variable_re
            .captures("where X is the number of Zombie Rogue creatures you control")
            .is_some()
    );
    assert!(
        variable_re
            .captures("where X is the number of creatures you control with power 5 or greater")
            .is_some()
    );

    assert_eq!(
        x_variable_expression("the greatest power among creatures you control")
            .expect("greatest power parses"),
        json!({
            "kind": "greatestPower",
            "player": controller(),
            "where": card_type("Creature"),
        })
    );
    assert_eq!(
        x_variable_expression("the greatest power among Allies you control")
            .expect("greatest Ally power parses"),
        json!({
            "kind": "greatestPower",
            "player": controller(),
            "where": subtype("Ally"),
        })
    );
    assert_eq!(
        x_variable_expression("the number of enchantments you control")
            .expect("enchantment count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": card_type("Enchantment"),
        })
    );
    assert_eq!(
        x_variable_expression("the number of creatures you control of the chosen type")
            .expect("chosen creature type count parses"),
        json!({
            "kind": "countChosenCreatureType",
            "decisionId": "chosenCreatureType",
        })
    );
    assert_eq!(
        x_variable_expression("the number of Allies you control").expect("Ally count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": subtype("Ally"),
        })
    );
    assert_eq!(
        x_variable_expression("the number of Zombie Rogue creatures you control")
            .expect("subtyped creature count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![
                subtype("Zombie"),
                subtype("Rogue"),
                card_type("Creature"),
            ]),
        })
    );
    assert_eq!(
        x_variable_expression("the number of non-Human creatures you control")
            .expect("negative subtype creature count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![
                not(subtype("Human")),
                card_type("Creature"),
            ]),
        })
    );
    assert_eq!(
        x_variable_expression("the number of creatures you control with power 5 or greater")
            .expect("power threshold count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![
                card_type("Creature"),
                compare(
                    ">=",
                    json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                    integer(5),
                ),
            ]),
        })
    );
    assert_eq!(
        x_variable_expression("the number of tapped artifacts and/or creatures you control")
            .expect("compound tapped count parses"),
        json!({
            "kind": "countPermanents",
            "player": controller(),
            "where": and(vec![
                json!({ "kind": "isTapped" }),
                or(vec![card_type("Artifact"), card_type("Creature")]),
            ]),
        })
    );
}

#[test]
fn criteria_grammar_combines_supertypes_subtypes_and_characteristics() {
    assert_eq!(
        parse_permanent_criteria("a legendary creature", ""),
        Some(and(vec![
            json!({ "kind": "isLegendary" }),
            card_type("Creature"),
        ]))
    );
    assert_eq!(
        parse_permanent_criteria("Bear, Spider, or Wolf", ""),
        Some(or(vec![
            subtype("Bear"),
            subtype("Spider"),
            subtype("Wolf")
        ]))
    );
    assert_eq!(
        parse_permanent_criteria("creature with power or toughness 2 or less", ""),
        Some(and(vec![
            card_type("Creature"),
            or(vec![
                compare(
                    "<=",
                    json!({ "kind": "powerOf", "object": { "kind": "candidate" } }),
                    integer(2),
                ),
                compare(
                    "<=",
                    json!({ "kind": "toughnessOf", "object": { "kind": "candidate" } }),
                    integer(2),
                ),
            ]),
        ]))
    );
}

#[test]
fn activation_cost_grammar_parses_arbitrary_sacrifice_criteria() {
    let mana = parse_mana_ability("{T}, Sacrifice another Goblin: Add {B}{R}.")
        .expect("subtype sacrifice mana ability parses");
    assert_eq!(mana.rule["costs"][1]["kind"], "sacrificePermanent");
    assert_eq!(
        mana.rule["declaration"]["decisions"][0]["candidates"]["where"],
        subtype("Goblin")
    );
    assert_eq!(
        mana.rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
        true
    );

    let activated =
        parse_simple_activated_ability("{2}, Sacrifice a legendary artifact or creature: Scry 2.")
            .expect("compound sacrifice cost parses");
    assert_eq!(activated.rule["effects"][0]["kind"], "scry");
    assert_eq!(activated.rule["effects"][0]["count"], integer(2));
}

#[test]
fn general_effect_grammar_is_shared_by_spells_and_triggers() {
    let spell = parse_simple_spell_ability(
        "Target creature gets +3/+0 and gains reach and first strike until end of turn.",
    )
    .expect("target bonus spell parses");
    assert_eq!(spell.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(spell.rule["effects"][1]["keyword"], "reach");
    assert_eq!(spell.rule["effects"][2]["keyword"], "firstStrike");

    let enter = parse_expansion_triggered("When this Aura enters, scry 2.", "Test Aura")
        .expect("enter scry trigger parses");
    assert_eq!(enter.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(enter.rule["effects"][0]["kind"], "scry");

    let death = parse_expansion_triggered("When this creature dies, draw a card.", "Test Creature")
        .expect("death draw trigger parses");
    assert_eq!(death.rule["event"]["kind"], "permanentDied");
    assert_eq!(death.rule["effects"][0]["kind"], "drawCards");

    let second_draw = parse_expansion_triggered(
        "Whenever you draw your second card each turn, put a +1/+1 counter on this creature.",
        "Test Creature",
    )
    .expect("second draw trigger parses");
    assert_eq!(second_draw.rule["event"]["drawOrdinal"], integer(2));
    assert_eq!(second_draw.rule["effects"][0]["counter"], "+1/+1");

    let connive = parse_expansion_triggered(
            "At the beginning of combat on your turn, target creature you control connives. (Draw a card, then discard a card. If you discarded a nonland card, put a +1/+1 counter on that creature.)",
            "Test Operative",
        )
        .expect("target connive trigger parses");
    assert_eq!(connive.rule["effects"][0]["kind"], "connive");
    assert_eq!(
        connive.rule["declaration"]["decisions"][0]["candidates"]["where"],
        card_type("Creature")
    );

    let self_connive = parse_expansion_triggered(
            "When this creature enters, it connives. (Draw a card, then discard a card. If you discarded a nonland card, put a +1/+1 counter on this creature.)",
            "Test Operative",
        )
        .expect("self connive trigger parses");
    assert_eq!(self_connive.rule["effects"][0]["permanent"], self_ref());

    let draw_and_treasure = parse_simple_spell_ability(
            "Draw two cards and create two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        )
        .expect("independent effects joined by and compose");
    assert_eq!(draw_and_treasure.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draw_and_treasure.rule["effects"][1]["kind"], "createTokens");
    assert_eq!(
        draw_and_treasure.rule["effects"][1]["token"]["name"],
        "Treasure"
    );

    let impulse = parse_simple_spell_ability(
            "Exile the top card of your library. Until the end of your next turn, you may play that card.",
        )
        .expect("top-card play permission parses by quantity and duration");
    assert_eq!(impulse.rule["effects"][0]["kind"], "exileTopCards");
    assert_eq!(impulse.rule["effects"][0]["count"], integer(1));
    assert_eq!(
        impulse.rule["effects"][1]["duration"]["kind"],
        "untilEndOfNextTurn"
    );
}

#[test]
fn opponent_second_draw_and_multiple_target_player_draws_share_leaf_effects() {
    let trigger = parse_expansion_triggered(
        "Whenever an opponent draws their second card each turn, you create a Treasure token.",
        "Test Splendor",
    )
    .expect("opponent draw ordinal composes with controller token creation");
    assert_eq!(trigger.rule["event"]["kind"], "cardDrawn");
    assert_eq!(trigger.rule["event"]["drawOrdinal"], integer(2));
    assert_eq!(trigger.rule["effects"][0]["kind"], "createTokens");
    assert!(crate::engine::rule_is_executable(&trigger.rule));

    for text in [
        "{2}{W}: Two target players each draw a card.",
        "{U}: Up to three target players each draw two cards.",
    ] {
        let parsed = parse_simple_activated_ability(text)
            .expect("multiple target-player draws use the shared draw effect");
        let decision = &parsed.rule["declaration"]["decisions"][0];
        assert_eq!(decision["candidates"]["kind"], "players");
        assert_eq!(parsed.rule["effects"][0]["kind"], "drawCards");
        assert_eq!(parsed.rule["effects"][0]["player"]["kind"], "chosenTargets");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_simple_activated_ability("{2}{W}: Two target permanents each draw a card.").is_none()
    );
}

#[test]
fn keyword_reminder_text_does_not_hide_ward_or_crew_parameters() {
    let ward = parse_keyword_ability(
            "Ward {2} (Whenever this creature becomes the target of a spell or ability an opponent controls, counter it unless that player pays {2}.)",
            "Test Guardian",
        )
        .expect("ward with reminder text parses");
    assert_eq!(ward.rule["ability"]["kind"], "ward");
    assert_eq!(ward.rule["ability"]["cost"]["manaCost"], "{2}");

    let crew = parse_avatar_deck_static(
            "Crew 2 (Tap any number of creatures you control with total power 2 or more: This Vehicle becomes an artifact creature until end of turn.)",
        )
        .expect("crew with reminder text parses");
    assert_eq!(crew.rule["effects"][0]["operation"], "crewVehicle");
    assert_eq!(crew.rule["effects"][0]["minimumPower"], integer(2));

    let teamwork = parse_keyword_ability(
            "Teamwork 4 (As an additional cost to cast this spell, you may tap any number of creatures you control with total power 4 or more.)",
            "Test Maneuver",
        )
        .expect("teamwork threshold parses");
    assert_eq!(teamwork.rule["ability"]["kind"], "teamwork");
    assert_eq!(teamwork.rule["ability"]["minimumPower"], integer(4));
}

#[test]
fn recurring_combat_keywords_and_non_gameplay_markers_parse_generically() {
    for (text, expected) in [
        (
            "Fear (This creature can't be blocked except by artifact creatures and/or black creatures.)",
            "fear",
        ),
        (
            "Infect (This creature deals damage to creatures in the form of -1/-1 counters and to players in the form of poison counters.)",
            "infect",
        ),
        (
            "Swampwalk (This creature can't be blocked as long as defending player controls a Swamp.)",
            "swampwalk",
        ),
    ] {
        let parsed =
            parse_keyword_ability(text, "Test Creature").unwrap_or_else(|| panic!("{text} parses"));
        assert_eq!(parsed.rule["ability"]["kind"], expected);
    }

    let restricted = parse_keyword_ability(
        "This creature can block only creatures with flying.",
        "Test Creature",
    )
    .expect("flying-only block restriction parses");
    assert_eq!(restricted.rule["ability"]["kind"], "canBlockOnlyFlying");

    let theme = parse_keyword_ability("(Theme color: {W})", "Test Card")
        .expect("theme color metadata parses");
    assert_eq!(theme.rule["kind"], "rulesMarker");

    for keyword in ["Horsemanship", "Intimidate", "Skulk"] {
        let parsed = parse_keyword_ability(keyword, "Test Creature")
            .unwrap_or_else(|| panic!("{keyword} parses"));
        assert_eq!(parsed.rule["ability"]["kind"], keyword.to_ascii_lowercase());
    }

    let toxic = parse_keyword_ability(
        "Toxic 3 (Players dealt combat damage by this creature also get three poison counters.)",
        "Test Creature",
    )
    .expect("toxic with reminder text parses");
    assert_eq!(toxic.rule["ability"]["kind"], "toxic");
    assert_eq!(toxic.rule["ability"]["count"], integer(3));

    let flanking = parse_keyword_ability(
            "Flanking (Whenever a creature without flanking blocks this creature, the blocking creature gets -1/-1 until end of turn.)",
            "Test Creature",
        )
        .expect("flanking with reminder text parses");
    assert_eq!(flanking.rule["ability"]["kind"], "flanking");

    let partner = parse_keyword_ability(
        "Partner (You can have two commanders if both have partner.)",
        "Test Commander",
    )
    .expect("partner deck-construction marker parses");
    assert_eq!(partner.rule["kind"], "rulesMarker");
}

#[test]
fn recent_effect_families_compose_without_card_specific_branches() {
    let named_partner = parse_keyword_ability(
        "Partner—Character select (You can have two commanders if both have this ability.)",
        "Test Commander",
    )
    .expect("named partner variant parses through the keyword grammar");
    assert_eq!(named_partner.rule["kind"], "rulesMarker");

    for (text, expected_type) in [
        (
            "Affinity for artifacts (This spell costs {1} less to cast for each artifact you control.)",
            "Artifact",
        ),
        (
            "Affinity for creatures (This spell costs {1} less to cast for each creature you control.)",
            "Creature",
        ),
    ] {
        let affinity = parse_common_static_ability(text, "")
            .unwrap_or_else(|| panic!("{text} parses through criteria grammar"));
        assert_eq!(
            affinity.rule["modifiers"][0]["amount"]["where"]["value"],
            expected_type
        );
    }
    let granted_affinity = parse_common_static_ability(
        "Instant and sorcery spells you cast have affinity for creatures.",
        "",
    )
    .expect("granted affinity composes spell and permanent criteria");
    assert_eq!(
        granted_affinity.rule["modifiers"][0]["amount"]["where"]["value"],
        "Creature"
    );
    assert_eq!(granted_affinity.rule["modifiers"][0]["where"]["kind"], "or");

    let conditional_bonus = parse_common_static_ability(
        "Infusion — This creature gets +2/+0 as long as you gained life this turn.",
        "Test Creature",
    )
    .expect("named conditional bonus uses the general condition grammar");
    assert_eq!(conditional_bonus.rule["condition"]["kind"], "compare");
    assert_eq!(
        conditional_bonus.rule["modifiers"][0]["kind"],
        "modifyPowerToughness"
    );
    let conditional_team_bonus = parse_common_static_ability(
            "Infusion — Creatures you control get +1/+0 and have trample as long as you gained life this turn.",
            "Test Creature",
        )
        .expect("conditional team bonus composes criteria, stats, keyword, and condition");
    assert_eq!(
        conditional_team_bonus.rule["modifiers"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    assert_eq!(
        conditional_team_bonus.rule["modifiers"][1]["keyword"],
        "trample"
    );

    let draw_then_land = parse_simple_spell_ability(
        "Draw three cards. You may put a land card from your hand onto the battlefield tapped.",
    )
    .expect("independent sentences compose in Oracle order");
    assert_eq!(draw_then_land.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(draw_then_land.rule["effects"][1]["kind"], "moveTargetCard");
    assert_eq!(draw_then_land.rule["effects"][1]["tapped"], true);

    let optional_destroy = parse_expansion_triggered(
        "When this creature enters, destroy up to one target artifact or enchantment.",
        "Test Creature",
    )
    .expect("optional qualified destruction composes with an entry trigger");
    assert_eq!(
        optional_destroy.rule["declaration"]["decisions"][0]["minimum"],
        0
    );

    let optional_return = parse_expansion_triggered(
        "When this creature enters, return up to one other target creature to its owner's hand.",
        "Test Creature",
    )
    .expect("optional other-creature return uses reusable target criteria");
    assert_eq!(
        optional_return.rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
        true
    );

    let aura_keyword = parse_expansion_triggered(
        "When this Aura enters, enchanted creature gains hexproof until end of turn.",
        "Test Aura",
    )
    .expect("attached-permanent keyword effect composes with an Aura entry trigger");
    assert_eq!(aura_keyword.rule["effects"][0]["kind"], "grantKeyword");
    assert_eq!(
        aura_keyword.rule["effects"][0]["object"]["kind"],
        "attachedPermanent"
    );

    let leaves = parse_expansion_triggered(
        "When this creature leaves the battlefield, create a Food token.",
        "Test Creature",
    )
    .expect("source-leaves event composes with named-token creation");
    assert_eq!(leaves.rule["event"]["kind"], "permanentLeftBattlefield");
    assert_eq!(leaves.rule["effects"][0]["kind"], "createTokens");

    assert!(
        parse_general_effect_sequence(
            "Draw a card. It deals 1 damage to any target.",
            "Test Spell",
        )
        .is_none(),
        "anaphoric clauses must not be composed as independent effects"
    );
}

#[test]
fn recurring_aura_energy_regeneration_and_delayed_sacrifice_grammar_composes() {
    let energy = parse_expansion_triggered(
        "When this creature enters, you get {E}{E} (two energy counters).",
        "Test Creature",
    )
    .expect("energy ETB trigger parses");
    assert_eq!(energy.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(energy.rule["effects"][0]["kind"], "addPlayerCounters");
    assert_eq!(energy.rule["effects"][0]["count"], integer(2));

    let aura_tap = parse_expansion_triggered(
        "When this Aura enters, tap enchanted creature.",
        "Test Aura",
    )
    .expect("Aura ETB tap trigger parses");
    assert_eq!(aura_tap.rule["effects"][0]["kind"], "tapPermanent");
    assert_eq!(
        aura_tap.rule["effects"][0]["permanent"]["kind"],
        "attachedPermanent"
    );

    let sacrifice = parse_expansion_triggered(
        "At the beginning of the end step, sacrifice this creature.",
        "Test Creature",
    )
    .expect("end-step self-sacrifice trigger parses");
    assert_eq!(sacrifice.rule["event"]["kind"], "stepBegan");
    assert_eq!(sacrifice.rule["event"]["step"], "endStep");
    assert_eq!(sacrifice.rule["effects"][0]["kind"], "sacrificePermanent");

    let regenerate = parse_common_activated_ability("{1}{G}: Regenerate this creature.")
        .expect("generic self-regeneration activation parses");
    assert_eq!(regenerate.rule["costs"][0]["kind"], "payMana");
    assert_eq!(
        regenerate.rule["effects"][0]["kind"],
        "installRegenerationShield"
    );

    let exile_choice = parse_common_activated_ability(
        "Exile a creature card from your graveyard: Regenerate this creature.",
    )
    .expect("graveyard-card regeneration cost parses");
    assert_eq!(exile_choice.rule["costs"][0]["kind"], "exileGraveyardCard");
    assert_eq!(
        exile_choice.rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Creature"
    );

    let exile_top = parse_common_activated_ability(
        "Exile the top creature card of your graveyard: Regenerate this creature.",
    )
    .expect("top matching graveyard-card regeneration cost parses");
    assert_eq!(
        exile_top.rule["costs"][0]["kind"],
        "exileTopMatchingGraveyardCard"
    );

    let controlled = parse_common_static_ability("You control enchanted creature.", "")
        .expect("Aura control effect parses");
    assert_eq!(
        controlled.rule["modifiers"][0]["kind"],
        "controlAttachedPermanent"
    );

    for text in [
        "Enchant artifact",
        "Enchant artifact or creature",
        "Enchant artifact (Target an artifact as you cast this.)",
    ] {
        let enchant =
            parse_common_static_ability(text, "").unwrap_or_else(|| panic!("{text} parses"));
        assert_eq!(enchant.rule["ability"]["kind"], "enchant");
    }
}

#[test]
fn static_grammar_parses_lords_and_enduring_story_without_card_names() {
    let lord = parse_common_static_ability("Other Elves you control get +1/+1.", "")
        .expect("generic lord ability parses");
    assert_eq!(lord.rule["modifiers"][0]["kind"], "modifyPowerToughness");
    assert_eq!(
        lord.rule["modifiers"][0]["objects"]["where"],
        subtype("Elf")
    );
    assert_eq!(lord.rule["modifiers"][0]["objects"]["excludeSource"], true);

    let story = parse_common_static_ability(
        "As long as you have an enduring story, creatures you control get +1/+1.",
        "",
    )
    .expect("enduring story wrapper parses");
    assert_eq!(
        story.rule["modifiers"][0]["condition"]["kind"],
        "hasEnduringStory"
    );

    for (text, amount) in [
        (
            "As long as you have an enduring story, creatures can't attack you unless their controller pays {1} for each of those creatures.",
            1,
        ),
        (
            "Creatures can't attack you or planeswalkers you control unless their controller pays {2} for each of those creatures.",
            2,
        ),
        (
            "Creatures can't attack you unless their controller pays {3} for each creature they control that's attacking you.",
            3,
        ),
    ] {
        let tax = parse_common_static_ability(text, "")
            .expect("per-attacker taxes parse across Oracle phrasings");
        assert_eq!(tax.rule["modifiers"][0]["kind"], "attackTax");
        assert_eq!(tax.rule["modifiers"][0]["amount"]["value"], amount);
        assert!(crate::engine::rule_is_executable(&tax.rule));
    }
    assert!(
        parse_common_static_ability(
            "Creatures can't attack you unless their controller pays {1} for each permanent.",
            "",
        )
        .is_none()
    );

    let storied = parse_common_static_ability(
            "Storied (If you control three or more artifacts, legendaries, and/or Sagas, you have an enduring story for the rest of the game.)",
            "",
        )
        .expect("storied parses as a keyword");
    assert_eq!(storied.rule["ability"]["kind"], "storied");

    let early_turns = parse_common_static_ability(
        "You can't cast Test Walker during your first, second, or third turns of the game.",
        "Test Walker",
    )
    .expect("early-turn casting restriction parses without a card-name branch");
    assert_eq!(early_turns.rule["kind"], "rulesMarker");
    assert_eq!(early_turns.rule["cantCastThroughTurn"], 3);

    let must_attack =
        parse_common_static_ability("Test Bruiser attacks each combat if able.", "Test Bruiser")
            .expect("source attack requirement parses without a card name branch");
    assert_eq!(
        must_attack.rule["modifiers"][0]["kind"],
        "attackEachCombatIfAble"
    );

    let subtype_reduction =
        parse_common_static_ability("Villain spells you cast cost {1} less to cast.", "")
            .expect("subtype spell reduction parses");
    assert_eq!(
        subtype_reduction.rule["modifiers"][0]["where"],
        subtype("Villain")
    );
    assert_eq!(subtype_reduction.rule["modifiers"][0]["amount"], integer(1));
}

#[test]
fn first_equip_ability_alternative_cost_composes_with_enduring_story() {
    for (text, mana_cost, conditioned) in [
        (
            "As long as you have an enduring story, you may pay {0} rather than pay the equip cost of the first equip ability you activate each turn.",
            "{0}",
            true,
        ),
        (
            "You may pay {W}{U} rather than pay the equip cost of the first equip ability you activate each turn.",
            "{W}{U}",
            false,
        ),
    ] {
        let parsed = parse_common_static_ability(text, "")
            .expect("first equip alternative cost parses through the static leaves");
        let modifier = &parsed.rule["modifiers"][0];
        assert_eq!(modifier["kind"], "firstActivatedAbilityAlternativeCost");
        assert_eq!(modifier["abilityKind"], "equip");
        assert_eq!(modifier["cost"]["manaCost"], mana_cost);
        assert_eq!(modifier.get("condition").is_some(), conditioned);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_static_ability(
            "You may pay {0} rather than pay the equip cost of the second equip ability you activate each turn.",
            "",
        )
        .is_none()
    );
}

#[test]
fn single_source_characteristic_scales_with_controlled_permanent_criteria() {
    for (text, face_name, characteristic) in [
        (
            "Esgaroth Garrison's power is equal to the number of creatures you control.",
            "Esgaroth Garrison",
            "power",
        ),
        (
            "Test Guardian's toughness is equal to the number of artifacts you control.",
            "Test Guardian",
            "toughness",
        ),
    ] {
        let parsed = parse_common_static_ability(text, face_name)
            .expect("single source characteristics delegate to permanent criteria");
        let modifier = &parsed.rule["modifiers"][0];
        assert_eq!(modifier["kind"], "modifyPowerToughness");
        assert_eq!(modifier[characteristic]["kind"], "countPermanents");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_static_ability(
            "Other Garrison's power is equal to the number of creatures you control.",
            "Esgaroth Garrison",
        )
        .is_none()
    );
}

#[test]
fn recent_set_grammar_reuses_sources_costs_keywords_and_thresholds() {
    let named_attack =
        parse_common_static_ability("Hulk attacks each combat if able.", "Hulk, Always Angry")
            .expect("an abbreviated source name parses");
    assert_eq!(
        named_attack.rule["modifiers"][0]["kind"],
        "attackEachCombatIfAble"
    );

    let named_unblockable =
        parse_common_static_ability("Sygg can't be blocked.", "Sygg, Wanderwine Wisdom")
            .expect("an abbreviated source name parses for unblockable");
    assert_eq!(
        named_unblockable.rule["modifiers"][0]["kind"],
        "cantBeBlocked"
    );

    let wither = parse_keyword_ability(
        "Wither (This deals damage to creatures in the form of -1/-1 counters.)",
        "Test Witherer",
    )
    .expect("wither parses through generic keyword reminder stripping");
    assert_eq!(wither.rule["ability"]["kind"], "wither");

    let surveil = parse_simple_spell_ability(
            "Surveil 2. (Look at the top two cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
        )
        .expect("surveil shares the library-selection grammar");
    assert_eq!(surveil.rule["effects"][0]["kind"], "surveil");
    assert_eq!(surveil.rule["effects"][0]["count"], integer(2));

    let mutual_draw = parse_simple_spell_ability("You and target opponent each draw three cards.")
        .expect("mutual draw composes two generic draw effects");
    assert_eq!(
        mutual_draw.rule["effects"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        mutual_draw.rule["declaration"]["decisions"][0]["id"],
        "targetOpponent"
    );

    let discard =
        parse_simple_spell_ability("As an additional cost to cast this spell, discard a card.")
            .expect("additional discard reuses activation cost grammar");
    assert_eq!(
        discard.rule["declaration"]["additionalCosts"][0]["kind"],
        "discardCard"
    );

    let opponent_lands = parse_conditional_enter_tapped(
        "This land enters tapped unless your opponents control eight or more lands.",
    )
    .expect("opponent permanent threshold parses");
    assert_eq!(
        opponent_lands.rule["condition"]["operand"]["left"]["kind"],
        "greatestOpponentPermanentCount"
    );

    let creature_counter = parse_avatar_deck_static(
            "If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead.",
        )
        .expect("counter replacement keeps its permanent criteria");
    assert_eq!(
        creature_counter.rule["modifiers"][0]["where"],
        card_type("Creature")
    );

    let goaded = parse_avatar_deck_static(
            "Enchanted creature gets +1/+1 and is goaded. (It attacks each combat if able and attacks a player other than you if able.)",
        )
        .expect("attached goad bonus accepts arbitrary fixed power and toughness");
    assert_eq!(goaded.rule["modifiers"][0]["power"], integer(1));

    let alliance = parse_expansion_triggered(
            "Alliance — Whenever another creature you control enters, this creature gets +1/+0 until end of turn.",
            "Test Ally",
        )
        .expect("named ability prefix composes with generic trigger grammar");
    assert_eq!(alliance.rule["event"]["kind"], "permanentEntered");
    assert_eq!(alliance.rule["effects"][0]["kind"], "modifyPowerToughness");

    let return_to_hand = parse_composed_entry_triggered(
        "When this creature leaves the battlefield, return the exiled card to its owner's hand.",
    )
    .expect("linked exile can select the owner's hand destination");
    assert_eq!(
        return_to_hand.rule["effects"][0]["operation"],
        "returnCardsExiledWithSourceToOwnersHand"
    );
}

#[test]
fn conditional_alternative_cost_uses_spell_color_and_cost_grammar() {
    let parsed = parse_avatar_deck_spell(
            "If an opponent cast a blue spell this turn, you may pay {R} rather than pay this spell's mana cost. Change the target of target spell with a single target.",
        )
        .expect("color-conditional alternative cost parses without a card-name branch");
    assert_eq!(parsed.rule["kind"], "rulesMarker");
    assert_eq!(
        parsed.rule["conditionalAlternativeCost"]["opponentCastColor"],
        "blue"
    );
    assert_eq!(parsed.rule["conditionalAlternativeCost"]["manaCost"], "{R}");
}

#[test]
fn entry_replacement_grammar_uses_criteria_and_quantities_instead_of_land_names() {
    let legendary = parse_conditional_enter_tapped(
        "Test Sanctuary enters tapped unless you control a legendary creature.",
    )
    .expect("named land criterion parses");
    assert_eq!(
        legendary.rule["condition"]["operand"]["where"],
        and(vec![
            json!({ "kind": "isLegendary" }),
            card_type("Creature"),
        ])
    );

    let basics = parse_conditional_enter_tapped(
        "This land enters tapped unless you control two or more basic lands.",
    )
    .expect("controlled land count parses");
    assert_eq!(basics.rule["condition"]["operand"]["operator"], ">=");
    assert_eq!(
        basics.rule["condition"]["operand"]["left"]["where"],
        and(vec![
            json!({ "kind": "typeLineContains", "value": "Basic" }),
            card_type("Land"),
        ])
    );

    let entering_counter =
        parse_special_static_ability("Test Hero enters with an indestructible counter on her.")
            .expect("named permanent entering counter parses");
    assert_eq!(
        entering_counter.rule["replacement"][0]["counter"],
        "indestructible"
    );
    assert_eq!(entering_counter.rule["replacement"][0]["count"], integer(1));
}

#[test]
fn inset_spell_grammar_composes_targeted_bonuses_and_variable_player_values() {
    assert!(
        Regex::new(
            r"(?i)^Target (.+?) gets ([+-]\d+)/([+-]\d+)(?: and gains? (.+))? until end of turn\.$",
        )
        .unwrap()
        .is_match("Target creature gets +1/+0 and gains flying until end of turn.")
    );
    assert!(permanent_target_candidates("creature", "").is_some());
    assert!(oracle_keyword_list("flying").is_some());
    let (bonus_effects, bonus_decisions) = parse_general_effect_instruction(
        "Target creature gets +1/+0 and gains flying until end of turn.",
        "",
    )
    .expect("targeted bonus and keyword parse together");
    assert_eq!(bonus_decisions[0]["kind"], "chooseTargets");
    assert_eq!(bonus_effects[0]["kind"], "modifyPowerToughness");
    assert_eq!(bonus_effects[1]["kind"], "grantKeyword");

    let (draw_effects, draw_decisions) =
        parse_general_effect_instruction("Target player draws X cards.", "")
            .expect("variable target-player draw parses");
    assert_eq!(draw_decisions[0]["kind"], "chooseNumber");
    assert_eq!(draw_effects[0]["count"], decision_result("xValue"));
}

#[test]
fn prepare_trigger_grammar_uses_events_conditions_and_designation_effects() {
    let upkeep = parse_prepare_triggered_ability(
            "At the beginning of your upkeep, if this creature isn't prepared, it becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)",
        )
        .expect("conditional upkeep preparation parses");
    assert_eq!(upkeep.rule["event"]["step"], "upkeep");
    assert_eq!(upkeep.rule["condition"]["kind"], "not");
    assert_eq!(upkeep.rule["effects"][0]["kind"], "setPrepared");

    let creature_cast = parse_prepare_triggered_ability(
            "Whenever you cast a creature spell, Abigale becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)",
        )
        .expect("creature-spell preparation parses");
    assert_eq!(creature_cast.rule["event"]["kind"], "spellCast");
    assert_eq!(creature_cast.rule["event"]["where"], card_type("Creature"));

    let next_spell = parse_prepare_triggered_ability(
            "When you next cast an instant or sorcery spell this turn, copy that spell. You may choose new targets for the copy.",
        )
        .expect("next-spell delayed trigger parses");
    assert_eq!(next_spell.rule["kind"], "spellAbility");
    assert_eq!(
        next_spell.rule["effects"][0]["kind"],
        "installDelayedSpellCastTrigger"
    );

    let (graveyard_effects, graveyard_decisions) = parse_general_effect_instruction(
        "Put target card from your graveyard on the bottom of your library.",
        "",
    )
    .expect("unqualified graveyard target parses");
    assert_eq!(graveyard_decisions[0]["id"], "targetGraveyardCard");
    assert_eq!(graveyard_effects[0]["to"], "libraryBottom");

    let (mill_effects, mill_decisions) =
        parse_general_effect_instruction("Target player mills twice X cards.", "")
            .expect("arithmetic target-player mill parses");
    assert_eq!(mill_effects[0]["count"]["kind"], "multiply");
    assert!(
        mill_decisions
            .iter()
            .any(|decision| { decision["id"].as_str() == Some("xValue") })
    );
}

#[test]
fn numeric_expression_grammar_composes_arithmetic_and_comparisons() {
    assert_eq!(
        parse_numeric_expression_text("three minus X"),
        Some(json!({
            "kind": "subtract",
            "left": integer(3),
            "right": decision_result("xValue"),
        }))
    );
    assert_eq!(
        parse_numeric_expression_text("twice X plus one"),
        Some(json!({
            "kind": "add",
            "left": {
                "kind": "multiply",
                "left": decision_result("xValue"),
                "right": integer(2),
            },
            "right": integer(1),
        }))
    );
    assert_eq!(
        parse_numeric_expression_text("half of X, rounded up"),
        Some(json!({
            "kind": "divide",
            "left": decision_result("xValue"),
            "right": integer(2),
            "round": "up",
        }))
    );
    assert_eq!(
        parse_numeric_comparison_text("X is 2 or less"),
        Some(compare("<=", decision_result("xValue"), integer(2)))
    );

    let slumbering = parse_special_static_ability(
            "This creature enters with a number of stun counters on it equal to three minus X. If X is 2 or less, it enters tapped. (If a permanent with a stun counter would become untapped, remove one from it instead.)",
        )
        .expect("generic entering arithmetic grammar parses");
    assert_eq!(slumbering.rule["replacement"][0]["counter"], "stun");
    assert_eq!(
        slumbering.rule["replacement"][0]["count"]["kind"],
        "subtract"
    );
    assert_eq!(
        slumbering.rule["replacement"][1]["condition"]["operator"],
        "<="
    );
}

#[test]
fn generic_graveyard_search_and_return_grammar_preserves_criteria_and_destination() {
    let entomb = parse_simple_spell_ability(
        "Search your library for a card, put that card into your graveyard, then shuffle.",
    )
    .expect("an unrestricted library-to-graveyard search parses");
    assert_eq!(entomb.rule["effects"][1]["to"]["kind"], "graveyard");

    let buried_alive = parse_simple_spell_ability(
            "Search your library for up to three creature cards, put them into your graveyard, then shuffle.",
        )
        .expect("a quantified typed library-to-graveyard search parses");
    assert_eq!(buried_alive.rule["effects"][0]["maximum"], 3);
    assert_eq!(
        buried_alive.rule["effects"][0]["candidates"]["where"],
        card_type("Creature")
    );

    let unmarked_grave = parse_simple_spell_ability(
            "Search your library for a nonlegendary card, put that card into your graveyard, then shuffle.",
        )
        .expect("a negative supertype criterion parses");
    assert_eq!(
        unmarked_grave.rule["effects"][0]["candidates"]["where"]["kind"],
        "not"
    );

    let reanimate = parse_simple_activated_ability(
            "{B}, {T}, Sacrifice this creature: Return target creature card from your graveyard to the battlefield.",
        )
        .expect("a targeted reanimation activation parses");
    assert_eq!(reanimate.rule["effects"][0]["to"], "battlefield");

    let top = parse_simple_activated_ability(
        "{1}{U}, {T}: Put target artifact card from your graveyard on top of your library.",
    )
    .expect("a typed graveyard-to-library-top activation parses");
    assert_eq!(top.rule["effects"][0]["to"], "libraryTop");
}

#[test]
fn unearth_is_a_generic_graveyard_activation_with_both_exile_rules() {
    let parsed = parse_keyword_ability(
            "Unearth {3}{B} ({3}{B}: Return this card from your graveyard to the battlefield. It gains haste. Exile it at the beginning of the next end step or if it would leave the battlefield. Unearth only as a sorcery.)",
            "Test Creature",
        )
        .expect("unearth reminder text parses");

    assert_eq!(parsed.rule["kind"], "activatedAbility");
    assert_eq!(parsed.rule["activationZone"], "graveyard");
    assert_eq!(parsed.rule["activationCondition"]["kind"], "sorceryTiming");
    assert_eq!(parsed.rule["costs"][0]["manaCost"], "{3}{B}");
    assert_eq!(parsed.rule["effects"][0]["grantKeywords"], json!(["haste"]));
    assert_eq!(parsed.rule["effects"][0]["exileIfLeavesBattlefield"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn dungeon_and_graveyard_trigger_grammar_is_data_driven() {
    let venture = parse_simple_activated_ability(
        "{3}, {T}: Venture into the dungeon. Activate only as a sorcery.",
    )
    .expect("venture parses as a reusable effect");
    assert_eq!(venture.rule["effects"][0]["kind"], "ventureDungeon");

    let graveyard = parse_expansion_triggered(
            "Whenever one or more creature cards are put into your graveyard from anywhere, venture into the dungeon. This ability triggers only once each turn.",
            "Test Delver",
        )
        .expect("a typed graveyard-entry trigger parses");
    assert_eq!(graveyard.rule["event"]["kind"], "cardsEnteredGraveyard");
    assert_eq!(graveyard.rule["event"]["where"], card_type("Creature"));
    assert_eq!(graveyard.rule["triggerLimit"]["kind"], "onceEachTurn");

    let completed = parse_expansion_triggered(
            "Create Undead â€” Whenever you complete a dungeon, return target creature card from your graveyard to the battlefield.",
            "Test Delver",
        )
        .expect("a named dungeon-completion trigger parses");
    assert_eq!(completed.rule["event"]["kind"], "dungeonCompleted");
    assert_eq!(completed.rule["effects"][0]["to"], "battlefield");

    let room = parse_dungeon_room(
        "Cave Entrance â€” Scry 1. (Leads to: Goblin Lair, Mine Tunnels)",
        "Lost Mine of Phandelver",
    )
    .expect("a Dungeon room and its routes parse from Oracle text");
    assert_eq!(room.rule["room"], "Cave Entrance");
    assert_eq!(room.rule["effects"][0]["kind"], "scry");
    assert_eq!(room.rule["nextRooms"].as_array().map(Vec::len), Some(2));

    let twisted_caverns = parse_dungeon_room(
            "Twisted Caverns \u{2014} Target creature can't attack until your next turn. (Leads to: Runestone Caverns)",
            "Dungeon of the Mad Mage",
        )
        .expect("next-turn attack restrictions reuse permanent targeting");
    assert_eq!(twisted_caverns.rule["effects"][0]["kind"], "grantKeyword");
    assert_eq!(
        twisted_caverns.rule["effects"][0]["duration"]["kind"],
        "untilNextTurn"
    );

    let fungi = parse_dungeon_room(
            "Fungi Cavern \u{2014} Target creature gets -4/-0 until your next turn. (Leads to: Lost Level)",
            "Lost Mine of Phandelver",
        )
        .expect("next-turn stat changes reuse the numeric modifier grammar");
    assert_eq!(fungi.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(fungi.rule["effects"][0]["power"]["value"], -4);

    let runestone = parse_dungeon_room(
            "Runestone Caverns \u{2014} Exile the top two cards of your library. You may play them. (Leads to: Muiral's Graveyard)",
            "Dungeon of the Mad Mage",
        )
        .expect("linked top-card exile and play permission compose");
    assert_eq!(runestone.rule["effects"][0]["kind"], "exileTopCards");
    assert_eq!(runestone.rule["effects"][1]["kind"], "grantCardPermission");

    let mad_wizard = parse_dungeon_room(
            "Mad Wizard's Lair \u{2014} Draw three cards and reveal them. You may cast one of them without paying its mana cost.",
            "Dungeon of the Mad Mage",
        )
        .expect("drawn cards can be linked to a bounded free-cast choice");
    assert_eq!(mad_wizard.rule["effects"][0]["bind"], "drawnCards");
    assert_eq!(mad_wizard.rule["effects"][2]["sourceZone"], "hand");
    assert_eq!(mad_wizard.rule["effects"][2]["maximum"]["value"], 1);
    assert!(crate::engine::rule_is_executable(&twisted_caverns.rule));
    assert!(crate::engine::rule_is_executable(&fungi.rule));
    assert!(crate::engine::rule_is_executable(&runestone.rule));
    assert!(crate::engine::rule_is_executable(&mad_wizard.rule));

    let veils = parse_dungeon_room(
            "Veils of Fear \u{2014} Each player loses 2 life unless they discard a card. (Leads to: Sandfall Cell)",
            "Tomb of Annihilation",
        )
        .expect("each player may pay a reusable discard cost");
    assert_eq!(
        veils.rule["effects"][0]["kind"],
        "eachPlayerPaysCostOrLosesLife"
    );
    assert_eq!(veils.rule["effects"][0]["cost"]["kind"], "discardCard");

    let sandfall = parse_dungeon_room(
            "Sandfall Cell \u{2014} Each player loses 2 life unless they sacrifice a creature, artifact, or land of their choice. (Leads to: Cradle of the Death God)",
            "Tomb of Annihilation",
        )
        .expect("a sacrifice payment reuses disjunctive permanent criteria");
    assert_eq!(sandfall.rule["effects"][0]["cost"]["where"]["kind"], "or");

    let oubliette = parse_dungeon_room(
            "Oubliette \u{2014} Discard a card and sacrifice a creature, an artifact, and a land. (Leads to: Cradle of the Death God)",
            "Tomb of Annihilation",
        )
        .expect("ordered discard and sacrifice costs become normal zone mutations");
    assert_eq!(oubliette.rule["effects"].as_array().map(Vec::len), Some(4));
    assert_eq!(oubliette.rule["effects"][0]["kind"], "discardCards");
    assert!(oubliette.rule["effects"].as_array().is_some_and(|effects| {
        effects[1..]
            .iter()
            .all(|effect| effect["kind"] == "sacrificePermanents")
    }));
    assert!(crate::engine::rule_is_executable(&veils.rule));
    assert!(crate::engine::rule_is_executable(&sandfall.rule));
    assert!(crate::engine::rule_is_executable(&oubliette.rule));

    let entry_restriction = parse_common_static_ability(
        "You can't enter this dungeon unless you \"venture into the Deep Vault.\"",
        "Test Dungeon",
    )
    .expect("a quoted dungeon-entry procedure parses generically");
    assert_eq!(
        entry_restriction.rule["modifiers"][0]["kind"],
        "dungeonEntryRestriction"
    );
    assert_eq!(
        entry_restriction.rule["modifiers"][0]["procedure"],
        "venture into the Deep Vault"
    );
    assert!(crate::engine::rule_is_executable(&entry_restriction.rule));

    let throne = parse_dungeon_room(
            "Final Vault \u{2014} Reveal the top ten cards of your library. Put a creature card from among them onto the battlefield with three +1/+1 counters on it. It gains hexproof until your next turn. Then shuffle.",
            "Test Dungeon",
        )
        .expect("a linked reveal, selection, move, counter, keyword, and shuffle sequence parses");
    assert_eq!(throne.rule["effects"][0]["kind"], "lookAtTopCards");
    assert_eq!(throne.rule["effects"][2]["kind"], "chooseCards");
    assert_eq!(throne.rule["effects"][3]["kind"], "moveCards");
    assert_eq!(throne.rule["effects"][4]["kind"], "putCounters");
    assert_eq!(throne.rule["effects"][5]["kind"], "grantKeyword");
    assert_eq!(throne.rule["effects"][6]["kind"], "shuffleZone");
    assert!(crate::engine::rule_is_executable(&throne.rule));

    assert!(
        parse_dungeon_room(
            "Unclear Room \u{2014} Target creature gets much weaker until your next turn.",
            "Test Dungeon",
        )
        .is_some_and(
            |draft| draft.rule["effects"][0]["kind"] == "unsupportedDungeonRoomInstruction"
        )
    );
}

#[test]
fn dungeon_deck_patterns_compose_names_conditions_zones_and_keywords() {
    let named_entry = parse_expansion_triggered(
            "When Barrowin enters, venture into the dungeon. (Enter the first room or advance to the next room.)",
            "Barrowin of Clan Undurr",
        )
        .expect("a short self-name identifies an enter trigger");
    assert_eq!(named_entry.rule["event"]["kind"], "enterBattlefield");

    let completed = parse_expansion_triggered(
            "Whenever Barrowin attacks, return up to one creature card with mana value 3 or less from your graveyard to the battlefield if you've completed a dungeon.",
            "Barrowin of Clan Undurr",
        )
        .expect("dungeon-completion condition and resolution-time graveyard choice parse");
    assert_eq!(completed.rule["event"]["kind"], "declaredAttacker");
    assert_eq!(completed.rule["effects"][0]["kind"], "conditional");
    assert_eq!(
        completed.rule["effects"][0]["condition"]["kind"],
        "completedDungeon"
    );
    assert_eq!(
        completed.rule["effects"][0]["then"][0]["candidates"]["where"]["kind"],
        "and"
    );

    let reversal = parse_simple_spell_ability(
            "Return up to one target creature card from your graveyard to your hand. Venture into the dungeon. (Enter the first room or advance to the next room.)",
        )
        .expect("ordered graveyard return and venture effects compose");
    assert_eq!(reversal.rule["effects"][0]["to"]["kind"], "hand");
    assert_eq!(reversal.rule["effects"][1]["kind"], "ventureDungeon");

    let solar_entry = parse_expansion_triggered(
            "Whenever this creature or another nontoken creature you control enters, venture into the dungeon. (Enter the first room or advance to the next room.)",
            "Radiant Solar",
        )
        .expect("self-or-another nontoken entry trigger parses");
    assert_eq!(solar_entry.rule["event"]["nontoken"], true);

    let solar_hand = parse_simple_activated_ability(
        "{W}, Discard this card: Venture into the dungeon and you gain 3 life.",
    )
    .expect("conjoined activation effects parse");
    assert_eq!(solar_hand.rule["activationZone"], "hand");
    assert_eq!(solar_hand.rule["effects"][0]["kind"], "ventureDungeon");
    assert_eq!(solar_hand.rule["effects"][1]["kind"], "gainLife");

    let first_strike = parse_common_static_ability(
        "During your turn, this creature has first strike.",
        "Triumphant Adventurer",
    )
    .expect("controller-turn keyword parses");
    assert_eq!(
        first_strike.rule["modifiers"][0]["condition"]["kind"],
        "duringControllerTurn"
    );

    let attacking_alone = parse_common_static_ability(
        "This creature can't be blocked as long as it's attacking alone.",
        "Yuan-Ti Malison",
    )
    .expect("attacking-alone evasion parses");
    assert_eq!(
        attacking_alone.rule["modifiers"][0]["condition"]["kind"],
        "sourceAttackingAlone"
    );

    for rule in [
        named_entry.rule,
        completed.rule,
        reversal.rule,
        solar_entry.rule,
        solar_hand.rule,
        first_strike.rule,
        attacking_alone.rule,
    ] {
        assert!(crate::engine::rule_is_executable(&rule));
    }
}

#[test]
fn copy_permanent_activation_reuses_cost_target_and_copy_grammar() {
    for text in [
        "{2}, {T}: This land becomes a copy of target land, except it has this ability.",
        "{U}, {T}: This artifact becomes a copy of target artifact, except it has this ability.",
    ] {
        let parsed = parse_common_activated_ability(text)
            .expect("a permanent can copy a matching target while retaining this activation");
        assert_eq!(parsed.rule["costs"][0]["kind"], "payMana");
        assert_eq!(parsed.rule["costs"][1]["kind"], "tap");
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
            "cardTypeContains"
        );
        assert_eq!(parsed.rule["effects"][0]["kind"], "becomeCopyOfPermanent");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_activated_ability(
            "{2}, {T}: This land becomes a copy of target spell, except it has this ability."
        )
        .is_none()
    );
}

#[test]
fn minimum_casting_mana_is_a_generic_untapped_static_modifier() {
    for text in [
        "As long as this artifact is untapped, each spell that would cost less than three mana to cast costs three mana to cast.",
        "As long as this enchantment is untapped, each spell that would cost less than four mana to cast costs four mana to cast.",
    ] {
        let parsed = parse_common_static_ability(text, "Test Permanent")
            .expect("a total casting-mana floor parses independently of permanent type");
        assert_eq!(parsed.rule["modifiers"][0]["kind"], "minimumCastingMana");
        assert_eq!(parsed.rule["modifiers"][0]["whileSourceUntapped"], true);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(parse_common_static_ability(
            "As long as this artifact is untapped, each spell that would cost less than three mana to cast costs four mana to cast.",
            "Test Permanent",
        )
        .is_none());
}

#[test]
fn shared_free_casting_permission_composes_players_filter_cost_and_timing() {
    for text in [
        "Any player may cast creature spells with mana value 3 or less without paying their mana costs and as though they had flash.",
        "Any player may cast artifact spells with mana value two or less without paying their mana costs and as though they had flash.",
    ] {
        let parsed = parse_common_static_ability(text, "Test Permission")
            .expect("a bounded shared casting permission parses");
        let modifier = &parsed.rule["modifiers"][0];
        assert_eq!(modifier["kind"], "castingPermission");
        assert_eq!(modifier["players"]["kind"], "eachPlayer");
        assert_eq!(modifier["where"]["kind"], "and");
        assert_eq!(modifier["withoutPayingManaCost"], true);
        assert_eq!(modifier["asThoughFlash"], true);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_static_ability(
            "Any player may cast spells without paying their mana costs.",
            "Test Permission",
        )
        .is_none()
    );
}

#[test]
fn variable_player_counter_sweep_links_gain_payment_and_mana_value() {
    for text in [
        "You get X {E} (energy counters), then you may pay any amount of {E}. Destroy each artifact, creature, and enchantment with mana value less than or equal to the amount of {E} paid this way.",
        "You get three {E} (energy counters), then you may pay any amount of {E}. Destroy each creature with mana value less than or equal to the amount of {E} paid this way.",
    ] {
        let (effects, _) = parse_general_effect_sequence(text, "Test Sweep")
            .expect("a variable player-counter payment sweep parses");
        assert_eq!(effects[0]["kind"], "addPlayerCounters");
        assert_eq!(effects[1]["kind"], "chooseNumber");
        assert_eq!(effects[2]["kind"], "removePlayerCounters");
        assert_eq!(effects[3]["kind"], "destroyPermanentsByManaValue");
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        })));
    }

    assert!(
        parse_general_effect_sequence("You get X {E}, then destroy all creatures.", "Test Sweep",)
            .is_none()
    );
}

#[test]
fn colors_spent_condition_uses_payment_metadata_and_target_criteria() {
    for text in [
        "Converge \u{2014} Exile target nonland permanent if its mana value is less than or equal to the number of colors of mana spent to cast this spell.",
        "Exile target creature if its mana value is less than or equal to the number of colors of mana spent to cast this spell.",
    ] {
        let (effects, decisions) = parse_general_effect_instruction(text, "Test Spell")
            .expect("a colors-spent guard composes with permanent targeting");
        assert_eq!(effects[0]["kind"], "conditional");
        assert_eq!(
            effects[0]["condition"]["right"]["kind"],
            "colorsOfManaSpentToCastSource"
        );
        assert_eq!(effects[0]["then"][0]["kind"], "exilePermanent");
        assert_eq!(decisions[0]["id"], "targetPermanent");
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "declaration": { "kind": "castingDeclaration", "decisions": decisions },
            "effects": effects,
        })));
    }

    assert!(
        parse_general_effect_instruction(
            "Exile target nonland permanent if you feel like it.",
            "Test Spell",
        )
        .is_none()
    );
}

#[test]
fn per_opponent_cost_or_controller_effect_is_composed_from_shared_grammar() {
    for text in [
        "Whenever this creature attacks, for each opponent, you create a 2/2 black Zombie creature token unless that player sacrifices a creature of their choice.",
        "Whenever this creature attacks, for each opponent, you create a Treasure token unless that player discards a card.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Attacker")
            .expect("an attack trigger can offer each opponent a card cost");
        assert_eq!(parsed.rule["event"]["kind"], "declaredAttacker");
        assert_eq!(
            parsed.rule["effects"][0]["kind"],
            "forEachOpponentPaysCostOrControllerEffect"
        );
        assert_eq!(
            parsed.rule["effects"][0]["otherwise"][0]["kind"],
            "createTokens"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_general_effect_instruction(
            "For each opponent, maybe create something.",
            "Test Attacker",
        )
        .is_none()
    );
}

#[test]
fn targeted_sacrifice_discard_and_life_sequence_composes_shared_primitives() {
    for text in [
        "Whenever this creature enters or attacks, target opponent sacrifices a creature or planeswalker of their choice, discards a card, and loses 3 life. You draw a card and gain 3 life.",
        "Whenever Test Horror attacks, target player sacrifices two artifacts of their choice, discards two cards, then loses X life.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Horror")
            .expect("the multi-effect target-player sequence parses");
        assert!(matches!(
            parsed.rule["event"]["kind"].as_str(),
            Some("oneOf" | "declaredAttacker")
        ));
        assert_eq!(parsed.rule["effects"][0]["kind"], "sacrificePermanents");
        assert_eq!(parsed.rule["effects"][1]["kind"], "discardCards");
        assert_eq!(parsed.rule["effects"][2]["kind"], "loseLife");
        assert_eq!(
            parsed.rule["declaration"]["decisions"]
                .as_array()
                .expect("the trigger declares its target")
                .last()
                .and_then(|decision| decision["id"].as_str()),
            Some("targetPlayer")
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_general_effect_instruction(
            "Target opponent sacrifices perhaps a creature, discards a card, and loses 3 life.",
            "Test Horror",
        )
        .is_none()
    );
}

#[test]
fn targeted_temporary_protection_then_explore_preserves_attacking_criteria() {
    for text in [
        "Whenever you attack, target attacking Cleric, Rogue, Warrior, or Wizard gains protection from creatures until end of turn. It explores.",
        "Whenever you attack, target attacking creature gains protection from red and from black until end of turn. That creature explores.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Dungeoneer")
            .expect("temporary protection followed by explore parses");
        assert_eq!(parsed.rule["effects"][0]["kind"], "grantProtection");
        assert_eq!(parsed.rule["effects"][1]["kind"], "explore");
        assert!(contains_rule_kind(
            &parsed.rule["declaration"]["decisions"][0]["candidates"]["where"],
            "isAttacking",
        ));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
            parse_general_effect_instruction(
                "Target attacking creature gains protection from the color of your choice until end of turn. It explores.",
                "Test Dungeoneer",
            )
            .is_none()
        );
}

#[test]
fn optional_combat_damage_mill_binds_cards_for_a_filtered_hand_choice() {
    for text in [
        "Whenever this creature deals combat damage to a player, you may mill that many cards. If you do, you may put a creature card from among them into your hand.",
        "Whenever Test Goyf deals combat damage to a player, you may mill that many cards. If you do, you may put an artifact creature card from among them into your hand.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Goyf")
            .expect("combat-damage mill and filtered recovery parse");
        assert_eq!(parsed.rule["event"]["kind"], "combatDamageToPlayer");
        assert_eq!(parsed.rule["effects"][0]["kind"], "optionalAction");
        assert_eq!(
            parsed.rule["effects"][0]["action"]["count"]["decisionId"],
            "damageAmount"
        );
        assert_eq!(
            parsed.rule["effects"][0]["onPerformed"][0]["from"]["binding"],
            "milledCards"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_general_effect_instruction(
            "You may mill some cards. If you do, take whichever card you want.",
            "Test Goyf",
        )
        .is_none()
    );
}

#[test]
fn delayed_blink_can_conditionally_reward_its_source_and_reuses_target_criteria() {
    for text in [
        "Whenever Test Shepherd attacks, exile up to one other target nonland permanent. At the beginning of the next end step, return that card to the battlefield under its owner's control. If it entered under your control, put a +1/+1 counter on Test Shepherd.",
        "Whenever Test Shepherd attacks, exile another target creature. Return that card to the battlefield under its owner's control at the beginning of the next end step. If it entered under your control, put a shield counter on Test Shepherd.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Shepherd")
            .expect("a delayed blink with a source reward parses");
        assert_eq!(parsed.rule["effects"][0]["kind"], "exileUntilNextEndStep");
        assert!(parsed.rule["effects"][0]["sourceCounterIfReturnedUnderController"].is_string());
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
            true
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let linked = parse_composed_entry_triggered(
            "When this creature enters, exile up to one target nonland, nontoken permanent you don't control with mana value 4 or less.",
        )
        .expect("embedded controller and comma-separated negative criteria parse");
    assert_eq!(
        linked.rule["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "opponentsOf"
    );
    assert!(contains_rule_kind(
        &linked.rule["declaration"]["decisions"][0]["candidates"]["where"],
        "not"
    ));

    assert!(
        parse_general_effect_instruction(
            "Exile target creature. At the next convenient time, return it and reward somebody.",
            "Test Shepherd",
        )
        .is_none()
    );
}

#[test]
fn optional_entry_discard_is_a_criteria_driven_replacement() {
    for text in [
        "If this artifact would enter, you may discard a land card instead. If you do, put this artifact onto the battlefield. If you don't, put it into its owner's graveyard.",
        "If this enchantment would enter, you may discard a creature card instead. If you do, put this enchantment onto the battlefield. If you don't, put it into its owner's graveyard.",
    ] {
        let parsed = parse_composed_entry_replacement(text, "")
            .expect("an optional entry-discard replacement parses");
        assert_eq!(parsed.rule["kind"], "replacementEffect");
        assert_eq!(
            parsed.rule["decisions"][0]["kind"],
            "chooseCardForReplacement"
        );
        assert_eq!(
            parsed.rule["replacement"][0]["kind"],
            "discardChosenCardOrReplaceEntry"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
            parse_composed_entry_replacement(
                "If this artifact would enter, you may discard a land card instead. If you do, put this creature onto the battlefield. If you don't, put it into its owner's graveyard.",
                "",
            )
            .is_none()
        );
}

#[test]
fn linked_entry_exile_preserves_a_followup_effect_without_swallowing_criteria() {
    for text in [
        "When this enchantment enters, exile target nonland permanent an opponent controls until this enchantment leaves the battlefield. You get {E}{E} (two energy counters).",
        "When this artifact enters, exile up to one another target creature until this artifact leaves the battlefield. You gain three life.",
    ] {
        let parsed = parse_composed_entry_triggered(text)
            .expect("linked exile and its independent followup effect parse");
        assert_eq!(
            parsed.rule["effects"][0]["kind"],
            "exilePermanentWithSource"
        );
        assert_eq!(parsed.rule["effects"][0]["returnWhenSourceLeaves"], true);
        assert_eq!(parsed.rule["effects"].as_array().map(Vec::len), Some(2));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
            parse_composed_entry_triggered(
                "When this enchantment enters, exile target nonland permanent until this artifact leaves the battlefield. You gain three life.",
            )
            .is_none()
        );
}

#[test]
fn per_opponent_linked_entry_exile_uses_distinct_controllers() {
    for text in [
        "When this enchantment enters, for each opponent, exile up to one target nonland permanent that player controls until this enchantment leaves the battlefield.",
        "When this artifact enters, for each opponent, exile up to one target nontoken creature that player controls until this artifact leaves the battlefield.",
    ] {
        let parsed = parse_composed_entry_triggered(text)
            .expect("per-opponent linked exile parses from reusable criteria");
        let decision = &parsed.rule["declaration"]["decisions"][0];
        assert_eq!(decision["kind"], "chooseTargets");
        assert_eq!(decision["maximum"]["kind"], "countOpponents");
        assert_eq!(
            decision["selectionConstraint"]["kind"],
            "distinctPermanentControllers"
        );
        assert_eq!(decision["candidates"]["controller"]["kind"], "opponentsOf");
        assert_eq!(
            parsed.rule["effects"][0]["permanent"]["kind"],
            "chosenTargets"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_composed_entry_triggered(
            "When this enchantment enters, for each opponent, exile up to one target nonland permanent that player controls until this artifact leaves the battlefield.",
        )
        .is_none()
    );
}

#[test]
fn linked_exiled_card_owner_and_mana_value_feed_variable_tokens() {
    for text in [
        "When this creature leaves the battlefield, the exiled card's owner creates an X/X blue Illusion creature token, where X is the mana value of the exiled card.",
        "When this artifact leaves the battlefield, the exiled card's owner creates an X/X red Elemental creature token, where X is the mana value of the exiled card.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Prison")
            .expect("linked exiled-card owner and mana value parse generically");
        assert_eq!(parsed.rule["event"]["kind"], "permanentLeftBattlefield");
        assert_eq!(parsed.rule["effects"][0]["kind"], "createTokens");
        assert_eq!(parsed.rule["effects"][0]["controller"]["kind"], "ownerOf");
        assert_eq!(
            parsed.rule["effects"][0]["token"]["power"]["kind"],
            "manaValueOf"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
            parse_general_effect_instruction(
                "The exiled card's owner creates an X/X blue Illusion creature token, where X is the power of the exiled card.",
                "Test Prison",
            )
            .is_none()
        );
}

#[test]
fn chosen_name_entry_and_opponent_taxes_share_stored_decision_grammar() {
    for text in [
        "As this creature enters, look at an opponent's hand, then choose any card name.",
        "As Test Artifact enters, choose a card name.",
    ] {
        let parsed = parse_composed_entry_replacement(text, "")
            .expect("the entering card-name choice parses");
        assert_eq!(parsed.rule["decisions"][0]["kind"], "chooseCardName");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let spell_tax = parse_common_static_ability(
        "Spells your opponents cast with the chosen name cost {2} more to cast.",
        "Test Peacekeeper",
    )
    .expect("the opposing chosen-name spell tax parses");
    assert_eq!(
        spell_tax.rule["modifiers"][0]["kind"],
        "additionalCastingCost"
    );
    assert_eq!(
        spell_tax.rule["modifiers"][0]["players"]["kind"],
        "opponentsOf"
    );
    assert!(crate::engine::rule_is_executable(&spell_tax.rule));

    let activation_tax = parse_common_static_ability(
            "Activated abilities of sources with the chosen name cost {2} more to activate unless they're mana abilities.",
            "Test Peacekeeper",
        )
        .expect("the nonmana chosen-name activation tax parses");
    assert_eq!(
        activation_tax.rule["modifiers"][0]["kind"],
        "additionalActivationCost"
    );
    assert!(crate::engine::rule_is_executable(&activation_tax.rule));

    assert!(
        parse_common_static_ability(
            "Activated abilities of sources with the chosen name cost approximately two more.",
            "Test Peacekeeper",
        )
        .is_none()
    );
}

#[test]
fn simultaneous_hand_choices_filter_before_comparing_mana_values() {
    let parsed = parse_simple_spell_ability(
            "Each player chooses a card in their hand. Then each player reveals their chosen card. The owner of each creature card revealed this way with the lowest mana value puts it onto the battlefield.",
        )
        .expect("simultaneous hand choices and the filtered minimum parse");
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "eachPlayerChoosesHandCardThenMoveLowestManaValue"
    );
    assert_eq!(parsed.rule["effects"][0]["where"], card_type("Creature"));
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_general_effect_instruction(
            "Each player points at a card and the cheapest one wins.",
            "",
        )
        .is_none()
    );
}

#[test]
fn zone_specific_entry_triggers_compose_with_optional_hand_replacement() {
    let parsed = parse_expansion_triggered(
            "When this creature enters from your graveyard, you may discard your hand. If you do, draw three cards.",
            "Test Engine",
        )
        .expect("entry from a specified zone and optional hand replacement parse");
    assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(parsed.rule["event"]["fromZone"], "graveyard");
    assert_eq!(parsed.rule["effects"][0]["kind"], "optionalAction");
    assert_eq!(parsed.rule["effects"][0]["action"]["kind"], "discardHand");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let named = parse_expansion_triggered(
            "When Test Engine enters from your exile, you may discard your hand. If you do, draw two cards.",
            "Test Engine",
        )
        .expect("a named source can use the same zone-specific event");
    assert_eq!(named.rule["event"]["fromZone"], "exile");

    let marker = parse_common_static_ability("(Melds with Test Partner.)", "Test Engine")
        .expect("meld partner reminder parses as reusable metadata");
    assert_eq!(marker.rule["kind"], "rulesMarker");
    assert_eq!(marker.rule["meldsWith"], "Test Partner");
    assert!(crate::engine::rule_is_executable(&marker.rule));

    assert!(
        parse_expansion_triggered(
            "When Another Engine enters from your graveyard, draw a card.",
            "Test Engine",
        )
        .is_none()
    );
}

#[test]
fn chosen_name_reveal_until_sequence_tracks_every_destination() {
    let parsed = parse_simple_spell_ability(
            "Choose a card name. Reveal cards from the top of your library until you reveal a card with that name, then put that card into your hand. Exile all other cards revealed this way, and you lose 1 life for each of the exiled cards.",
        )
        .expect("chosen-name reveal-until sequence parses");
    assert_eq!(parsed.rule["effects"][0]["kind"], "chooseCardName");
    assert_eq!(parsed.rule["effects"][1]["kind"], "revealUntilChosenName");
    assert_eq!(parsed.rule["effects"][1]["matchingDestination"], "hand");
    assert_eq!(parsed.rule["effects"][1]["otherDestination"], "exile");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_general_effect_instruction(
            "Choose a card name. Look around until you happen to see it.",
            "",
        )
        .is_none()
    );
}

#[test]
fn opponent_library_searches_compose_selection_movement_and_permissions() {
    let acquire = parse_simple_spell_ability(
            "Search target opponent's library for an artifact card and put that card onto the battlefield under your control. Then that player shuffles.",
        )
        .expect("a typed opponent-library search to the battlefield parses");
    assert_eq!(acquire.rule["effects"][0]["kind"], "chooseCards");
    assert_eq!(
        acquire.rule["effects"][0]["candidates"]["where"]["value"],
        "Artifact"
    );
    assert_eq!(acquire.rule["effects"][1]["to"]["kind"], "battlefield");
    assert_eq!(acquire.rule["effects"][2]["kind"], "shuffleZone");
    assert!(crate::engine::rule_is_executable(&acquire.rule));

    let grasp = parse_simple_spell_ability(
            "Search target opponent's library for a card and exile it face down. Then that player shuffles. You may play that card for as long as it remains exiled.",
        )
        .expect("an opponent-library search with a linked exile permission parses");
    assert_eq!(grasp.rule["effects"][1]["to"]["kind"], "exile");
    assert_eq!(grasp.rule["effects"][1]["to"]["faceDown"], true);
    assert_eq!(grasp.rule["effects"][3]["kind"], "grantCardPermission");
    assert_eq!(
        grasp.rule["effects"][3]["play"]["castingModifier"]["kind"],
        "none"
    );
    assert!(crate::engine::rule_is_executable(&grasp.rule));
}

#[test]
fn control_sacrifice_blink_and_attack_tax_families_are_generic() {
    let agent = parse_expansion_triggered(
        "When this creature enters, gain control of target permanent.",
        "Agent of Treachery",
    )
    .expect("generic enter-the-battlefield control parses");
    assert_eq!(agent.rule["effects"][0]["kind"], "gainControlPermanent");

    let empress = parse_simple_activated_ability(
            "{U}{U}, {T}: Gain control of target legendary permanent. (This effect lasts indefinitely.)",
        )
        .expect("generic activated control parses");
    assert_eq!(empress.rule["effects"][0]["kind"], "gainControlPermanent");

    let altar = parse_simple_activated_ability(
        "Sacrifice a creature: Target player mills cards equal to the sacrificed creature's power.",
    )
    .expect("a sacrificed permanent can bind its power for the effect");
    assert_eq!(altar.rule["costs"][0]["bindPowerAs"], "sacrificedPower");
    assert_eq!(altar.rule["effects"][0]["count"]["kind"], "decisionResult");

    let marauder = parse_expansion_triggered(
        "When this creature enters, each player sacrifices a nontoken creature of their choice.",
        "Accursed Marauder",
    )
    .expect("each player choosing a sacrifice parses");
    assert_eq!(
        marauder.rule["effects"][0]["kind"],
        "sacrificePermanentsEachPlayer"
    );

    let felidar = parse_expansion_triggered(
            "When this creature enters, you may exile another target permanent you control, then return that card to the battlefield under its owner's control.",
            "Felidar Guardian",
        )
        .expect("an optional other-permanent blink parses");
    assert_eq!(felidar.rule["effects"][0]["kind"], "blinkPermanent");
    assert_eq!(felidar.rule["declaration"]["decisions"][0]["minimum"], 0);

    let restoration = parse_expansion_triggered(
            "When this creature enters, you may exile target non-Angel creature you control, then return that card to the battlefield under your control.",
            "Restoration Angel",
        )
        .expect("a blink returning under the ability controller parses");
    assert_eq!(
        restoration.rule["effects"][0]["controller"]["kind"],
        "controllerOf"
    );

    let titan = parse_expansion_triggered(
            "Whenever this creature enters or attacks, you may return target permanent card with mana value 3 or less from your graveyard to the battlefield.",
            "Sun Titan",
        )
        .expect("a source entering-or-attacking event and filtered reanimation parse");
    assert_eq!(titan.rule["event"]["kind"], "oneOf");
    assert_eq!(
        titan.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );

    let prison = parse_common_static_ability(
            "Creatures can't attack you unless their controller pays {2} for each creature they control that's attacking you.",
            "Ghostly Prison",
        )
        .expect("generic per-attacker tax parses");
    assert_eq!(prison.rule["modifiers"][0]["kind"], "attackTax");
    assert_eq!(prison.rule["modifiers"][0]["amount"]["value"], 2);

    let agent_value = parse_expansion_triggered(
            "At the beginning of your end step, if you control three or more permanents you don't own, draw three cards.",
            "Agent of Treachery",
        )
        .expect("a threshold of controlled but not owned permanents parses");
    assert_eq!(
        agent_value.rule["effects"][0]["condition"]["left"]["ownership"],
        "notOwned"
    );

    let vega = parse_expansion_triggered(
        "Whenever you cast a spell from anywhere other than your hand, draw a card.",
        "Vega, the Watcher",
    )
    .expect("a cast-from-nonhand-zone trigger parses");
    assert_eq!(vega.rule["event"]["fromZoneNot"], "hand");

    let frantic = parse_simple_spell_ability(
        "Draw two cards, then discard two cards. Untap up to three lands.",
    )
    .expect("independent ordered sentences compose into one spell ability");
    assert_eq!(frantic.rule["effects"][0]["kind"], "drawThenDiscard");
    assert_eq!(frantic.rule["effects"][1]["kind"], "untapPermanents");

    let shriekmaw = parse_expansion_triggered(
        "When this creature enters, destroy target nonartifact, nonblack creature.",
        "Shriekmaw",
    )
    .expect("multiple negative permanent criteria parse generically");
    assert_eq!(
        shriekmaw.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );

    let yahenni = parse_simple_activated_ability(
        "Sacrifice another creature: Yahenni gains indestructible until end of turn.",
    )
    .expect("a named ability source can gain a temporary keyword");
    assert_eq!(yahenni.rule["effects"][0]["keyword"], "indestructible");

    let launderer_connive = parse_expansion_triggered(
        "Whenever another nontoken creature you control dies, this creature connives.",
        "Body Launderer",
    )
    .expect("another controlled nontoken death trigger composes with connive");
    assert_eq!(launderer_connive.rule["event"]["nontoken"], true);
    assert_eq!(launderer_connive.rule["effects"][0]["kind"], "connive");

    let yahenni_growth = parse_expansion_triggered(
        "Whenever a creature an opponent controls dies, put a +1/+1 counter on Yahenni.",
        "Yahenni, Undying Partisan",
    )
    .expect("an opposing creature death trigger parses without a card exception");
    assert_eq!(yahenni_growth.rule["event"]["kind"], "opponentCreatureDied");

    let force = parse_counter_spell(
            "Counter target noncreature spell. If that spell is countered this way, exile it instead of putting it into its owner's graveyard.",
        )
        .expect("countering with an exile replacement parses");
    assert_eq!(force.rule["effects"][0]["exileInstead"], true);

    let damn = parse_simple_spell_ability(
        "Destroy target creature. A creature destroyed this way can't be regenerated.",
    )
    .expect("destroying without regeneration parses");
    assert_eq!(damn.rule["effects"][0]["cannotRegenerate"], true);

    let enchant_enchantment = parse_common_static_ability("Enchant enchantment", "")
        .expect("enchant supports any permanent card type criterion");
    let control_enchantment = parse_common_static_ability("You control enchanted enchantment.", "")
        .expect("control follows a valid attached permanent");
    assert_eq!(enchant_enchantment.rule["ability"]["kind"], "enchant");
    assert_eq!(
        control_enchantment.rule["modifiers"][0]["kind"],
        "controlAttachedPermanent"
    );

    let final_parting = parse_simple_spell_ability(
            "Search your library for two cards. Put one into your hand and the other into your graveyard. Then shuffle.",
        )
        .expect("a two-card search can split cards across distinct destinations");
    assert_eq!(final_parting.rule["effects"][1]["to"]["kind"], "hand");
    assert_eq!(final_parting.rule["effects"][3]["to"]["kind"], "graveyard");

    for (index, rule) in [
        agent.rule,
        empress.rule,
        altar.rule,
        marauder.rule,
        felidar.rule,
        restoration.rule,
        titan.rule,
        prison.rule,
        agent_value.rule,
        vega.rule,
        frantic.rule,
        shriekmaw.rule,
        yahenni.rule,
        launderer_connive.rule,
        yahenni_growth.rule,
        force.rule,
        damn.rule,
        enchant_enchantment.rule,
        control_enchantment.rule,
        final_parting.rule,
    ]
    .into_iter()
    .enumerate()
    {
        assert!(
            crate::engine::rule_is_executable(&rule),
            "generic family rule {index} is not executable: {rule}"
        );
    }
}

#[test]
fn hand_and_library_value_patterns_parse_without_card_names() {
    let tutor = parse_simple_spell_ability(
        "Search your library for a card, then shuffle and put that card on top. You lose 2 life.",
    )
    .expect("a search-to-top followed by life loss composes");
    assert_eq!(tutor.rule["effects"][0]["kind"], "chooseCards");
    assert_eq!(tutor.rule["effects"][1]["kind"], "shuffleZone");
    assert_eq!(tutor.rule["effects"][2]["to"]["position"], "top");
    assert_eq!(tutor.rule["effects"][3]["kind"], "loseLife");
    assert!(crate::engine::rule_is_executable(&tutor.rule));

    let wheel = parse_simple_spell_ability(
            "Each player discards their hand, then draws cards equal to the greatest number of cards a player discarded this way.",
        )
        .expect("a greatest-discarded-count wheel parses");
    assert_eq!(
        wheel.rule["effects"][0]["kind"],
        "discardHandThenDrawEachPlayer"
    );
    assert_eq!(
        wheel.rule["effects"][0]["count"]["kind"],
        "greatestHandSize"
    );
    assert!(crate::engine::rule_is_executable(&wheel.rule));

    let revealed = parse_common_static_ability(
        "Your opponents play with their hands revealed.",
        "Telepathy",
    )
    .expect("a continuous opposing-hand reveal parses");
    assert_eq!(revealed.rule["modifiers"][0]["kind"], "revealHands");
    assert!(crate::engine::rule_is_executable(&revealed.rule));

    let protected = parse_keyword_ability("Flying, protection from black", "Karmic Guide")
        .expect("a mixed keyword and protection group parses");
    assert_eq!(protected.rule["kind"], "staticAbility");
    assert_eq!(protected.rule["modifiers"][0]["keyword"], "flying");
    assert_eq!(protected.rule["modifiers"][1]["kind"], "grantProtection");
    assert_eq!(protected.rule["modifiers"][1]["from"], json!(["black"]));
    assert!(crate::engine::rule_is_executable(&protected.rule));

    let echo = parse_keyword_ability(
            "Echo {3}{W}{W} (At the beginning of your upkeep, if this came under your control since the beginning of your last upkeep, sacrifice it unless you pay its echo cost.)",
            "Karmic Guide",
        )
        .expect("echo parses as a conditional upkeep payment");
    assert_eq!(echo.rule["event"]["step"], "upkeep");
    assert_eq!(echo.rule["condition"]["kind"], "sourceNeedsEcho");
    assert_eq!(echo.rule["effects"][0]["manaCost"], "{3}{W}{W}");
    assert!(crate::engine::rule_is_executable(&echo.rule));
}

#[test]
fn opponent_hand_control_trigger_uses_zone_permissions_and_player_restrictions() {
    let parsed = parse_expansion_triggered(
            "At the beginning of your upkeep, choose target opponent. This turn, that player can't cast spells or activate abilities and plays with their hand revealed. You may play lands and cast spells from that player's hand this turn.",
            "Test Controller",
        )
        .expect("an upkeep hand-control trigger parses generically");
    assert_eq!(parsed.rule["event"]["kind"], "stepBegan");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "targetOpponent"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "restrictPlayerActions");
    assert_eq!(parsed.rule["effects"][1]["kind"], "revealHand");
    assert_eq!(parsed.rule["effects"][2]["kind"], "grantHandPlayPermission");
}

#[test]
fn counter_followups_share_the_same_stack_target_grammar() {
    let offer = parse_counter_spell(
            "Counter target noncreature spell. Its controller creates two Treasure tokens. (They're artifacts with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        )
        .expect("counter followed by controller token creation parses");
    assert_eq!(
        offer.rule["effects"][0]["controllerCreatesTokens"]["quantity"],
        integer(2)
    );

    let venture = parse_counter_spell(
            "Counter target creature or planeswalker spell. Venture into the dungeon. (Enter the first room or advance to the next room.)",
        )
        .expect("counter followed by venture parses");
    assert_eq!(venture.rule["effects"][1]["kind"], "ventureDungeon");

    let silence =
        parse_counter_spell("Counter target spell. Its controller can't cast spells this turn.")
            .expect("counter followed by a controller restriction parses");
    assert_eq!(
        silence.rule["effects"][0]["prohibitControllerSpellsThisTurn"],
        true
    );

    let drain = parse_counter_spell(
            "Counter target spell. At the beginning of your next main phase, add an amount of {C} equal to that spell's mana value.",
        )
        .expect("counter followed by delayed mana parses");
    assert_eq!(drain.rule["effects"][0]["addManaAtNextMainPhase"], "C");
}

#[test]
fn new_deck_common_families_parse_to_executable_rules() {
    let tapped_tokens =
        parse_simple_spell_ability("Create thirteen tapped 2/2 black Zombie creature tokens.")
            .expect("a word-sized quantity of tapped tokens parses");
    assert_eq!(tapped_tokens.rule["effects"][0]["quantity"], integer(13));
    assert_eq!(tapped_tokens.rule["effects"][0]["tapped"], true);

    let paid_entry = parse_shock_land_replacement(
        "As this land enters, you may pay 3 life. If you don't, it enters tapped.",
    )
    .expect("an arbitrary optional life payment on entry parses");
    assert_eq!(
        paid_entry.rule["decisions"][0]["cost"]["amount"],
        integer(3)
    );

    let death_baron = parse_common_static_ability(
        "Skeletons you control and other Zombies you control get +1/+1 and have deathtouch.",
        "Death Baron",
    )
    .expect("multiple controlled selectors share a bonus and keyword");
    assert_eq!(
        death_baron.rule["modifiers"].as_array().map(Vec::len),
        Some(4)
    );

    let opposing_penalty = parse_common_static_ability(
        "Creatures your opponents control get -2/-2.",
        "Elesh Norn, Grand Cenobite",
    )
    .expect("opposing permanents receive a static penalty");
    assert_eq!(
        opposing_penalty.rule["modifiers"][0]["objects"]["controller"]["kind"],
        "opponentsOf"
    );

    let blackblade = parse_common_static_ability(
        "Equipped creature gets +1/+1 for each land you control.",
        "Blackblade Reforged",
    )
    .expect("an attached permanent receives a counted bonus");
    assert_eq!(
        blackblade.rule["modifiers"][0]["power"]["kind"],
        "countPermanents"
    );

    let sword = parse_common_static_ability(
        "Equipped creature gets +2/+2 and has protection from black and from green.",
        "Sword of Feast and Famine",
    )
    .expect("an attached permanent receives multi-quality protection");
    assert_eq!(
        sword.rule["modifiers"][1]["from"],
        json!(["black", "green"])
    );

    let urborg = parse_common_static_ability(
        "Each land is a Swamp in addition to its other land types.",
        "Urborg, Tomb of Yawgmoth",
    )
    .expect("all lands can gain any basic-land subtype");
    assert_eq!(urborg.rule["modifiers"][0]["subtype"], "Swamp");

    let modal = parse_general_modal_spell(
            "Choose two â€”\nâ€¢ Destroy all artifacts.\nâ€¢ Destroy all enchantments.\nâ€¢ Destroy all creatures with mana value 3 or less.\nâ€¢ Destroy all creatures with mana value 4 or greater.",
        )
        .expect("a generic choose-two spell parses every mode");
    assert_eq!(modal.rule["declaration"]["decisions"][0]["minimum"], 2);
    assert_eq!(modal.rule["effects"].as_array().map(Vec::len), Some(4));

    let farewell = parse_general_modal_spell(
            "Choose one or more â€”\nâ€¢ Exile all artifacts.\nâ€¢ Exile all creatures.\nâ€¢ Exile all enchantments.\nâ€¢ Exile all graveyards.",
        )
        .expect("a choose-one-or-more spell can include all graveyards");
    assert_eq!(
        farewell.rule["effects"][3]["then"][0]["kind"],
        "exileAllGraveyards"
    );

    let zombie_cast = parse_expansion_triggered(
        "Whenever you cast a Zombie spell, create a tapped 2/2 black Zombie creature token.",
        "Diregraf Colossus",
    )
    .expect("a controlled subtype spell trigger composes with tapped token creation");
    assert_eq!(
        zombie_cast.rule["event"]["where"]["kind"],
        "subtypeContains"
    );

    let creature_died = parse_expansion_triggered(
        "Whenever a creature you control dies, draw a card.",
        "Liliana, Dreadhorde General",
    )
    .expect("a generic controlled permanent death trigger parses");
    assert_eq!(creature_died.rule["event"]["kind"], "permanentDied");

    let reminder_static = parse_common_static_ability(
            "Skeletons you control and other Zombies you control get +1/+1 and have deathtouch. (Any amount of damage they deal to a creature is enough to destroy it.)",
            "Death Baron",
        )
        .expect("reminder text does not hide a reusable static ability");

    let authority = parse_common_static_ability(
        "Creatures your opponents control enter tapped.",
        "Authority of the Consuls",
    )
    .expect("opposing matching permanents can enter tapped");
    assert_eq!(
        authority.rule["modifiers"][0]["kind"],
        "permanentsEnterTapped"
    );

    let black_market = parse_expansion_triggered(
            "At the beginning of your first main phase, choose one or more â€”\nâ€¢ Sell Contraband â€” Create a Treasure token. You lose 1 life.\nâ€¢ Buy Information â€” Draw a card. You lose 2 life.\nâ€¢ Hire a Mercenary â€” Create a 3/2 colorless Shapeshifter creature token with changeling. You lose 3 life. (It is every creature type.)",
            "Black Market Connections",
        )
        .expect("modal choices nested in a first-main-phase trigger parse");
    assert_eq!(black_market.rule["event"]["step"], "precombatMain");
    assert_eq!(
        black_market.rule["declaration"]["decisions"][0]["kind"],
        "chooseModes"
    );

    let bojuka = parse_expansion_triggered(
        "When this land enters, exile target player's graveyard.",
        "Bojuka Bog",
    )
    .expect("a targeted graveyard-exile entry trigger parses");
    assert_eq!(bojuka.rule["effects"][0]["kind"], "exilePlayerGraveyard");

    let map_search = parse_general_effect_instruction(
            "Search your library for up to two basic Plains cards, reveal them, put them into your hand, then shuffle.",
            "Archaeomancer's Map",
        )
        .expect("counted subtype searches to hand parse");
    assert_eq!(map_search.0[0]["maximum"], json!(2));

    let gift_search = parse_general_effect_instruction(
            "If an opponent controls more lands than you, search your library for up to three Plains cards, reveal them, put them into your hand, then shuffle.",
            "Gift of Estates",
        )
        .expect("land-advantage conditions compose with library searches");
    assert_eq!(gift_search.0[0]["kind"], "conditionalEffect");

    let animated_land = parse_simple_activated_ability(
            "{1}: This land becomes a 1/1 Phyrexian Blinkmoth artifact creature with flying and infect until end of turn. It's still a land.",
        )
        .expect("source lands animate with reusable characteristics");
    assert_eq!(animated_land.rule["effects"][0]["kind"], "becomeCreature");
    assert_eq!(
        animated_land.rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );

    let reprieve =
        parse_general_effect_instruction("Return target spell to its owner's hand.", "Reprieve")
            .expect("spell bounce targets a spell rather than a permanent");
    assert_eq!(reprieve.1[0]["candidates"]["where"]["kind"], "isSpell");

    let geier = parse_general_effect_instruction(
        "Each player draws a card, then discards a card.",
        "Geier Reach Sanitarium",
    )
    .expect("each-player draw-discard parses as one ordered effect");
    assert_eq!(geier.0[0]["kind"], "drawEachPlayerThenDiscard");

    let ten_cards = parse_common_static_ability("Your maximum hand size is ten.", "The Ten Rings")
        .expect("a numeric maximum hand size parses");
    assert_eq!(ten_cards.rule["modifiers"][0]["amount"], integer(10));

    let sword_trigger = parse_expansion_triggered(
            "Whenever equipped creature deals combat damage to a player, that player discards a card and you untap all lands you control.",
            "Sword of Feast and Famine",
        )
        .expect("equipped-creature combat damage composes discard and untap");
    assert_eq!(
        sword_trigger.rule["event"]["kind"],
        "attachedPermanentCombatDamageToPlayer"
    );
    assert_eq!(
        sword_trigger.rule["effects"][1]["kind"],
        "untapPermanentsMatching"
    );

    let diregraf_entry = parse_common_static_ability(
        "This creature enters with a +1/+1 counter on it for each Zombie card in your graveyard.",
        "Diregraf Colossus",
    )
    .expect("graveyard card counts can define entering counters");
    assert_eq!(
        diregraf_entry.rule["replacement"][0]["count"]["kind"],
        "countCards"
    );

    let endless = parse_expansion_triggered(
            "At the beginning of your upkeep, create X 2/2 black Zombie creature tokens, where X is half the number of Zombies you control, rounded down.",
            "Endless Ranks of the Dead",
        )
        .expect("half a controlled subtype count can size token creation");
    assert_eq!(endless.rule["effects"][0]["quantity"]["kind"], "divide");

    let x_entry = parse_expansion_triggered(
            "When Wan Shi Tong enters, put X +1/+1 counters on him. Then draw half X cards, rounded down.",
            "Wan Shi Tong, Librarian",
        )
        .expect("source cast X composes counters followed by a rounded-down draw");
    assert_eq!(x_entry.rule["effects"][0]["kind"], "putCounters");
    assert_eq!(
        x_entry.rule["effects"][0]["count"]["kind"],
        "sourceCastXValue"
    );
    assert_eq!(x_entry.rule["effects"][1]["kind"], "drawCards");
    assert_eq!(x_entry.rule["effects"][1]["count"]["kind"], "divide");
    assert_eq!(
        x_entry.rule["effects"][1]["count"]["left"]["kind"],
        "sourceCastXValue"
    );

    let field = parse_expansion_triggered(
            "Whenever this land or another land you control enters, if you control seven or more lands with different names, create a 2/2 black Zombie creature token.",
            "Field of the Dead",
        )
        .expect("distinct controlled land names can gate a trigger");
    assert_eq!(
        field.rule["effects"][0]["condition"]["left"]["kind"],
        "countDistinctPermanentNames"
    );

    let merchant = parse_expansion_triggered(
            "When this creature enters, each opponent loses X life, where X is your devotion to black. You gain life equal to the life lost this way. (Each {B} in the mana costs of permanents you control counts toward your devotion to black.)",
            "Gray Merchant of Asphodel",
        )
        .expect("devotion can size a multiplayer drain");
    assert_eq!(merchant.rule["effects"][0]["kind"], "drainEachOpponent");

    let necroduality = parse_expansion_triggered(
            "Whenever a nontoken Zombie you control enters, create a token that's a copy of that creature.",
            "Necroduality",
        )
        .expect("a nontoken subtype entry can copy its triggering permanent");
    assert_eq!(
        necroduality.rule["effects"][0]["object"]["kind"],
        "triggeringPermanent"
    );

    let massacre = parse_expansion_triggered(
        "When The Meathook Massacre enters, each creature gets -X/-X until end of turn.",
        "The Meathook Massacre",
    )
    .expect("each-creature variable stat changes parse");
    assert_eq!(massacre.rule["effects"][0]["kind"], "modifyPowerToughness");

    let deluge =
        parse_simple_spell_ability("As an additional cost to cast this spell, pay X life.")
            .expect("variable life can be an additional spell cost");
    assert_eq!(
        deluge.rule["declaration"]["additionalCosts"][0]["amount"]["kind"],
        "sourceCastXValue"
    );

    let apocalypse = parse_simple_spell_ability(
            "Return all Zombie creature cards from your graveyard to the battlefield tapped, then destroy all Humans.",
        )
        .expect("bulk graveyard returns compose with global destruction");
    assert_eq!(apocalypse.rule["effects"][0]["kind"], "moveCards");

    let living_weapon = parse_keyword_ability("Living weapon", "Kaldra Compleat")
        .expect("living weapon does not require reminder text");
    assert_eq!(living_weapon.rule["ability"]["kind"], "livingWeapon");

    let liliana = parse_simple_activated_ability(
        "+1: Create a 2/2 black Zombie creature token. Mill two cards.",
    )
    .expect("generic loyalty costs compose with ordinary effects");
    assert_eq!(liliana.rule["costs"][0]["kind"], "payLoyalty");

    let restricted_equip = parse_special_static_ability("Equip legendary creature {3}")
        .expect("equip accepts reusable target criteria before its cost");
    assert_eq!(restricted_equip.rule["ability"]["where"]["kind"], "and");

    let multiplayer_draw = parse_simple_spell_ability(
            "Each opponent draws a card, then you draw a card for each opponent who drew a card this way.",
        )
        .expect("multiplayer dependent draws stay one ordered effect");
    assert_eq!(
        multiplayer_draw.rule["effects"][0]["kind"],
        "drawEachOpponentThenControllerForEach"
    );

    let combat_damage = parse_simple_spell_ability(
        "Razorgrass Ambush deals 3 damage to target attacking or blocking creature.",
    )
    .expect("named spell sources can damage a criteria-qualified permanent");
    assert_eq!(
        combat_damage.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );

    let coup = parse_simple_spell_ability(
            "Create X 1/1 white Soldier creature tokens. If X is 5 or more, destroy all other creatures.",
        )
        .expect("variable tokens bind before conditional global destruction");
    assert_eq!(coup.rule["effects"][0]["bind"], "createdTokens");
    assert_eq!(
        coup.rule["effects"][1]["then"][0]["permanent"]["exclude"]["kind"],
        "boundObjects"
    );

    let foretell_zone = parse_keyword_ability(
            "(After you foretell a card, you can place the exiled card here. You may cast it on a later turn for its foretell cost.)",
            "",
        )
        .expect("foretell game-piece zone reminder is recognized");
    let poison_loss = parse_keyword_ability(
        "(A player with ten or more poison counters loses the game.)",
        "",
    )
    .expect("poison game-piece reminder is recognized");

    let sidisi = parse_expansion_triggered(
            "When Sidisi exploits a creature, you may search your library for a card, put it into your hand, then shuffle.",
            "Sidisi, Undead Vizier",
        )
        .expect("named exploit events reuse the generic exploit trigger");
    assert_eq!(sidisi.rule["event"]["kind"], "creatureExploited");

    let bottom_on_death = parse_expansion_triggered(
        "When this creature dies, put it on the bottom of its owner's library.",
        "Murderous Rider",
    )
    .expect("self death can move the triggering card to library bottom");
    assert_eq!(bottom_on_death.rule["effects"][0]["to"], "libraryBottom");

    let granted_undying = parse_common_static_ability(
            "Other non-Human creatures you control get +1/+1 and have undying. (When a creature with undying dies, if it had no +1/+1 counters on it, return it to the battlefield under its owner's control with a +1/+1 counter on it.)",
            "Mikaeus, the Unhallowed",
        )
        .expect("static lords can grant undying through shared keyword grammar");
    assert_eq!(granted_undying.rule["modifiers"][1]["keyword"], "undying");

    let global_swampwalk = parse_common_static_ability(
            "Other Zombie creatures have swampwalk. (They can't be blocked as long as defending player controls a Swamp.)",
            "Zombie Master",
        )
        .expect("global other-subtype keyword grants parse");
    assert_eq!(
        global_swampwalk.rule["modifiers"][0]["keyword"],
        "swampwalk"
    );

    let filtered_attacker = parse_expansion_triggered(
            "Whenever a Zombie token you control with power 6 or greater attacks, it gains lifelink until end of turn.",
            "Dreadhorde Invasion",
        )
        .expect("attacker power and permanent criteria compose");
    assert_eq!(
        filtered_attacker.rule["effects"][0]["kind"],
        "grantKeywordToAttackingPermanents"
    );

    for rule in [
        tapped_tokens.rule,
        paid_entry.rule,
        death_baron.rule,
        opposing_penalty.rule,
        blackblade.rule,
        sword.rule,
        urborg.rule,
        modal.rule,
        farewell.rule,
        zombie_cast.rule,
        creature_died.rule,
        reminder_static.rule,
        authority.rule,
        black_market.rule,
        bojuka.rule,
        animated_land.rule,
        ten_cards.rule,
        sword_trigger.rule,
        diregraf_entry.rule,
        endless.rule,
        field.rule,
        merchant.rule,
        necroduality.rule,
        massacre.rule,
        deluge.rule,
        apocalypse.rule,
        living_weapon.rule,
        liliana.rule,
        restricted_equip.rule,
        multiplayer_draw.rule,
        combat_damage.rule,
        coup.rule,
        foretell_zone.rule,
        poison_loss.rule,
        sidisi.rule,
        bottom_on_death.rule,
        granted_undying.rule,
        global_swampwalk.rule,
        filtered_attacker.rule,
    ] {
        assert!(
            crate::engine::rule_is_executable(&rule),
            "new-deck family must be executable: {rule}"
        );
    }
}

#[test]
fn wipes_remaining_oracle_families_parse_to_executable_rules() {
    let cases = [
        (
            "spellAbility",
            "Approach of the Second Sun",
            "If this spell was cast from your hand and you've cast another spell named Approach of the Second Sun this game, you win the game. Otherwise, put Approach of the Second Sun into its owner's library seventh from the top and you gain 7 life.",
        ),
        (
            "triggeredAbility",
            "Dismantling Wave",
            "When you cycle this card, destroy all artifacts and enchantments.",
        ),
        (
            "spellAbility",
            "Sevinne's Reclamation",
            "Return target permanent card with mana value 3 or less from your graveyard to the battlefield. If this spell was cast from a graveyard, you may copy this spell and may choose a new target for the copy.",
        ),
    ];

    for (ability_kind, face_name, text) in cases {
        let parsed = parse_generalized_zone_and_combat_ability(text, ability_kind, face_name)
            .unwrap_or_else(|| panic!("{face_name} should parse through a reusable family"));
        assert!(
            crate::engine::rule_is_executable(&parsed.rule),
            "{face_name} should produce an executable rule: {}",
            parsed.rule
        );
    }
}

#[test]
fn legacy_staple_families_parse_to_executable_rules() {
    let rules = [
            parse_keyword_ability("({C} represents colorless mana.)", "")
                .expect("standalone symbol glossary parses")
                .rule,
            parse_keyword_ability("({B/P} can be paid with either {B} or 2 life.)", "")
                .expect("phyrexian symbol glossary parses")
                .rule,
            parse_composed_entry_replacement(
                "As this artifact enters, choose a card name.",
                "",
            )
            .expect("entering card-name choice parses")
            .rule,
            parse_common_static_ability(
                "Spells with the chosen name cost {3} more to cast.",
                "",
            )
            .expect("chosen-name spell tax parses")
            .rule,
            parse_common_static_ability("Noncreature spells cost {1} more to cast.", "")
                .expect("noncreature spell tax parses")
                .rule,
            parse_common_static_ability(
                "Each player can't cast more than one noncreature spell each turn.",
                "",
            )
            .expect("per-turn spell limit parses")
            .rule,
            parse_common_static_ability(
                "Activated abilities of artifacts, creatures, and planeswalkers can't be activated.",
                "",
            )
            .expect("multi-type activation prohibition parses")
            .rule,
            parse_common_static_ability(
                "If a nontoken creature would enter and it wasn't cast, exile it instead.",
                "",
            )
            .expect("uncast creature entry replacement parses")
            .rule,
            parse_counter_unless_paid("Counter target spell unless its controller pays {1}.")
                .expect("plain counter-unless-paid parses")
                .rule,
            parse_common_spell_ability(
                "Target player reveals their hand. You choose a nonland card from it. That player discards that card. You lose 2 life.",
            )
            .expect("targeted hand disruption parses")
            .rule,
            parse_alternative_cost_ability(
                "If it's not your turn, you may exile a blue card from your hand rather than pay this spell's mana cost.",
            )
            .expect("turn-conditional hand exile alternative parses")
            .rule,
            parse_alternative_cost_ability(
                "You may return an Island you control to its owner's hand rather than pay this spell's mana cost.",
            )
            .expect("controlled permanent return alternative parses")
            .rule,
            parse_alternative_cost_ability(
                "If an opponent cast three or more spells this turn, you may pay {0} rather than pay this spell's mana cost.",
            )
            .expect("opponent spell-count alternative parses")
            .rule,
            parse_ancient_vendetta(
                "Choose target card in a graveyard other than a basic land card. Search its owner's graveyard, hand, and library for any number of cards with the same name as that card and exile them. Then that player shuffles.",
            )
            .expect("targeted same-name multi-zone search parses")
            .rule,
        ];

    for rule in rules {
        assert!(
            crate::engine::rule_is_executable(&rule),
            "Legacy grammar family must be executable: {rule}"
        );
    }
}

#[test]
fn legacy_targets_costs_searches_and_stack_objects_use_shared_grammars() {
    assert!(create_token_effect("Create a 1/1 white Monk creature token with prowess.").is_some());
    assert!(parse_general_effect_sequence(
            "create a 1/1 white Monk creature token with prowess. You may attach this Equipment to it.",
            "",
        )
        .is_some());
    let ordinal_trigger = strip_short_oracle_label(
        "Flurry \u{2014} Whenever you cast your second spell each turn, create a 1/1 white Monk creature token with prowess. You may attach this Equipment to it.",
    );
    assert!(parse_expansion_trigger_event(ordinal_trigger, "").is_some());
    let rules = [
            parse_simple_activated_ability(
                "Discard a land card: This creature deals 3 damage to any target.",
            )
            .expect("typed discard costs delegate to card criteria")
            .rule,
            parse_simple_activated_ability(
                "Channel — {1}{R}, Discard this card: It deals 2 damage to any target.",
            )
            .expect("ability-word labels do not own activated-cost parsing")
            .rule,
            parse_simple_activated_ability(
                "{7}, {T}: Search your library for a colorless creature card, reveal it, put it into your hand, then shuffle.",
            )
            .expect("search criteria compose color and card type")
            .rule,
            parse_simple_activated_ability(
                "{1}{W}, {T}: You may put an Equipment card from your hand onto the battlefield.",
            )
            .expect("optional hand-to-battlefield choices use generic criteria")
            .rule,
            parse_common_static_ability("Spells cost {1} more to cast.", "")
                .expect("unqualified spell taxes parse")
                .rule,
            parse_common_static_ability(
                "Each spell costs {3} more to cast except during its controller's turn.",
                "",
            )
            .expect("off-turn taxes compose a generic turn condition")
            .rule,
            parse_common_static_ability("Nonbasic lands are Mountains.", "")
                .expect("land subtype setting uses a reusable subtype modifier")
                .rule,
            parse_common_static_ability(
                "Threshold — As long as there are seven or more cards in your graveyard, this creature gets +1/+1 and can't block.",
                "",
            )
            .expect("threshold bonuses compose count conditions and modifiers")
            .rule,
            parse_common_static_ability(
                "Enchanted creature gets +1/+0 and has lifelink and ward {2}.",
                "",
            )
            .expect("attached keyword lists delegate ward to cost grammar")
            .rule,
            parse_simple_spell_ability(
                "Counter target activated or triggered ability. (Mana abilities can't be targeted.)",
            )
            .expect("stack ability criteria accept reusable alternatives")
            .rule,
            parse_simple_spell_ability(
                "Destroy target creature or planeswalker. Its controller may search their library for a basic land card, put it onto the battlefield tapped, then shuffle.",
            )
            .expect("destroyed permanent controller may perform a linked search")
            .rule,
            parse_simple_activated_ability(
                "{T}, Sacrifice this land: Destroy target land. Its controller may search their library for a basic land card, put it onto the battlefield, then shuffle.",
            )
            .expect("linked searches compose with activated costs")
            .rule,
            parse_expansion_triggered(
                "When this artifact enters, exile target card from a graveyard.",
                "",
            )
            .expect("unqualified graveyard-card targets parse")
            .rule,
            parse_simple_activated_ability(
                "{T}, Sacrifice this artifact: Exile each opponent's graveyard.",
            )
            .expect("opponent graveyards reuse global graveyard exile")
            .rule,
            parse_expansion_triggered(
                "Magecraft — Whenever you cast or copy an instant or sorcery spell, each opponent loses 1 life and you gain 1 life.",
                "",
            )
            .expect("cast-or-copy events compose with filtered spell criteria")
            .rule,
            parse_expansion_triggered(
                "When this artifact enters or leaves the battlefield, you may tap or untap target creature.",
                "",
            )
            .expect("source event alternatives include leaving the battlefield")
            .rule,
            parse_expansion_triggered(
                "Whenever Test Serpent deals combat damage to a player, create four 3/3 blue Serpent creature tokens named Test Coil.",
                "Test Serpent",
            )
            .expect("named source combat damage uses the source-reference grammar")
            .rule,
            parse_mana_ability(
                "Metalcraft — {T}: Add one mana of any color. Activate only if you control three or more artifacts.",
            )
            .expect("mana ability labels and counted activation conditions compose")
            .rule,
            parse_common_spell_ability(
                "Target opponent reveals their hand. You choose a noncreature, nonland card from it. That player discards that card.",
            )
            .expect("hand disruption can require an opponent")
            .rule,
            parse_expansion_triggered(
                "When this creature enters, target opponent reveals their hand. You choose a nonland card from it and exile that card.",
                "",
            )
            .expect("triggered hand extraction reuses criteria and zone changes")
            .rule,
            parse_expansion_triggered(
                "At the beginning of each of your main phases, if you haven't added mana with this ability this turn, you may add X mana of any one color, where X is the number of Islands target opponent controls.",
                "",
            )
            .expect("main-phase mana triggers count a targeted opponent's matching permanents")
            .rule,
            parse_common_static_ability("Each player can't cast more than one spell each turn.", "")
                .expect("unqualified per-turn spell limits parse")
                .rule,
            parse_common_static_ability(
                "Islands don't untap during their controllers' untap steps.",
                "",
            )
            .expect("untap restrictions delegate their permanent criteria")
            .rule,
            parse_simple_spell_ability(
                "Destroy up to two target artifacts and/or enchantments.",
            )
            .expect("multi-target zone changes use reusable target cardinality")
            .rule,
            parse_simple_spell_ability(
                "Return up to three target land cards from your graveyard to your hand.",
            )
            .expect("multiple graveyard targets use reusable zone criteria")
            .rule,
            parse_simple_activated_ability(
                "Discard this card: Exile up to two target cards from graveyards.",
            )
            .expect("graveyard targets are not consumed by battlefield target grammar")
            .rule,
            parse_simple_activated_ability(
                "{T}: Exile up to two target cards from a single graveyard.",
            )
            .expect("target groups can be constrained to one graveyard")
            .rule,
            parse_simple_spell_ability(
                "Each player sacrifices all permanents they control that are one or more colors.",
            )
            .expect("sacrifice-all composes player scope and color criteria")
            .rule,
            parse_simple_spell_ability(
                "Test Source deals damage to each player equal to twice the number of nonbasic lands that player controls.",
            )
            .expect("per-player damage composes arithmetic and permanent criteria")
            .rule,
            parse_simple_activated_ability(
                "{1}: Permanents your opponents control lose hexproof and indestructible until end of turn.",
            )
            .expect("keyword loss uses a reusable permanent selector")
            .rule,
            parse_simple_activated_ability(
                "{T}: You may put a creature card with mana value equal to the number of charge counters on this artifact from your hand onto the battlefield.",
            )
            .expect("source-counter variables compose inside hand-card criteria")
            .rule,
            parse_expansion_triggered(
                "When this creature enters, you may search your library for a creature card with toughness 2 or less, reveal it, put it into your hand, then shuffle.",
                "",
            )
            .expect("search criteria can include characteristic thresholds")
            .rule,
            parse_simple_activated_ability(
                "{1}, Exile this artifact: Exile all graveyards. Draw a card.",
            )
            .expect("source exile composes with an ordered activated-effect sequence")
            .rule,
            parse_simple_activated_ability(
                "{T}: Target player exiles a card from their graveyard.",
            )
            .expect("a targeted player chooses their own graveyard card at resolution")
            .rule,
            parse_simple_activated_ability(
                "{T}: Choose target artifact card in your graveyard. You may cast that card this turn.",
            )
            .expect("a graveyard target can receive normal casting permission")
            .rule,
            promote_activated_mana_ability(
                parse_mana_ability("Exile this card from your hand: Add {R}.")
                    .expect("a source-exile hand cost determines the activation zone"),
            )
            .rule,
            parse_simple_spell_ability(
                "Return target spell or nonland permanent an opponent controls to its owner's hand.",
            )
            .expect("one target can be selected from stack and battlefield candidates")
            .rule,
            parse_simple_spell_ability(
                "Target player discards two cards. That player may copy this spell and may choose a new target for that copy.",
            )
            .expect("a resolving spell can offer its target a copy with new targets")
            .rule,
            parse_simple_spell_ability(
                "Return the top creature card of your graveyard to the battlefield. That creature gains haste until end of turn. Exile it at the beginning of the next end step.",
            )
            .expect("top matching graveyard return keeps its delayed exile")
            .rule,
            parse_simple_triggered_ability(
                "When this land enters untapped, you may put target instant or sorcery card from your graveyard on top of your library.",
            )
            .expect("an enters-untapped condition composes with a graveyard move")
            .rule,
            parse_simple_triggered_ability(
                "Whenever equipped creature becomes tapped, it deals 1 damage to each opponent.",
            )
            .expect("an attachment observes its equipped permanent becoming tapped")
            .rule,
            parse_expansion_triggered(
                "When this Equipment enters, return target creature card with mana value 3 or less from your graveyard to the battlefield and attach this Equipment to it.",
                "",
            )
            .expect("returning a graveyard permanent composes with attachment")
            .rule,
            parse_alternative_cost_ability(
                "If an opponent controls a Plains and you control a Swamp, you may cast this spell without paying its mana cost.",
            )
            .expect("free casting composes two reusable permanent conditions")
            .rule,
            parse_alternative_cost_ability(
                "If this spell is the first spell you've cast this game, you may cast it without paying its mana cost.",
            )
            .expect("first-spell free casting uses the reusable event count")
            .rule,
            parse_common_static_ability(
                "Delirium \u{2014} As long as there are four or more card types among cards in your graveyard, this creature gets +2/+2, has flying, and attacks each combat if able.",
                "",
            )
            .expect("card-type thresholds compose stats, keywords, and attack requirements")
            .rule,
            parse_library_spell(
                "Look at the top five cards of your library. You may reveal a creature or land card from among them and put it into your hand. Put the rest on the bottom of your library in a random order.",
            )
            .expect("optional filtered top-card selection preserves its true cardinality")
            .rule,
            parse_library_spell(
                "Reveal the top four cards of your library. You may put a permanent card from among them into your hand. Put the rest into your graveyard. Create a 0/1 colorless Eldrazi Spawn creature token with \"Sacrifice this token: Add {C}.\"",
            )
            .expect("top-card partitions compose with a trailing token effect")
            .rule,
            parse_library_spell(
                "Look at the top three cards of your library. Put one of them into your hand and the rest on the bottom of your library in any order. If there is an instant card and a sorcery card in your graveyard, instead put two of them into your hand and the rest on the bottom of your library in any order.",
            )
            .expect("conditional top-card cardinality uses graveyard criteria")
            .rule,
            parse_library_spell(
                "Look at the top five cards of your library. Put two of them into your hand and the rest on the bottom of your library in any order.",
            )
            .expect("fixed top-card cardinalities preserve their numeric words")
            .rule,
            parse_expansion_triggered(
                "At the beginning of combat on your turn, target creature you control gains haste and gets +X/+0 until end of turn, where X is the number of Eldrazi you control.",
                "",
            )
            .expect("variable targeted bonuses compose criteria, keywords, and counts")
            .rule,
            parse_expansion_triggered(
                "Whenever this creature deals combat damage to a player, reveal the top three cards of your library. Put all land cards revealed this way into your hand and the rest into your graveyard.",
                "",
            )
            .expect("all matching revealed cards use a filtered object set")
            .rule,
            parse_expansion_triggered(
                "When this creature enters, exile another target permanent. Return that card to the battlefield under its owner's control at the beginning of the next end step.",
                "",
            )
            .expect("delayed blink composes a target and linked end-step return")
            .rule,
            parse_expansion_triggered(
                "Whenever this creature becomes the target of a spell an opponent controls, counter that spell unless its controller discards a card.",
                "",
            )
            .expect("spell-only targeting triggers reuse the cost grammar")
            .rule,
            parse_expansion_triggered(
                "Whenever a player or permanent becomes the target of an ability you control, draw a card. This ability triggers only once each turn.",
                "",
            )
            .expect("controlled-ability targeting composes with the shared trigger limit")
            .rule,
            parse_simple_activated_ability(
                "{T}, Sacrifice this artifact: Look at a card at random in target player's hand. You draw a card at the beginning of the next turn's upkeep.",
            )
            .expect("random hand inspection composes with a delayed step trigger")
            .rule,
            parse_expansion_triggered(
                "Flurry \u{2014} Whenever you cast your second spell each turn, create a 1/1 white Monk creature token with prowess. You may attach this Equipment to it.",
                "",
            )
            .expect("ordinal spell triggers can bind and attach their created token")
            .rule,
            parse_alternative_cost_ability("Evoke\u{2014}Exile a green card from your hand.")
                .expect("evoke delegates its payment to the shared cost grammar")
                .rule,
            parse_expansion_triggered(
                "When this creature enters, up to one target player puts all the cards from their graveyard on the bottom of their library in a random order.",
                "Endurance",
            )
            .expect("a target player's full graveyard can move to library bottom")
            .rule,
            parse_expansion_triggered(
                "When this creature enters, exile up to one other target creature. That creature's controller gains life equal to its power.",
                "Solitude",
            )
            .expect("optional other-creature exile preserves the target's live power")
            .rule,
            parse_simple_activated_ability(
                "{W}: Exile target card from a graveyard. If it was a permanent card, put a +1/+1 counter on this permanent.",
            )
            .expect("graveyard exile can conditionally count a reusable card criterion")
            .rule,
            parse_common_static_ability(
                "Barrowgoyf's power is equal to the number of card types among cards in all graveyards and its toughness is equal to that number plus 1.",
                "Barrowgoyf",
            )
            .expect("graveyard card-type characteristics use a shared numeric expression")
            .rule,
            parse_common_static_ability(
                "Unlicensed Hearse's power and toughness are each equal to the number of cards exiled with it.",
                "Unlicensed Hearse",
            )
            .expect("linked-exile characteristics count objects associated with their source")
            .rule,
            parse_common_static_ability(
                "Equipped creature gets +1/+1 for each +1/+1 counter on this Equipment.",
                "Lion Sash",
            )
            .expect("equipment bonuses can scale from arbitrary source counters")
            .rule,
            parse_common_static_ability(
                "As long as you have one or fewer cards in hand, if you would draw one or more cards, you draw that many cards plus one instead.",
                "",
            )
            .expect("conditional draw replacements compose hand-size conditions and arithmetic")
            .rule,
            parse_common_static_ability(
                "Reconfigure {2} ({2}: Attach to target creature you control; or unattach from a creature. Reconfigure only as a sorcery. While attached, this isn't a creature.)",
                "Lion Sash",
            )
            .expect("reconfigure uses an arbitrary cost parsed by the shared grammar")
            .rule,
            parse_simple_activated_ability(
                "Channel — {3}{U}, Discard this card: Return target artifact, creature, enchantment, or planeswalker to its owner's hand. This ability costs {1} less to activate for each legendary creature you control.",
            )
            .expect("channel reuses hand activation, targeting, and cost-reduction grammars")
            .rule,
            parse_simple_activated_ability(
                "Channel — {1}{G}, Discard this card: Destroy target artifact, enchantment, or nonbasic land an opponent controls. That player may search their library for a land card with a basic land type, put it onto the battlefield, then shuffle. This ability costs {1} less to activate for each legendary creature you control.",
            )
            .expect("affected permanent controllers can perform an optional library search")
            .rule,
            parse_expansion_triggered(
                "At the beginning of your first main phase, sacrifice this enchantment unless you pay {E}.",
                "",
            )
            .expect("step triggers can offer a shared nonmana cost before sacrificing their source")
            .rule,
            parse_common_static_ability(
                "All creatures have \"At the beginning of your upkeep, destroy this creature unless you pay {1}.\"",
                "",
            )
            .expect("global granted triggers reuse criteria, events, costs, and effects")
            .rule,
        ];

    for rule in rules {
        assert!(
            crate::engine::rule_is_executable(&rule),
            "shared Legacy grammar must be executable: {rule}"
        );
    }

    assert!(parse_simple_spell_ability("Counter target activated or copied ability.").is_none());
    assert!(parse_mana_ability("Exile this card from your library: Add {R}.").is_none());
    assert!(
        parse_alternative_cost_ability("Evoke\u{2014}Exile a green card from your library.")
            .is_none()
    );
    assert!(
        parse_alternative_cost_ability(
            "If the weather is pleasant, you may cast this spell without paying its mana cost.",
        )
        .is_none()
    );
    assert!(
            parse_common_static_ability(
                "Delirium \u{2014} As long as there are plenty of card types among cards in your graveyard, this creature gets +2/+2, has flying, and attacks each combat if able.",
                "",
            )
            .is_none()
        );
    assert!(
            parse_library_spell(
                "Look at the top three cards of your library. Put several of them into your hand and the rest on the bottom of your library.",
            )
            .is_none()
        );
    assert!(
        parse_simple_activated_ability(
            "{T}: Exile up to two target cards from neighboring graveyards.",
        )
        .is_none()
    );
    assert!(
            parse_simple_activated_ability(
                "Channel — {3}{U}, Discard this card: Return target creature to its owner's hand. This ability costs {1} less to activate for each legendary creature in exile.",
            )
            .is_none()
        );
    assert!(
        parse_simple_spell_ability(
            "Target player discards some cards. That player may copy this spell.",
        )
        .is_none()
    );
    assert!(
        parse_common_static_ability(
            "As long as you remember one card, if you would draw cards, draw plenty instead.",
            "",
        )
        .is_none()
    );
    assert!(
        parse_expansion_triggered(
            "At some point in your turn, add mana for things an opponent remembers.",
            "",
        )
        .is_none()
    );
}

#[test]
fn token_creation_can_attach_the_source_optionally_or_mandatorily() {
    for text in [
        "When this Equipment enters, create a 2/2 red Dwarf creature token, then attach this Equipment to it.",
        "When this Aura enters, create a 1/1 white Soldier creature token, then attach this Aura to it.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Equipment")
            .expect("mandatory source attachment delegates to token creation");
        assert_eq!(parsed.rule["effects"][0]["kind"], "createTokens");
        assert_eq!(parsed.rule["effects"][0]["bind"], "createdTokens");
        assert_eq!(parsed.rule["effects"][1]["kind"], "attachPermanent");
        assert_eq!(parsed.rule["effects"][1]["to"]["kind"], "boundObjects");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let optional = parse_general_effect_sequence(
        "Create a 1/1 white Monk creature token with prowess. You may attach this Equipment to it.",
        "",
    )
    .expect("optional source attachment remains optional");
    assert_eq!(optional.0[1]["kind"], "optionalEffects");

    assert!(
        parse_general_effect_sequence(
            "Create a 1/1 white Soldier creature token, then attach target Equipment to it.",
            "",
        )
        .is_none()
    );
}

#[test]
fn named_artifact_tokens_parse_embedded_static_and_equip_abilities() {
    assert!(
        parse_special_static_ability("Equipped creature gets +1/+0").is_some(),
        "the embedded static leaf parses"
    );
    assert!(
        parse_special_static_ability("Equip {2}").is_some(),
        "the embedded equip leaf parses"
    );
    assert!(
        create_token_effect(
            "create a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\" and equip {2}.",
        )
        .is_some(),
        "the composed token leaf parses"
    );
    for text in [
        "When this creature enters, create a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\" and equip {2}.",
        "When this creature enters, create a blue Equipment artifact token named Harness with \"Equipped creature gets +0/+2\" and equip {1}{U}.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Smith")
            .expect("named artifact tokens compose embedded static and equip abilities");
        let token = &parsed.rule["effects"][0]["token"];
        assert_eq!(token["types"][0], "Artifact");
        assert_eq!(token["subtypes"][0], "Equipment");
        assert_eq!(token["abilities"][0]["kind"], "staticAbility");
        assert_eq!(token["abilities"][1]["ability"]["kind"], "equip");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_expansion_triggered(
            "When this creature enters, create a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\".",
            "Test Smith",
        )
        .is_none()
    );
}

#[test]
fn created_tokens_can_feed_reflexive_attachment_and_equipped_attacker_filters() {
    let enters = parse_expansion_triggered(
        "When Dáin enters, create a colorless Equipment artifact token named Axe with \"Equipped creature gets +1/+0\" and equip {2}. When you do, attach it to target creature you control.",
        "Dáin Ironfoot",
    )
    .expect("a created Equipment remains bound for its reflexive attachment trigger");
    assert_eq!(enters.rule["effects"][0]["kind"], "createTokens");
    assert_eq!(enters.rule["effects"][0]["bind"], "createdTokens");
    assert_eq!(enters.rule["effects"][1]["kind"], "createReflexiveTrigger");
    assert_eq!(
        enters.rule["effects"][1]["ability"]["effects"][0]["attachment"]["kind"],
        "boundObjects"
    );
    assert_eq!(
        enters.rule["effects"][1]["ability"]["declaration"]["decisions"][0]["candidates"]["controller"]
            ["kind"],
        "controllerOf"
    );
    assert!(crate::engine::rule_is_executable(&enters.rule));

    for (text, expected_state) in [
        (
            "Whenever Dáin attacks, each equipped attacking creature gains double strike until end of turn.",
            "isAttacking",
        ),
        (
            "Whenever Dáin attacks, each equipped blocking creature gains first strike until end of turn.",
            "isBlocking",
        ),
    ] {
        let parsed = parse_expansion_triggered(text, "Dáin Ironfoot")
            .expect("equipped combat-state criteria compose with a shared keyword effect");
        let operands = parsed.rule["effects"][0]["object"]["where"]["operands"]
            .as_array()
            .expect("the criteria are conjunctive");
        assert!(
            operands
                .iter()
                .any(|operand| operand["kind"] == "isEquipped")
        );
        assert!(
            operands
                .iter()
                .any(|operand| operand["kind"] == expected_state)
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_expansion_triggered(
            "When Dáin enters, create an Axe token. When you do, attach it somewhere.",
            "Dáin Ironfoot",
        )
        .is_none()
    );
}

#[test]
fn temporary_source_bonus_scales_with_other_controlled_permanents() {
    for text in [
        "Whenever this creature attacks, it gets +1/+1 until end of turn for each other creature you control.",
        "Whenever Test Construct attacks, Test Construct gets +2/+0 until end of turn for each other artifact you control.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Construct")
            .expect("temporary counted source bonuses parse from permanent criteria");
        let effect = &parsed.rule["effects"][0];
        assert_eq!(effect["kind"], "modifyPowerToughness");
        let count = if effect["power"]["kind"] == "multiply" {
            &effect["power"]["value"]
        } else {
            &effect["power"]
        };
        assert_eq!(count["kind"], "countPermanents");
        assert_eq!(count["excludeSource"], true);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_expansion_triggered(
            "Whenever this creature attacks, it gets +1/+1 until end of turn for each other creature an opponent controls.",
            "Test Construct",
        )
        .is_none()
    );
}

#[test]
fn cards_drawn_threshold_and_another_target_compose_in_combat_triggers() {
    for (condition_text, operator, threshold) in [
        ("you've drawn two or more cards this turn", ">=", 2),
        ("you have drawn three or more cards this turn", ">=", 3),
        ("you've drawn more than one card this turn", ">", 1),
    ] {
        let condition = parse_condition_text(condition_text)
            .expect("cards-drawn thresholds parse through the event-count leaf");
        assert_eq!(condition["operator"], operator);
        assert_eq!(condition["left"]["kind"], "countEventsThisTurn");
        assert_eq!(condition["left"]["event"], "cardDrawn");
        assert_eq!(condition["right"]["value"], threshold);
    }

    let parsed = parse_expansion_triggered(
        "At the beginning of combat on your turn, if you've drawn two or more cards this turn, another target creature you control gets +3/+0 and gains first strike until end of turn.",
        "Test Toymaker",
    )
    .expect("draw-threshold combat trigger composes from shared leaves");
    assert_eq!(parsed.rule["event"]["kind"], "stepBegan");
    assert_eq!(parsed.rule["condition"]["left"]["event"], "cardDrawn");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["candidates"]["excludeSource"], true,
        "parsed rule: {:#}",
        parsed.rule
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(parsed.rule["effects"][1]["keyword"], "firstStrike");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(parse_condition_text("an opponent has drawn two or more cards this turn").is_none());
}

#[test]
fn target_qualified_casting_reduction_reuses_permanent_criteria() {
    for (text, amount, filter_kind) in [
        (
            "This spell costs {3} less to cast if it targets a tapped creature.",
            3,
            "and",
        ),
        (
            "This spell costs {2} less to cast if it targets an artifact.",
            2,
            "cardTypeContains",
        ),
    ] {
        let parsed = parse_common_spell_ability(text)
            .expect("target-qualified reduction parses through casting and criteria leaves");
        let modifier = &parsed.rule["modifiers"][0];
        assert_eq!(modifier["kind"], "reduceOwnGenericCastingCost");
        assert_eq!(modifier["amount"]["value"], amount);
        assert_eq!(modifier["targetWhere"]["kind"], filter_kind);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_spell_ability("This spell costs {3} less to cast if it targets a player.",)
            .is_none()
    );
}

#[test]
fn kicked_owned_target_group_binds_returns_for_the_next_upkeep() {
    for text in [
        "Choose target creature you own. If this spell was kicked, instead choose any number of target creatures you own. Return each chosen creature to your hand. At the beginning of the next upkeep, create a 4/4 white Bird Soldier creature token with flying for each creature returned to your hand this way.",
        "Choose target artifact you own. If this spell was kicked, instead choose any number of target artifacts you own. Return each chosen artifact to your hand. At the beginning of the next upkeep, create a Treasure token for each artifact returned to your hand this way.",
    ] {
        let parsed = parse_simple_spell_ability(text)
            .expect("kicked target expansion and delayed token creation parse generically");
        let decision = &parsed.rule["declaration"]["decisions"][0];
        assert_eq!(decision["candidates"]["ownership"], "owned");
        assert_eq!(decision["minimum"]["kind"], "conditionalValue");
        assert_eq!(decision["maximum"]["kind"], "conditionalValue");
        assert_eq!(parsed.rule["effects"][0]["bind"], "returnedPermanents");
        assert_eq!(
            parsed.rule["effects"][1]["kind"],
            "installDelayedStepTrigger"
        );
        assert_eq!(
            parsed.rule["effects"][1]["effects"][0]["quantity"]["kind"],
            "countObjects"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_simple_spell_ability(
            "Choose target creature you own. If this spell was kicked, instead choose any number of target artifacts you own. Return each chosen creature to your hand. At the beginning of the next upkeep, create a 4/4 white Bird Soldier creature token with flying for each creature returned to your hand this way.",
        )
        .is_none()
    );
}

#[test]
fn linked_permanent_and_graveyard_card_exchange_uses_relational_targets() {
    let parsed = parse_simple_activated_ability(
            "{T}: Choose target artifact a player controls and target artifact card in that player's graveyard. If both targets are still legal as this ability resolves, that player simultaneously sacrifices the artifact and returns the artifact card to the battlefield.",
        )
        .expect("linked battlefield and graveyard targets parse");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["selectionConstraint"]["kind"],
        "zoneOwnerMatchesTargetController"
    );
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "exchangePermanentWithGraveyardCard"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn temporary_colored_spell_protection_composes_as_independent_effects() {
    let parsed = parse_simple_spell_ability(
            "Draw a card if an opponent has cast a blue or black spell this turn. Spells you control can't be countered this turn. You and permanents you control gain hexproof from blue and from black until end of turn. (You and they can't be the targets of blue or black spells or abilities your opponents control.)",
        )
        .expect("conditional draw and temporary colored protection parse");

    assert_eq!(parsed.rule["effects"].as_array().unwrap().len(), 3);
    assert_eq!(
        parsed.rule["effects"][0]["condition"]["kind"],
        "opponentCastSpellWithAnyColor"
    );
    assert_eq!(
        parsed.rule["effects"][1]["kind"],
        "installTemporaryCantBeCountered"
    );
    assert_eq!(
        parsed.rule["effects"][2]["kind"],
        "grantTemporaryHexproofFrom"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn linked_hand_or_controlled_permanent_exile_uses_target_relations() {
    let parsed = parse_expansion_triggered(
            "When Cloak and Dagger enter, choose target opponent and up to one target creature they control. They reveal their hand. You may exile a nonland card from their hand or the chosen creature until Cloak and Dagger leave the battlefield.",
            "Cloak and Dagger, Entwined",
        )
        .expect("linked hand-or-permanent exile parses");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["selectionConstraint"]["kind"],
        "targetControllerMatchesTargetPlayer"
    );
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "chooseExileFromHandOrPermanentUntilSourceLeaves"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn optional_energy_payment_creates_a_targeted_reflexive_trigger() {
    let parsed = parse_expansion_triggered(
            "Whenever you attack, you may pay {E}{E}{E}. When you do, put two +1/+1 counters and a flying counter on target attacking creature. It becomes an Angel in addition to its other types.",
            "Test Guide",
        )
        .expect("optional energy payment and reflexive trigger parse");

    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "optionalPayCostCreateReflexiveTrigger"
    );
    assert_eq!(
        parsed.rule["effects"][0]["cost"]["kind"],
        "payPlayerCounters"
    );
    assert_eq!(
        parsed.rule["effects"][0]["ability"]["effects"][2]["kind"],
        "addSubtypeToPermanent"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn delve_and_cast_payment_dependent_rules_parse_generically() {
    let delve = parse_keyword_ability(
        "Delve (Each card you exile from your graveyard while casting this spell pays for {1}.)",
        "Test Regent",
    )
    .expect("delve parses");
    assert_eq!(delve.rule["ability"]["kind"], "delve");
    assert!(crate::engine::rule_is_executable(&delve.rule));

    let entering = parse_common_static_ability(
            "This creature enters with a +1/+1 counter on it for each instant and sorcery card exiled with it.",
            "Test Regent",
        )
        .expect("cast-payment-dependent entry counters parse");
    assert_eq!(
        entering.rule["replacement"][0]["count"]["kind"],
        "countDecisionCardsMatching"
    );
    assert_eq!(
        entering.rule["replacement"][0]["count"]["where"]["kind"],
        "or"
    );
    assert!(crate::engine::rule_is_executable(&entering.rule));

    let leaves = parse_expansion_triggered(
            "Whenever an instant or sorcery card leaves your graveyard, put a +1/+1 counter on this creature.",
            "Test Regent",
        )
        .expect("filtered graveyard-leave trigger parses");
    assert_eq!(leaves.rule["event"]["kind"], "cardsLeftGraveyard");
    assert_eq!(leaves.rule["event"]["where"]["kind"], "or");
    assert!(crate::engine::rule_is_executable(&leaves.rule));
}

#[test]
fn legacy_modal_spell_effect_lines_use_the_shared_effect_grammar() {
    let escalate = parse_keyword_ability(
        "Escalate—Discard a card. (Pay this cost for each mode chosen beyond the first.)",
        "Test Modal Spell",
    )
    .expect("escalate uses the shared keyword-cost parser");
    assert_eq!(escalate.rule["ability"]["kind"], "escalate");
    assert_eq!(escalate.rule["ability"]["cost"]["kind"], "discardCard");
    assert!(crate::engine::rule_is_executable(&escalate.rule));

    assert!(parse_permanent_criteria("instant or sorcery", "").is_some());
    assert!(Regex::new(
            r"(?i)^Target (player|opponent) reveals their hand\. You choose (?:a|an) (.+?) card from it\. That player discards that card\.$",
        )
        .unwrap()
        .is_match(
            "Target opponent reveals their hand. You choose an instant or sorcery card from it. That player discards that card."
        ));
    for instruction in [
        "Target opponent reveals their hand. You choose an instant or sorcery card from it. That player discards that card.",
        "Target creature gets -2/-2 until end of turn.",
        "Target opponent loses 2 life and you gain 2 life.",
        "Target player creates X 0/1 colorless Eldrazi Spawn creature tokens with \"Sacrifice this token: Add {C}.\"",
        "Target player scries X, then draws a card.",
        "Exile target creature with mana value X or less.",
        "Exile up to X target cards from graveyards.",
        "Exile target nonland permanent, then return it to the battlefield tapped under its owner's control.",
        "Create a 1/1 colorless Eldrazi Scion creature token with \"Sacrifice this token: Add {C}.\"",
    ] {
        assert!(
            parse_general_effect_instruction(instruction, "").is_some(),
            "shared effect grammar did not parse: {instruction}"
        );
    }

    for text in [
        "Choose one or more —\n• Target opponent reveals their hand. You choose an instant or sorcery card from it. That player discards that card.\n• Target creature gets -2/-2 until end of turn.\n• Target opponent loses 2 life and you gain 2 life.",
        "Choose two —\n• Target player creates X 0/1 colorless Eldrazi Spawn creature tokens with \"Sacrifice this token: Add {C}.\"\n• Target player scries X, then draws a card.\n• Exile target creature with mana value X or less.\n• Exile up to X target cards from graveyards.",
    ] {
        let parsed = parse_general_modal_spell(text).expect("legacy modal spell parses");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
        if text.contains("scries X") {
            assert_eq!(
                parsed.rule["declaration"]["decisions"]
                    .as_array()
                    .expect("modal decisions")
                    .iter()
                    .filter(|decision| decision["id"].as_str() == Some("xValue"))
                    .count(),
                1,
                "all selected modes must share the spell's single X declaration"
            );
            assert!(
                !parsed.rule.to_string().contains("mode2:xValue"),
                "mode scoping must not create an independent X value"
            );
        }
    }

    let repeatable = parse_general_modal_spell(
            "Choose three. You may choose the same mode more than once.\n• Target creature gets +3/-3 until end of turn.\n• Exile target nonland permanent, then return it to the battlefield tapped under its owner's control.\n• Create a 1/1 colorless Eldrazi Scion creature token with \"Sacrifice this token: Add {C}.\"",
        )
        .expect("repeatable modes reuse the shared modal and effect grammars");
    assert_eq!(
        repeatable.rule["declaration"]["decisions"][0]["allowRepeated"],
        true
    );
    assert!(
        repeatable
            .rule
            .to_string()
            .contains("mode1:3:targetPermanent")
    );
    assert!(crate::engine::rule_is_executable(&repeatable.rule));
}

#[test]
fn creature_token_abilities_use_the_shared_activated_ability_grammar() {
    for instruction in [
        "Create a 1/1 colorless Eldrazi Scion creature token with \"Sacrifice this token: Add {C}.\"",
        "Create a 1/1 colorless Eldrazi Scion creature token. It has \"Sacrifice this token: Add {C}.\"",
        "Create two 0/1 colorless Eldrazi Spawn creature tokens. They have \"Sacrifice this token: Add {C}.\"",
        "Create two 1/1 colorless Eldrazi Scion creature tokens. They have \"Sacrifice this token: Add {C}.\" ({C} represents colorless mana.)",
        "Create X 0/1 colorless Eldrazi Spawn creature tokens. Those tokens have \"Sacrifice this token: Add {C}.\"",
    ] {
        let effect = create_token_effect(instruction)
            .unwrap_or_else(|| panic!("token instruction did not parse: {instruction}"));
        assert_eq!(effect["kind"], "createTokens");
        assert_eq!(effect["token"]["abilities"][0]["kind"], "manaAbility");
        assert_eq!(
            effect["token"]["abilities"][0]["costs"][0]["kind"],
            "sacrificePermanent"
        );
        assert_eq!(
            effect["token"]["abilities"][0]["effects"][0]["kind"],
            "addMana"
        );
        assert_eq!(effect["token"]["abilities"][0]["effects"][0]["mana"], "{C}");
        let parsed = parse_simple_spell_ability(instruction)
            .unwrap_or_else(|| panic!("token spell instruction did not parse: {instruction}"));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        create_token_effect(
            "Create two 0/1 colorless Eldrazi Spawn creature tokens. It attacks this turn."
        )
        .is_none(),
        "an unrelated follow-up sentence must not be mistaken for a token ability"
    );
}

#[test]
fn sacrifice_activation_costs_use_permanent_criteria_without_the_article() {
    let (costs, cost_decisions) = parse_activation_costs("Sacrifice an Eldrazi Scion")
        .expect("qualified sacrifice cost parses independently");
    assert_eq!(costs[0]["kind"], "sacrificePermanent");
    assert_eq!(cost_decisions[0]["candidates"]["kind"], "permanents");
    assert!(parse_general_effect_instruction("Tap target creature.", "").is_some());

    for text in [
        "Sacrifice an Eldrazi Scion: Tap target creature.",
        "Sacrifice an artifact: Tap target creature.",
        "Sacrifice a creature: Tap target creature.",
        "Sacrifice another Goblin: Tap target creature.",
    ] {
        let parsed = parse_simple_activated_ability(text)
            .unwrap_or_else(|| panic!("qualified sacrifice cost did not parse: {text}"));
        assert_eq!(parsed.rule["costs"][0]["kind"], "sacrificePermanent");
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["candidates"]["kind"],
            "permanents"
        );
        assert_eq!(
            parsed.rule["declaration"]["decisions"][1]["candidates"]["kind"],
            "permanents"
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    let scion = parse_simple_activated_ability("Sacrifice an Eldrazi Scion: Tap target creature.")
        .expect("Scion cost parses through shared permanent criteria");
    assert!(scion.rule.to_string().contains("Eldrazi"));
    assert!(scion.rule.to_string().contains("Scion"));
}

#[test]
fn controlled_permanents_can_share_a_parsed_quoted_mana_ability() {
    for (text, expected_mana, expected_modifiers) in [
        ("Creatures you control have \"{T}: Add {G}.\"", "{G}", 1),
        ("Tokens you control have \"{T}: Add {G}.\"", "{G}", 1),
        (
            "Creatures you control have vigilance and \"{T}: Add one mana of any color. Spend this mana only to cast a creature spell.\"",
            "chooseColor",
            2,
        ),
    ] {
        let parsed = parse_common_static_ability(text, "")
            .unwrap_or_else(|| panic!("quoted granted mana ability did not parse: {text}"));
        let modifiers = parsed.rule["modifiers"]
            .as_array()
            .expect("static modifiers");
        assert_eq!(modifiers.len(), expected_modifiers);
        let mana_modifier = modifiers.last().expect("mana modifier");
        assert_eq!(mana_modifier["kind"], "grantManaAbility");
        if expected_mana == "chooseColor" {
            assert_eq!(mana_modifier["mana"]["kind"], expected_mana);
            assert_eq!(
                mana_modifier["spendRestriction"]["where"]["kind"],
                "cardTypeContains"
            );
        } else {
            assert_eq!(mana_modifier["mana"], expected_mana);
        }
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn color_count_qualifiers_compose_with_permanent_types() {
    for instruction in [
        "Destroy target monocolored creature.",
        "Destroy target multicolored creature.",
    ] {
        let (effects, decisions) = parse_general_effect_instruction(instruction, "")
            .unwrap_or_else(|| panic!("color-count target did not parse: {instruction}"));
        assert_eq!(effects[0]["kind"], "destroyPermanent");
        assert_eq!(decisions[0]["candidates"]["kind"], "permanents");
        assert!(decisions[0].to_string().contains("colorCountOf"));
    }
}

#[test]
fn chosen_creature_type_composes_with_a_controlled_permanent_selector() {
    for text in [
        "Creatures you control of the chosen type get +1/+0.",
        "Artifact creatures you control of the chosen type get +2/-1.",
    ] {
        let parsed = parse_common_static_ability(text, "")
            .unwrap_or_else(|| panic!("chosen-type bonus did not parse: {text}"));
        let selector = &parsed.rule["modifiers"][0]["objects"];
        assert_eq!(selector["kind"], "permanents");
        assert_eq!(selector["where"]["kind"], "and");
        assert!(selector.to_string().contains("chosenCreatureType"));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn metadata_reminders_do_not_create_fake_game_actions() {
    for text in [
        "(Transforms from Test Front.)",
        "Partnerâ€”Father & son (You can have two commanders if both have this ability.)",
    ] {
        let parsed = parse_keyword_ability(text, "Test Back")
            .unwrap_or_else(|| panic!("metadata reminder did not parse: {text}"));
        assert_eq!(parsed.rule["kind"], "rulesMarker");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn variable_target_sets_reuse_permanent_criteria_for_blink_and_phase_out() {
    for instruction in [
        "Exile any number of target creatures you control. Return those cards to the battlefield under their owner's control at the beginning of the next end step.",
        "Any number of target nonland permanents you control phase out.",
    ] {
        let (effects, decisions) = parse_general_effect_sequence(instruction, "")
            .or_else(|| parse_general_effect_instruction(instruction, ""))
            .unwrap_or_else(|| panic!("variable target set did not parse: {instruction}"));
        assert_eq!(decisions[0]["kind"], "chooseTargets");
        assert_eq!(decisions[0]["minimum"], 0);
        assert_eq!(decisions[0]["maximum"], 64);
        assert_eq!(decisions[0]["candidates"]["kind"], "permanents");
        assert!(matches!(
            effects[0]["kind"].as_str(),
            Some("exileUntilNextEndStep" | "phaseOutPermanent")
        ));
    }
}

#[test]
fn source_or_qualified_permanent_death_uses_shared_event_and_effect_grammars() {
    for (text, face_name) in [
        (
            "Whenever Test Scourge or another colorless creature you control dies, you get an experience counter.",
            "Test Scourge",
        ),
        (
            "Whenever Test Chief or another legendary artifact you control dies, you get two quest counters.",
            "Test Chief",
        ),
    ] {
        let parsed = parse_expansion_triggered(text, face_name)
            .unwrap_or_else(|| panic!("qualified death trigger did not parse: {text}"));
        assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
        assert_eq!(parsed.rule["effects"][0]["kind"], "addPlayerCounters");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn controlled_sets_receive_variable_stats_and_parameterized_keywords() {
    let parsed = parse_simple_activated_ability(
        "{W}{U}{B}{R}{G}: Creatures you control get +X/+X until end of turn, where X is the number of experience counters you have. Scions and Spawns you control gain indestructible and annihilator 1 until end of turn.",
    )
    .expect("composed variable bonus and parameterized keyword ability parses");
    assert_eq!(parsed.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert_eq!(parsed.rule["effects"][1]["keyword"], "indestructible");
    assert_eq!(parsed.rule["effects"][2]["keyword"], "annihilator");
    assert_eq!(parsed.rule["effects"][2]["quantity"]["value"], 1);
    for effect in parsed.rule["effects"].as_array().expect("ability effects") {
        let isolated = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": [effect],
        });
        assert!(
            crate::engine::rule_is_executable(&isolated),
            "generated effect is not executable: {effect:#}"
        );
    }
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn spell_cast_thresholds_compose_card_type_and_mana_value_criteria() {
    for text in [
        "Whenever you cast a creature spell with mana value 7 or greater, you gain 4 life.",
        "Whenever you cast an artifact spell with mana value two or less, you may draw a card.",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Permanent")
            .unwrap_or_else(|| panic!("mana-value spell trigger did not parse: {text}"));
        assert_eq!(parsed.rule["event"]["kind"], "spellCast");
        assert_eq!(parsed.rule["event"]["where"]["kind"], "and");
        assert!(parsed.rule["event"].to_string().contains("manaValueOf"));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }
}

#[test]
fn token_creation_events_keep_their_token_criteria() {
    let parsed = parse_expansion_triggered(
        "Whenever you create one or more creature tokens, put a story counter on this artifact.",
        "Test Artifact",
    )
    .expect("qualified token-creation trigger parses");
    assert_eq!(parsed.rule["event"]["kind"], "tokenCreated");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "cardTypeContains");
    assert_eq!(parsed.rule["effects"][0]["kind"], "putCounters");
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn blink_token_and_chosen_type_families_compose_from_shared_primitives() {
    for instruction in [
        "Any number of target nonland permanents you control phase out. (Treat them as though they don't exist until your next turn.)",
        "Exile target artifact or creature. Return it to the battlefield under its owner's control at the beginning of the next end step.",
        "Choose a creature type. Return all creature cards of the chosen type from your graveyard to the battlefield.",
        "Choose a creature type. Draw a card for each permanent you control of that type.",
        "Converge — You draw X cards and lose X life, where X is the number of colors of mana spent to cast this spell.",
        "Put any number of cards from your hand on the bottom of your library, then draw that many cards plus one.",
        "Exile any number of creatures you control, then return them to the battlefield under their owner's control. Then repeat this process X more times.",
        "Choose target creature. You lose 2 life. Until end of turn, that creature gains \"When this creature dies, return it to the battlefield tapped under its owner's control.\"",
    ] {
        let stripped = strip_short_oracle_label(instruction);
        let (effects, decisions) = parse_general_effect_sequence(stripped, "Test Permanent")
            .unwrap_or_else(|| panic!("shared sequence did not parse: {instruction}"));
        let mut rule = json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        });
        if !decisions.is_empty() {
            rule["declaration"] = json!({
                "kind": "castingDeclaration",
                "decisions": decisions,
            });
        }
        assert!(
            crate::engine::rule_is_executable(&rule),
            "shared sequence is not executable: {instruction}\n{rule:#}"
        );
    }
}

#[test]
fn token_selectors_multipliers_and_recurring_triggers_are_generic() {
    for text in [
        "Creature tokens you control have \"{T}: Add one mana of any color.\"",
        "If one or more creature tokens would be created under your control, three times that many of those tokens are created instead.",
    ] {
        let parsed = parse_special_static_ability(text)
            .or_else(|| parse_common_static_ability(text, "Test Permanent"))
            .unwrap_or_else(|| panic!("token static family did not parse: {text}"));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    for text in [
        "At the beginning of each end step, if you created a token this turn, draw a card.",
        "At the beginning of your end step, if you control four or more creatures, transform Test Haven.",
        "When you cast this spell and when this creature dies, create a 0/1 colorless Eldrazi Spawn creature token with \"Sacrifice this token: Add {C}.\"",
        "Whenever this creature deals combat damage to a player, create that many 0/1 colorless Eldrazi Spawn creature tokens. They have \"Sacrifice this token: Add {C}.\"",
    ] {
        let parsed = parse_expansion_triggered(text, "Test Haven")
            .unwrap_or_else(|| panic!("recurring trigger family did not parse: {text}"));
        assert!(
            crate::engine::rule_is_executable(&parsed.rule),
            "recurring trigger is not executable: {text}\n{:#}",
            parsed.rule
        );
    }
}

#[test]
fn counter_scaled_draw_discard_and_followup_damage_compose_generically() {
    let parsed = parse_simple_activated_ability(
        "{3}, {T}: Draw a card for each experience counter you have, then discard a card. Test Sage deals 2 damage to each opponent.",
    )
    .expect("counter-scaled draw, discard, and damage compose through shared instructions");

    assert_eq!(parsed.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(
        parsed.rule["effects"][0]["count"]["kind"],
        "countPlayerCounters"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "discardCards");
    assert_eq!(
        parsed.rule["effects"][2]["kind"],
        "dealDamageToEachOpponent"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn controlled_sacrifice_events_use_permanent_criteria() {
    let parsed = parse_expansion_triggered(
        "Whenever you sacrifice another Eldrazi, put a +1/+1 counter on this creature.",
        "Test Chrysalis",
    )
    .expect("qualified sacrifice trigger parses");
    assert_eq!(parsed.rule["event"]["kind"], "permanentSacrificed");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "subtypeContains");
    assert_eq!(parsed.rule["event"]["excludeSource"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn battlefield_to_graveyard_wording_normalizes_to_a_death_event() {
    let parsed = parse_expansion_triggered(
        "Whenever another artifact or creature you control is put into a graveyard from the battlefield, put an oil counter on this artifact.",
        "Test Vat",
    )
    .expect("battlefield-to-graveyard trigger parses");
    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "or");
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn chosen_creature_type_can_filter_a_death_event() {
    let parsed = parse_expansion_triggered(
        "Whenever a creature of the chosen type dies, you may draw a card.",
        "Test Specialist",
    )
    .expect("chosen-type death trigger parses");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "and");
    assert!(
        parsed.rule["event"]
            .to_string()
            .contains("chosenCreatureType")
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "optionalEffects");
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn companion_and_delayed_mass_blink_parse_as_shared_mechanics() {
    let game_rule = parse_keyword_ability(
            "As each game begins, you can place one card with companion here if your starting deck meets its condition. You may cast it once from here.",
            "Companion",
        )
        .expect("the shared companion rules object parses");
    assert!(crate::engine::rule_is_executable(&game_rule.rule));

    let companion = parse_keyword_ability(
            "Companion — Your starting deck contains at least twenty cards more than the minimum deck size. (If this card is your chosen companion, you may put it into your hand from outside the game for {3} as a sorcery.)",
            "Test Companion",
        )
        .expect("companion condition and acquisition cost parse");
    assert_eq!(
        companion.rule["ability"]["deckCondition"]["count"],
        integer(20)
    );
    assert!(crate::engine::rule_is_executable(&companion.rule));

    let blink = parse_expansion_triggered(
            "When Test Companion enters, exile any number of other nonland permanents you own and control. Return those cards to the battlefield at the beginning of the next end step.",
            "Test Companion",
        )
        .expect("delayed mass blink parses through permanent criteria and choices");
    assert_eq!(blink.rule["effects"][0]["kind"], "choosePermanents");
    assert_eq!(blink.rule["effects"][1]["kind"], "exileUntilNextEndStep");
    assert!(crate::engine::rule_is_executable(&blink.rule));
}

#[test]
fn split_second_and_temporary_quoted_trigger_parse_compositionally() {
    let split_second = parse_keyword_ability(
            "Split second (As long as this spell is on the stack, players can't cast spells or activate abilities that aren't mana abilities.)",
            "Test Reflexes",
        )
        .expect("split second parses as a keyword");
    assert_eq!(split_second.rule["ability"]["kind"], "splitSecond");
    assert!(crate::engine::rule_is_executable(&split_second.rule));

    let splice = parse_keyword_ability(
            "Splice onto Arcane {2}{B} (As you cast an Arcane spell, you may reveal this card from your hand and pay its splice cost. If you do, add this card's effects to that spell.)",
            "Test Vengeance",
        )
        .expect("splice receiver and mana cost parse through shared grammars");
    assert_eq!(splice.rule["ability"]["kind"], "splice");
    assert_eq!(splice.rule["ability"]["onto"]["kind"], "subtypeContains");
    assert_eq!(splice.rule["ability"]["cost"]["manaCost"], "{2}{B}");
    assert!(crate::engine::rule_is_executable(&splice.rule));

    let nonmana_splice = parse_keyword_ability(
            "Splice onto Arcane—Tap an untapped white creature you control. (As you cast an Arcane spell, you may reveal this card from your hand and pay its splice cost. If you do, add this card's effects to that spell.)",
            "Test Strike",
        )
        .expect("nonmana splice cost reuses the shared cost and criteria grammars");
    assert_eq!(nonmana_splice.rule["ability"]["cost"]["kind"], "tap");
    assert_eq!(
        nonmana_splice.rule["ability"]["cost"]["where"]["kind"],
        "and"
    );
    assert!(crate::engine::rule_is_executable(&nonmana_splice.rule));

    let escape = parse_keyword_ability(
            "Escapeâ€”{2}{B}, Exile any number of other cards from your graveyard with four or more card types among them. (You may cast this card from your graveyard for its escape cost.)",
            "Test Goyf",
        )
        .expect("escape composes mana, graveyard selection, and aggregate criteria");
    assert_eq!(escape.rule["ability"]["kind"], "escape");
    assert_eq!(escape.rule["ability"]["costs"][0]["manaCost"], "{2}{B}");
    assert_eq!(
        escape.rule["ability"]["costs"][1]["aggregateCondition"]["count"],
        integer(4)
    );
    assert!(crate::engine::rule_is_executable(&escape.rule));

    let (effects, decisions) = parse_general_effect_instruction(
            "Untap target creature. Until end of turn, it gains hexproof, reach, and \"Whenever this creature becomes tapped, it deals damage equal to its power to up to one target creature.\"",
            "Test Reflexes",
        )
        .expect("untap, keyword list, and quoted trigger compose");
    assert_eq!(decisions.len(), 1);
    assert_eq!(effects[0]["kind"], "untapPermanent");
    assert!(effects.iter().any(|effect| {
        effect["kind"] == "grantAbility" && effect["ability"]["event"]["kind"] == "permanentTapped"
    }));
    let rule = json!({
        "kind": "spellAbility",
        "source": self_ref(),
        "declaration": { "kind": "castingDeclaration", "decisions": decisions },
        "effects": effects,
    });
    assert!(crate::engine::rule_is_executable(&rule));
}

#[test]
fn zone_enchant_and_linked_aura_reanimation_use_shared_rule_primitives() {
    let enchant = parse_common_static_ability("Enchant creature card in a graveyard", "")
        .expect("a zone-scoped enchant restriction parses");
    assert_eq!(enchant.rule["ability"]["kind"], "enchant");
    assert_eq!(enchant.rule["ability"]["zone"]["kind"], "anyGraveyard");
    assert_eq!(enchant.rule["ability"]["where"]["value"], "Creature");
    assert!(crate::engine::rule_is_executable(&enchant.rule));

    let linked = parse_composed_entry_triggered(
            "When this Aura enters, if it's on the battlefield, it loses \"enchant creature card in a graveyard\" and gains \"enchant creature put onto the battlefield with this Aura.\" Return enchanted creature card to the battlefield under your control and attach this Aura to it. When this Aura leaves the battlefield, that creature's controller sacrifices it.",
        )
        .expect("linked Aura reanimation composes from generic zone and ability primitives");
    assert_eq!(linked.rule["condition"]["kind"], "sourceOnBattlefield");
    assert_eq!(linked.rule["effects"][0]["kind"], "replaceAbility");
    assert_eq!(linked.rule["effects"][1]["kind"], "moveCards");
    assert_eq!(linked.rule["effects"][1]["cards"]["kind"], "attachedObject");
    assert_eq!(linked.rule["effects"][2]["kind"], "grantAbility");
    assert!(crate::engine::rule_is_executable(&linked.rule));
}

#[test]
fn activated_ability_sharing_uses_static_filters_and_linked_exile() {
    let flexible_mana = parse_common_static_ability(
            "You may spend mana as though it were mana of any color to activate abilities of creatures you control.",
            "Test Cauldron",
        )
        .expect("activation mana permission parses through permanent criteria");
    assert_eq!(
        flexible_mana.rule["modifiers"][0]["kind"],
        "spendManaAsAnyColorForActivatedAbilities"
    );
    assert!(crate::engine::rule_is_executable(&flexible_mana.rule));

    let shared = parse_common_static_ability(
            "Creatures you control with +1/+1 counters on them have all activated abilities of all creature cards exiled with Test Cauldron.",
            "Test Cauldron",
        )
        .expect("linked-exile ability sharing parses through filters and counters");
    assert_eq!(
        shared.rule["modifiers"][0]["kind"],
        "grantActivatedAbilitiesFromLinkedExile"
    );
    assert_eq!(
        shared.rule["modifiers"][0]["objects"]["where"]["operands"][1]["counter"],
        "+1/+1"
    );
    assert!(crate::engine::rule_is_executable(&shared.rule));
}

#[test]
fn cast_from_hand_energy_cascade_composes_search_and_alternative_cost() {
    let parsed = parse_expansion_triggered(
            "When this creature enters, you get {E}{E} (two energy counters). Then if you cast it from your hand, exile cards from the top of your library until you exile a nonland card. You may cast that card by paying an amount of {E} equal to its mana value rather than paying its mana cost.",
            "Test Raptor",
        )
        .expect("energy cascade parses through shared event, filter, and cost primitives");
    assert_eq!(parsed.rule["effects"][0]["kind"], "addPlayerCounters");
    assert_eq!(parsed.rule["effects"][0]["count"], integer(2));
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["stopWhere"]["kind"],
        "not"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][1]["alternativeCost"]["count"]["kind"],
        "manaValueOfCastCard"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn impending_and_enter_or_attack_value_parse_compositionally() {
    let impending = parse_keyword_ability(
            "Impending 5\u{2014}{1}{B} (If you cast this spell for its impending cost, it enters with five time counters and isn't a creature until the last is removed. At the beginning of your end step, remove a time counter from it.)",
            "Test Overlord",
        )
        .expect("impending parses as a parameterized alternative cost");
    assert_eq!(impending.rule["ability"]["kind"], "impending");
    assert_eq!(impending.rule["ability"]["timeCounters"], integer(5));
    assert_eq!(impending.rule["ability"]["cost"]["manaCost"], "{1}{B}");
    assert!(crate::engine::rule_is_executable(&impending.rule));

    let value_trigger = parse_expansion_triggered(
            "Whenever this permanent enters or attacks, mill four cards, then you may return a non-Avatar creature card or a planeswalker card from your graveyard to your hand.",
            "Test Overlord",
        )
        .expect("enter-or-attack composes mill and an optional filtered graveyard return");
    assert_eq!(value_trigger.rule["event"]["kind"], "oneOf");
    assert_eq!(value_trigger.rule["effects"][0]["kind"], "mill");
    assert_eq!(value_trigger.rule["effects"][1]["kind"], "optionalEffects");
    assert_eq!(
        value_trigger.rule["effects"][1]["effects"][0]["kind"],
        "chooseCards"
    );
    assert_eq!(
        value_trigger.rule["effects"][1]["effects"][1]["kind"],
        "moveCards"
    );
    assert!(crate::engine::rule_is_executable(&value_trigger.rule));
}

#[test]
fn graveyard_destination_replacement_uses_players_and_zones() {
    let parsed = parse_common_static_ability(
        "If a card would be put into an opponent's graveyard from anywhere, exile it instead.",
        "Test Leyline",
    )
    .expect("the graveyard destination replacement parses generically");
    assert_eq!(
        parsed.rule["modifiers"][0]["kind"],
        "replaceGraveyardDestination"
    );
    assert_eq!(
        parsed.rule["modifiers"][0]["graveyardOwners"]["kind"],
        "opponentsOf"
    );
    assert_eq!(parsed.rule["modifiers"][0]["from"]["kind"], "anyZone");
    assert_eq!(parsed.rule["modifiers"][0]["to"]["kind"], "exile");
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn land_play_events_preserve_whether_the_played_land_must_be_another_object() {
    for (oracle_text, excludes_source) in [
        ("When you play another land, draw a card.", true),
        ("Whenever you play a land, draw a card.", false),
    ] {
        let (event, instruction) = parse_expansion_trigger_event(oracle_text, "Test Land")
            .expect("the generic land-play event parses");

        assert_eq!(event["kind"], "landPlayed");
        assert_eq!(event["player"]["kind"], "controllerOf");
        assert_eq!(
            event["excludeSource"].as_bool().unwrap_or(false),
            excludes_source
        );
        assert_eq!(instruction, "draw a card.");
    }

    assert!(
        parse_expansion_trigger_event(
            "Whenever an opponent plays a land, draw a card.",
            "Test Land",
        )
        .is_none()
    );
}

#[test]
fn cast_from_zone_counter_amendment_reuses_counter_and_criteria_leaves() {
    let parsed = parse_simple_spell_ability(
        "Put a +1/+1 counter on target creature you control. If this spell was cast from a graveyard, also put a +1/+1 counter on each other creature you control.",
    )
    .expect("target and conditional group counters compose");

    assert_eq!(parsed.rule["effects"][0]["kind"], "putCounters");
    assert_eq!(
        parsed.rule["effects"][1]["condition"],
        json!({
            "kind": "wasCastFromZone",
            "object": { "kind": "self" },
            "zone": "graveyard",
        })
    );
    let group = &parsed.rule["effects"][1]["then"][0]["permanent"];
    assert_eq!(group["kind"], "eachPermanent");
    assert_eq!(group["where"]["value"], "Creature");
    assert_eq!(group["player"]["kind"], "controllerOf");
    assert_eq!(group["excludeSource"], true);
    assert_eq!(group["exclude"]["kind"], "chosenTargets");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    for (text, zone) in [
        ("this spell was cast from the hand", "hand"),
        ("this spell was cast from exile", "exile"),
        ("this spell was cast from the command zone", "commandZone"),
    ] {
        assert_eq!(parse_condition_text(text).unwrap()["zone"], zone);
    }
    assert!(parse_condition_text("this permanent was cast from a graveyard").is_none());
    assert!(
        parse_general_effect_instruction(
            "Put a +1/+1 counter on each creature an opponent controls.",
            "",
        )
        .is_none()
    );
}

#[test]
fn saga_chapters_delegate_to_shared_effect_leaves() {
    let recruit = parse_generalized_zone_and_combat_ability(
        "I — Recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)",
        "staticAbility",
        "The Test Saga",
    )
    .expect("a recruit chapter delegates to the recruit leaf");
    assert_eq!(recruit.rule["event"]["chapters"], json!([integer(1)]));
    assert_eq!(recruit.rule["effects"][0]["kind"], "recruit");
    assert!(crate::engine::rule_is_executable(&recruit.rule));

    let reanimate = parse_generalized_zone_and_combat_ability(
        "II — Return target creature card with mana value 3 or less from your graveyard to the battlefield.",
        "staticAbility",
        "The Test Saga",
    )
    .expect("a targeted reanimation chapter delegates to graveyard movement leaves");
    assert_eq!(reanimate.rule["event"]["chapters"], json!([integer(2)]));
    assert_eq!(reanimate.rule["effects"][0]["kind"], "moveTargetCard");
    assert_eq!(reanimate.rule["declaration"]["decisions"][0]["minimum"], 1);
    assert!(crate::engine::rule_is_executable(&reanimate.rule));

    let counters = parse_generalized_zone_and_combat_ability(
        "II, III — Put a +1/+1 counter on up to one target creature.",
        "staticAbility",
        "The Test Saga",
    )
    .expect("grouped chapters delegate to the optional target-counter leaf");
    assert_eq!(
        counters.rule["event"]["chapters"],
        json!([integer(2), integer(3)])
    );
    assert_eq!(counters.rule["effects"][0]["kind"], "putCounters");
    assert_eq!(counters.rule["declaration"]["decisions"][0]["minimum"], 0);
    assert!(crate::engine::rule_is_executable(&counters.rule));

    assert!(
        parse_generalized_zone_and_combat_ability(
            "XI — Recruit.",
            "staticAbility",
            "The Test Saga",
        )
        .is_none()
    );
    assert!(
        parse_generalized_zone_and_combat_ability("I — Recruit.", "spellAbility", "The Test Saga",)
            .is_none()
    );
}

#[test]
fn abbreviated_source_name_composes_with_enduring_story_static_modifiers() {
    let parsed = parse_common_static_ability(
        "As long as you have an enduring story, Ori gets +1/+0 and has vigilance.",
        "Ori, Keeper of Songs",
    )
    .expect("an abbreviated source name uses the generic source-reference leaf");

    assert_eq!(parsed.rule["modifiers"][0]["kind"], "modifyPowerToughness");
    assert_eq!(parsed.rule["modifiers"][0]["power"], integer(1));
    assert_eq!(parsed.rule["modifiers"][1]["kind"], "grantKeyword");
    assert_eq!(parsed.rule["modifiers"][1]["keyword"], "vigilance");
    assert!(
        parsed.rule["modifiers"]
            .as_array()
            .unwrap()
            .iter()
            .all(|modifier| modifier["condition"]["kind"] == "hasEnduringStory")
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_common_static_ability(
            "Someone Else gets +1/+0 and has vigilance.",
            "Ori, Keeper of Songs",
        )
        .is_none()
    );
}

#[test]
fn conditional_self_untap_restriction_reuses_the_condition_tree() {
    let parsed = parse_common_static_ability(
        "Bombur doesn't untap during your untap step unless you have an enduring story.",
        "Bombur, Gentle Dreamer",
    )
    .expect("a named source and an unless condition compose");

    assert_eq!(parsed.rule["modifiers"][0]["kind"], "doesNotUntap");
    assert_eq!(parsed.rule["modifiers"][0]["objects"]["kind"], "self");
    assert_eq!(parsed.rule["modifiers"][0]["condition"]["kind"], "not");
    assert_eq!(
        parsed.rule["modifiers"][0]["condition"]["operand"]["kind"],
        "hasEnduringStory"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let unconditional = parse_common_static_ability(
        "This creature doesn't untap during your untap step.",
        "Test Sleeper",
    )
    .expect("the unconditional leaf remains available");
    assert!(
        unconditional.rule["modifiers"][0]
            .get("condition")
            .is_none()
    );

    assert!(
        parse_common_static_ability(
            "Another creature doesn't untap during your untap step unless you have an enduring story.",
            "Bombur, Gentle Dreamer",
        )
        .is_none()
    );
}

#[test]
fn opponent_filtered_spell_ordinal_composes_with_recruit() {
    let parsed = parse_expansion_triggered(
        "Whenever an opponent casts their first noncreature spell each turn, you recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)",
        "The Test Queen",
    )
    .expect("opponent, ordinal, filter, and recruit leaves compose");

    assert_eq!(parsed.rule["event"]["kind"], "spellCast");
    assert_eq!(parsed.rule["event"]["opponentOfSourceController"], true);
    assert_eq!(parsed.rule["event"]["spellCastOrdinal"], integer(1));
    assert_eq!(parsed.rule["event"]["where"]["kind"], "not");
    assert_eq!(
        parsed.rule["event"]["where"]["operand"]["value"],
        "Creature"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "recruit");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(parse_permanent_criteria("nonmystery", "").is_none());
    assert!(
        parse_expansion_triggered(
            "Whenever an opponent casts their first noncreature spell each game, you recruit.",
            "The Test Queen",
        )
        .is_none()
    );
}

#[test]
fn any_player_spell_ordinal_composes_with_generic_effects_and_filters() {
    let lotho = parse_expansion_triggered(
        "Whenever a player casts their second spell each turn, you lose 1 life and create a Treasure token. (It's an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        "Test Shirriff",
    )
    .expect("an any-player ordinal spell trigger composes with life loss and token creation");
    assert_eq!(lotho.rule["event"]["kind"], "spellCast");
    assert_eq!(lotho.rule["event"]["anyPlayer"], true);
    assert!(lotho.rule["event"]["where"].is_null());
    assert_eq!(lotho.rule["event"]["spellCastOrdinal"], integer(2));
    assert_eq!(lotho.rule["effects"][0]["kind"], "loseLife");
    assert_eq!(lotho.rule["effects"][1]["kind"], "createTokens");
    assert_eq!(lotho.rule["effects"][1]["token"]["name"], "Treasure");
    assert!(crate::engine::rule_is_executable(&lotho.rule));

    let filtered = parse_expansion_triggered(
        "Whenever a player casts their third artifact spell each turn, draw a card.",
        "Test Observer",
    )
    .expect("the same event leaf delegates a spell filter to the criteria parser");
    assert_eq!(filtered.rule["event"]["spellCastOrdinal"], integer(3));
    assert_eq!(filtered.rule["event"]["where"]["kind"], "cardTypeContains");
    assert_eq!(filtered.rule["event"]["where"]["value"], "Artifact");

    assert!(
        parse_expansion_triggered(
            "Whenever each player casts their second spell each turn, draw a card.",
            "Test Observer",
        )
        .is_none()
    );
}

#[test]
fn controlled_filtered_combat_damage_and_ring_temptation_events_compose() {
    let army = parse_expansion_triggered(
        "Whenever an Army you control deals combat damage to a player, the Ring tempts you.",
        "Test Dark Lord",
    )
    .expect("controlled subtype combat damage composes with Ring temptation");
    assert_eq!(
        army.rule["event"]["kind"],
        "controlledCreaturesCombatDamageToPlayer"
    );
    assert_eq!(army.rule["event"]["where"]["kind"], "subtypeContains");
    assert_eq!(army.rule["event"]["where"]["value"], "Army");
    assert_eq!(army.rule["effects"][0]["kind"], "ringTemptsPlayer");
    assert!(crate::engine::rule_is_executable(&army.rule));

    let dragons = parse_expansion_triggered(
        "Whenever one or more Dragons you control deal combat damage to a player, draw a card.",
        "Test Observer",
    )
    .expect("plural controlled subtype combat damage uses the same event leaf");
    assert_eq!(dragons.rule["event"]["where"]["value"], "Dragon");

    let tempted = parse_expansion_triggered(
        "Whenever the Ring tempts you, you may discard your hand. If you do, draw four cards.",
        "Test Dark Lord",
    )
    .expect("Ring temptation composes with the existing optional hand replacement effect");
    assert_eq!(tempted.rule["event"]["kind"], "ringTempted");
    assert_eq!(tempted.rule["effects"][0]["kind"], "optionalAction");
    assert_eq!(tempted.rule["effects"][0]["action"]["kind"], "discardHand");
    assert_eq!(
        tempted.rule["effects"][0]["onPerformed"][0]["kind"],
        "drawCards"
    );
    assert_eq!(
        tempted.rule["effects"][0]["onPerformed"][0]["count"],
        integer(4)
    );
    assert!(crate::engine::rule_is_executable(&tempted.rule));

    assert!(
        parse_expansion_triggered("Whenever a Ring tempts you, draw a card.", "Test Observer",)
            .is_none()
    );
}

#[test]
fn temporary_control_untap_and_keyword_sequence_preserves_one_target() {
    let parsed = parse_expansion_triggered(
        "When Test Eye enters, gain control of target creature an opponent controls until end of turn. Untap it. It gains haste until end of turn.",
        "Test Eye",
    )
    .expect("temporary control, untap, and haste share the original target");
    assert_eq!(parsed.rule["effects"][0]["kind"], "gainControlPermanent");
    assert_eq!(
        parsed.rule["effects"][0]["duration"]["kind"],
        "untilEndOfCurrentTurn"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "untapPermanent");
    assert_eq!(parsed.rule["effects"][2]["kind"], "grantKeyword");
    assert_eq!(parsed.rule["effects"][2]["keyword"], "haste");
    for effect in parsed.rule["effects"].as_array().expect("effect list") {
        assert_eq!(
            effect["permanent"].as_str().or(effect["object"].as_str()),
            None
        );
        let reference = effect.get("permanent").unwrap_or(&effect["object"]);
        assert_eq!(reference["id"], "targetPermanent");
    }
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let artifact = parse_expansion_triggered(
        "When this artifact enters, gain control of target artifact an opponent controls until end of turn. Untap it. It gains indestructible until end of turn.",
        "Test Device",
    )
    .expect("a distinct target criterion and keyword use the same linked sequence");
    assert_eq!(artifact.rule["effects"][2]["keyword"], "indestructible");

    assert!(parse_expansion_triggered(
        "When Test Eye enters, gain control of target creature an opponent controls until end of turn. Untap them. They gain haste until end of turn.",
        "Test Eye",
    )
    .is_none());
}

#[test]
fn linked_exile_and_temporary_attack_saga_reuse_shared_leaves() {
    let search = parse_generalized_zone_and_combat_ability(
        "I — Search your library for up to two basic Plains cards, exile them, then shuffle. You gain 2 life.",
        "staticAbility",
        "The Test Road",
    )
    .expect("linked search and life gain compose in a Saga chapter");
    assert_eq!(search.rule["effects"][0]["kind"], "searchLibrary");
    assert_eq!(search.rule["effects"][0]["destination"], "exile");
    assert_eq!(search.rule["effects"][0]["linkToSource"], true);
    assert_eq!(search.rule["effects"][1]["kind"], "gainLife");
    assert!(crate::engine::rule_is_executable(&search.rule));

    let retrieve = parse_generalized_zone_and_combat_ability(
        "II, III — Put a card exiled with this Saga into its owner's hand.",
        "staticAbility",
        "The Test Road",
    )
    .expect("linked exile selection and owner destination compose");
    assert_eq!(retrieve.rule["effects"][0]["kind"], "chooseCards");
    assert_eq!(
        retrieve.rule["effects"][0]["from"]["kind"],
        "cardsExiledWithSource"
    );
    assert_eq!(retrieve.rule["effects"][1]["to"]["kind"], "ownersHand");
    assert!(crate::engine::rule_is_executable(&retrieve.rule));

    let attack = parse_generalized_zone_and_combat_ability(
        "IV — Whenever you attack this turn, target creature you control gets +1/+1 until end of turn for each Plains you control.",
        "staticAbility",
        "The Test Road",
    )
    .expect("temporary attack trigger delegates its nested target effect");
    let install = &attack.rule["effects"][0];
    assert_eq!(install["kind"], "installAttackTrigger");
    assert_eq!(
        install["declaration"]["decisions"][0]["id"],
        "targetPermanent"
    );
    assert_eq!(install["effects"][0]["power"]["kind"], "countPermanents");
    assert_eq!(install["effects"][0]["power"]["where"]["value"], "Plains");
    assert!(crate::engine::rule_is_executable(&attack.rule));

    assert!(
        parse_general_effect_instruction(
            "Search your library for basic Plains cards, exile them, then shuffle.",
            "",
        )
        .is_none()
    );
}

#[test]
fn target_player_group_exile_binds_variable_library_search_maximum() {
    let oracle = "Exile all attacking creatures target player controls. That player may search their library for that many basic land cards, put those cards onto the battlefield tapped, then shuffle.";
    let first_instruction = parse_general_effect_instruction(
        "Exile all attacking creatures target player controls.",
        "",
    )
    .expect("the group-exile leaf parses independently");
    assert_eq!(first_instruction.0[0]["bind"], "exiledPermanents");
    let sequence = parse_general_effect_sequence(oracle, "")
        .expect("the linked target-player sequence composes");
    assert_eq!(sequence.0.len(), 2);

    let parsed =
        parse_simple_spell_ability(oracle).expect("group exile and its variable search compose");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "targetPlayer"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "exilePermanent");
    assert_eq!(parsed.rule["effects"][0]["bind"], "exiledPermanents");
    assert_eq!(
        parsed.rule["effects"][0]["permanent"]["where"]["kind"],
        "and"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "searchLibrary");
    assert_eq!(parsed.rule["effects"][1]["maximum"]["kind"], "countObjects");
    assert_eq!(parsed.rule["effects"][1]["player"]["id"], "targetPlayer");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_general_effect_instruction(
            "Exile some attacking creatures target player controls.",
            "",
        )
        .is_none()
    );
    assert!(parse_general_effect_instruction(
        "That player may search their library for that many basic land cards, put them onto the battlefield tapped, then shuffle.",
        "",
    )
    .is_none());
}

#[test]
fn modal_type_addition_reuses_power_criteria_duration_and_keyword_leaves() {
    let oracle = "Choose one \u{2014}\n\u{2022} Destroy target creature with power 4 or greater.\n\u{2022} Until end of turn, target creature becomes an artifact in addition to its other types and gains indestructible.";
    let parsed = parse_general_modal_spell(oracle).expect("the generic modal spell composes");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "chosenModes"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["id"],
        "mode1:targetPermanent"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["candidates"]["where"]["operands"][1]["left"]["kind"],
        "powerOf"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][2]["id"],
        "mode2:targetPermanent"
    );
    assert_eq!(parsed.rule["effects"][1]["then"][0]["kind"], "addCardType");
    assert_eq!(parsed.rule["effects"][1]["then"][0]["cardType"], "Artifact");
    assert_eq!(parsed.rule["effects"][1]["then"][1]["kind"], "grantKeyword");
    assert_eq!(
        parsed.rule["effects"][1]["then"][1]["keyword"],
        "indestructible"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let (effects, decisions) = parse_general_effect_instruction(
        "Until end of turn, target enchantment becomes a creature in addition to its other types and gains flying and vigilance.",
        "",
    )
    .expect("another type and a keyword list reuse the same leaves");
    assert_eq!(effects.len(), 3);
    assert_eq!(decisions[0]["candidates"]["where"]["value"], "Enchantment");
    assert!(
        parse_general_effect_instruction(
            "Until end of turn, target creature becomes an artifact and gains indestructible.",
            "",
        )
        .is_none()
    );
}

#[test]
fn bounded_multi_target_tap_reuses_target_cardinality_and_criteria_leaves() {
    let oracle = "Tap one or two target creatures. (Then exile this card. You may cast the creature later from exile.)";
    let parsed = parse_simple_spell_ability(oracle).expect("the Adventure tap spell parses");

    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "targetPermanents");
    assert_eq!(decision["minimum"], 1);
    assert_eq!(decision["maximum"], 2);
    assert_eq!(decision["candidates"]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["effects"][0]["kind"], "tapPermanent");
    assert_eq!(
        parsed.rule["effects"][0]["permanent"]["kind"],
        "chosenTargets"
    );
    assert_eq!(parsed.rule["exileAfterResolution"], true);
    assert_eq!(parsed.rule["adventureAfterResolution"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let up_to_two = parse_general_effect_instruction("Tap up to two target artifacts.", "")
        .expect("optional cardinality reuses the same target leaf");
    assert_eq!(up_to_two.1[0]["minimum"], 0);
    assert_eq!(up_to_two.1[0]["maximum"], 2);
    assert_eq!(up_to_two.1[0]["candidates"]["where"]["value"], "Artifact");
    assert!(parse_general_effect_instruction("Tap three target creatures.", "").is_none());
}

#[test]
fn linked_target_bonus_condition_and_attachment_reuse_shared_leaves() {
    let oracle = "Untap target creature you control. It gets +2/+2 until end of turn. If it's a Dwarf, you may attach an Equipment you control to it.";
    let parsed = parse_simple_spell_ability(oracle).expect("the linked target sequence composes");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "targetPermanent"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "untapPermanent");
    assert_eq!(parsed.rule["effects"][1]["kind"], "modifyPowerToughness");
    assert_eq!(parsed.rule["effects"][2]["kind"], "conditionalEffect");
    assert_eq!(
        parsed.rule["effects"][2]["condition"]["where"]["value"],
        "Dwarf"
    );
    assert_eq!(
        parsed.rule["effects"][2]["then"][0]["kind"],
        "choosePermanents"
    );
    assert_eq!(parsed.rule["effects"][2]["then"][0]["minimum"]["value"], 0);
    assert_eq!(
        parsed.rule["effects"][2]["then"][1]["attachment"]["kind"],
        "decisionResult"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_general_effect_sequence(
        "Tap target permanent you control. It gets -1/+3 until end of turn. If it's an Elf, you may attach an Equipment you control to it.",
        "",
    )
    .expect("another action, subtype, and modifier reuse the same leaves");
    assert_eq!(variant.0[0]["kind"], "tapPermanent");
    assert_eq!(variant.0[1]["power"]["value"], -1);
    assert_eq!(variant.0[2]["condition"]["where"]["value"], "Elf");
    assert!(parse_general_effect_sequence(
        "Untap target creature you control. It gets +2/+2 until end of turn. If it's a Dwarf, attach an Equipment you control to it.",
        "",
    )
    .is_none());
}

#[test]
fn shared_card_type_control_exchange_reuses_target_criteria_and_cardinality() {
    let oracle = "Exchange control of two target nonland permanents that share a card type. (Then exile this card. You may cast the creature later from exile.)";
    let parsed =
        parse_simple_spell_ability(oracle).expect("the shared-card-type control exchange composes");

    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "targetPermanents");
    assert_eq!(decision["minimum"], 2);
    assert_eq!(decision["maximum"], 2);
    assert_eq!(decision["candidates"]["where"]["kind"], "not");
    assert_eq!(decision["selectionConstraint"]["kind"], "shareCardType");
    assert_eq!(parsed.rule["effects"][0]["kind"], "exchangeControl");
    assert_eq!(parsed.rule["exileAfterResolution"], true);
    assert_eq!(parsed.rule["adventureAfterResolution"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let compound_variant = parse_permanent_criteria("artifacts or enchantments", "")
        .expect("plural card types connected by or reuse the criteria leaf");
    assert_eq!(compound_variant["kind"], "or");

    let variant = parse_general_effect_instruction(
        "Exchange control of two target creatures that share a card type.",
        "",
    )
    .expect("another permanent criterion reuses the exchange leaf");
    assert_eq!(variant.1[0]["candidates"]["where"]["value"], "Creature");
    assert!(
        parse_general_effect_instruction(
            "Exchange control of two target creatures that share a color.",
            "",
        )
        .is_none()
    );
}

#[test]
fn source_zone_reduction_and_linked_resolution_cast_reuse_zone_and_filter_leaves() {
    let reduction = parse_common_static_ability(
        "Spells you cast from anywhere other than your hand cost {1} less to cast.",
        "Bilbo, Thief in the Night",
    )
    .expect("the non-hand casting reduction parses");
    assert_eq!(reduction.rule["modifiers"][0]["kind"], "reduceCastingCost");
    assert_eq!(reduction.rule["modifiers"][0]["sourceZoneNot"], "hand");
    assert!(crate::engine::rule_is_executable(&reduction.rule));

    let graveyard_variant = parse_common_static_ability(
        "Spells you cast from your graveyard cost {2} less to cast.",
        "Test Reducer",
    )
    .expect("a named source zone and another amount reuse the reduction leaf");
    assert_eq!(
        graveyard_variant.rule["modifiers"][0]["sourceZone"],
        "graveyard"
    );
    assert_eq!(graveyard_variant.rule["modifiers"][0]["amount"]["value"], 2);

    let triggered = parse_expansion_triggered(
        "Whenever Bilbo attacks, you may cast an artifact, instant, or sorcery spell from your graveyard. If an instant or sorcery spell cast this way would be put into your graveyard, exile it instead.",
        "Bilbo, Thief in the Night",
    )
    .expect("the attack-triggered cast and linked replacement compose");
    assert_eq!(triggered.rule["event"]["kind"], "declaredAttacker");
    assert_eq!(triggered.rule["effects"][0]["kind"], "optionalEffects");
    assert_eq!(
        triggered.rule["effects"][0]["effects"][0]["kind"],
        "castOneCard"
    );
    assert_eq!(
        triggered.rule["effects"][0]["effects"][0]["sourceZone"],
        "graveyard"
    );
    assert_eq!(
        triggered.rule["effects"][0]["effects"][0]["optional"],
        false
    );
    assert_eq!(
        triggered.rule["effects"][0]["effects"][1]["kind"],
        "replaceCastSpellDestination"
    );
    assert_eq!(
        triggered.rule["effects"][0]["effects"][1]["destination"],
        "exile"
    );
    assert!(crate::engine::rule_is_executable(&triggered.rule));

    let exile_variant =
        parse_general_effect_instruction("You may cast a creature spell from your exile.", "")
            .expect("another source zone and spell criterion reuse the cast leaf");
    assert_eq!(exile_variant.0[0]["effects"][0]["sourceZone"], "exile");
    assert!(
        parse_common_static_ability(
            "Spells you cast from your library cost {1} less to cast.",
            "Test Reducer",
        )
        .is_none()
    );
    assert!(
        parse_general_effect_instruction(
            "If an instant spell cast this way would be put into your hand, exile it instead.",
            "",
        )
        .is_none()
    );
}

#[test]
fn unqualified_counter_unless_paid_composes_inside_a_generic_modal_spell() {
    let oracle = "Choose one \u{2014}\n\u{2022} Counter target spell unless its controller pays {4}.\n\u{2022} Draw two cards, then discard a card.";
    let parsed = parse_general_modal_spell(oracle)
        .expect("the unqualified counter and draw-discard modes compose");

    assert_eq!(parsed.rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(parsed.rule["declaration"]["decisions"][0]["maximum"], 1);
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["id"],
        "mode1:targetStackObject"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["candidates"]["where"],
        Value::Null
    );
    assert_eq!(
        parsed.rule["effects"][0]["then"][0]["kind"],
        "counterStackObjectUnlessPays"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["kind"],
        "drawThenDiscard"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let qualified = parse_general_effect_instruction(
        "Counter target noncreature spell unless its controller pays {2}.",
        "",
    )
    .expect("a qualified target still reuses the same leaf");
    assert_eq!(qualified.1[0]["candidates"]["where"]["kind"], "not");
    assert!(
        parse_general_effect_instruction(
            "Counter target spell unless its controller pays 4 life.",
            "",
        )
        .is_none()
    );
}

#[test]
fn bounded_delayed_blink_and_activated_source_trigger_reuse_shared_leaves() {
    let activated = parse_simple_activated_ability(
        "{5}{U}{U}: Exile up to two other target nonland permanents you control. Return those cards to the battlefield under their owner's control at the beginning of the next end step.",
    )
    .expect("the bounded delayed blink activation composes");
    assert_eq!(activated.rule["costs"][0]["kind"], "payMana");
    assert_eq!(
        activated.rule["effects"][0]["kind"],
        "exileUntilNextEndStep"
    );
    let decision = &activated.rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 0);
    assert_eq!(decision["maximum"], 2);
    assert_eq!(decision["candidates"]["excludeSource"], true);
    assert_eq!(decision["candidates"]["controller"]["kind"], "controllerOf");
    assert!(crate::engine::rule_is_executable(&activated.rule));

    let blink_variant = parse_general_effect_sequence(
        "Exile up to three other target artifacts you control. Return those cards to the battlefield under their owner's control at the beginning of the next end step.",
        "",
    )
    .expect("another bound and criterion reuse the delayed blink leaf");
    assert_eq!(blink_variant.1[0]["maximum"], 3);
    assert_eq!(
        blink_variant.1[0]["candidates"]["where"]["value"],
        "Artifact"
    );

    let triggered = parse_expansion_triggered(
        "Whenever you activate an ability of a creature, draw a card. This ability triggers only once each turn.",
        "Test Reader",
    )
    .expect("the activated-source trigger and generic limit compose");
    assert_eq!(triggered.rule["event"]["kind"], "abilityActivated");
    assert_eq!(triggered.rule["event"]["where"]["value"], "Creature");
    assert_eq!(triggered.rule["triggerLimit"]["kind"], "onceEachTurn");
    assert_eq!(triggered.rule["effects"][0]["kind"], "drawCards");
    assert!(crate::engine::rule_is_executable(&triggered.rule));

    assert!(parse_general_effect_sequence(
        "Exile up to several other target artifacts you control. Return those cards to the battlefield under their owner's control at the beginning of the next end step.",
        "",
    )
    .is_none());
    assert!(
        parse_expansion_triggered(
            "Whenever you activate an ability belonging to a creature, draw a card.",
            "Test Reader",
        )
        .is_none()
    );
}

#[test]
fn attached_permanent_counter_clear_and_ability_loss_reuse_attachment_leaves() {
    let enters = parse_expansion_triggered(
        "When this Aura enters, tap enchanted creature and remove all counters from it.",
        "Test Grasp",
    )
    .expect("the Aura enters trigger composes from attached-permanent effect leaves");
    assert_eq!(enters.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(enters.rule["effects"][0]["kind"], "tapPermanent");
    assert_eq!(
        enters.rule["effects"][0]["permanent"]["kind"],
        "attachedPermanent"
    );
    assert_eq!(enters.rule["effects"][1]["kind"], "removeAllCounters");
    assert!(crate::engine::rule_is_executable(&enters.rule));

    let untap_variant = parse_general_effect_instruction(
        "Untap enchanted permanent and remove all counters from it.",
        "",
    )
    .expect("another tap direction and permanent noun reuse the same leaves");
    assert_eq!(untap_variant.0[0]["kind"], "untapPermanent");
    assert_eq!(untap_variant.0[1]["kind"], "removeAllCounters");

    let static_ability = parse_common_static_ability(
        "Enchanted creature loses all abilities and doesn't untap during its controller's untap step.",
        "Test Grasp",
    )
    .expect("the two continuous modifiers compose on the attached permanent");
    assert_eq!(
        static_ability.rule["modifiers"][0]["kind"],
        "loseAllAbilities"
    );
    assert_eq!(static_ability.rule["modifiers"][1]["kind"], "doesNotUntap");
    assert!(crate::engine::rule_is_executable(&static_ability.rule));

    let permanent_variant = parse_common_static_ability(
        "Enchanted permanent loses all abilities and doesn't untap during its controller's untap step.",
        "Test Grasp",
    )
    .expect("the attached-permanent noun variant reuses both modifiers");
    assert_eq!(
        permanent_variant.rule["modifiers"][0]["objects"]["kind"],
        "attachedPermanent"
    );

    assert!(
        parse_general_effect_instruction(
            "Tap enchanted creature and remove all abilities from it.",
            "",
        )
        .is_none()
    );
    assert!(parse_common_static_ability(
        "Enchanted creature loses all colors and doesn't untap during its controller's untap step.",
        "Test Grasp",
    )
    .is_none());
}

#[test]
fn source_owner_shuffle_and_draw_reuses_the_permanent_destination_effect() {
    let activated = parse_simple_activated_ability(
        "{6}: Gandalf's owner shuffles him into their library and draws three cards.",
    )
    .expect("the named source, owner destination, and draw count compose");
    assert_eq!(activated.rule["costs"][0]["kind"], "payMana");
    assert_eq!(activated.rule["costs"][0]["manaCost"], "{6}");
    assert_eq!(
        activated.rule["effects"][0]["kind"],
        "shufflePermanentIntoOwnersLibraryThenDraw"
    );
    assert_eq!(activated.rule["effects"][0]["permanent"]["kind"], "self");
    assert_eq!(activated.rule["effects"][0]["count"]["value"], 3);
    assert!(crate::engine::rule_is_executable(&activated.rule));

    let source_variant = parse_general_effect_instruction(
        "This creature's owner shuffles it into their library and draws two cards.",
        "Test Creature",
    )
    .expect("a self-reference and another count reuse the same effect leaf");
    assert_eq!(source_variant.0[0]["count"]["value"], 2);

    assert!(
        parse_general_effect_instruction(
            "Target creature's owner shuffles it into their library and draws two cards.",
            "",
        )
        .is_none()
    );
    assert!(
        parse_general_effect_instruction(
            "This creature's owner puts it into their library and draws two cards.",
            "Test Creature",
        )
        .is_none()
    );
}

#[test]
fn exact_count_immediate_blink_reuses_cardinality_and_compound_criteria() {
    let parsed = parse_simple_spell_ability(
        "Exile two target creatures and/or lands you control, then return them to the battlefield under their owner's control.",
    )
    .expect("the exact target count, compound criteria, and immediate blink compose");
    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "blinkPermanents");
    assert_eq!(decision["minimum"], 2);
    assert_eq!(decision["maximum"], 2);
    assert_eq!(decision["candidates"]["controller"]["kind"], "controllerOf");
    assert_eq!(decision["candidates"]["where"]["kind"], "or");
    assert_eq!(
        decision["candidates"]["where"]["operands"][0]["value"],
        "Creature"
    );
    assert_eq!(
        decision["candidates"]["where"]["operands"][1]["value"],
        "Land"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "blinkPermanents");
    assert_eq!(parsed.rule["effects"][0]["repeat"]["value"], 1);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_general_effect_sequence(
        "Exile three target artifacts or enchantments you control, then return them to the battlefield under their owner's control.",
        "",
    )
    .expect("another count and compound card types reuse the same leaf");
    assert_eq!(variant.1[0]["minimum"], 3);
    assert_eq!(variant.1[0]["candidates"]["where"]["kind"], "or");

    assert!(parse_general_effect_instruction(
        "Exile several target creatures you control, then return them to the battlefield under their owner's control.",
        "",
    )
    .is_none());
    assert!(parse_general_effect_instruction(
        "Exile two target creatures you control, then return one to the battlefield under its owner's control.",
        "",
    )
    .is_none());
}

#[test]
fn own_cost_reduction_sums_power_through_shared_keyword_criteria() {
    let total = parse_common_static_ability(
        "This spell costs {X} less to cast, where X is the total power of creatures you control with flying.",
        "Test Eagle",
    )
    .expect("the total-power casting reduction composes");
    let amount = &total.rule["modifiers"][0]["amount"];
    assert_eq!(total.rule["activeWhile"]["zone"]["kind"], "stackOrCast");
    assert_eq!(
        total.rule["modifiers"][0]["kind"],
        "reduceOwnGenericCastingCost"
    );
    assert_eq!(amount["kind"], "sumPowers");
    assert_eq!(amount["where"]["kind"], "and");
    assert_eq!(amount["where"]["operands"][0]["value"], "Creature");
    assert_eq!(amount["where"]["operands"][1]["kind"], "hasKeyword");
    assert_eq!(amount["where"]["operands"][1]["value"], "flying");
    assert!(crate::engine::rule_is_executable(&total.rule));

    let greatest = parse_common_static_ability(
        "This spell costs {X} less to cast, where X is the greatest power among artifacts you control.",
        "Test Construct",
    )
    .expect("the greatest-power wording reuses the same casting leaf");
    assert_eq!(
        greatest.rule["modifiers"][0]["amount"]["kind"],
        "greatestPower"
    );
    assert_eq!(
        greatest.rule["modifiers"][0]["amount"]["where"]["value"],
        "Artifact"
    );

    assert_eq!(
        parse_permanent_criteria("creatures", "").expect("a plural type is a criteria leaf")["value"],
        "Creature"
    );
    assert!(parse_common_static_ability(
        "This spell costs {X} less to cast, where X is the average power of creatures you control.",
        "Test Eagle",
    )
    .is_none());
}

#[test]
fn self_bonus_counts_player_zones_that_meet_a_card_threshold() {
    let parsed = parse_common_static_ability(
        "This creature gets +2/+0 for each graveyard with seven or more cards in it.",
        "Test Councillor",
    )
    .expect("the per-graveyard threshold bonus composes");
    let modifier = &parsed.rule["modifiers"][0];
    assert_eq!(modifier["kind"], "modifyPowerToughness");
    assert_eq!(modifier["objects"]["kind"], "self");
    assert_eq!(modifier["power"]["kind"], "multiply");
    assert_eq!(
        modifier["power"]["value"]["kind"],
        "countPlayerZonesByCardCount"
    );
    assert_eq!(modifier["power"]["value"]["zone"], "graveyard");
    assert_eq!(modifier["power"]["value"]["operator"], ">=");
    assert_eq!(modifier["power"]["value"]["count"]["value"], 7);
    assert_eq!(modifier["power"]["factor"]["value"], 2);
    assert_eq!(modifier["toughness"]["factor"]["value"], 0);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let fewer = parse_common_static_ability(
        "This creature gets +1/+1 for each hand with three or fewer cards in it.",
        "Test Scholar",
    )
    .expect("another zone and the inverse threshold reuse the same leaves");
    assert_eq!(fewer.rule["modifiers"][0]["power"]["zone"], "hand");
    assert_eq!(fewer.rule["modifiers"][0]["power"]["operator"], "<=");

    assert!(
        parse_common_static_ability(
            "This creature gets +2/+0 for each graveyard with exactly seven cards in it.",
            "Test Councillor",
        )
        .is_none()
    );
}

#[test]
fn landfall_optional_base_power_toughness_uses_event_and_source_effect_leaves() {
    let parsed = parse_expansion_triggered(
        "Landfall — Whenever a land you control enters, you may have this creature's base power and toughness become 4/2 until end of turn.",
        "Mirkwood Meditator",
    )
    .expect("landfall composes with the optional temporary base-stat effect");
    assert_eq!(parsed.rule["event"]["kind"], "permanentEntered");
    assert_eq!(parsed.rule["event"]["where"]["value"], "Land");
    assert_eq!(parsed.rule["effects"][0]["kind"], "optionalEffects");
    let effect = &parsed.rule["effects"][0]["effects"][0];
    assert_eq!(effect["kind"], "setBasePowerToughness");
    assert_eq!(effect["object"]["kind"], "self");
    assert_eq!(effect["power"]["value"], 4);
    assert_eq!(effect["toughness"]["value"], 2);
    assert_eq!(effect["duration"]["kind"], "untilEndOfCurrentTurn");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let named_variant = parse_general_effect_instruction(
        "You may have Test Scout's base power and toughness become 3/5 until end of turn.",
        "Test Scout",
    )
    .expect("a named source and different base stats reuse the same effect leaf");
    assert_eq!(named_variant.0[0]["effects"][0]["power"]["value"], 3);
    assert_eq!(named_variant.0[0]["effects"][0]["toughness"]["value"], 5);

    assert!(
        parse_general_effect_instruction(
            "You may have this creature get 4/2 until end of turn.",
            "Mirkwood Meditator",
        )
        .is_none()
    );
}

#[test]
fn graveyard_threshold_and_milled_card_destination_reuse_existing_leaves() {
    let threshold = parse_common_static_ability(
        "Threshold — This creature gets +1/+1 as long as there are seven or more cards in your graveyard.",
        "Test Bird",
    )
    .expect("an unqualified graveyard count composes with the conditional bonus");
    assert_eq!(threshold.rule["condition"]["kind"], "compare");
    assert_eq!(threshold.rule["condition"]["left"]["kind"], "countCards");
    assert!(threshold.rule["condition"]["left"]["where"].is_null());
    assert_eq!(threshold.rule["condition"]["right"]["value"], 7);
    assert_eq!(threshold.rule["modifiers"][0]["power"]["value"], 1);
    assert!(crate::engine::rule_is_executable(&threshold.rule));

    let qualified = parse_condition_text("there are three or more land cards in your graveyard")
        .expect("the previously supported qualified threshold still parses");
    assert_eq!(qualified["left"]["where"]["value"], "Land");

    let mill_to_hand = parse_simple_spell_ability(
        "Mill four cards, then put an instant or sorcery card from among them into your hand.",
    )
    .expect("the mill binding composes with a hand destination");
    assert_eq!(mill_to_hand.rule["effects"][0]["kind"], "mill");
    assert_eq!(mill_to_hand.rule["effects"][0]["bind"], "milledCards");
    assert_eq!(mill_to_hand.rule["effects"][1]["kind"], "chooseCards");
    assert_eq!(mill_to_hand.rule["effects"][1]["where"]["kind"], "or");
    assert_eq!(mill_to_hand.rule["effects"][2]["to"]["kind"], "hand");
    assert!(crate::engine::rule_is_executable(&mill_to_hand.rule));

    let battlefield_variant = parse_simple_spell_ability(
        "Mill two cards. Then put a creature card from among them onto the battlefield.",
    )
    .expect("the original punctuation and battlefield destination remain supported");
    assert_eq!(
        battlefield_variant.rule["effects"][2]["to"]["kind"],
        "battlefield"
    );

    assert!(
        parse_simple_spell_ability(
            "Mill four cards, then put an instant or sorcery card from your graveyard into your hand.",
        )
        .is_none()
    );
}

#[test]
fn saga_chapters_link_keywords_and_damage_prevention_to_their_source() {
    let keyword = parse_generalized_zone_and_combat_ability(
        "I — Target creature you control gains hexproof for as long as this Saga remains on the battlefield.",
        "triggeredAbility",
        "Test Saga",
    )
    .expect("the Saga chapter composes with a linked keyword effect");
    assert_eq!(keyword.rule["event"]["kind"], "sagaChapterReached");
    assert_eq!(keyword.rule["event"]["chapters"][0]["value"], 1);
    assert_eq!(keyword.rule["effects"][0]["kind"], "installLinkedKeyword");
    assert_eq!(keyword.rule["effects"][0]["keyword"], "hexproof");
    assert_eq!(
        keyword.rule["effects"][0]["duration"]["kind"],
        "whileSourceOnBattlefield"
    );
    assert_eq!(
        keyword.rule["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "controllerOf"
    );
    assert!(crate::engine::rule_is_executable(&keyword.rule));

    let prevention = parse_generalized_zone_and_combat_ability(
        "II — Prevent all damage that would be dealt by up to one target creature for as long as this Saga remains on the battlefield.",
        "staticAbility",
        "Test Saga",
    )
    .expect("the Saga chapter composes with linked source-damage prevention");
    assert_eq!(prevention.rule["event"]["chapters"][0]["value"], 2);
    assert_eq!(
        prevention.rule["effects"][0]["kind"],
        "installLinkedDamagePrevention"
    );
    assert_eq!(prevention.rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(prevention.rule["declaration"]["decisions"][0]["maximum"], 1);
    assert!(crate::engine::rule_is_executable(&prevention.rule));

    let permanent_variant = parse_general_effect_instruction(
        "Target artifact you control gains lifelink for as long as this permanent remains on the battlefield.",
        "Test Relic",
    )
    .expect("another target type and keyword reuse the linked leaf");
    assert_eq!(permanent_variant.0[0]["keyword"], "lifelink");

    assert!(
        parse_general_effect_instruction(
            "Prevent all damage that would be dealt to target creature for as long as this Saga remains on the battlefield.",
            "Test Saga",
        )
        .is_none()
    );
}

#[test]
fn conditional_instead_sequence_reuses_conditions_and_effect_instructions() {
    let parsed = parse_simple_spell_ability(
        "Draw a card. If this spell was cast from a graveyard, draw two cards instead.",
    )
    .expect("the base and alternate draw instructions compose around the zone condition");
    let branch = &parsed.rule["effects"][0];
    assert_eq!(branch["kind"], "conditionalEffect");
    assert_eq!(branch["condition"]["kind"], "wasCastFromZone");
    assert_eq!(branch["condition"]["zone"], "graveyard");
    assert_eq!(branch["else"][0]["kind"], "drawCards");
    assert_eq!(branch["else"][0]["count"]["value"], 1);
    assert_eq!(branch["then"][0]["count"]["value"], 2);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let exile_variant = parse_simple_spell_ability(
        "You gain two life. If this spell was cast from exile, you gain four life instead.",
    )
    .expect("another zone, effect kind, and amount reuse the conditional sequence");
    assert_eq!(
        exile_variant.rule["effects"][0]["condition"]["zone"],
        "exile"
    );
    assert_eq!(
        exile_variant.rule["effects"][0]["then"][0]["amount"]["value"],
        4
    );

    assert!(
        parse_simple_spell_ability(
            "Draw a card. If this spell was cast from a graveyard, draw two cards also.",
        )
        .is_none()
    );
}

#[test]
fn top_library_piles_capture_visibility_chooser_and_destinations() {
    let parsed = parse_simple_spell_ability(
        "Look at the top four cards of your library and separate them into a face-down pile and a face-up pile. An opponent chooses one of the piles. Put that pile into your hand and the other into your graveyard.",
    )
    .expect("the top-card count and two pile contracts compose");
    let effect = &parsed.rule["effects"][0];
    assert_eq!(effect["kind"], "separateTopCardsIntoPiles");
    assert_eq!(effect["count"]["value"], 4);
    assert_eq!(effect["firstPileVisibility"], "face-down");
    assert_eq!(effect["secondPileVisibility"], "face-up");
    assert_eq!(effect["pileChooser"]["kind"], "chosenOpponent");
    assert_eq!(effect["chosenPileDestination"], "hand");
    assert_eq!(effect["otherPileDestination"], "graveyard");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let reversed = parse_simple_spell_ability(
        "Look at the top three cards of your library and separate them into a face-up pile and a face-down pile. An opponent chooses one of the piles. Put that pile into your graveyard and the other into your hand.",
    )
    .expect("visibility order, count, and destinations are independent leaves");
    assert_eq!(reversed.rule["effects"][0]["count"]["value"], 3);
    assert_eq!(
        reversed.rule["effects"][0]["chosenPileDestination"],
        "graveyard"
    );

    assert!(
        parse_simple_spell_ability(
            "Look at the top four cards of your library and separate them into a face-up pile and a face-up pile. An opponent chooses one of the piles. Put that pile into your hand and the other into your graveyard.",
        )
        .is_none()
    );
}

#[test]
fn saga_multi_chapter_delayed_blink_accepts_the_if_you_do_connector() {
    let parsed = parse_generalized_zone_and_combat_ability(
        "I, II, III, IV — Exile up to one target creature or land you control. If you do, return it to the battlefield under its owner's control at the beginning of the next end step.",
        "triggeredAbility",
        "Test Voyage",
    )
    .expect("the shared Saga chapter and delayed-blink leaves compose");
    assert_eq!(parsed.rule["event"]["kind"], "sagaChapterReached");
    assert_eq!(
        parsed.rule["event"]["chapters"].as_array().unwrap().len(),
        4
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "exileUntilNextEndStep");
    assert_eq!(parsed.rule["declaration"]["decisions"][0]["minimum"], 0);
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["candidates"]["controller"]["kind"],
        "controllerOf"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "or"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let direct_variant = parse_general_effect_sequence(
        "Exile target artifact. Return that card to the battlefield under its owner's control at the beginning of the next end step.",
        "",
    )
    .expect("the original connector remains supported");
    assert_eq!(direct_variant.1[0]["minimum"], 1);

    assert!(
        parse_general_effect_sequence(
            "Exile up to one target creature you control. If you do, put it onto the battlefield at the beginning of the next end step.",
            "",
        )
        .is_none()
    );
}

#[test]
fn counter_follow_up_binds_the_spells_mana_value_before_it_leaves_stack() {
    let parsed = parse_simple_spell_ability(
        "Counter target spell. If that spell's mana value was 2 or less, recruit. (Draw a card, then discard a card. If you discarded a nonland card, create a 1/1 white Human Soldier creature token.)",
    )
    .expect("counter, bound mana value, and recruit compose");
    assert_eq!(parsed.rule["effects"][0]["kind"], "bind");
    assert_eq!(parsed.rule["effects"][0]["value"]["kind"], "manaValueOf");
    assert_eq!(parsed.rule["effects"][1]["kind"], "counterSpell");
    let follow_up = &parsed.rule["effects"][2];
    assert_eq!(follow_up["kind"], "conditionalEffect");
    assert_eq!(follow_up["condition"]["operator"], "<=");
    assert_eq!(follow_up["condition"]["left"]["kind"], "boundValue");
    assert_eq!(follow_up["then"][0]["kind"], "recruit");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let greater_variant = parse_simple_spell_ability(
        "Counter target spell. If that spell's mana value was three or greater, draw a card.",
    )
    .expect("another threshold direction and follow-up effect reuse the sequence");
    assert_eq!(
        greater_variant.rule["effects"][2]["condition"]["operator"],
        ">="
    );
    assert_eq!(
        greater_variant.rule["effects"][2]["then"][0]["kind"],
        "drawCards"
    );

    assert!(
        parse_simple_spell_ability(
            "Counter target spell. If that spell's mana value is 2 or less, recruit.",
        )
        .is_none()
    );
}

#[test]
fn filtered_counter_exile_binds_a_free_cast_permission() {
    let parsed = parse_simple_spell_ability(
        "Counter target spell. If a permanent spell is countered this way, exile it instead of putting it into its owner's graveyard. You may cast that card without paying its mana cost for as long as it remains exiled.",
    )
    .expect("the filtered exile replacement binds into the card permission");
    let counter = &parsed.rule["effects"][0];
    assert_eq!(counter["kind"], "counterSpell");
    assert_eq!(counter["exileInsteadWhere"]["kind"], "isPermanentCard");
    assert_eq!(counter["bindExiledAs"], "counteredExiledSpell");
    let permission = &parsed.rule["effects"][1];
    assert_eq!(permission["kind"], "grantCardPermission");
    assert_eq!(permission["cards"]["kind"], "boundObjects");
    assert_eq!(permission["play"]["normalCostsApply"], false);
    assert_eq!(permission["duration"]["kind"], "whileObjectsRemainInZone");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let artifact_variant = parse_simple_spell_ability(
        "Counter target spell. If an artifact spell is countered this way, exile it instead of putting it into its owner's graveyard. You may cast that card without paying its mana cost for as long as it remains exiled.",
    )
    .expect("another spell filter reuses the counter and permission leaves");
    assert_eq!(
        artifact_variant.rule["effects"][0]["exileInsteadWhere"]["value"],
        "Artifact"
    );

    assert!(
        parse_simple_spell_ability(
            "Counter target spell. If a permanent spell is countered this way, put it into exile. You may cast that card without paying its mana cost for as long as it remains exiled.",
        )
        .is_none()
    );
}

#[test]
fn optional_variable_draw_then_discard_reuses_trigger_mana_spent() {
    assert_eq!(
        parse_permanent_criteria("noncreature", ""),
        Some(not(card_type("Creature")))
    );
    let (standalone_effects, standalone_decisions) = parse_general_effect_sequence(
        "You may draw X cards, where X is the amount of mana spent to cast that spell. If you do, discard two cards.",
        "",
    )
    .expect("the optional variable effect sequence parses independently");
    assert!(standalone_decisions.is_empty());
    assert_eq!(standalone_effects[0]["kind"], "optionalAction");

    let parsed = parse_expansion_triggered(
        "Whenever you cast a noncreature spell, you may draw X cards, where X is the amount of mana spent to cast that spell. If you do, discard two cards.",
        "Test Moon-Letters",
    )
    .expect("the spell filter and optional variable sequence compose");
    assert_eq!(parsed.rule["event"]["kind"], "spellCast");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "not");
    let optional = &parsed.rule["effects"][0];
    assert_eq!(optional["kind"], "optionalAction");
    assert_eq!(
        optional["action"]["count"]["kind"],
        "triggeringSpellManaSpent"
    );
    assert_eq!(optional["onPerformed"][0]["kind"], "discardCards");
    assert_eq!(optional["onPerformed"][0]["count"]["value"], 2);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "Whenever you cast an artifact spell, you may draw X cards, where X is the amount of mana spent to cast that spell. If you do, discard a card.",
        "Test Relic",
    )
    .expect("another spell criterion and discard count reuse the same leaves");
    assert_eq!(variant.rule["event"]["where"]["value"], "Artifact");
    assert_eq!(
        variant.rule["effects"][0]["onPerformed"][0]["count"]["value"],
        1
    );

    let mana_value_variant = parse_expansion_triggered(
        "Whenever you cast a noncreature spell, you may draw X cards, where X is that spell's mana value. If you do, discard two cards.",
        "Test Moon-Letters",
    )
    .expect("the triggering spell mana-value variable remains independently reusable");
    assert_eq!(
        mana_value_variant.rule["effects"][0]["action"]["count"]["kind"],
        "triggeringSpellManaValue"
    );

    assert!(
        parse_expansion_triggered(
            "Whenever you cast a noncreature spell, you may draw X cards, where X is that spell's mana value. If you don't, discard two cards.",
            "Test Moon-Letters",
        )
        .is_none()
    );
}

#[test]
fn opponent_top_exile_reuses_quantity_permission_and_life_cost_leaves() {
    let parsed = parse_simple_spell_ability(
        "Exile the top X cards of target opponent's library. You may play those cards this turn. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.",
    )
    .expect("the variable exile and life alternative cost compose");
    assert_eq!(parsed.rule["declaration"]["decisions"][0]["id"], "xValue");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["id"],
        "targetOpponent"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "exileTopCards");
    assert_eq!(parsed.rule["effects"][0]["count"]["kind"], "decisionResult");
    let permission = &parsed.rule["effects"][1];
    assert_eq!(permission["kind"], "grantCardPermission");
    assert_eq!(permission["play"]["normalCostsApply"], false);
    assert_eq!(
        permission["play"]["alternativeCost"]["amount"]["object"]["kind"],
        "grantedCard"
    );
    assert_eq!(permission["duration"]["kind"], "untilEndOfCurrentTurn");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let fixed_variant = parse_simple_spell_ability(
        "Exile the top two cards of target opponent's library. You may play them this turn. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.",
    )
    .expect("a fixed plural quantity reuses the same leaves");
    assert_eq!(fixed_variant.rule["effects"][0]["count"]["value"], 2);

    let singular_variant = parse_simple_spell_ability(
        "Exile the top card of target opponent's library. You may play that card this turn. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.",
    )
    .expect("the singular form reuses the same leaves");
    assert_eq!(singular_variant.rule["effects"][0]["count"]["value"], 1);

    assert!(
        parse_simple_spell_ability(
            "Exile the top card of target opponent's library. You may play those cards this turn. If you cast a spell this way, pay life equal to its mana value rather than pay its mana cost.",
        )
        .is_none()
    );
    assert!(
        parse_simple_spell_ability(
            "Exile the top two cards of target opponent's library. You may play them this turn. If you cast a spell this way, pay life equal to its power rather than pay its mana cost.",
        )
        .is_none()
    );
}

#[test]
fn target_owner_library_end_choice_reuses_target_criteria() {
    let parsed = parse_simple_spell_ability(
        "Target creature's owner puts it on their choice of the top or bottom of their library.",
    )
    .expect("the target criterion composes with the owner's library-end choice");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Creature"
    );
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "putPermanentOnOwnerLibrary"
    );
    assert_eq!(
        parsed.rule["effects"][0]["position"],
        "ownerChoiceTopOrBottom"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let artifact = parse_simple_spell_ability(
        "Target artifact's owner puts it on their choice of the top or bottom of their library.",
    )
    .expect("another permanent criterion reuses the movement leaf");
    assert_eq!(
        artifact.rule["declaration"]["decisions"][0]["candidates"]["where"]["value"],
        "Artifact"
    );

    assert!(
        parse_simple_spell_ability(
            "Target creature's controller puts it on their choice of the top or bottom of their library.",
        )
        .is_none()
    );
}

#[test]
fn equipment_keywords_trigger_multipliers_and_subtype_equip_compose() {
    let prowess = parse_special_static_ability(
        "Equipped creature has prowess. (Whenever its controller casts a noncreature spell, that creature gets +1/+1 until end of turn.)",
    )
    .expect("the equipped-keyword leaf recognizes prowess");
    assert_eq!(prowess.rule["modifiers"][0]["kind"], "grantKeyword");
    assert_eq!(prowess.rule["modifiers"][0]["keyword"], "prowess");
    assert_eq!(
        prowess.rule["modifiers"][0]["objects"]["kind"],
        "attachedPermanent"
    );
    assert!(crate::engine::rule_is_executable(&prowess.rule));

    let multiplier = parse_common_static_ability(
        "If a triggered ability of equipped creature triggers, that ability triggers an additional time.",
        "Test Staff",
    )
    .expect("the repeated-trigger scaffold accepts the attached permanent selector");
    assert_eq!(
        multiplier.rule["modifiers"][0]["kind"],
        "multiplyTriggeredAbility"
    );
    assert_eq!(
        multiplier.rule["modifiers"][0]["sources"]["kind"],
        "attachedPermanent"
    );
    assert!(crate::engine::rule_is_executable(&multiplier.rule));

    let equip = parse_special_static_ability("Equip Wizard {1}")
        .expect("the restricted-equip leaf accepts a creature subtype");
    assert_eq!(equip.rule["ability"]["kind"], "equip");
    assert_eq!(equip.rule["ability"]["costs"][0]["manaCost"], "{1}");
    assert_eq!(equip.rule["ability"]["where"]["kind"], "and");
    assert!(
        equip.rule["ability"]["where"]["operands"]
            .as_array()
            .is_some_and(|operands| operands
                .iter()
                .any(|operand| operand["kind"] == "subtypeContains"))
    );
    assert!(crate::engine::rule_is_executable(&equip.rule));

    let old_variant = parse_special_static_ability("Equip legendary creature {2}")
        .expect("the existing characteristic restriction remains supported");
    assert!(
        old_variant.rule["ability"]["where"]["operands"]
            .as_array()
            .is_some_and(|operands| operands
                .iter()
                .any(|operand| operand["kind"] == "isLegendary"))
    );

    assert!(
        parse_common_static_ability(
            "If a triggered ability of equipped permanent triggers, that ability triggers an additional time.",
            "Test Staff",
        )
        .is_none()
    );
}

#[test]
fn destroy_then_controller_amass_binds_last_known_values_and_follow_up() {
    let parsed = parse_expansion_triggered(
        "When Azog enters, destroy up to one other target creature. Its controller amasses Goblins X, where X is that creature's power. If you controlled that creature, draw a card.",
        "Azog, Moria's Ruin",
    )
    .expect("the target, last-known values, amass, and controller branch compose");
    assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["minimum"], 0);
    assert_eq!(decision["maximum"], 1);
    assert_eq!(decision["candidates"]["excludeSource"], true);
    assert_eq!(decision["candidates"]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["effects"][0]["value"]["kind"], "controllerOf");
    assert_eq!(parsed.rule["effects"][1]["value"]["kind"], "powerOf");
    assert_eq!(parsed.rule["effects"][2]["kind"], "destroyPermanent");
    assert_eq!(parsed.rule["effects"][3]["kind"], "amass");
    assert_eq!(parsed.rule["effects"][3]["armySubtype"], "Goblin");
    assert_eq!(
        parsed.rule["effects"][4]["condition"]["kind"],
        "playersEqual"
    );
    assert_eq!(parsed.rule["effects"][4]["then"][0]["kind"], "drawCards");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When Test Ravager enters, destroy target artifact. Its controller amasses Orcs X, where X is that permanent's power. If you controlled that permanent, you gain two life.",
        "Test Ravager",
    )
    .expect("another criterion, Army subtype, and follow-up reuse the sequence");
    assert_eq!(variant.rule["declaration"]["decisions"][0]["minimum"], 1);
    assert_eq!(variant.rule["effects"][3]["armySubtype"], "Orc");
    assert_eq!(variant.rule["effects"][4]["then"][0]["kind"], "gainLife");

    assert!(
        parse_expansion_triggered(
            "When Azog enters, destroy up to one other target creature. Its controller amasses Goblins X, where X is that creature's power. If you owned that creature, draw a card.",
            "Azog, Moria's Ruin",
        )
        .is_none()
    );
}

#[test]
fn targeted_player_sacrifice_reuses_player_targets_and_permanent_criteria() {
    let parsed = parse_expansion_triggered(
        "When this Equipment enters, target opponent sacrifices a creature of their choice.",
        "Crude Bent Blade",
    )
    .expect("the Equipment entry, opponent target, and chosen sacrifice compose");
    assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "targetPlayer");
    assert_eq!(decision["candidates"]["where"]["kind"], "isOpponentOf");
    assert_eq!(parsed.rule["effects"][0]["kind"], "sacrificePermanents");
    assert_eq!(parsed.rule["effects"][0]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["effects"][0]["count"]["value"], 1);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_general_effect_instruction(
        "Target player sacrifices two artifacts of their choice.",
        "",
    )
    .expect("another player scope, count, and criterion reuse the sacrifice leaf");
    assert_eq!(variant.0[0]["where"]["value"], "Artifact");
    assert_eq!(variant.0[0]["count"]["value"], 2);
    assert!(variant.1[0]["candidates"].get("where").is_none());

    assert!(
        parse_general_effect_instruction("Target opponent sacrifices a creature you choose.", "",)
            .is_none()
    );
}

#[test]
fn conditional_own_casting_reduction_reuses_turn_event_conditions() {
    let parsed = parse_common_spell_ability(
        "This spell costs {3} less to cast if a creature died this turn.",
    )
    .expect("the generic reduction and death-event condition compose");
    let modifier = &parsed.rule["modifiers"][0];
    assert_eq!(modifier["kind"], "reduceOwnGenericCastingCost");
    assert_eq!(modifier["amount"]["value"], 3);
    assert_eq!(modifier["condition"]["kind"], "compare");
    assert_eq!(modifier["condition"]["left"]["kind"], "countEventsThisTurn");
    assert_eq!(modifier["condition"]["left"]["event"], "permanentDied");
    assert_eq!(modifier["condition"]["left"]["where"]["value"], "Creature");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let token_variant = parse_common_spell_ability(
        "This spell costs {2} less to cast if you created a token this turn.",
    )
    .expect("another existing turn-event condition reuses the reduction scaffold");
    assert_eq!(
        token_variant.rule["modifiers"][0]["condition"]["left"]["event"],
        "tokenCreated"
    );

    let targeted =
        parse_common_spell_ability("This spell costs {1} less to cast if it targets an artifact.")
            .expect("the target-qualified reduction remains owned by its narrower leaf");
    assert_eq!(
        targeted.rule["modifiers"][0]["targetWhere"]["value"],
        "Artifact"
    );

    assert!(
        parse_common_spell_ability(
            "This spell costs {3} less to cast if a creature attacked this turn.",
        )
        .is_none()
    );
}

#[test]
fn modal_temporary_death_replacement_and_target_player_group_modifier_compose() {
    let oracle = "Choose one \u{2014}\n\u{2022} Target creature gets -5/-5 until end of turn. If that creature would die this turn, exile it instead.\n\u{2022} Creatures target player controls get -1/-1 until end of turn.";
    let parsed = parse_general_modal_spell(oracle)
        .expect("the modal spell composes a targeted modifier and death replacement");

    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["id"],
        "chosenModes"
    );
    assert!(
        parsed.rule["declaration"]["decisions"]
            .as_array()
            .is_some_and(|decisions| decisions.iter().any(|decision| {
                decision["id"] == "mode1:targetPermanent" && decision["minimum"] == 1
            }))
    );
    assert!(
        parsed.rule["declaration"]["decisions"]
            .as_array()
            .is_some_and(|decisions| decisions.iter().any(|decision| {
                decision["id"] == "mode2:targetPlayer" && decision["minimum"] == 1
            }))
    );
    assert_eq!(
        parsed.rule["effects"][0]["then"][1]["kind"],
        "installDeathExileReplacement"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["object"]["kind"],
        "eachPermanent"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["object"]["player"]["id"],
        "mode2:targetPlayer"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_general_effect_sequence(
        "Target artifact creature gets -2/-2 until end of turn. If that permanent would die this turn, exile it instead.",
        "",
    )
    .expect("another permanent criterion reuses the temporary replacement leaf");
    assert_eq!(variant.0[1]["kind"], "installDeathExileReplacement");

    assert!(parse_general_effect_sequence(
        "Target creature gets -2/-2 until end of turn. If another creature would die this turn, exile it instead.",
        "",
    )
    .is_none());
}

#[test]
fn named_entry_or_attack_can_target_an_opponents_graveyard_optionally() {
    let parsed = parse_expansion_triggered(
        "Whenever Gollum enters or attacks, exile up to one target card from an opponent's graveyard. If you do, each opponent loses 2 life.",
        "Gollum the Abandoned",
    )
    .expect("the named source event, optional graveyard target, and follow-up compose");

    assert_eq!(parsed.rule["event"]["kind"], "oneOf");
    assert_eq!(
        parsed.rule["event"]["events"][0]["kind"],
        "enterBattlefield"
    );
    assert_eq!(
        parsed.rule["event"]["events"][1]["kind"],
        "declaredAttacker"
    );
    let decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(decision["id"], "targetGraveyardCard");
    assert_eq!(decision["minimum"], 0);
    assert_eq!(decision["candidates"]["zone"]["kind"], "graveyard");
    assert_eq!(
        decision["candidates"]["zone"]["player"]["kind"],
        "opponentsOf"
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "moveTargetCard");
    assert_eq!(parsed.rule["effects"][0]["to"], "exile");
    assert_eq!(parsed.rule["effects"][1]["kind"], "ifTargetWasChosen");
    assert_eq!(parsed.rule["effects"][1]["then"][0]["kind"], "loseLife");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let static_rule = parse_common_static_ability("Gollum can't block.", "Gollum the Abandoned")
        .expect("an unambiguous abbreviated source name reuses the combat restriction leaf");
    assert_eq!(static_rule.rule["modifiers"][0]["keyword"], "cantBlock");
    assert!(crate::engine::rule_is_executable(&static_rule.rule));

    let activated = parse_simple_activated_ability(
        "{2}, Sacrifice an artifact or creature: Return this card from your graveyard to your hand. Activate only as a sorcery.",
    )
    .expect("the graveyard self-return composes with mana and sacrifice costs");
    assert_eq!(activated.rule["activationZone"], "graveyard");
    assert_eq!(
        activated.rule["activationCondition"]["kind"],
        "sorceryTiming"
    );
    assert_eq!(
        activated.rule["effects"][0]["kind"],
        "moveAbilitySourceToHand"
    );
    assert!(crate::engine::rule_is_executable(&activated.rule));

    assert!(parse_expansion_triggered(
        "Whenever Gollum enters or attacks, exile up to one target card from your graveyard. If you do, each opponent loses 2 life.",
        "Gollum the Abandoned",
    )
    .is_some(), "your-graveyard scope remains a valid sibling variant");
}

#[test]
fn other_permanent_death_trigger_does_not_imply_a_controller_scope() {
    let parsed = parse_expansion_triggered(
        "Whenever one or more other creatures die, scry 1.",
        "Great Fierce Bee",
    )
    .expect("the unrestricted other-creature death event composes with scry");
    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(parsed.rule["event"]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["event"]["excludeSource"], true);
    assert!(parsed.rule["event"].get("player").is_none());
    assert_eq!(parsed.rule["effects"][0]["kind"], "scry");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let nontoken = parse_expansion_triggered(
        "Whenever another nontoken artifact dies, draw a card.",
        "Test Observer",
    )
    .expect("another criterion and the nontoken boundary reuse the event leaf");
    assert_eq!(nontoken.rule["event"]["where"]["value"], "Artifact");
    assert_eq!(nontoken.rule["event"]["nontoken"], true);

    let controlled = parse_expansion_triggered(
        "Whenever one or more other creatures you control die, scry 1.",
        "Test Observer",
    )
    .expect("the narrower controlled event remains owned by its dedicated leaf");
    assert_eq!(controlled.rule["event"]["player"]["kind"], "controller");
}

#[test]
fn stored_mana_value_parity_and_persistent_unused_modes_compose() {
    let replacement = parse_composed_entry_replacement(
        "As Gollum enters, choose odd or even. (Zero is even.)",
        "Gollum, Riddle Master",
    )
    .expect("the as-enters parity choice parses as a stored option");
    assert_eq!(replacement.rule["decisions"][0]["kind"], "chooseOption");
    assert_eq!(
        replacement.rule["decisions"][0]["options"],
        json!(["odd", "even"])
    );
    assert_eq!(replacement.rule["replacement"][0]["kind"], "storeDecision");
    assert!(crate::engine::rule_is_executable(&replacement.rule));

    let oracle = "Whenever an opponent casts a spell with mana value of the chosen quality, choose one that hasn't been chosen \u{2014}\n\u{2022} Put a +1/+1 counter on Gollum.\n\u{2022} Each opponent loses 2 life and you gain 2 life.\n\u{2022} Draw a card.";
    let triggered = parse_expansion_triggered(oracle, "Gollum, Riddle Master")
        .expect("the opponent spell event and persistent modes compose");
    assert_eq!(triggered.rule["event"]["kind"], "spellCast");
    assert_eq!(
        triggered.rule["event"]["manaValueParityDecisionId"],
        "chosenManaValueParity"
    );
    assert_eq!(triggered.rule["effects"][0]["kind"], "chooseUnusedMode");
    assert_eq!(
        triggered.rule["effects"][0]["modes"][0]["effects"][0]["kind"],
        "putCounters"
    );
    assert_eq!(
        triggered.rule["effects"][0]["modes"][1]["effects"][0]["kind"],
        "loseLife"
    );
    assert_eq!(
        triggered.rule["effects"][0]["modes"][2]["effects"][0]["kind"],
        "drawCards"
    );
    assert!(crate::engine::rule_is_executable(&triggered.rule));

    assert!(
        parse_composed_entry_replacement(
            "As Another Creature enters, choose odd or even.",
            "Gollum, Riddle Master",
        )
        .is_none()
    );
}

#[test]
fn life_loss_triggers_compose_with_triggering_player_quantities_and_zone_counts() {
    let mill = parse_expansion_triggered(
        "Whenever a player loses life, that player mills that many cards. (Damage causes loss of life.)",
        "Test Chronicler",
    )
    .expect("the life-loss event and linked player quantity compose");
    assert_eq!(mill.rule["event"]["kind"], "lifeLost");
    assert_eq!(mill.rule["event"]["player"]["kind"], "eachPlayer");
    assert_eq!(mill.rule["effects"][0]["kind"], "mill");
    assert_eq!(
        mill.rule["effects"][0]["player"]["kind"],
        "triggeringPlayer"
    );
    assert_eq!(
        mill.rule["effects"][0]["count"]["decisionId"],
        "lifeLostAmount"
    );
    assert!(crate::engine::rule_is_executable(&mill.rule));

    let draw = parse_expansion_triggered(
        "When this creature dies, draw a card for each graveyard with seven or more cards in it.",
        "Test Chronicler",
    )
    .expect("the death event and sized-zone count compose");
    assert_eq!(draw.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(
        draw.rule["effects"][0]["count"]["kind"],
        "countPlayerZonesByCardCount"
    );
    assert_eq!(draw.rule["effects"][0]["count"]["count"]["value"], 7);
    assert!(crate::engine::rule_is_executable(&draw.rule));

    let named_death = parse_expansion_triggered(
        "When The Master of Lake-town dies, draw a card for each graveyard with seven or more cards in it.",
        "The Master of Lake-town",
    )
    .expect("a named source death reuses the source-death event leaf");
    assert_eq!(named_death.rule["event"]["kind"], "permanentDied");
    assert_eq!(named_death.rule["event"]["object"]["kind"], "self");
    assert!(crate::engine::rule_is_executable(&named_death.rule));

    assert!(
        parse_expansion_triggered(
            "Whenever a player loses life, that player mills cards equal to your life total.",
            "Test Chronicler",
        )
        .is_none()
    );
}

#[test]
fn attacks_while_condition_reuses_permanent_criteria_and_shared_effects() {
    let pump = parse_expansion_triggered(
        "Ferocious \u{2014} Whenever this creature attacks while you control a creature with power 4 or greater, this creature gets +2/+2 until end of turn.",
        "Test Pursuer",
    )
    .expect("the source attack, controlled-power condition, and pump compose");
    assert_eq!(pump.rule["event"]["kind"], "declaredAttacker");
    assert_eq!(pump.rule["condition"]["kind"], "controlsPermanent");
    assert_eq!(pump.rule["condition"]["where"]["kind"], "and");
    assert_eq!(pump.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert!(crate::engine::rule_is_executable(&pump.rule));

    let life = parse_expansion_triggered(
        "Whenever Test Warg attacks while you control an artifact creature with power 5 or greater, you gain 2 life.",
        "Test Warg",
    )
    .expect("a named source and another criteria/effect pair reuse the scaffold");
    assert_eq!(life.rule["effects"][0]["kind"], "gainLife");
    assert!(crate::engine::rule_is_executable(&life.rule));

    assert!(parse_expansion_triggered(
        "Whenever another creature attacks while you control a creature with power 4 or greater, you gain 2 life.",
        "Test Warg",
    )
    .is_none());
}

#[test]
fn optional_sacrifice_links_last_known_power_to_counters_and_amass() {
    let counters = parse_expansion_triggered(
        "Whenever this creature attacks, you may sacrifice another creature. If you do, put a number of +1/+1 counters on this creature equal to the sacrificed creature's power.",
        "Test Rampager",
    )
    .expect("the optional sacrifice and linked power compose");
    let action = &counters.rule["effects"][0]["action"];
    assert_eq!(action["kind"], "sacrificePermanents");
    assert_eq!(action["excludeSource"], true);
    assert_eq!(action["bindPowerAs"], "sacrificedPermanentPower");
    assert_eq!(
        counters.rule["effects"][0]["onPerformed"][0]["count"]["kind"],
        "boundValue"
    );
    assert!(crate::engine::rule_is_executable(&counters.rule));

    let amass = parse_expansion_triggered(
        "When this creature dies, amass Goblins X, where X is this creature's power. (Put X +1/+1 counters on an Army you control.)",
        "Test Rampager",
    )
    .expect("death amass uses the source's last-known power");
    assert_eq!(amass.rule["effects"][0]["kind"], "amass");
    assert_eq!(amass.rule["effects"][0]["armySubtype"], "Goblin");
    assert_eq!(
        amass.rule["effects"][0]["count"]["kind"],
        "abilitySourceLastKnownPower"
    );
    assert!(crate::engine::rule_is_executable(&amass.rule));

    assert!(parse_general_effect_sequence(
        "You may sacrifice another creature. If you do, put a number of +1/+1 counters on this creature equal to the sacrificed artifact's power.",
        "Test Rampager",
    )
    .is_none());
}

#[test]
fn alternative_additional_costs_parse_in_either_order_through_shared_costs() {
    for oracle in [
        "As an additional cost to cast this spell, sacrifice an artifact or creature or pay {4}.",
        "As an additional cost to cast this spell, pay {2}{B} or sacrifice a nonland permanent.",
    ] {
        let parsed =
            parse_simple_spell_ability(oracle).expect("the alternative additional costs compose");
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["id"],
            "additionalCostMode"
        );
        assert_eq!(
            parsed.rule["declaration"]["additionalCosts"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_simple_spell_ability(
            "As an additional cost to cast this spell, sacrifice a creature and pay {4}.",
        )
        .is_none(),
        "the alternative leaf must not reinterpret conjunctive costs"
    );
}

#[test]
fn opponent_discard_and_optional_hand_count_are_generic_effects() {
    let discard = parse_expansion_triggered(
        "When this creature enters, each opponent discards a card.",
        "Test Goblins",
    )
    .expect("the entry event and opponent discard compose");
    assert_eq!(discard.rule["effects"][0]["kind"], "discardCards");
    assert_eq!(discard.rule["effects"][0]["player"]["kind"], "opponentsOf");
    assert!(crate::engine::rule_is_executable(&discard.rule));

    let linked = parse_expansion_triggered(
        "Whenever Test Loremaster or another Elf you control enters, you may discard your hand. Draw X cards, where X is the number of cards discarded this way. If you have an enduring story, Test Loremaster deals X damage to each opponent.",
        "Test Loremaster",
    )
    .expect("the optional hand discard binds its count for draw and damage");
    assert_eq!(linked.rule["event"]["kind"], "permanentEntered");
    assert_eq!(linked.rule["effects"][0]["kind"], "optionalAction");
    assert_eq!(
        linked.rule["effects"][0]["action"]["bindCountAs"],
        "discardedHandCount"
    );
    assert_eq!(
        linked.rule["effects"][0]["onPerformed"][1]["condition"]["kind"],
        "hasEnduringStory"
    );
    assert!(crate::engine::rule_is_executable(&linked.rule));

    assert!(parse_general_effect_instruction("Each opponent discards their hand.", "").is_none());
}

#[test]
fn recent_opponent_graveyard_cards_can_enter_with_replaced_types_and_an_ability() {
    let parsed = parse_generalized_zone_and_combat_ability(
        "Put onto the battlefield under your control all creature cards in your opponents' graveyards that were put there from the battlefield this turn. They are Food artifacts with \"{2}, {T}, Sacrifice this artifact: You gain 3 life.\" (They lose all other types and subtypes.)",
        "spellAbility",
        "Test Supper",
    )
    .expect("the recent multi-graveyard return and entry characteristics compose");
    let effect = &parsed.rule["effects"][0];
    assert_eq!(effect["kind"], "returnRecentGraveyardCards");
    assert_eq!(effect["player"]["kind"], "opponentsOf");
    assert_eq!(effect["all"], true);
    assert_eq!(effect["enterAs"]["types"], json!(["Artifact"]));
    assert_eq!(effect["enterAs"]["subtypes"], json!(["Food"]));
    assert_eq!(
        effect["enterAs"]["grantAbilities"][0]["kind"],
        "activatedAbility"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_generalized_zone_and_combat_ability(
        "Put onto the battlefield under your control all artifact cards in your opponents' graveyards that were put there from the battlefield this turn. They are Map artifacts with \"{1}, Sacrifice this artifact: Draw a card.\" (They lose all other types and subtypes.)",
        "spellAbility",
        "Test Recovery",
    )
    .expect("another card criterion, subtype, and granted ability reuse the scaffold");
    assert_eq!(
        variant.rule["effects"][0]["enterAs"]["subtypes"],
        json!(["Map"])
    );
    assert!(crate::engine::rule_is_executable(&variant.rule));

    assert!(parse_generalized_zone_and_combat_ability(
        "Put onto the battlefield under your control all creature cards in your graveyard that were put there from the battlefield this turn. They are Food artifacts with \"{2}, {T}, Sacrifice this artifact: You gain 3 life.\" (They lose all other types and subtypes.)",
        "spellAbility",
        "Test Supper",
    )
    .is_none());
}

#[test]
fn divided_damage_composes_target_count_and_positive_integer_distribution() {
    let parsed = parse_expansion_triggered(
        "When Test Spark enters, he deals 3 damage divided as you choose among one, two, or three targets.",
        "Test Spark",
    )
    .expect("source pronouns, target ranges, and divided damage compose");
    assert_eq!(parsed.rule["effects"][0]["kind"], "dealDividedDamage");
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["kind"],
        "chooseTargets"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["maximum"]["value"],
        3
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["kind"],
        "divideQuantityAmongTargets"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When this creature enters, it deals 2 damage divided as you choose among one or two targets.",
        "Test Ember",
    )
    .expect("another total and target range reuse the same leaves");
    assert!(crate::engine::rule_is_executable(&variant.rule));

    assert!(
        parse_expansion_triggered(
            "When this creature enters, it deals 2 damage divided as you choose among one, two, or three targets.",
            "Test Ember",
        )
        .is_none(),
        "every chosen target must receive at least one damage"
    );
}

#[test]
fn controlled_attachment_composes_two_independent_target_leaves() {
    let parsed = parse_expansion_triggered(
        "When this creature enters, attach target Equipment you control to up to one target creature you control.",
        "Test Stalwart",
    )
    .expect("the entry trigger composes two controlled permanent targets and attachment");
    let decisions = parsed.rule["declaration"]["decisions"]
        .as_array()
        .expect("the trigger declares its targets");
    assert_eq!(decisions.len(), 2);
    assert_eq!(decisions[0]["candidates"]["where"]["value"], "Equipment");
    assert_eq!(decisions[1]["candidates"]["where"]["value"], "Creature");
    assert_eq!(decisions[1]["minimum"]["value"], 0);
    assert_eq!(parsed.rule["effects"][0]["kind"], "attachPermanent");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let required = parse_expansion_triggered(
        "When this artifact enters, attach target Aura you control to target permanent you control.",
        "Test Binder",
    )
    .expect("different attachment types and a required recipient reuse the same leaves");
    assert_eq!(
        required.rule["declaration"]["decisions"][1]["minimum"]["value"],
        1
    );
    assert!(crate::engine::rule_is_executable(&required.rule));
}

#[test]
fn targeted_effect_can_install_a_linked_death_exile_replacement() {
    let damage = parse_general_effect_sequence(
        "Test Bolt deals 3 damage to target creature. If that creature would die this turn, exile it instead.",
        "Test Bolt",
    )
    .expect("targeted damage composes with the temporary death replacement");
    assert_eq!(damage.0[0]["kind"], "dealDamage");
    assert_eq!(damage.0[1]["kind"], "installDeathExileReplacement");
    assert_eq!(damage.0[1]["object"], chosen_target("targetPermanent"));

    let modifier = parse_general_effect_sequence(
        "Target creature gets -2/-2 until end of turn. If that creature would die this turn, exile it instead.",
        "Test Wither",
    )
    .expect("power/toughness modification still reuses the generalized sequence");
    assert_eq!(modifier.0[0]["kind"], "modifyPowerToughness");
    assert_eq!(modifier.0[1]["kind"], "installDeathExileReplacement");

    let modal = parse_general_modal_spell(
        "Choose one or both —\n• Test Strike deals 3 damage to target creature. If that creature would die this turn, exile it instead.\n• Destroy target artifact token.",
    )
    .expect("both generic modes compose into a modal spell");
    assert_eq!(modal.rule["declaration"]["decisions"][0]["maximum"], 2);
    assert!(crate::engine::rule_is_executable(&modal.rule));
}

#[test]
fn optional_discard_count_composes_with_an_independent_followup_effect() {
    let parsed = parse_expansion_triggered(
        "When this Equipment enters, you may discard a card. If you do, draw two cards.",
        "Test Spear",
    )
    .expect("an optional counted discard composes with the follow-up draw");
    let optional = &parsed.rule["effects"][0];
    assert_eq!(optional["kind"], "optionalAction");
    assert_eq!(optional["action"]["kind"], "discardCards");
    assert_eq!(optional["action"]["count"], integer(1));
    assert_eq!(optional["onPerformed"][0]["kind"], "drawCards");
    assert_eq!(optional["onPerformed"][0]["count"], integer(2));
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When this artifact enters, you may discard two cards. If you do, draw a card.",
        "Test Cache",
    )
    .expect("another discard quantity and leaf effect reuse the sequence");
    assert_eq!(variant.rule["effects"][0]["action"]["count"], integer(2));
    assert_eq!(
        variant.rule["effects"][0]["onPerformed"][0]["kind"],
        "drawCards"
    );
    assert!(crate::engine::rule_is_executable(&variant.rule));

    assert!(
        parse_general_effect_sequence(
            "You may discard a card. If you don't, draw two cards.",
            "Test Spear",
        )
        .is_none()
    );
}

#[test]
fn optional_sacrifice_composes_with_independent_followup_effects() {
    let parsed = parse_expansion_triggered(
        "When Test Host enters, you may sacrifice another creature or artifact. If you do, draw a card and create a Treasure token.",
        "Test Host",
    )
    .expect("the sacrifice criteria and independent follow-up effects compose");
    let optional = &parsed.rule["effects"][0];
    assert_eq!(optional["kind"], "optionalAction");
    assert_eq!(optional["action"]["kind"], "sacrificePermanents");
    assert_eq!(optional["action"]["where"]["kind"], "or");
    assert_eq!(optional["action"]["excludeSource"], true);
    assert_eq!(optional["onPerformed"][0]["kind"], "drawCards");
    assert_eq!(optional["onPerformed"][1]["kind"], "createTokens");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When Test Altar enters, you may sacrifice an artifact. If you do, you gain two life.",
        "Test Altar",
    )
    .expect("another sacrifice criterion and follow-up reuse the same leaves");
    assert_eq!(variant.rule["effects"][0]["action"]["excludeSource"], false);
    assert_eq!(
        variant.rule["effects"][0]["onPerformed"][0]["kind"],
        "gainLife"
    );

    let plural_named_source = parse_expansion_triggered(
        "When The Test Council enter, you may sacrifice another creature or artifact. If you do, draw a card and create a Treasure token.",
        "The Test Council",
    )
    .expect("a plural printed source name reuses the generic entry event");
    assert_eq!(
        plural_named_source.rule["event"]["kind"],
        "enterBattlefield"
    );
    assert_eq!(
        plural_named_source.rule["effects"][0]["kind"],
        "optionalAction"
    );

    assert!(
        parse_expansion_triggered(
            "When Test Host enters, you may sacrifice another creature. If you don't, draw a card.",
            "Test Host",
        )
        .is_none()
    );
}

#[test]
fn revealed_cards_can_select_a_random_matching_card_and_randomize_the_rest() {
    let parsed = parse_expansion_triggered(
        "When this artifact is put into a graveyard from the battlefield, reveal the top thirteen cards of your library. Put a random creature card from among them onto the battlefield. Put the rest on the bottom of your library in a random order.",
        "Test Barrel",
    )
    .expect("the death event and random top-card partition compose");
    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(parsed.rule["effects"][0]["kind"], "lookAtTopCards");
    assert_eq!(parsed.rule["effects"][0]["count"]["value"], 13);
    assert_eq!(parsed.rule["effects"][1]["kind"], "revealCards");
    assert_eq!(parsed.rule["effects"][2]["kind"], "chooseCards");
    assert_eq!(parsed.rule["effects"][2]["selection"]["kind"], "random");
    assert_eq!(parsed.rule["effects"][2]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["effects"][4]["order"]["kind"], "random");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When this creature dies, reveal the top five cards of your library. Put one random artifact card from among them onto the battlefield. Put the rest on the bottom of your library in random order.",
        "Test Salvager",
    )
    .expect("another quantity and criterion reuse the random partition leaves");
    assert_eq!(variant.rule["effects"][0]["count"]["value"], 5);
    assert_eq!(variant.rule["effects"][2]["where"]["value"], "Artifact");

    assert!(
        parse_expansion_triggered(
            "When this artifact dies, reveal the top five cards of your library. Put a chosen creature card from among them onto the battlefield. Put the rest on the bottom of your library in a random order.",
            "Test Barrel",
        )
        .is_none()
    );
}

#[test]
fn first_matching_spell_reuses_cost_reduction_and_flash_leaves() {
    let parsed = parse_common_static_ability(
        "The first creature spell you cast each turn costs {2} less to cast and can be cast as though it had flash.",
        "Test Sage",
    )
    .expect("first-spell qualification composes reduction and flash");
    assert_eq!(parsed.rule["modifiers"][0]["kind"], "reduceCastingCost");
    assert_eq!(parsed.rule["modifiers"][0]["where"]["value"], "Creature");
    assert_eq!(parsed.rule["modifiers"][0]["amount"]["value"], 2);
    assert_eq!(parsed.rule["modifiers"][0]["firstEachTurn"], true);
    assert_eq!(parsed.rule["modifiers"][1]["kind"], "grantFlashCasting");
    assert_eq!(parsed.rule["modifiers"][1]["firstEachTurn"], true);
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_common_static_ability(
        "The first artifact spell you cast each turn costs {1} less to cast and can be cast as though it had flash.",
        "Test Artificer",
    )
    .expect("another spell criterion and reduction reuse the same leaves");
    assert_eq!(variant.rule["modifiers"][0]["where"]["value"], "Artifact");
    assert_eq!(variant.rule["modifiers"][0]["amount"]["value"], 1);

    assert!(
        parse_common_static_ability(
            "The first creature spell you cast each turn costs {G} less to cast and can be cast as though it had flash.",
            "Test Sage",
        )
        .is_none()
    );
}

#[test]
fn reveal_until_uses_a_numeric_threshold_to_choose_the_destination() {
    let parsed = parse_expansion_triggered(
        "Whenever a nontoken creature you control dies, reveal cards from the top of your library until you reveal a creature card. If its mana value is less than or equal to the number of lands you control, put it onto the battlefield. Otherwise, put it into your hand. Put the rest on the bottom of your library in a random order. This ability triggers only once each turn.",
        "Test Parting",
    )
    .expect("the death event, reveal partition, threshold, and trigger limit compose");
    assert_eq!(parsed.rule["event"]["kind"], "permanentDied");
    assert_eq!(parsed.rule["event"]["nontoken"], true);
    assert_eq!(parsed.rule["triggerLimit"]["kind"], "onceEachTurn");
    let effect = &parsed.rule["effects"][0];
    assert_eq!(effect["kind"], "revealUntilAndPutOntoBattlefield");
    assert_eq!(effect["where"]["value"], "Creature");
    assert_eq!(effect["maximumManaValue"]["kind"], "countPermanents");
    assert_eq!(effect["maximumManaValue"]["where"]["value"], "Land");
    assert_eq!(effect["otherwise"], "hand");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let variant = parse_expansion_triggered(
        "When this artifact dies, reveal cards from the top of your library until you reveal an artifact card. If its mana value is less than or equal to the number of creatures you control, put it onto the battlefield tapped. Otherwise, put it into your hand. Put the rest on the bottom of your library in random order.",
        "Test Reliquary",
    )
    .expect("other card criteria, threshold criteria, and tapped destination reuse the leaves");
    assert_eq!(variant.rule["effects"][0]["where"]["value"], "Artifact");
    assert_eq!(
        variant.rule["effects"][0]["maximumManaValue"]["where"]["value"],
        "Creature"
    );
    assert_eq!(variant.rule["effects"][0]["tapped"], true);

    assert!(
        parse_expansion_triggered(
            "When this artifact dies, reveal cards from the top of your library until you reveal an artifact card. If its power is less than or equal to the number of creatures you control, put it onto the battlefield. Otherwise, put it into your hand. Put the rest on the bottom of your library in random order.",
            "Test Reliquary",
        )
        .is_none()
    );
}

#[test]
fn top_library_exile_permission_accepts_both_oracle_clause_orders() {
    for text in [
        "Exile the top card of your library. Until the end of your next turn, you may play that card.",
        "Exile the top card of your library. You may play it until the end of your next turn.",
        "Exile the top two cards of your library. You may play them until end of turn.",
    ] {
        let parsed = parse_general_effect_instruction(text, "Test Impulse")
            .unwrap_or_else(|| panic!("top-library permission did not parse: {text}"));
        assert_eq!(parsed.0[0]["kind"], "exileTopCards");
        assert_eq!(parsed.0[1]["kind"], "grantPermission");
    }

    let activated = parse_simple_activated_ability(
        "Sacrifice another creature or artifact: Exile the top card of your library. You may play it until the end of your next turn. Activate only during your turn and only once each turn.",
    )
    .expect("generic costs, effects, timing, and frequency compose");
    assert_eq!(activated.rule["costs"][0]["kind"], "sacrificePermanent");
    assert_eq!(
        activated.rule["activationCondition"]["kind"],
        "duringControllerTurn"
    );
    assert_eq!(activated.rule["activationLimit"]["kind"], "oncePerTurn");
    assert!(crate::engine::rule_is_executable(&activated.rule));

    assert!(
        parse_general_effect_instruction(
            "Exile the top card of your library. You may draw it until the end of your next turn.",
            "Test Impulse",
        )
        .is_none()
    );
}

#[test]
fn hob_compositions_reuse_leaf_grammar_and_reject_mismatched_restrictions() {
    let variable_counter = parse_general_effect_instruction(
        "Put X charge counters on target artifact you control, where X is that spell's mana value.",
        "Test Charge",
    )
    .expect("a triggering spell mana value composes with the target-counter leaf");
    assert_eq!(
        variable_counter.0[0]["count"]["kind"],
        "triggeringSpellManaValue"
    );
    assert_eq!(
        variable_counter.1[0]["candidates"]["where"]["value"],
        "Artifact"
    );

    let linked = parse_general_effect_sequence(
        "Untap another target artifact you control. If that permanent is a Clue, put two charge counters on it.",
        "Test Investigator",
    )
    .expect("another-target selection composes with a linked filter and counter");
    assert_eq!(linked.1[0]["candidates"]["excludeSource"], true);
    assert_eq!(linked.0[1]["kind"], "conditionalEffect");
    assert_eq!(linked.0[1]["then"][0]["count"], integer(2));

    let animation = parse_simple_activated_ability(
        "{4}{U}: This artifact becomes a Serpent creature in addition to its other types and gains \"This creature's power and toughness are each equal to the number of lands you control.\" (This effect doesn't end.)",
    )
    .expect("a trailing reminder after a quoted static ability is removed as one leaf");
    assert_eq!(animation.rule["effects"][0]["kind"], "addCardType");
    assert_eq!(animation.rule["effects"][1]["subtype"], "Serpent");
    assert_eq!(animation.rule["effects"][2]["kind"], "grantAbility");
    assert!(crate::engine::rule_is_executable(&animation.rule));

    let restricted = parse_mana_ability(
        "{T}: Add X mana of any one color, where X is this creature's power. Spend this mana only to cast Wizard spells or activate abilities of Wizard sources.",
    )
    .expect("cast-or-activation restrictions share one matching subtype filter");
    assert_eq!(
        restricted.rule["effects"][0]["spendRestriction"]["kind"],
        "castSpellOrActivateAbility"
    );
    assert!(crate::engine::rule_is_executable(&restricted.rule));

    assert!(
        parse_mana_ability(
            "{T}: Add X mana of any one color, where X is this creature's power. Spend this mana only to cast Wizard spells and activate abilities of Elf sources.",
        )
        .is_none(),
        "different cast and activation filters cannot be merged without changing semantics"
    );
}

#[test]
fn face_down_top_exile_links_a_live_conditional_play_permission() {
    for (text, count) in [
        (
            "Look at the top card of your library and exile it face down. For as long as it remains exiled, you may play it if you control an Elf.",
            1,
        ),
        (
            "Look at the top two cards of your library and exile them face down. For as long as they remain exiled, you may play them if you control a Wizard.",
            2,
        ),
    ] {
        let (effects, decisions) = parse_general_effect_sequence(text, "Test Seer")
            .unwrap_or_else(|| panic!("linked face-down permission should parse: {text}"));
        assert!(decisions.is_empty());
        assert_eq!(effects[0]["kind"], "exileTopCards");
        assert_eq!(effects[0]["count"], integer(count));
        assert_eq!(effects[0]["faceDown"], true);
        assert_eq!(effects[1]["kind"], "grantCardPermission");
        assert_eq!(effects[1]["condition"]["kind"], "controlsPermanent");
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "spellAbility",
            "source": self_ref(),
            "effects": effects,
        })));
    }

    assert!(
        parse_general_effect_sequence(
            "Look at the top two cards of your library and exile it face down. For as long as they remain exiled, you may play them if you control a Wizard.",
            "Test Seer",
        )
        .is_none(),
        "singular and plural linked references must agree",
    );
}

#[test]
fn target_attachment_groups_feed_a_reflexive_power_damage_trigger() {
    for text in [
        "Attach any number of target Equipment you control to target creature you control. When one or more Equipment become attached to that creature this way, that creature deals damage equal to its power to up to one target creature.",
        "Attach any number of target Auras you control to target permanent you control. When one or more Auras become attached to that permanent this way, that permanent deals damage equal to its power to target creature.",
    ] {
        let (effects, decisions) = parse_general_effect_sequence(text, "Test Armorer")
            .unwrap_or_else(|| panic!("attachment-reflexive family should parse: {text}"));
        assert_eq!(decisions.len(), 2);
        assert_eq!(decisions[0]["kind"], "chooseTargets");
        assert_eq!(decisions[0]["minimum"], integer(0));
        assert_eq!(decisions[0]["maximum"]["kind"], "countPermanents");
        assert_eq!(effects[0]["kind"], "bind");
        assert_eq!(effects[1]["kind"], "attachPermanent");
        assert_eq!(effects[2]["condition"]["kind"], "bindingNotEmpty");
        assert_eq!(
            effects[2]["then"][0]["ability"]["effects"][0]["source"]["kind"],
            "boundValue"
        );
        assert!(crate::engine::rule_is_executable(&json!({
            "kind": "triggeredAbility",
            "source": self_ref(),
            "event": { "kind": "enterBattlefield", "object": self_ref() },
            "declaration": { "kind": "castingDeclaration", "decisions": decisions },
            "effects": effects,
        })));
    }

    assert!(
        parse_general_effect_sequence(
            "Attach any number of target Equipment you control to target creature you control. When one or more Auras become attached to that creature this way, that creature deals damage equal to its power to target creature.",
            "Test Armorer",
        )
        .is_none(),
        "the later attachment criterion must refer to the selected attachment family",
    );
}

#[test]
fn source_can_inherit_filtered_activated_abilities_from_its_controllers_zone() {
    for (text, zone, filter_kind) in [
        (
            "Thranduil has all activated abilities of all Elf cards in your graveyard.",
            "graveyard",
            "subtypeContains",
        ),
        (
            "This creature has all activated abilities of all artifact cards in your exile.",
            "exile",
            "cardTypeContains",
        ),
    ] {
        let parsed = parse_common_static_ability(text, "Thranduil")
            .unwrap_or_else(|| panic!("zone ability inheritance should parse: {text}"));
        let modifier = &parsed.rule["modifiers"][0];
        assert_eq!(modifier["kind"], "grantActivatedAbilitiesFromZone");
        assert_eq!(modifier["objects"]["kind"], "self");
        assert_eq!(modifier["cards"]["zone"]["kind"], zone);
        assert_eq!(modifier["cards"]["where"]["kind"], filter_kind);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_common_static_ability(
            "Another creature has all activated abilities of all Elf cards in your graveyard.",
            "Thranduil",
        )
        .is_none(),
        "the subject must resolve through the reusable source-reference grammar",
    );
}

#[test]
fn conditional_source_sacrifice_composes_counted_permanents_and_followup_effects() {
    let parsed = parse_generalized_zone_and_combat_ability(
        "I, II, III, IV — Create a Treasure token. Then if you control four or more Treasures, sacrifice this Saga. If you do, create a 6/6 red Dragon creature token with flying.",
        "triggeredAbility",
        "Test Mountain",
    )
    .expect("Saga chapters compose token creation, a permanent threshold, and source sacrifice");
    assert_eq!(
        parsed.rule["event"]["chapters"],
        json!([integer(1), integer(2), integer(3), integer(4)])
    );
    assert_eq!(parsed.rule["effects"][0]["kind"], "createTokens");
    let conditional = &parsed.rule["effects"][1];
    assert_eq!(conditional["condition"]["kind"], "compare");
    assert_eq!(conditional["condition"]["left"]["kind"], "countPermanents");
    assert_eq!(
        conditional["condition"]["left"]["where"]["value"],
        "Treasure"
    );
    assert_eq!(conditional["then"][0]["kind"], "sacrificePermanent");
    assert_eq!(conditional["then"][0]["bind"], "sacrificedSource");
    assert_eq!(
        conditional["then"][1]["condition"]["kind"],
        "bindingNotEmpty"
    );
    assert_eq!(
        conditional["then"][1]["then"][0]["token"]["subtypes"][0],
        "Dragon"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let reminder = parse_generalized_zone_and_combat_ability(
        "I, II, III, IV — Create a Treasure token. Then if you control four or more Treasures, sacrifice this Saga. If you do, create a 6/6 red Dragon creature token with flying. (A Treasure token is an artifact with \"{T}, Sacrifice this token: Add one mana of any color.\")",
        "staticAbility",
        "Test Mountain",
    )
    .expect("a trailing token reminder does not obscure the shared Saga instructions");
    assert_eq!(reminder.rule, parsed.rule);

    let variant = parse_generalized_zone_and_combat_ability(
        "II — Create a Food token. Then if you control five or more Foods, sacrifice this Saga. If you do, create a 4/4 green Beast creature token with trample.",
        "staticAbility",
        "Test Feast",
    )
    .expect("another token subtype, threshold, and reward reuse the same leaves");
    assert_eq!(
        variant.rule["effects"][1]["condition"]["left"]["where"]["value"],
        "Food"
    );
    assert_eq!(
        variant.rule["effects"][1]["then"][1]["then"][0]["token"]["subtypes"][0],
        "Beast"
    );
    assert!(crate::engine::rule_is_executable(&variant.rule));

    assert!(
        parse_general_effect_sequence(
            "Create a Treasure token. Then if you own four or more Treasures, sacrifice this Saga. If you do, create a 6/6 red Dragon creature token with flying.",
            "Test Mountain",
        )
        .is_none()
    );
}
#[test]
fn temporary_quoted_trigger_grant_composes_target_and_trigger_grammar() {
    for text in [
        "Until end of turn, target creature gains \"Whenever this creature deals combat damage to a player, create a Treasure token.\"",
        "Until end of turn, target artifact creature gains \"Whenever this creature deals combat damage to a player, create a Food token.\"",
    ] {
        let (effects, decisions) = parse_general_effect_instruction(text, "Test Hall")
            .unwrap_or_else(|| panic!("temporary quoted trigger did not parse: {text}"));
        assert_eq!(decisions.len(), 1);
        assert_eq!(effects.len(), 1);
        assert_eq!(effects[0]["kind"], "grantAbility");
        assert_eq!(effects[0]["duration"]["kind"], "untilEndOfCurrentTurn");
        assert_eq!(
            effects[0]["ability"]["event"]["kind"],
            "combatDamageToPlayer"
        );
        assert!(crate::engine::effect_supported(&effects[0]));
    }

    assert!(
        parse_general_effect_instruction(
            "Until end of turn, target creature gains \"This is not a triggered ability.\"",
            "Test Hall",
        )
        .is_none()
    );
}

#[test]
fn hoc_damage_control_mana_and_alternative_events_compose_shared_leaves() {
    let damaged = parse_expansion_triggered(
        "Whenever Test Dragon is dealt noncombat damage, create that many Treasure tokens.",
        "Test Dragon",
    )
    .expect("noncombat source damage should feed the recorded amount into token creation");
    assert_eq!(damaged.rule["event"]["kind"], "permanentDealtDamage");
    assert_eq!(damaged.rule["event"]["noncombatOnly"], true);
    assert_eq!(
        damaged.rule["effects"][0]["quantity"]["decisionId"],
        "damageAmount"
    );
    assert!(crate::engine::rule_is_executable(&damaged.rule));

    let control = parse_generalized_zone_and_combat_ability(
        "For each opponent, gain control of up to one target artifact that player controls.",
        "spellAbility",
        "Test Burglaring",
    )
    .expect("per-opponent control change should compose multiplayer target constraints");
    let decision = &control.rule["declaration"]["decisions"][0];
    assert_eq!(decision["maximum"]["kind"], "countOpponents");
    assert_eq!(
        decision["selectionConstraint"]["kind"],
        "distinctPermanentControllers"
    );
    assert_eq!(
        control.rule["effects"][0]["permanent"]["kind"],
        "chosenTargets"
    );
    assert!(crate::engine::rule_is_executable(&control.rule));

    let mana = parse_simple_spell_ability("Add {R} for each artifact your opponents control.")
        .expect("fixed mana should accept the shared opponent permanent count expression");
    assert_eq!(mana.rule["effects"][0]["mana"]["kind"], "fixedMana");
    assert_eq!(mana.rule["effects"][0]["mana"]["symbol"], "R");
    assert_eq!(
        mana.rule["effects"][0]["mana"]["amount"]["kind"],
        "countPermanentsControlledByOpponents"
    );
    assert!(crate::engine::rule_is_executable(&mana.rule));

    let alternatives = parse_expansion_triggered(
        "Whenever you cast a green spell and whenever a Forest you control enters, put a +1/+1 counter on target creature you control.",
        "Test Necklace",
    )
    .expect("two independent events should share one counter effect and declaration");
    assert_eq!(alternatives.rule["event"]["kind"], "oneOf");
    assert_eq!(alternatives.rule["event"]["events"][0]["kind"], "spellCast");
    assert_eq!(
        alternatives.rule["event"]["events"][1]["kind"],
        "permanentEntered"
    );
    assert_eq!(
        alternatives.rule["event"]["events"][1]["where"]["value"],
        "Forest"
    );
    assert_eq!(alternatives.rule["effects"][0]["kind"], "putCounters");
    assert!(crate::engine::rule_is_executable(&alternatives.rule));

    assert!(parse_expansion_triggered(
        "Whenever you cast a green spell and sometimes a Forest enters, put a +1/+1 counter on target creature you control.",
        "Test Necklace",
    )
    .is_none());
}

#[test]
fn named_source_counter_cost_and_linked_counter_distribution_compose() {
    for text in [
        "{1}, Remove an indestructible counter from Test Queen: Another target creature gains indestructible until end of turn. Put a +1/+1 counter and a lifelink counter on that creature and a +1/+1 counter and a lifelink counter on Test Queen.",
        "{U}, Remove one charge counter from Test Device: Another target artifact creature gains indestructible until end of turn. Put a flying counter on that permanent and a charge counter on Test Device.",
    ] {
        let parsed = parse_simple_activated_ability(text)
            .unwrap_or_else(|| panic!("named-source counter activation did not parse: {text}"));
        assert_eq!(parsed.rule["costs"][1]["kind"], "removeCounters");
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["candidates"]["excludeSource"],
            true
        );
        assert_eq!(parsed.rule["effects"][0]["kind"], "grantKeyword");
        assert!(parsed.rule["effects"].as_array().is_some_and(|effects| {
            effects
                .iter()
                .filter(|effect| effect["kind"] == "putCounters")
                .count()
                >= 2
        }));
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(parse_simple_activated_ability(
        "{1}, Remove a charge counter from an artifact you control: Another target creature gains indestructible until end of turn. Put a +1/+1 counter on that creature and a charge counter on Test Device.",
    )
    .is_none());
}

#[test]
fn activated_modal_instructions_reuse_cost_and_effect_grammars() {
    for text in [
        "{1}, Sacrifice this creature: Choose one \u{2014}\n\u{2022} This creature deals 2 damage to target creature.\n\u{2022} Destroy target colorless nonland permanent.",
        "{U}, Sacrifice this artifact: Choose one \u{2014}\n\u{2022} This artifact deals 3 damage to target attacking creature.\n\u{2022} Destroy target nonland enchantment.",
    ] {
        let parsed = parse_simple_activated_ability(text)
            .unwrap_or_else(|| panic!("activated modal instruction did not parse: {text}"));
        assert_eq!(parsed.rule["kind"], "activatedAbility");
        assert_eq!(parsed.rule["costs"][1]["kind"], "sacrificePermanent");
        assert_eq!(
            parsed.rule["declaration"]["decisions"][0]["kind"],
            "chooseModes"
        );
        assert_eq!(parsed.rule["effects"][0]["kind"], "conditional");
        assert_eq!(parsed.rule["effects"][1]["kind"], "conditional");
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(parse_simple_activated_ability(
        "{1}, Sacrifice this creature: Choose whichever you prefer \u{2014}\n\u{2022} This creature deals 2 damage to target creature.\n\u{2022} Destroy target artifact.",
    )
    .is_none());
}

#[test]
fn graveyard_source_return_composes_zone_condition_and_tapped_state() {
    for (text, tapped, criteria) in [
        (
            "{2}{B}: Return this card from your graveyard to the battlefield tapped. Activate only if you control a legendary creature.",
            true,
            "a legendary creature",
        ),
        (
            "{1}{G}: Return this card from your graveyard to the battlefield. Activate only if you control an artifact.",
            false,
            "an artifact",
        ),
    ] {
        let parsed = parse_simple_activated_ability(text)
            .unwrap_or_else(|| panic!("conditional graveyard return did not parse: {text}"));
        assert_eq!(parsed.rule["activationZone"], "graveyard");
        assert_eq!(
            parsed.rule["activationCondition"]["kind"],
            "controlsPermanent"
        );
        assert_eq!(
            parsed.rule["activationCondition"]["where"],
            parse_permanent_criteria(criteria, "").expect("test criteria parses")
        );
        assert_eq!(parsed.rule["effects"][0]["tapped"], tapped);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(parse_simple_activated_ability(
        "{2}{B}: Return another card from your graveyard to the battlefield tapped. Activate only if you control a legendary creature.",
    )
    .is_none());
}

#[test]
fn activated_combat_effects_compose_target_criteria_and_conditional_reduction() {
    let unblockable = parse_simple_activated_ability(
        "{T}: Target creature with power 2 or less can't be blocked this turn.",
    )
    .expect("power-qualified target should compose with temporary unblockable");
    assert_eq!(
        unblockable.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );
    assert_eq!(unblockable.rule["effects"][0]["keyword"], "cantBeBlocked");
    assert!(crate::engine::rule_is_executable(&unblockable.rule));

    let anthem = parse_simple_activated_ability(
        "{4}{W}, {T}: Creatures you control get +1/+1 until end of turn. This ability costs {2} less to activate if you control a legendary creature.",
    )
    .expect("conditional fixed reduction should compose with a controlled-creature anthem");
    assert_eq!(anthem.rule["manaCostReduction"]["kind"], "conditionalValue");
    assert_eq!(anthem.rule["manaCostReduction"]["ifTrue"]["value"], 2);
    assert_eq!(anthem.rule["effects"][0]["kind"], "modifyPowerToughness");
    assert!(crate::engine::rule_is_executable(&anthem.rule));

    assert!(parse_simple_activated_ability(
        "{4}{W}, {T}: Creatures you control get +1/+1 until end of turn. This ability costs two less to activate if you control a legendary creature.",
    )
    .is_none());
}

#[test]
fn source_entry_counter_threshold_and_each_combat_forms_compose() {
    let entry = parse_shock_land_replacement(
        "As Test Gate enters, you may pay 3 life. If you don't, it enters tapped.",
    )
    .expect("a named source should reuse optional life entry replacement");
    assert_eq!(entry.rule["decisions"][0]["cost"]["amount"]["value"], 3);
    assert!(crate::engine::rule_is_executable(&entry.rule));

    let threshold = parse_common_static_ability(
        "As long as there are four or more lore counters among Sagas you control, Test Bard has hexproof and indestructible.",
        "Test Bard",
    )
    .expect("counter totals among controlled permanents should gate source keywords");
    assert_eq!(
        threshold.rule["modifiers"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(
        threshold.rule["modifiers"][0]["condition"]["left"]["kind"],
        "countCountersOnPermanents"
    );
    assert_eq!(
        threshold.rule["modifiers"][0]["condition"]["left"]["counter"],
        "lore"
    );
    assert!(crate::engine::rule_is_executable(&threshold.rule));

    let combat = parse_expansion_triggered(
        "At the beginning of each combat, other Goblins and Orcs you control get +2/+2 until end of turn. Creatures your opponents control get -1/-1 until end of turn.",
        "Test Warlord",
    )
    .expect("each-combat event should compose two shared group stat effects");
    assert_eq!(combat.rule["event"]["step"], "beginCombat");
    assert_eq!(combat.rule["event"]["player"]["kind"], "eachPlayer");
    assert_eq!(combat.rule["effects"].as_array().map(Vec::len), Some(2));
    assert!(crate::engine::rule_is_executable(&combat.rule));

    assert!(
        parse_shock_land_replacement(
            "As another land enters, you may pay 3 life. If you don't, it enters tapped.",
        )
        .is_none()
    );
}

#[test]
fn qualified_blockers_and_attached_turn_keywords_reuse_static_modifiers() {
    let blockers = parse_common_static_ability(
        "Test Adventurer can't be blocked by creatures with power 3 or greater.",
        "Test Adventurer",
    )
    .expect("a named source should compose with qualified blocker criteria");
    assert_eq!(blockers.rule["modifiers"][0]["kind"], "blockRestriction");
    assert_eq!(
        blockers.rule["modifiers"][0]["blockers"]["where"]["kind"],
        "and"
    );
    assert!(crate::engine::rule_is_executable(&blockers.rule));

    let attached = parse_common_static_ability(
        "During your turn, equipped creature has hexproof and can't be blocked.",
        "Test Ring",
    )
    .expect("attached permanents should receive controller-turn keyword lists");
    assert_eq!(attached.rule["modifiers"].as_array().map(Vec::len), Some(2));
    assert!(
        attached.rule["modifiers"]
            .as_array()
            .is_some_and(|modifiers| {
                modifiers.iter().all(|modifier| {
                    modifier["objects"]["kind"] == "attachedPermanent"
                        && modifier["condition"]["kind"] == "duringControllerTurn"
                })
            })
    );
    assert!(crate::engine::rule_is_executable(&attached.rule));

    assert!(
        parse_common_static_ability(
            "Another creature can't be blocked by creatures with power 3 or greater.",
            "Test Adventurer",
        )
        .is_none()
    );
}

#[test]
fn life_threshold_counter_scaling_and_qualified_counterspell_compose() {
    let end_step = parse_expansion_triggered(
        "At the beginning of each end step, if you gained 3 or more life this turn, draw a card.",
        "Test Host",
    )
    .expect("life-gained thresholds should compose with each-end-step triggers");
    assert_eq!(end_step.rule["condition"]["kind"], "compare");
    assert_eq!(
        end_step.rule["condition"]["left"]["kind"],
        "lifeGainedThisTurn"
    );
    assert_eq!(end_step.rule["condition"]["right"]["value"], 3);
    assert!(crate::engine::rule_is_executable(&end_step.rule));

    let upkeep = parse_expansion_triggered(
        "At the beginning of your upkeep, you lose 1 life for each burden counter on Test Ring.",
        "Test Ring",
    )
    .expect("life loss should scale from counters on a named source");
    assert_eq!(upkeep.rule["effects"][0]["kind"], "loseLife");
    assert_eq!(upkeep.rule["effects"][0]["amount"]["kind"], "countCounters");
    assert_eq!(upkeep.rule["effects"][0]["amount"]["counter"], "burden");
    assert!(crate::engine::rule_is_executable(&upkeep.rule));

    let counter =
        parse_counter_spell("Counter target creature spell with power or toughness 2 or less.")
            .expect("spell targets should reuse power-or-toughness criteria");
    assert_eq!(
        counter.rule["declaration"]["decisions"][0]["candidates"]["where"]["kind"],
        "and"
    );
    assert!(crate::engine::rule_is_executable(&counter.rule));

    assert!(parse_condition_text("you gained several life this turn").is_none());
    assert!(
        parse_general_effect_instruction(
            "You lose 1 life for each burden counter on another permanent.",
            "Test Ring",
        )
        .is_none()
    );
}

#[test]
fn filtered_block_prohibition_and_static_ward_bonus_compose() {
    let spell = parse_simple_spell_ability(
        "Destroy target artifact or land. Creatures without flying can't block this turn.",
    )
    .expect("destruction should sequence with a filtered blocking prohibition");
    assert_eq!(spell.rule["effects"][0]["kind"], "destroyPermanent");
    assert_eq!(spell.rule["effects"][1]["kind"], "grantKeyword");
    assert_eq!(spell.rule["effects"][1]["keyword"], "cantBlock");
    assert_eq!(spell.rule["effects"][1]["object"]["where"]["kind"], "and");
    assert!(crate::engine::rule_is_executable(&spell.rule));

    let anthem = parse_common_static_ability(
        "Legendary creatures you control get +2/+1 and have ward {1}.",
        "Test Tree",
    )
    .expect("a controlled selector should share a stat bonus and parsed ward cost");
    assert_eq!(anthem.rule["modifiers"][0]["kind"], "modifyPowerToughness");
    assert_eq!(anthem.rule["modifiers"][1]["kind"], "grantWard");
    assert_eq!(anthem.rule["modifiers"][1]["cost"]["manaCost"], "{1}");
    assert!(crate::engine::rule_is_executable(&anthem.rule));

    assert!(
        parse_general_effect_instruction("Target creature can't block this turn.", "Test Fire")
            .is_none()
    );
}

#[test]
fn filtered_attack_hand_power_entry_count_and_block_condition_compose() {
    let attack = parse_expansion_triggered(
        "Whenever you attack with one or more Elves, scry 1.",
        "Test Seer",
    )
    .expect("filtered controlled attackers should compose with a shared effect");
    assert_eq!(attack.rule["event"]["kind"], "controlledCreaturesAttacked");
    assert_eq!(attack.rule["event"]["where"]["value"], "Elf");
    assert_eq!(attack.rule["effects"][0]["kind"], "scry");
    assert!(crate::engine::rule_is_executable(&attack.rule));

    let hand_power = parse_common_static_ability(
        "Test Garrison's power is equal to the number of cards in your hand.",
        "Test Garrison",
    )
    .expect("named source power should reuse the hand-count numeric expression");
    assert_eq!(
        hand_power.rule["modifiers"][0]["power"]["kind"],
        "countCards"
    );
    assert!(crate::engine::rule_is_executable(&hand_power.rule));

    let entry = parse_common_static_ability(
        "This enchantment enters with a hope counter on it for each creature you control.",
        "Test Dawn",
    )
    .expect("entry counters should accept a reusable controlled-permanent count");
    assert_eq!(entry.rule["replacement"][0]["kind"], "putEnteringCounters");
    assert_eq!(entry.rule["replacement"][0]["counter"], "hope");
    assert_eq!(
        entry.rule["replacement"][0]["count"]["kind"],
        "countPermanents"
    );
    assert!(crate::engine::rule_is_executable(&entry.rule));

    let blocker = parse_common_static_ability(
        "This creature can't block unless you control a Goblin or Orc.",
        "Test Crusher",
    )
    .expect("a source blocking prohibition should negate a controlled-permanent condition");
    assert_eq!(blocker.rule["modifiers"][0]["keyword"], "cantBlock");
    assert_eq!(blocker.rule["modifiers"][0]["condition"]["kind"], "not");
    assert!(crate::engine::rule_is_executable(&blocker.rule));

    assert!(
        parse_common_static_ability(
            "Another creature can't block unless you control a Goblin.",
            "Test Crusher",
        )
        .is_none()
    );
}

#[test]
fn attached_attacks_alone_composes_event_scope_and_draw_life_effects() {
    let parsed = parse_expansion_triggered(
        "Whenever equipped creature attacks alone, you draw a card and you lose 1 life.",
        "Test Ring",
    )
    .expect("attached solo attacker should compose with draw and life loss");
    assert_eq!(
        parsed.rule["event"]["kind"],
        "attachedPermanentDeclaredAttacker"
    );
    assert_eq!(parsed.rule["event"]["attackingAlone"], true);
    assert_eq!(parsed.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(parsed.rule["effects"][1]["kind"], "loseLife");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_expansion_triggered(
            "Whenever another equipped creature attacks alone, you draw a card.",
            "Test Ring",
        )
        .is_none()
    );
}

#[test]
fn source_counter_removal_if_you_do_and_zero_counter_followup_compose() {
    let parsed = parse_expansion_triggered(
        "At the beginning of your end step, remove a hope counter from this enchantment. If you do, draw a card. Then if this enchantment has no hope counters on it, sacrifice it and you gain 4 life.",
        "Test Dawn",
    )
    .expect("source counter removal should bind its result for the following conditions");

    assert_eq!(parsed.rule["event"]["kind"], "stepBegan");
    assert_eq!(parsed.rule["event"]["step"], "endStep");
    assert_eq!(parsed.rule["effects"][0]["kind"], "removeCounters");
    assert_eq!(parsed.rule["effects"][0]["counter"], "hope");
    assert_eq!(parsed.rule["effects"][0]["bind"], "removedSourceCounters");
    assert_eq!(
        parsed.rule["effects"][1]["condition"]["kind"],
        "bindingNotEmpty"
    );
    assert_eq!(
        parsed.rule["effects"][2]["condition"]["left"]["kind"],
        "countCounters"
    );
    assert_eq!(
        parsed.rule["effects"][2]["then"][0]["kind"],
        "sacrificePermanent"
    );
    assert_eq!(parsed.rule["effects"][2]["then"][1]["kind"], "gainLife");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_expansion_triggered(
            "At the beginning of your end step, remove a hope counter from this enchantment. If you do, draw a card. Then if this enchantment has no lore counters on it, sacrifice it and you gain 4 life.",
            "Test Dawn",
        )
        .is_none()
    );
}

#[test]
fn kicked_phase_out_instead_composes_conditional_targets_and_object_scope() {
    let parsed = parse_simple_spell_ability(
        "Target creature phases out. If this spell was kicked, each creature target player controls phases out instead. (Treat phased-out creatures and anything attached to them as though they don't exist until their controller's next turn.)",
    )
    .expect("kicked phase-out replacement should compose from generic targets and effects");

    assert_eq!(
        parsed.rule["declaration"]["decisions"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][0]["condition"]["kind"],
        "not"
    );
    assert_eq!(
        parsed.rule["declaration"]["decisions"][1]["condition"]["kind"],
        "wasKicked"
    );
    assert_eq!(
        parsed.rule["effects"][0]["then"][0]["kind"],
        "phaseOutPermanent"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["permanent"]["kind"],
        "eachPermanent"
    );
    assert_eq!(
        parsed.rule["effects"][1]["then"][0]["permanent"]["where"]["value"],
        "Creature"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn greatest_opponent_count_reduction_and_triggering_player_token_count_compose() {
    let reduction = parse_common_static_ability(
        "This spell costs {X} less to cast, where X is the greatest number of artifacts an opponent controls.",
        "Test Dragon",
    )
    .expect("greatest opposing permanent count should scale an own casting reduction");
    assert_eq!(
        reduction.rule["modifiers"][0]["amount"]["kind"],
        "greatestOpponentPermanentCount"
    );
    assert_eq!(
        reduction.rule["modifiers"][0]["amount"]["where"]["value"],
        "Artifact"
    );
    assert!(crate::engine::rule_is_executable(&reduction.rule));

    let trigger = parse_expansion_triggered(
        "Whenever this creature deals combat damage to a player, you create a Treasure token for each artifact that player controls.",
        "Test Dragon",
    )
    .expect("the damaged player's permanents should determine the token count");
    assert_eq!(trigger.rule["event"]["kind"], "combatDamageToPlayer");
    assert_eq!(trigger.rule["effects"][0]["kind"], "createTokens");
    assert_eq!(
        trigger.rule["effects"][0]["quantity"]["kind"],
        "countPermanents"
    );
    assert_eq!(
        trigger.rule["effects"][0]["quantity"]["player"]["kind"],
        "triggeringPlayer"
    );
    assert!(crate::engine::rule_is_executable(&trigger.rule));
}

#[test]
fn other_controlled_entry_counters_scale_with_source_stat() {
    let parsed = parse_common_static_ability(
        "Each other creature you control enters with a number of additional +1/+1 counters on it equal to Test Weaver's toughness.",
        "Test Weaver",
    )
    .expect("external entry counters should compose with a source-stat amount");
    assert_eq!(parsed.rule["modifiers"][0]["kind"], "addEnteringCounters");
    assert_eq!(parsed.rule["modifiers"][0]["counter"], "+1/+1");
    assert_eq!(parsed.rule["modifiers"][0]["count"]["kind"], "toughnessOf");
    assert_eq!(
        parsed.rule["modifiers"][0]["objects"]["excludeSource"],
        true
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));
}

#[test]
fn greatest_toughness_draw_and_any_hand_creatures_compose() {
    let parsed = parse_simple_spell_ability(
        "Draw cards equal to the greatest toughness among creatures you control, then put any number of creature cards from your hand onto the battlefield.",
    )
    .expect("greatest-toughness draw should sequence with an unrestricted hand selection");
    assert_eq!(parsed.rule["effects"][0]["kind"], "drawCards");
    assert_eq!(
        parsed.rule["effects"][0]["count"]["kind"],
        "greatestToughness"
    );
    assert_eq!(
        parsed.rule["effects"][1]["kind"],
        "putCardsFromHandOntoBattlefield"
    );
    assert_eq!(parsed.rule["effects"][1]["where"]["value"], "Creature");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert_eq!(
        x_variable_expression("the greatest toughness among Treefolk you control")
            .expect("the generic greatest-stat leaf accepts toughness"),
        json!({
            "kind": "greatestToughness",
            "player": controller(),
            "where": subtype("Treefolk"),
        })
    );
}

#[test]
fn damaged_player_exile_until_matching_card_cast_and_remainder_compose() {
    let parsed = parse_expansion_triggered(
        "Whenever Test Footman deals combat damage to a player, that player exiles cards from the top of their library until they exile an instant or sorcery card. You may cast that card without paying its mana cost. Then that player puts the exiled cards that weren't cast this way on the bottom of their library in a random order.",
        "Test Footman",
    )
    .expect("the linked exile, optional cast, and random-bottom sequence should parse");
    assert_eq!(parsed.rule["event"]["kind"], "combatDamageToPlayer");
    assert_eq!(parsed.rule["effects"][0]["kind"], "exileFromTopUntil");
    assert_eq!(
        parsed.rule["effects"][0]["zone"]["player"]["kind"],
        "triggeringPlayer"
    );
    assert_eq!(parsed.rule["effects"][0]["stopWhere"]["kind"], "or");
    assert_eq!(
        parsed.rule["effects"][0]["stopCardBind"],
        "exiledUntilMatchingCard"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "castAnyNumber");
    assert_eq!(
        parsed.rule["effects"][1]["cards"]["binding"],
        "exiledUntilMatchingCard"
    );
    assert_eq!(parsed.rule["effects"][2]["kind"], "moveCards");
    assert_eq!(parsed.rule["effects"][2]["order"]["kind"], "random");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let creature_variant = parse_general_effect_sequence(
        "That player exiles cards from the top of their library until they exile a creature card. You may cast that card without paying its mana cost. Then that player puts the exiled cards that weren't cast this way on the bottom of their library in a random order.",
        "Test Source",
    )
    .expect("the linked sequence delegates a distinct stop criterion to the criteria parser");
    assert_eq!(creature_variant.0[0]["stopWhere"]["value"], "Creature");

    assert!(parse_general_effect_sequence(
        "That player exiles cards from the top of their library until they exile an instant card. You may cast that card without paying its mana cost. Then that player puts the exiled cards into their hand.",
        "Test Source",
    )
    .is_none());
}

#[test]
fn opponent_mill_reflexive_graveyard_copy_and_free_cast_compose() {
    let parsed = parse_expansion_triggered(
        "Whenever you cast your second spell each turn, each opponent mills two cards. When one or more cards are milled this way, exile target enchantment, instant, or sorcery card with equal or lesser mana value than that spell from an opponent's graveyard. Copy the exiled card. You may cast the copy without paying its mana cost.",
        "Test Many Colors",
    )
    .expect("the ordinal spell trigger composes with mill, reflexive targeting, and a card copy");
    assert_eq!(parsed.rule["event"]["kind"], "spellCast");
    assert_eq!(parsed.rule["event"]["spellCastOrdinal"]["value"], 2);
    assert_eq!(parsed.rule["effects"][0]["kind"], "millEachPlayer");
    assert_eq!(parsed.rule["effects"][0]["players"]["kind"], "opponentsOf");
    assert_eq!(parsed.rule["effects"][0]["bind"], "milledOpponentCards");
    let reflexive = &parsed.rule["effects"][1]["then"][0]["ability"];
    let candidates = &reflexive["declaration"]["decisions"][0]["candidates"];
    assert_eq!(candidates["zone"]["kind"], "anyGraveyard");
    assert_eq!(candidates["owner"]["kind"], "opponentsOf");
    assert_eq!(candidates["where"]["kind"], "and");
    assert_eq!(
        candidates["where"]["operands"][1]["right"]["kind"],
        "triggeringSpellManaValue"
    );
    assert_eq!(reflexive["effects"][0]["kind"], "moveTargetCard");
    assert_eq!(reflexive["effects"][1]["kind"], "createCardCopy");
    assert_eq!(reflexive["effects"][2]["kind"], "castAnyNumber");
    assert_eq!(reflexive["effects"][3]["kind"], "ceaseToExist");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let leaf = parse_general_effect_instruction("Each opponent mills one card.", "Test Source")
        .expect("each-opponent mill is a reusable instruction leaf");
    assert_eq!(leaf.0[0]["kind"], "millEachPlayer");
    assert_eq!(leaf.0[0]["count"]["value"], 1);

    assert!(parse_general_effect_sequence(
        "Each opponent mills two cards. When one or more cards are milled this way, return target instant card from a graveyard to its owner's hand.",
        "Test Source",
    )
    .is_none());
}

#[test]
fn final_saga_chapter_resolution_and_reveal_sequence_compose() {
    let parsed = parse_expansion_triggered(
        "Whenever the final chapter ability of a Saga you control resolves, reveal cards from the top of your library until you reveal a Saga card. Put that card onto the battlefield and the rest on the bottom of your library in a random order. This ability triggers only once each turn.",
        "Test Storyteller",
    )
    .expect("the final chapter event composes with the shared reveal sequence");
    assert_eq!(
        parsed.rule["event"]["kind"],
        "sagaFinalChapterAbilityResolved"
    );
    assert_eq!(parsed.rule["event"]["where"]["value"], "Saga");
    assert_eq!(parsed.rule["triggerLimit"]["kind"], "onceEachTurn");
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "revealUntilAndPutOntoBattlefield"
    );
    assert_eq!(parsed.rule["effects"][0]["where"]["value"], "Saga");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(
        parse_expansion_triggered(
            "Whenever a chapter ability of a Saga you control resolves, draw a card.",
            "Test Storyteller",
        )
        .is_none()
    );
}

#[test]
fn equipped_attack_conditional_tokens_and_counters_compose() {
    let spirits = parse_expansion_triggered(
        "Whenever equipped creature attacks, create two tapped 1/1 white Spirit creature tokens with flying. If that creature is legendary, instead create two of those tokens that are tapped and attacking.",
        "Test Flame",
    )
    .expect("the attached attacker event composes with a conditional token entry state");
    assert_eq!(
        spirits.rule["event"]["kind"],
        "attachedPermanentDeclaredAttacker"
    );
    let spirit_effect = &spirits.rule["effects"][0];
    assert_eq!(spirit_effect["kind"], "conditionalEffect");
    assert_eq!(
        spirit_effect["condition"]["object"]["kind"],
        "triggeringPermanent"
    );
    assert_eq!(spirit_effect["condition"]["where"]["kind"], "isLegendary");
    assert_eq!(spirit_effect["then"][0]["kind"], "createTokens");
    assert_eq!(spirit_effect["then"][0]["tapped"], true);
    assert_eq!(spirit_effect["then"][0]["attacking"], true);
    assert_eq!(spirit_effect["else"][0]["tapped"], true);
    assert!(spirits.rule["effects"][0]["else"][0]["attacking"].is_null());
    assert!(crate::engine::rule_is_executable(&spirits.rule));

    let counters = parse_expansion_triggered(
        "Whenever equipped creature attacks, put a +1/+1 counter on each creature you control. If you have the city's blessing, put two +1/+1 counters on each creature you control instead.",
        "Test Reforged",
    )
    .expect("the attached attacker event composes with city-blessing counter quantity");
    assert_eq!(
        counters.rule["event"]["kind"],
        "attachedPermanentDeclaredAttacker"
    );
    let counter_effect = &counters.rule["effects"][0];
    assert_eq!(counter_effect["condition"]["kind"], "hasCityBlessing");
    assert_eq!(counter_effect["then"][0]["count"]["value"], 2);
    assert_eq!(counter_effect["else"][0]["count"]["value"], 1);
    assert_eq!(
        counter_effect["then"][0]["permanent"]["where"]["value"],
        "Creature"
    );
    assert!(crate::engine::rule_is_executable(&counters.rule));
}

#[test]
fn cast_source_entry_and_player_protection_until_next_turn_compose() {
    let parsed = parse_expansion_triggered(
        "When Test Relic enters, if you cast it, you gain protection from everything until your next turn.",
        "Test Relic",
    )
    .expect("the cast-source entry condition composes with player protection");
    assert_eq!(parsed.rule["event"]["kind"], "enterBattlefield");
    assert_eq!(parsed.rule["condition"]["kind"], "wasCast");
    assert_eq!(parsed.rule["effects"][0]["kind"], "grantPlayerProtection");
    assert_eq!(parsed.rule["effects"][0]["from"][0], "everything");
    assert_eq!(
        parsed.rule["effects"][0]["duration"]["kind"],
        "untilNextTurn"
    );
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let generic_subject = parse_expansion_triggered(
        "When this artifact enters, if you cast that permanent, you gain protection from everything until your next turn.",
        "Test Relic",
    )
    .expect("a generic source reference and pronoun use the same leaves");
    assert_eq!(generic_subject.rule["condition"]["kind"], "wasCast");
}

#[test]
fn target_counter_links_subtype_and_keyword_while_it_remains() {
    let parsed = parse_simple_activated_ability(
        "{3}{B}, {T}: Put a shadow counter on target creature. For as long as that creature has a shadow counter on it, it's a Wraith in addition to its other types. (A creature with shadow can block or be blocked by only creatures with shadow.)",
    )
    .expect("the target counter composes with counter-linked characteristics");
    assert_eq!(parsed.rule["costs"][0]["kind"], "payMana");
    assert_eq!(parsed.rule["costs"][1]["kind"], "tap");
    assert_eq!(parsed.rule["effects"][0]["kind"], "putCounters");
    assert_eq!(parsed.rule["effects"][0]["counter"], "shadow");
    assert_eq!(
        parsed.rule["effects"][1]["kind"],
        "installCounterLinkedCharacteristics"
    );
    assert_eq!(parsed.rule["effects"][1]["addSubtypes"][0], "Wraith");
    assert_eq!(parsed.rule["effects"][1]["keywords"][0], "shadow");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(parse_general_effect_sequence(
        "Put a shadow counter on target creature. For as long as that creature has a burden counter on it, it's a Wraith in addition to its other types.",
        "Test Fortress",
    )
    .is_none());
}

#[test]
fn discard_cost_source_keyword_and_gendered_tap_compose() {
    let parsed = parse_simple_activated_ability_for_face(
        "Discard a card: Test Wraith gains indestructible until end of turn. Tap him.",
        "Test Wraith",
    )
    .expect("discard cost composes with source keyword and gendered tap leaves");
    assert_eq!(parsed.rule["costs"][0]["kind"], "discardCard");
    assert_eq!(parsed.rule["effects"][0]["kind"], "grantKeyword");
    assert_eq!(parsed.rule["effects"][0]["keyword"], "indestructible");
    assert_eq!(parsed.rule["effects"][1]["kind"], "tapPermanent");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let multiple = parse_general_effect_instruction(
        "This creature gains flying and vigilance until end of turn.",
        "Test Wraith",
    )
    .expect("the source keyword leaf delegates a list to the keyword grammar");
    assert_eq!(multiple.0.len(), 2);
}

#[test]
fn combat_damage_received_sacrifice_and_ring_tempt_compose() {
    let parsed = parse_expansion_triggered(
        "Whenever one or more creatures deal combat damage to you, each opponent sacrifices a creature of their choice that dealt combat damage to you this turn. The Ring tempts you.",
        "Test Wraith",
    )
    .expect("combat damage sources compose with constrained sacrifice and Ring temptation");
    assert_eq!(parsed.rule["event"]["kind"], "combatDamageReceived");
    assert_eq!(parsed.rule["event"]["where"]["kind"], "cardTypeContains");
    assert_eq!(parsed.rule["event"]["where"]["value"], "Creature");
    assert_eq!(
        parsed.rule["effects"][0]["kind"],
        "sacrificePermanentsEachPlayer"
    );
    assert_eq!(
        parsed.rule["effects"][0]["candidateIdsDecision"],
        "combatDamageSourceIds"
    );
    assert_eq!(parsed.rule["effects"][1]["kind"], "ringTemptsPlayer");
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    assert!(parse_expansion_triggered(
        "Whenever one or more things deal combat damage to you, each opponent sacrifices a creature of their choice that dealt combat damage to you this turn.",
        "Test Wraith",
    )
    .is_none());
}

#[test]
fn defending_player_least_power_sacrifice_composes() {
    for text in [
        "Whenever Test Wraith attacks, defending player sacrifices a creature with the least power among creatures they control.",
        "Whenever Test Construct attacks, defending player sacrifices an artifact creature with the least power among artifact creatures they control.",
    ] {
        let face_name = if text.contains("Construct") {
            "Test Construct"
        } else {
            "Test Wraith"
        };
        let parsed = parse_expansion_triggered(text, face_name)
            .unwrap_or_else(|| panic!("least-power attack trigger did not parse: {text}"));
        assert_eq!(parsed.rule["event"]["kind"], "declaredAttacker");
        assert_eq!(parsed.rule["effects"][0]["kind"], "sacrificePermanents");
        assert_eq!(
            parsed.rule["effects"][0]["minimumPowerAmongCandidates"],
            true
        );
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(parse_expansion_triggered(
        "Whenever Test Wraith attacks, defending player sacrifices a creature with the least power among artifacts they control.",
        "Test Wraith",
    )
    .is_none());
}

#[test]
fn repeated_cascade_instances_and_event_value_sum_compose() {
    let cascade = parse_keyword_ability(
        "Cascade, cascade (When you cast this spell, exile cards from the top of your library until you exile a nonland card that costs less. You may cast it without paying its mana cost. Put the exiled cards on the bottom of your library in a random order. Then do it again.)",
        "Test Tempest",
    )
    .expect("repeated cascade instances compose as a keyword group");
    assert_eq!(cascade.rule["kind"], "keywordAbilityGroup");
    assert_eq!(cascade.rule["abilities"].as_array().map(Vec::len), Some(2));
    assert!(
        cascade.rule["abilities"]
            .as_array()
            .is_some_and(|abilities| abilities.iter().all(|ability| ability["kind"] == "cascade"))
    );
    assert!(crate::engine::rule_is_executable(&cascade.rule));

    let (effects, decisions) = parse_general_effect_instruction(
        "Test Tempest deals damage to each creature your opponents control equal to the total mana value of other spells you've cast this turn.",
        "Test Tempest",
    )
    .expect("opposing permanent damage delegates its amount to the numeric event-sum leaf");
    assert!(decisions.is_empty());
    assert_eq!(effects[0]["kind"], "dealDamage");
    assert_eq!(effects[0]["recipient"]["kind"], "eachPermanent");
    assert_eq!(effects[0]["amount"]["kind"], "sumEventValuesThisTurn");
    assert_eq!(effects[0]["amount"]["eventKind"], "spellCast");
    assert_eq!(effects[0]["amount"]["detailField"], "manaValue");
    assert_eq!(effects[0]["amount"]["excludeSource"], true);
    assert!(crate::engine::effect_supported(&effects[0]));

    assert!(parse_keyword_ability("Cascade, unknown ability", "Test Tempest").is_none());
}

#[test]
fn enter_or_attack_mass_counters_and_counted_life_compose() {
    for (text, face_name) in [
        (
            "Whenever Test Couple enters or attacks, put a +1/+1 counter on each other creature you control. You gain 1 life for each other creature you control.",
            "Test Couple",
        ),
        (
            "Whenever this permanent enters or attacks, put a shield counter on each other artifact you control. You gain two life for each other artifact you control.",
            "Test Relic",
        ),
    ] {
        let parsed = parse_expansion_triggered(text, face_name)
            .expect("enter-or-attack composes mass counters and counted life");
        assert_eq!(parsed.rule["event"]["kind"], "oneOf");
        assert_eq!(parsed.rule["effects"][0]["kind"], "putCounters");
        assert_eq!(
            parsed.rule["effects"][0]["permanent"]["excludeSource"],
            true
        );
        let amount = &parsed.rule["effects"][1]["amount"];
        let count = if amount["kind"] == "multiply" {
            &amount["left"]
        } else {
            amount
        };
        assert_eq!(count["kind"], "countPermanents");
        assert_eq!(count["excludeSource"], true);
        assert!(crate::engine::rule_is_executable(&parsed.rule));
    }

    assert!(
        parse_general_effect_instruction(
            "Put a +1/+1 counter on each other creature an opponent controls.",
            "Test Couple",
        )
        .is_none()
    );
}

#[test]
fn flame_of_anor_conditional_modal_maximum_delegates_controlled_permanent_criteria() {
    let oracle = "Choose one. If you control a Wizard as you cast this spell, you may choose two instead.\n\u{2022} Target player draws two cards.\n\u{2022} Destroy target artifact.\n\u{2022} Test Flame deals 5 damage to target creature.";
    let parsed = parse_general_modal_spell(oracle)
        .expect("the conditional modal maximum composes with generic modes");
    let mode_decision = &parsed.rule["declaration"]["decisions"][0];
    assert_eq!(mode_decision["kind"], "chooseModes");
    assert_eq!(mode_decision["minimum"], 1);
    assert_eq!(mode_decision["maximum"]["kind"], "conditionalValue");
    assert_eq!(
        mode_decision["maximum"]["condition"]["kind"],
        "controlsPermanent"
    );
    assert_eq!(
        mode_decision["maximum"]["condition"]["where"]["kind"],
        "subtypeContains"
    );
    assert_eq!(mode_decision["maximum"]["ifTrue"], integer(2));
    assert_eq!(mode_decision["maximum"]["ifFalse"], integer(1));
    assert_eq!(parsed.rule["effects"].as_array().map(Vec::len), Some(3));
    assert!(crate::engine::rule_is_executable(&parsed.rule));

    let choose_three = parse_general_modal_spell(
        "Choose one. If you control an artifact as you cast this spell, you may choose three instead.\n\u{2022} Draw a card.\n\u{2022} You gain 1 life.\n\u{2022} Create a Treasure token.",
    )
    .expect("a different criterion and maximum reuse the same header grammar");
    assert_eq!(
        choose_three.rule["declaration"]["decisions"][0]["maximum"]["ifTrue"],
        integer(3)
    );
    assert!(crate::engine::rule_is_executable(&choose_three.rule));

    assert!(parse_general_modal_spell(
        "Choose one. If an opponent controls a Wizard as you cast this spell, you may choose two instead.\n\u{2022} Draw a card.\n\u{2022} You gain 1 life.",
    )
    .is_none());
}
