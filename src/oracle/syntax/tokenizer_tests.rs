use crate::oracle::{OracleCardParseRequest, parse_oracle_card};

use super::{oracle_tokenization_count, recognize_entities, reset_oracle_tokenization_count};

#[test]
fn symbolic_stat_pair_preserves_spans_and_does_not_enable_arithmetic_division() {
    let text = "+X/+0";
    let tokens = recognize_entities(text);
    assert_eq!(
        tokens
            .iter()
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>(),
        ["+", "X", "/", "+", "0"]
    );
    assert_eq!(tokens[2].token_type, "statPairSeparator");
    for token in &tokens {
        assert_eq!(&text[token.span_start..token.span_end], token.raw);
    }
    assert_eq!(recognize_entities("+1/+1")[0].token_type, "counterValue");
    assert_eq!(
        recognize_entities("8 / 2")[1].token_type,
        "statPairSeparator"
    );
    let unknown = recognize_entities("Mystery/Word");
    assert_eq!(unknown.len(), 1);
    assert_eq!(unknown[0].raw, "Mystery/Word");
    assert_eq!(unknown[0].token_type, "unknown");
}

#[test]
fn unknown_tokens_preserve_exact_half_open_source_spans() {
    let text = "Lightning Helix deals 3 damage.";
    let tokens = recognize_entities(text);
    let unknown = tokens
        .iter()
        .filter(|token| token.token_type == "unknown")
        .collect::<Vec<_>>();

    assert_eq!(unknown.len(), 2);
    assert_eq!((unknown[0].span_start, unknown[0].span_end), (0, 9));
    assert_eq!((unknown[1].span_start, unknown[1].span_end), (10, 15));
    for token in unknown {
        assert_eq!(&text[token.span_start..token.span_end], token.raw);
    }
}

#[test]
fn dies_is_typed_as_a_trigger_event() {
    let tokens = recognize_entities("Whenever another creature dies, draw a card.");
    let dies = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("dies"))
        .expect("dies event token");

    assert_eq!(dies.category, "trigger");
    assert_eq!(dies.token_type, "eventWord");
}

