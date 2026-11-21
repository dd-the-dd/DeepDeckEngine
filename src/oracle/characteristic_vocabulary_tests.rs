use super::*;

#[test]
fn all_card_types_have_strict_oracle_forms_and_stable_ir_names() {
    let cases = [
        (CardType::Artifact, "artifact", "artifacts", "Artifact"),
        (CardType::Battle, "battle", "battles", "Battle"),
        (CardType::Creature, "creature", "creatures", "Creature"),
        (
            CardType::Enchantment,
            "enchantment",
            "enchantments",
            "Enchantment",
        ),
        (CardType::Instant, "instant", "instants", "Instant"),
        (CardType::Kindred, "kindred", "kindred", "Kindred"),
        (CardType::Land, "land", "lands", "Land"),
        (
            CardType::Planeswalker,
            "planeswalker",
            "planeswalkers",
            "Planeswalker",
        ),
        (CardType::Sorcery, "sorcery", "sorceries", "Sorcery"),
    ];

    for (card_type, singular, plural, ir_name) in cases {
        assert_eq!(card_type.oracle_singular(), singular);
        assert_eq!(card_type.oracle_plural(), plural);
        assert_eq!(card_type.ir_name(), ir_name);
        assert_eq!(
            lookup_card_type(singular, AcceptedForms::SingularOnly),
            Some(card_type)
        );
        assert_eq!(
            lookup_card_type(plural, AcceptedForms::SingularOnly),
            if singular == plural {
                Some(card_type)
            } else {
                None
            }
        );
        assert_eq!(
            lookup_card_type(plural, AcceptedForms::SingularOrPlural),
            Some(card_type)
        );
    }

    assert_eq!(
        lookup_card_type("CrEaTuReS", AcceptedForms::SingularOrPlural),
        Some(CardType::Creature)
    );
    assert_eq!(
        lookup_card_type("sorcerys", AcceptedForms::SingularOrPlural),
        None
    );
    assert_eq!(
        lookup_card_type("nonland", AcceptedForms::SingularOrPlural),
        None
    );
}

#[test]
fn permanent_card_types_include_lands_but_exclude_instant_and_sorcery() {
    for (raw, expected) in [
        ("artifact", Some(CardType::Artifact)),
        ("battle", Some(CardType::Battle)),
        ("creature", Some(CardType::Creature)),
        ("enchantment", Some(CardType::Enchantment)),
        ("kindred", None),
        ("land", Some(CardType::Land)),
        ("planeswalker", Some(CardType::Planeswalker)),
        ("instant", None),
        ("sorcery", None),
    ] {
        assert_eq!(
            lookup_permanent_card_type(raw, AcceptedForms::SingularOnly),
            expected
        );
    }
}

#[test]
fn kindred_is_known_but_another_card_type_establishes_permanent_domain() {
    // Cached official CR110.4: six permanent types; Kindred depends on its other card types.
    assert_eq!(
        lookup_card_type("kindred", AcceptedForms::SingularOnly),
        Some(CardType::Kindred)
    );
    assert!(type_line_has_permanent_card_type(
        "Kindred Enchantment — Elf"
    ));
    assert!(!type_line_has_permanent_card_type("Kindred Instant — Elf"));
    assert!(!type_line_has_permanent_card_type("Kindred"));
}

#[test]
fn supertypes_are_adjectives_without_plural_aliases() {
    for (raw, expected, ir_name) in [
        ("basic", CardSupertype::Basic, "Basic"),
        ("legendary", CardSupertype::Legendary, "Legendary"),
    ] {
        assert_eq!(
            lookup_supertype(raw, AcceptedForms::SingularOnly),
            Some(expected)
        );
        assert_eq!(expected.oracle_singular(), raw);
        assert_eq!(expected.ir_name(), ir_name);
    }

    assert_eq!(
        lookup_supertype("basics", AcceptedForms::SingularOrPlural),
        None
    );
    assert_eq!(
        lookup_supertype("legendaries", AcceptedForms::SingularOrPlural),
        None
    );
    assert_eq!(
        lookup_supertype("nonbasic", AcceptedForms::SingularOrPlural),
        None
    );
}

