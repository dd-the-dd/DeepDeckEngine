use deepdeck_card_domain::{
    AcceptedForms, NominalError, SubtypeFamily, all_creature_subtypes, parse_printed_type_line,
    resolve_creature_choice, resolve_query, resolve_surface,
};

#[test]
fn default_choices_are_the_complete_canonical_registry_not_observed_words() {
    let choices = all_creature_subtypes().unwrap();
    assert_eq!(choices.len(), 367);
    for pair in choices.windows(2) {
        assert!(
            pair[0].canonical_name().to_ascii_lowercase()
                < pair[1].canonical_name().to_ascii_lowercase()
        );
    }
    assert!(choices.contains(&resolve_creature_choice("Time Lord").unwrap()));
    assert!(choices.contains(&resolve_creature_choice("Elf").unwrap()));
    assert!(matches!(
        resolve_creature_choice("Equipment"),
        Err(NominalError::WrongChoiceFamily { .. })
    ));
    assert!(matches!(
        resolve_creature_choice("MadeUp"),
        Err(NominalError::UnknownQuery(_))
    ));
}

#[test]
fn complete_nominals_preserve_hyphens_and_do_not_match_fragments() {
    let workers = parse_printed_type_line("Artifact Creature — Assembly-Worker").unwrap();
    assert!(workers.contains(resolve_query("Assembly-Worker").unwrap()));
    assert!(matches!(
        resolve_query("Worker"),
        Err(NominalError::UnknownQuery(_))
    ));
    let land = parse_printed_type_line("Land — Urza's Power-Plant").unwrap();
    assert!(land.contains(resolve_query("Urza's").unwrap()));
    assert!(land.contains(resolve_query("Power-Plant").unwrap()));
    assert!(!land.contains(resolve_query("Plant").unwrap()));
    assert!(!land.contains(resolve_query("Urza").unwrap()));
    assert_eq!(land.iter().count(), 2);
    assert!(matches!(
        parse_printed_type_line("Artifact Creature — Assembly Worker"),
        Err(NominalError::UnknownPrintedSubtype { .. })
    ));
}

#[test]
fn time_lord_is_one_creature_type_without_changeling_or_component_choices() {
    let values = parse_printed_type_line("Legendary Creature — Time Lord Doctor").unwrap();
    assert_eq!(values.iter().count(), 2);
    assert!(values.contains(resolve_query("Time Lord").unwrap()));
    assert!(values.contains(resolve_query("Doctor").unwrap()));
    assert_eq!(values.creature_types().count(), 2);
    assert!(matches!(
        resolve_query("Time"),
        Err(NominalError::UnknownQuery(_))
    ));
    assert!(matches!(
        resolve_query("Lord"),
        Err(NominalError::UnknownQuery(_))
    ));
    assert!(matches!(
        resolve_query("Time Lord Doctor"),
        Err(NominalError::UnknownQuery(_))
    ));
}

#[test]
fn witnessed_apostrophe_aliases_share_identity_but_arbitrary_punctuation_does_not() {
    for (surface, canonical) in [
        ("C'tan", "C’tan"),
        ("Jem'Hadar", "Jem’Hadar"),
        ("Na'kuhl", "Na’kuhl"),
        ("Shi'ar", "Shi’ar"),
        ("Vau-N'Akat", "Vau-N’Akat"),
    ] {
        let id = resolve_query(surface).unwrap();
        assert_eq!(id, resolve_query(canonical).unwrap());
        assert_eq!(id.canonical_name(), canonical);
        assert_eq!(id.ir_name(), canonical);
        assert!(
            parse_printed_type_line(&format!("Creature — {surface}"))
                .unwrap()
                .contains(id)
        );
        assert!(
            parse_printed_type_line(&format!("Creature — {canonical}"))
                .unwrap()
                .contains(id)
        );
    }
    assert_eq!(resolve_query("Urza’s").unwrap().ir_name(), "Urza's");
    assert_eq!(
        resolve_query("Urza’s").unwrap(),
        resolve_query("Urza's").unwrap()
    );
    assert!(matches!(
        resolve_query("C-tan"),
        Err(NominalError::UnknownQuery(_))
    ));
    assert!(matches!(
        resolve_query("Assembly Worker"),
        Err(NominalError::UnknownQuery(_))
    ));
}