#[test]
fn rounded_half_expression_has_typed_numeric_vocabulary() {
    let tokens = recognize_entities("Draw half X cards, rounded down.");

    for (raw, category, token_type) in [
        ("half", "logic", "numericOperatorWord"),
        ("X", "logic", "quantity"),
        ("rounded", "logic", "roundingWord"),
        ("down", "syntax", "directionWord"),
    ] {
        let token = tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .unwrap_or_else(|| panic!("missing token {raw}"));
        assert_eq!(token.category, category);
        assert_eq!(token.token_type, token_type);
    }

    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn arithmetic_operator_words_are_typed_without_claiming_counter_values() {
    let tokens = recognize_entities(
        "one plus two minus three times four multiplied by five divided by six +1/+1",
    );

    for raw in ["plus", "minus", "times", "multiplied", "divided"] {
        let token = tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .unwrap_or_else(|| panic!("missing arithmetic operator {raw}"));
        assert_eq!(token.category, "logic");
        assert_eq!(token.token_type, "numericOperatorWord");
    }
    let counter = tokens
        .iter()
        .find(|token| token.raw == "+1/+1")
        .expect("the counter value remains one token");
    assert_eq!(counter.token_type, "counterValue");
}

#[test]
fn commander_free_cast_vocabulary_is_typed_by_semantic_role() {
    let tokens = recognize_entities(
        "If you control a commander, you may cast this spell without paying its mana cost.",
    );
    let token_type = |raw: &str| {
        tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .map(|token| token.token_type.as_str())
    };

    assert_eq!(token_type("commander"), Some("objectCharacteristicWord"));
    assert_eq!(token_type("without"), Some("costOmissionWord"));
    assert_eq!(token_type("paying"), Some("costActionWord"));
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn personal_source_anaphors_are_typed_reference_vocabulary() {
    let tokens = recognize_entities("Put a +1/+1 counter on her and him.");
    for raw in ["her", "him"] {
        let token = tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .unwrap_or_else(|| panic!("missing source anaphor {raw}"));

        assert_eq!(token.category, "entityReference");
        assert_eq!(token.token_type, "referenceWord");
    }
}

#[test]
fn possessive_library_owner_and_delayed_upkeep_vocabulary_are_typed() {
    let tokens = recognize_entities(
        "Look at target player's library at the beginning of the next turn's upkeep.",
    );

    for (raw, category, token_type) in [
        ("player's", "entityReference", "possessivePlayerRoleWord"),
        ("beginning", "syntax", "temporalMarkerWord"),
        ("turn's", "syntax", "possessiveTurnWord"),
        ("upkeep", "syntax", "turnStepWord"),
    ] {
        let token = tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case(raw))
            .unwrap_or_else(|| panic!("missing token {raw}"));
        assert_eq!(token.category, category);
        assert_eq!(token.token_type, token_type);
    }
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn player_role_possessives_do_not_steal_object_property_possessives() {
    let tokens = recognize_entities("target opponent's library and that creature's power");
    let opponent = tokens
        .iter()
        .find(|token| token.raw == "opponent's")
        .unwrap();
    let creature = tokens
        .iter()
        .find(|token| token.raw == "creature's")
        .unwrap();

    assert_eq!(opponent.token_type, "possessivePlayerRoleWord");
    assert_eq!(creature.token_type, "possessiveTypeWord");
}

#[test]
fn ordinal_draw_and_source_transform_vocabulary_have_no_unknown_tokens() {
    let tokens = recognize_entities(
        "you draw your fourth card in a turn, investigate, exile this creature, then return it to the battlefield transformed under its owner's control",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case("fourth"))
            .map(|token| token.token_type.as_str()),
        Some("ordinalWord")
    );
}

#[test]
fn rounded_library_count_vocabulary_has_no_unknown_tokens() {
    let tokens = recognize_entities(
        "Draw cards equal to half the number of cards in your library, rounded up.",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn loyalty_composition_vocabulary_has_no_unknown_tokens() {
    let tokens = recognize_entities(
        "+2: Until your next turn, whenever a creature attacks you or a planeswalker you control, it gets -1/-0 until end of turn. −3: Return target instant or sorcery card from your graveyard to your hand. If it's a green card, add one mana of any color. −7: Draw cards equal to half the number of cards in your library, rounded up. You get an emblem with \"You have no maximum hand size.\"",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn alternative_draw_event_and_amass_have_no_unknown_vocabulary() {
    let tokens = recognize_entities(
        "When this creature enters and whenever an opponent draws a card except the first one they draw in each of their draw steps, this creature deals 1 damage to any target. Then amass Orcs 1.",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
    for (raw, token_type) in [
        ("draws", "actionWord"),
        ("except", "exceptionWord"),
        ("steps", "turnStepWord"),
        ("amass", "actionWord"),
        ("Orcs", "subtypeWord"),
    ] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw.eq_ignore_ascii_case(raw))
                .map(|token| token.token_type.as_str()),
            Some(token_type)
        );
    }
}

#[test]
fn plural_turn_period_is_typed_for_persistent_permissions() {
    let tokens = recognize_entities("You may play two additional lands on each of your turns.");

    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case("turns"))
            .map(|token| token.token_type.as_str()),
        Some("repetitionUnitWord")
    );
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn surveil_is_a_typed_atomic_action() {
    let tokens = recognize_entities("surveil 2");
    let surveil = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("surveil"))
        .expect("surveil token");

    assert_eq!(surveil.category, "magicVocabulary");
    assert_eq!(surveil.token_type, "actionWord");
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn block_is_a_typed_atomic_action() {
    let tokens = recognize_entities("This creature can block only creatures with flying.");
    let block = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("block"))
        .expect("block token");

    assert_eq!(block.category, "magicVocabulary");
    assert_eq!(block.token_type, "actionWord");
}

#[test]
fn canonical_surveil_reminders_have_no_unknown_vocabulary() {
    for reminder in [
        "(Look at the top card of your library. You may put it into your graveyard.)",
        "(Look at the top two cards of your library, then put any number of them into your graveyard and the rest on top of your library in any order.)",
    ] {
        let tokens = recognize_entities(reminder);
        assert!(
            tokens.iter().all(|token| token.token_type != "unknown"),
            "unexpected UNKNOWN tokens: {:?}",
            tokens
                .iter()
                .filter(|token| token.token_type == "unknown")
                .map(|token| token.raw.as_str())
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn uncast_entry_replacement_has_typed_vocabulary() {
    let tokens = recognize_entities(
        "If a nontoken creature would enter the battlefield and it wasn't cast, exile it instead.",
    );

    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
    for (raw, token_type) in [
        ("nontoken", "negativeCharacteristicWord"),
        ("would", "replacementModalWord"),
        ("wasn't", "negativePredicateWord"),
    ] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw.eq_ignore_ascii_case(raw))
                .map(|token| token.token_type.as_str()),
            Some(token_type)
        );
    }
}

#[test]
fn negative_payment_predicate_is_typed_vocabulary() {
    let tokens = recognize_entities("If the player doesn't, you draw a card.");
    let predicate = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("doesn't"))
        .expect("negative predicate token");

    assert_eq!(predicate.token_type, "negativePredicateWord");
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn exalted_and_its_reminder_have_no_unknown_vocabulary() {
    let tokens = recognize_entities(
        "Exalted (Whenever a creature you control attacks alone, that creature gets +1/+1 until end of turn.)",
    );

    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case("exalted"))
            .map(|token| token.token_type.as_str()),
        Some("keywordWord")
    );
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw.eq_ignore_ascii_case("alone"))
            .map(|token| token.token_type.as_str()),
        Some("qualifierWord")
    );
}