#[test]
fn basic_land_subtypes_preserve_singular_plural_and_ir_spelling() {
    let cases = [
        (BasicLandSubtype::Plains, "plains", "plains", "Plains"),
        (BasicLandSubtype::Island, "island", "islands", "Island"),
        (BasicLandSubtype::Swamp, "swamp", "swamps", "Swamp"),
        (
            BasicLandSubtype::Mountain,
            "mountain",
            "mountains",
            "Mountain",
        ),
        (BasicLandSubtype::Forest, "forest", "forests", "Forest"),
    ];

    for (subtype, singular, plural, ir_name) in cases {
        assert_eq!(subtype.oracle_singular(), singular);
        assert_eq!(subtype.oracle_plural(), plural);
        assert_eq!(subtype.ir_name(), ir_name);
        assert_eq!(
            lookup_basic_land_subtype(singular, AcceptedForms::SingularOnly),
            Some(subtype)
        );
        assert_eq!(
            lookup_basic_land_subtype(plural, AcceptedForms::SingularOnly),
            if singular == plural {
                Some(subtype)
            } else {
                None
            }
        );
        assert_eq!(
            lookup_basic_land_subtype(plural, AcceptedForms::SingularOrPlural),
            Some(subtype)
        );
    }

    assert_eq!(
        lookup_card_type("forest", AcceptedForms::SingularOrPlural),
        None
    );
    assert_eq!(
        lookup_supertype("forest", AcceptedForms::SingularOrPlural),
        None
    );
}

#[test]
fn object_characteristics_keep_commander_distinct_from_card_types_and_supertypes() {
    assert_eq!(
        lookup_object_characteristic("commander", AcceptedForms::SingularOnly),
        Some(ObjectCharacteristic::Commander)
    );
    assert_eq!(
        lookup_object_characteristic("commanders", AcceptedForms::SingularOrPlural),
        Some(ObjectCharacteristic::Commander)
    );
    assert_eq!(
        lookup_card_type("commander", AcceptedForms::SingularOnly),
        None
    );
    assert_eq!(
        lookup_supertype("commander", AcceptedForms::SingularOnly),
        None
    );
}

#[test]
fn amass_subtypes_use_a_closed_exact_vocabulary() {
    for (subtype, singular, plural) in [
        (AmassArmySubtype::Goblin, "Goblin", "Goblins"),
        (AmassArmySubtype::Orc, "Orc", "Orcs"),
        (AmassArmySubtype::Zombie, "Zombie", "Zombies"),
    ] {
        assert_eq!(subtype.oracle_singular(), singular);
        assert_eq!(subtype.oracle_plural(), plural);
        assert_eq!(subtype.ir_name(), singular);
        assert_eq!(
            lookup_amass_army_subtype(singular, AcceptedForms::SingularOnly),
            Some(subtype)
        );
        assert_eq!(
            lookup_amass_army_subtype(plural, AcceptedForms::SingularOnly),
            None
        );
        assert_eq!(
            lookup_amass_army_subtype(plural, AcceptedForms::SingularOrPlural),
            Some(subtype)
        );
    }

    for unsupported in ["Army", "Armies", "Elves", "orcish", " Orcs"] {
        assert_eq!(
            lookup_amass_army_subtype(unsupported, AcceptedForms::SingularOrPlural),
            None
        );
    }
}

#[test]
fn creature_subtypes_use_the_versioned_complete_cr_205_3m_vocabulary() {
    let vocabulary = creature_subtype_vocabulary();
    assert_eq!(vocabulary.canonical.len(), 367);
    assert_eq!(vocabulary.source.authority, "Wizards of the Coast");
    assert_eq!(vocabulary.source.effective_date, "2026-11-06");
    assert_eq!(vocabulary.source.rule, "205.3m");
    assert_eq!(
        vocabulary.source.document_sha256,
        "524f5195a1e5f54bb13c735edf399ac717a4458e2713f398d1a3e6877186b829"
    );

    assert_eq!(CreatureSubtype::Shark.oracle_singular(), "Shark");
    assert_eq!(CreatureSubtype::Shark.ir_name(), "Shark");
    assert_eq!(
        lookup_creature_subtype("Shark", AcceptedForms::SingularOnly),
        Some(CreatureSubtype::Shark)
    );
    assert_eq!(
        lookup_creature_subtype("Sharks", AcceptedForms::SingularOnly),
        None
    );
    assert_eq!(
        lookup_creature_subtype("Sharks", AcceptedForms::SingularOrPlural),
        Some(CreatureSubtype::Shark)
    );
    for supported in [
        "Beast",
        "Elephant",
        "Goblin",
        "Phyrexian",
        "Time Lord",
        "Warrior",
    ] {
        assert_eq!(
            lookup_creature_subtype(supported, AcceptedForms::SingularOnly)
                .map(CreatureSubtype::ir_name),
            Some(supported)
        );
    }
    assert_eq!(
        lookup_creature_subtype_tokens("Time", Some("Lord"), AcceptedForms::SingularOnly)
            .map(|(subtype, width)| (subtype.ir_name(), width)),
        Some(("Time Lord", 2))
    );

    for unsupported in [
        "Imaginarytype",
        "shark creature",
        " Shark",
        "Shark ",
        "Elephants",
    ] {
        assert_eq!(
            lookup_creature_subtype(unsupported, AcceptedForms::SingularOrPlural),
            None
        );
    }
}