#[test]
fn grammar_plural_forms_are_separate_from_canonical_queries_and_printed_suffixes() {
    let elf = resolve_query("Elf").unwrap();
    assert_eq!(
        resolve_surface("Elves", AcceptedForms::SingularOrPlural).unwrap(),
        elf
    );
    assert!(resolve_surface("Elves", AcceptedForms::SingularOnly).is_err());
    assert!(matches!(
        resolve_query("Elves"),
        Err(NominalError::PluralQueryNotAllowed(_))
    ));
    assert!(matches!(
        parse_printed_type_line("Creature — Elves"),
        Err(NominalError::PrintedPluralNotAllowed { .. })
    ));
    assert_eq!(
        resolve_surface("Vehicles", AcceptedForms::SingularOrPlural).unwrap(),
        resolve_query("Vehicle").unwrap()
    );
    assert!(resolve_query("Vehicles").is_err());
    assert!(resolve_surface("Equipments", AcceptedForms::SingularOrPlural).is_err());
}

#[test]
fn multitype_prefix_correlates_each_nominal_without_inventing_card_types() {
    let mixed = parse_printed_type_line("Artifact Creature — Equipment Goblin").unwrap();
    assert_eq!(
        mixed
            .iter()
            .map(|m| (m.id.ir_name(), m.family))
            .collect::<Vec<_>>(),
        [
            ("Equipment", SubtypeFamily::Artifact),
            ("Goblin", SubtypeFamily::Creature),
        ]
    );
    for line in ["Kindred Sorcery — Goblin", "Kindred Instant — Shapeshifter"] {
        let values = parse_printed_type_line(line).unwrap();
        assert_eq!(values.creature_types().count(), 1);
        assert!(
            !values
                .iter()
                .next()
                .unwrap()
                .id
                .families()
                .establishes_permanent_domain()
        );
    }
    let recombined = parse_printed_type_line("Kindred Sorcery — Goblin Arcane").unwrap();
    assert_eq!(
        recombined.iter().map(|m| m.family).collect::<Vec<_>>(),
        [SubtypeFamily::Creature, SubtypeFamily::Spell]
    );
    assert!(matches!(
        parse_printed_type_line("Land — Equipment"),
        Err(NominalError::PrintedFamilyMismatch { .. })
    ));
}

#[test]
fn unknown_or_partial_suffix_never_becomes_a_known_empty_set() {
    assert!(matches!(
        parse_printed_type_line("Artifact — Equipment Unregistered"),
        Err(NominalError::UnknownPrintedSubtype { .. })
    ));
    assert!(matches!(
        parse_printed_type_line("Creature — Time"),
        Err(NominalError::UnknownPrintedSubtype { .. })
    ));
    assert!(matches!(
        parse_printed_type_line("Custom — Equipment"),
        Err(NominalError::UnknownTypeWord(_))
    ));
    assert!(matches!(
        parse_printed_type_line("Artifact —"),
        Err(NominalError::MalformedTypeLine(_))
    ));
    assert!(matches!(
        parse_printed_type_line(""),
        Err(NominalError::EmptyTypeLine)
    ));
    let plain = parse_printed_type_line("Artifact").unwrap();
    assert_eq!(plain.iter().count(), 0);
    assert!(!plain.contains(resolve_query("Equipment").unwrap()));
}

#[test]
fn unsupported_and_ambiguous_families_are_explicit() {
    let spacecraft = resolve_query("Spacecraft").unwrap();
    assert!(spacecraft.families().contains(SubtypeFamily::Artifact));
    assert!(spacecraft.families().contains(SubtypeFamily::Plane));
    assert!(!spacecraft.families().establishes_permanent_domain());
    assert_eq!(
        parse_printed_type_line("Artifact — Spacecraft")
            .unwrap()
            .iter()
            .next()
            .unwrap()
            .family,
        SubtypeFamily::Artifact
    );
    assert!(matches!(
        parse_printed_type_line("Plane — Spacecraft"),
        Err(NominalError::UnsupportedPrintedFamily(SubtypeFamily::Plane))
    ));
    assert!(matches!(
        parse_printed_type_line("Dungeon — Undercity"),
        Err(NominalError::UnsupportedPrintedFamily(
            SubtypeFamily::Dungeon
        ))
    ));
    assert!(matches!(
        parse_printed_type_line("Creature — Elf // Sorcery"),
        Err(NominalError::MalformedTypeLine(_))
    ));
}

#[test]
fn ascii_case_and_whitespace_rules_do_not_normalize_arbitrary_input() {
    assert_eq!(
        resolve_query("aSseMBly-wORkeR").unwrap(),
        resolve_query("Assembly-Worker").unwrap()
    );
    for invalid in [" Elf", "Elf ", "Time  Lord", "Time\tLord", "Time\u{a0}Lord"] {
        assert!(matches!(
            resolve_query(invalid),
            Err(NominalError::NonCanonicalWhitespace(_))
        ));
    }
    for line in [
        "Creature - Time Lord",
        "Creature – Time Lord",
        "Creature—Time Lord",
    ] {
        assert!(
            parse_printed_type_line(line)
                .unwrap()
                .contains(resolve_query("Time Lord").unwrap())
        );
    }
}