#[test]
fn temporary_animation_vocabulary_has_no_unknown_tokens() {
    let tokens = recognize_entities(
        "Until end of turn, this land becomes a 4/4 blue and black Shark creature with deathtouch. It's still a land.",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected UNKNOWN tokens: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
    for (raw, token_type) in [
        ("becomes", "actionWord"),
        ("Shark", "subtypeWord"),
        ("still", "qualifierWord"),
    ] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw.eq_ignore_ascii_case(raw))
                .map(|token| token.token_type.as_str()),
            Some(token_type)
        );
    }
}

#[test]
fn official_one_word_creature_subtype_is_lexical_not_unknown() {
    let tokens = recognize_entities("Its controller creates a 3/3 green Elephant creature token.");
    let token = tokens
        .iter()
        .find(|token| token.raw == "Elephant")
        .expect("the subtype token is preserved");
    assert_eq!(token.category, "entityReference");
    assert_eq!(token.token_type, "subtypeWord");
    assert!(
        tokens.iter().all(|token| token.category != "unknown"),
        "unexpected UNKNOWN tokens: {tokens:#?}"
    );
}

#[test]
fn an_unregistered_capitalized_word_remains_unknown() {
    let tokens =
        recognize_entities("Its controller creates a 3/3 green Imaginarytype creature token.");
    let token = tokens
        .iter()
        .find(|token| token.raw == "Imaginarytype")
        .expect("the unknown token is preserved");
    assert_eq!(token.category, "unknown");
    assert_eq!(token.token_type, "unknown");
}