#[test]
fn card_colors_preserve_oracle_and_ir_names_without_semantic_aliases() {
    for (color, oracle_name, ir_name, ir_symbol) in [
        (CardColor::White, "white", "White", "W"),
        (CardColor::Blue, "blue", "Blue", "U"),
        (CardColor::Black, "black", "Black", "B"),
        (CardColor::Red, "red", "Red", "R"),
        (CardColor::Green, "green", "Green", "G"),
    ] {
        assert_eq!(color.oracle_name(), oracle_name);
        assert_eq!(color.ir_name(), ir_name);
        assert_eq!(color.ir_symbol(), ir_symbol);
        assert_eq!(lookup_card_color(oracle_name), Some(color));
        assert_eq!(
            lookup_card_color(&oracle_name.to_ascii_uppercase()),
            Some(color)
        );
    }

    for unsupported in [
        "colorless",
        "multicolored",
        "monocolored",
        "purple",
        "W",
        "U/P",
        "{W}",
        "{U/P}",
        " white",
        "reds",
    ] {
        assert_eq!(lookup_card_color(unsupported), None);
    }
}

#[test]
fn permanent_is_disambiguated_by_the_requested_grammatical_role() {
    assert_eq!(
        lookup_card_category("permanent", AcceptedForms::SingularOnly),
        Some(CardCategory::Permanent)
    );
    assert_eq!(
        lookup_object_domain("permanent", AcceptedForms::SingularOnly),
        Some(ObjectDomain::Permanent)
    );
    assert_eq!(
        lookup_card_category("permanents", AcceptedForms::SingularOrPlural),
        None
    );
    assert_eq!(
        lookup_object_domain("permanents", AcceptedForms::SingularOnly),
        None
    );
    assert_eq!(
        lookup_object_domain("permanents", AcceptedForms::SingularOrPlural),
        Some(ObjectDomain::Permanent)
    );
    assert_eq!(CardCategory::Permanent.oracle_singular(), "permanent");
}

#[test]
fn object_domains_accept_only_their_declared_number_forms() {
    for (domain, singular, plural) in [
        (ObjectDomain::Card, "card", "cards"),
        (ObjectDomain::Permanent, "permanent", "permanents"),
        (ObjectDomain::Spell, "spell", "spells"),
    ] {
        assert_eq!(domain.oracle_singular(), singular);
        assert_eq!(domain.oracle_plural(), plural);
        assert_eq!(
            lookup_object_domain(singular, AcceptedForms::SingularOnly),
            Some(domain)
        );
        assert_eq!(
            lookup_object_domain(plural, AcceptedForms::SingularOnly),
            None
        );
        assert_eq!(
            lookup_object_domain(plural, AcceptedForms::SingularOrPlural),
            Some(domain)
        );
    }

    assert_eq!(
        lookup_object_domain("player", AcceptedForms::SingularOrPlural),
        None
    );
}

#[test]
fn generic_lookup_never_crosses_the_requested_role() {
    assert_eq!(
        lookup_characteristic("land", LexemeRole::CardType, AcceptedForms::SingularOnly),
        Some(CharacteristicLexeme::CardType(CardType::Land))
    );
    assert_eq!(
        lookup_characteristic(
            "land",
            LexemeRole::ObjectDomain,
            AcceptedForms::SingularOrPlural
        ),
        None
    );
    assert_eq!(
        lookup_characteristic(
            "island",
            LexemeRole::BasicLandSubtype,
            AcceptedForms::SingularOnly
        ),
        Some(CharacteristicLexeme::BasicLandSubtype(
            BasicLandSubtype::Island
        ))
    );
    assert_eq!(
        lookup_characteristic(
            " island",
            LexemeRole::BasicLandSubtype,
            AcceptedForms::SingularOnly
        ),
        None
    );
}