#[test]
fn inflected_library_search_action_is_typed_vocabulary() {
    let tokens = recognize_entities("Whenever an opponent searches their library, draw a card.");
    let searches = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("searches"))
        .expect("searches token");

    assert_eq!(searches.category, "magicVocabulary");
    assert_eq!(searches.token_type, "actionWord");
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn inflected_mill_action_is_typed_vocabulary() {
    let tokens = recognize_entities("Whenever this land attacks, target player mills four cards.");
    let mills = tokens
        .iter()
        .find(|token| token.raw.eq_ignore_ascii_case("mills"))
        .expect("mills token");

    assert_eq!(mills.category, "magicVocabulary");
    assert_eq!(mills.token_type, "actionWord");
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

#[test]
fn prefixed_negative_object_types_are_typed_vocabulary_not_unknown_names() {
    let tokens = recognize_entities(
        "Counter target noncreature spell. Counter target nonartifact spell. Counter target nonland permanent.",
    );
    let negative_types = tokens
        .iter()
        .filter(|token| token.token_type == "negativeTypeWord")
        .map(|token| token.raw.as_str())
        .collect::<Vec<_>>();

    assert_eq!(negative_types, ["noncreature", "nonartifact", "nonland"]);
    assert!(tokens.iter().all(|token| {
        !matches!(
            token.raw.as_str(),
            "noncreature" | "nonartifact" | "nonland"
        ) || token.category == "entityReference"
    }));
}

#[test]
fn owner_library_vocabulary_has_typed_role_and_inflected_action_tokens() {
    let tokens = recognize_entities("The owner shuffles a permanent into their library.");

    let owner = tokens
        .iter()
        .find(|token| token.raw == "owner")
        .expect("owner token");
    assert_eq!(owner.category, "entityReference");
    assert_eq!(owner.token_type, "playerRoleWord");

    let shuffles = tokens
        .iter()
        .find(|token| token.raw == "shuffles")
        .expect("shuffles token");
    assert_eq!(shuffles.category, "magicVocabulary");
    assert_eq!(shuffles.token_type, "actionWord");
}

#[test]
fn modal_boundaries_and_inflected_ability_loss_are_typed_vocabulary() {
    let tokens = recognize_entities("Choose one — • Each creature loses all abilities.");

    let em_dash = tokens
        .iter()
        .find(|token| token.raw == "—")
        .expect("modal em dash token");
    assert_eq!(em_dash.category, "boundary");
    assert_eq!(em_dash.token_type, "separator");

    let bullet = tokens
        .iter()
        .find(|token| token.raw == "•")
        .expect("modal bullet token");
    assert_eq!(bullet.category, "boundary");
    assert_eq!(bullet.token_type, "separator");

    let loses = tokens
        .iter()
        .find(|token| token.raw == "loses")
        .expect("loses token");
    assert_eq!(loses.category, "magicVocabulary");
    assert_eq!(loses.token_type, "actionWord");
}

#[test]
fn equipment_and_keyword_reminder_vocabulary_has_no_unknown_tokens() {
    let tokens = recognize_entities(
        "Equipped creature has hexproof and haste. It can't be the target of spells or abilities your opponents control. It can attack and {T} no matter when it came under your control. Equip {1}: Attach to target creature you control.",
    );

    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected unknowns: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
    for (raw, token_type) in [
        ("Equipped", "attachmentStateWord"),
        ("has", "predicateWord"),
        ("opponents", "playerQualifierWord"),
        ("attack", "actionWord"),
        ("matter", "qualifierWord"),
        ("came", "relationWord"),
        ("under", "relationWord"),
        ("Equip", "keywordWord"),
        ("Attach", "actionWord"),
    ] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw == raw)
                .unwrap_or_else(|| panic!("missing token {raw}"))
                .token_type,
            token_type
        );
    }
}

#[test]
fn conditional_mana_production_vocabulary_is_typed() {
    let tokens = recognize_entities(
        "Add one mana of any color that a land an opponent controls could produce.",
    );
    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected unknowns: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw == "could")
            .unwrap()
            .token_type,
        "modalWord"
    );
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw == "produce")
            .unwrap()
            .token_type,
        "actionWord"
    );
}

#[test]
fn kicker_keyword_and_condition_state_are_typed_vocabulary() {
    let tokens = recognize_entities("Kicker {4}. If this spell was kicked, draw a card.");

    let kicker = tokens
        .iter()
        .find(|token| token.raw == "Kicker")
        .expect("Kicker token");
    assert_eq!(kicker.category, "magicVocabulary");
    assert_eq!(kicker.token_type, "keywordWord");

    let kicked = tokens
        .iter()
        .find(|token| token.raw == "kicked")
        .expect("kicked token");
    assert_eq!(kicked.category, "magicVocabulary");
    assert_eq!(kicked.token_type, "keywordStateWord");
}

#[test]
fn ability_word_and_zone_history_event_are_typed_vocabulary() {
    let tokens = recognize_entities(
        "Revolt — Destroy that creature instead if a permanent left the battlefield under your control this turn.",
    );

    for (raw, token_type) in [("Revolt", "abilityWord"), ("left", "eventWord")] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw == raw)
                .unwrap_or_else(|| panic!("missing token {raw}"))
                .token_type,
            token_type
        );
    }
    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected unknowns: {:?}",
        tokens
            .iter()
            .filter(|token| token.token_type == "unknown")
            .map(|token| token.raw.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn pipeline_tokenizes_each_oracle_ability_exactly_once() {
    reset_oracle_tokenization_count();
    let parsed = parse_oracle_card(OracleCardParseRequest {
        card_name: "Lightning Helix".to_string(),
        type_line: "Instant".to_string(),
        mana_cost: Some("{R}{W}".to_string()),
        oracle_text: Some(
            "Lightning Helix deals 3 damage to any target.\nYou gain 3 life.".to_string(),
        ),
        layout: None,
        faces: Vec::new(),
    });

    assert_eq!(parsed.abilities.len(), 2);
    assert_eq!(oracle_tokenization_count(), parsed.abilities.len());
}

#[test]
fn exact_multiword_subtype_context_resolves_a_primary_class_collision() {
    let tokens = recognize_entities("Create a Time Lord creature token.");
    let time = tokens
        .iter()
        .find(|token| token.raw == "Time")
        .expect("Time token");
    let lord = tokens
        .iter()
        .find(|token| token.raw == "Lord")
        .expect("Lord token");

    assert_eq!(time.category, "entityReference");
    assert_eq!(time.token_type, "subtypeWord");
    assert_eq!(lord.category, "entityReference");
    assert_eq!(lord.token_type, "subtypeWord");
}

#[test]
fn subtype_collision_without_type_context_keeps_the_primary_logic_class() {
    let tokens = recognize_entities("Draw a card for each time this happened.");
    let time = tokens
        .iter()
        .find(|token| token.raw == "time")
        .expect("time token");

    assert_eq!(time.category, "logic");
    assert_eq!(time.token_type, "repetitionUnitWord");
}

#[test]
fn source_stat_variable_clause_is_fully_typed() {
    let tokens = recognize_entities("where X is this creature's power");

    for (raw, token_type) in [
        ("where", "relationWord"),
        ("X", "quantity"),
        ("is", "predicateWord"),
        ("this", "referenceWord"),
        ("creature's", "possessiveTypeWord"),
        ("power", "propertyWord"),
    ] {
        assert_eq!(
            tokens
                .iter()
                .find(|token| token.raw == raw)
                .unwrap_or_else(|| panic!("missing token {raw}"))
                .token_type,
            token_type
        );
    }
    assert!(
        tokens.iter().all(|token| token.token_type != "unknown"),
        "unexpected unknowns: {tokens:#?}"
    );
}

#[test]
fn attack_tax_relative_clause_is_fully_typed() {
    let tokens = recognize_entities(
        "Creatures can't attack you unless their controller pays {2} for each creature they control that's attacking you.",
    );

    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw == "that's")
            .expect("relative reference")
            .token_type,
        "referenceWord"
    );
    assert_eq!(
        tokens
            .iter()
            .find(|token| token.raw == "attacking")
            .expect("combat state")
            .token_type,
        "combatStateWord"
    );
    assert!(tokens.iter().all(|token| token.token_type != "unknown"));
}

fn label_token_types(text: &str) -> Vec<(String, String)> {
    let tokens = recognize_entities(text);
    let dash = tokens
        .iter()
        .position(|token| token.raw == "\u{2014}")
        .expect("a labelled ability");
    tokens[..dash]
        .iter()
        .map(|token| (token.raw.clone(), token.token_type.clone()))
        .collect()
}

/// CR 207.2c: a capitalized label before a trigger word is a rules-free
/// flavor word by its position, even when the lexicon knows its words or it
/// contains punctuation (`Exile Cannon`, `Exterminate!`, `10,000 Needles`).
#[test]
fn labels_before_a_trigger_word_are_flavor_words_by_position() {
    for text in [
        "Exile Cannon \u{2014} When this creature enters from a graveyard, exile up to one target nonland permanent.",
        "Exterminate! \u{2014} When this creature enters, destroy target creature an opponent controls.",
        "10,000 Needles \u{2014} Whenever this creature attacks, it gets +9999/+0 until end of turn.",
        "Flurry of Blows \u{2014} Whenever you cast your second spell each turn, put a +1/+1 counter on this creature.",
        "Warp Blast \u{2014} When this creature enters, it deals X damage to any target.",
    ] {
        let label = label_token_types(text);
        assert!(
            label
                .iter()
                .all(|(_, token_type)| token_type == "abilityWord"),
            "{text}: {label:?}"
        );
    }
}

/// A catalog mechanic name, a Saga chapter list, a cost dash and a label
/// opening with a trigger word keep their meaning: none is a flavor word.
#[test]
fn mechanic_names_chapters_costs_and_trigger_heads_are_not_flavor_labels() {
    for text in [
        "Max speed \u{2014} Whenever this creature attacks, draw a card.",
        "Equip \u{2014} Pay 3 life.",
        "Kicker \u{2014} Sacrifice a creature.",
        "II, III \u{2014} Whenever you cast a spell this turn, draw a card.",
        "When this creature enters, choose one \u{2014}\n\u{2022} Draw a card.\n\u{2022} You gain 3 life.",
    ] {
        let label = label_token_types(text);
        assert!(
            label
                .iter()
                .any(|(_, token_type)| token_type != "abilityWord"),
            "{text}: {label:?}"
        );
    }
}