#[test]
fn object_selector_lexemes_preserve_the_existing_number_policy() {
    for (raw, expected) in [
        (
            "basic",
            CharacteristicLexeme::Supertype(CardSupertype::Basic),
        ),
        (
            "legendary",
            CharacteristicLexeme::Supertype(CardSupertype::Legendary),
        ),
        (
            "artifact",
            CharacteristicLexeme::CardType(CardType::Artifact),
        ),
        (
            "artifacts",
            CharacteristicLexeme::CardType(CardType::Artifact),
        ),
        (
            "sorceries",
            CharacteristicLexeme::CardType(CardType::Sorcery),
        ),
        (
            "card",
            CharacteristicLexeme::ObjectDomain(ObjectDomain::Card),
        ),
        (
            "cards",
            CharacteristicLexeme::ObjectDomain(ObjectDomain::Card),
        ),
        (
            "permanents",
            CharacteristicLexeme::ObjectDomain(ObjectDomain::Permanent),
        ),
        (
            "spells",
            CharacteristicLexeme::ObjectDomain(ObjectDomain::Spell),
        ),
    ] {
        assert_eq!(lookup_object_selector_lexeme(raw), Some(expected));
    }

    for unsupported in [
        "basics",
        "legendaries",
        "sorcerys",
        "nonland",
        "forest",
        "player",
    ] {
        assert_eq!(lookup_object_selector_lexeme(unsupported), None);
    }
}

#[test]
fn player_role_vocabulary_preserves_the_two_singular_target_nouns() {
    for (raw, role) in [
        ("player", PlayerRole::Any),
        ("opponent", PlayerRole::Opponent),
    ] {
        assert_eq!(role.oracle_singular(), raw);
        assert_eq!(lookup_player_role(raw), Some(role));
        assert_eq!(lookup_player_role(&raw.to_ascii_uppercase()), Some(role));
    }

    for unsupported in ["players", "opponents", "you", "controller", " player"] {
        assert_eq!(lookup_player_role(unsupported), None);
    }
}
#[test]
fn token_characteristic_has_strict_singular_and_plural_forms() {
    assert_eq!(
        lookup_object_characteristic("token", AcceptedForms::SingularOnly),
        Some(ObjectCharacteristic::Token)
    );
    assert_eq!(
        lookup_object_characteristic("tokens", AcceptedForms::SingularOnly),
        None
    );
    assert_eq!(
        lookup_object_characteristic("tokens", AcceptedForms::SingularOrPlural),
        Some(ObjectCharacteristic::Token)
    );
    assert_eq!(
        lookup_object_characteristic("nontoken", AcceptedForms::SingularOrPlural),
        None
    );
}

#[test]
fn registered_plural_aliases_never_widen_singular_only_type_choices() {
    let snapshot: CreatureSubtypeSnapshot =
        serde_json::from_str(CREATURE_SUBTYPE_SNAPSHOT_JSON).unwrap();
    for alias in snapshot.exact_aliases {
        assert_eq!(
            lookup_creature_subtype(&alias.surface, AcceptedForms::SingularOnly),
            None,
            "a plural alias is not an exact singular type choice: {}",
            alias.surface
        );
        assert_eq!(
            lookup_creature_subtype(&alias.surface, AcceptedForms::SingularOrPlural)
                .map(CreatureSubtype::ir_name),
            Some(alias.canonical.as_str())
        );
    }
}

#[test]
fn corpus_verified_regular_and_irregular_plurals_preserve_canonical_identity() {
    for (surface, canonical) in [
        ("Allies", "Ally"),
        ("Wizards", "Wizard"),
        ("Goblins", "Goblin"),
        ("Zombies", "Zombie"),
        ("Elves", "Elf"),
        ("Dwarves", "Dwarf"),
        ("Mice", "Mouse"),
        ("Oxen", "Ox"),
        ("Pegasi", "Pegasus"),
        ("Fungi", "Fungus"),
    ] {
        assert_eq!(
            lookup_creature_subtype(surface, AcceptedForms::SingularOrPlural)
                .map(CreatureSubtype::ir_name),
            Some(canonical)
        );
        assert_eq!(
            lookup_creature_subtype(
                &surface.to_ascii_lowercase(),
                AcceptedForms::SingularOrPlural
            )
            .map(CreatureSubtype::ir_name),
            Some(canonical)
        );
    }
    for unsupported in [
        "Angeles",
        "Mystics",
        "Elfs",
        "Dwarfs",
        "Oxes",
        "Mouses",
        "Pegasuses",
        " Elves",
        "Elves ",
        "Elf creatures",
    ] {
        assert_eq!(
            lookup_creature_subtype(unsupported, AcceptedForms::SingularOrPlural),
            None,
            "unregistered morphological/name-like surface {unsupported}"
        );
    }
}
