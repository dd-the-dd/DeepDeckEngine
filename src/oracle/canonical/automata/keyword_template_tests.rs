use serde_json::Value;

use super::*;
use crate::oracle::ability_catalog;

/// The cached official rules text the template rows quote (CR 702).
const CACHED_RULES_CATALOG: &str =
    include_str!("../../../../data/ability-catalog/ability-catalog-v1.json");

fn normalized(text: &str) -> String {
    text.replace(['\u{2019}', '\u{2018}'], "'")
        .replace(['\u{201c}', '\u{201d}'], "\"")
        .to_lowercase()
}

fn line(text: &str) -> Option<MachineResult<KeywordTemplateInvocation>> {
    let tokens = recognize_oracle_entities(text);
    recognize_keyword_template_line(&tokens, text, "Creature \u{2014} Human Warrior")
}

#[test]
fn every_row_quotes_its_cached_rules_clause_and_matches_the_catalog_version() {
    let catalog: Value = serde_json::from_str(CACHED_RULES_CATALOG).expect("cached catalog");
    let document = keyword_template_document();
    assert_eq!(document.catalog_version, ability_catalog::catalog_version());
    assert_eq!(document.catalog_version, catalog["catalogVersion"]);
    assert_eq!(
        document.rules_effective_date,
        ability_catalog::rules_effective_date()
    );
    assert!(!document.templates.is_empty());
    let mut identities = std::collections::BTreeSet::new();
    for template in &document.templates {
        assert!(
            identities.insert(&template.canonical_id),
            "{}",
            template.canonical_id
        );
        let entry = catalog["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|entry| entry["canonicalId"] == template.canonical_id.as_str())
            .unwrap_or_else(|| panic!("{} is a cataloged ability", template.canonical_id));
        assert_eq!(
            entry["category"], "keywordAbility",
            "{}",
            template.canonical_id
        );
        let clause = entry["rulesText"]["clauses"]
            .as_array()
            .unwrap()
            .iter()
            .find(|clause| clause["clauseId"] == template.rules_clause.as_str())
            .unwrap_or_else(|| panic!("{} cites a cached clause", template.canonical_id));
        assert_eq!(clause["sourceSection"], template.rules_section.as_str());
        let rules_text = normalized(clause["text"].as_str().unwrap());
        assert!(!template.abilities.is_empty(), "{}", template.canonical_id);
        for ability in &template.abilities {
            let quoted = render_rules_form(ability).expect("a renderable template");
            let quoted = normalized(quoted.trim_end_matches('.'));
            assert!(
                rules_text.contains(&quoted),
                "{}: `{quoted}` is not quoted from CR {}: {rules_text}",
                template.canonical_id,
                template.rules_section
            );
            // A definition is never itself a template keyword line, so an
            // expansion compiles its parts exactly one level deep.
            assert!(
                line(ability).is_none(),
                "{}: {ability}",
                template.canonical_id
            );
        }
    }
}

#[test]
fn every_reminder_and_definition_instantiates_with_each_parameter_shape() {
    for template in &keyword_template_document().templates {
        let arguments = KeywordTemplateArguments {
            count: matches!(
                template.parameter,
                KeywordTemplateParameter::Count | KeywordTemplateParameter::CountThenManaCost
            )
            .then_some(2),
            cost: matches!(
                template.parameter,
                KeywordTemplateParameter::ManaCost | KeywordTemplateParameter::CountThenManaCost
            )
            .then(|| "{1}{G}".to_string()),
            subtypes: vec!["Human".to_string(), "Warrior".to_string()],
        };
        for text in template.abilities.iter().chain(&template.reminders) {
            let rendered = instantiate(text, &arguments)
                .unwrap_or_else(|error| panic!("{}: {error}", template.canonical_id));
            assert!(
                !rendered.contains('$'),
                "{}: {rendered}",
                template.canonical_id
            );
        }
    }
}

#[test]
fn placeholders_render_digits_number_words_agreement_and_the_printed_cost() {
    let one = KeywordTemplateArguments {
        count: Some(1),
        cost: Some("{2}{W}".to_string()),
        ..KeywordTemplateArguments::default()
    };
    let three = KeywordTemplateArguments {
        count: Some(3),
        ..KeywordTemplateArguments::default()
    };
    let template = "gets +$N/+$N, put $N{a +1/+1 counter|$Nw +1/+1 counters} on it";
    assert_eq!(
        instantiate(template, &one).unwrap(),
        "gets +1/+1, put a +1/+1 counter on it"
    );
    assert_eq!(
        instantiate(template, &three).unwrap(),
        "gets +3/+3, put three +1/+1 counters on it"
    );
    assert_eq!(
        instantiate("$cost, {T}: x", &one).unwrap(),
        "{2}{W}, {T}: x"
    );
    assert_eq!(
        render_rules_form(template).unwrap(),
        "gets +N/+N, put N +1/+1 counters on it"
    );
    // Missing arguments and unknown placeholders are errors, never blanks.
    assert!(instantiate("$cost", &three).is_err());
    assert!(instantiate("$X", &three).is_err());
    assert!(instantiate("$N{a counter}", &three).is_err());
    assert!(
        instantiate(
            "$Nw",
            &KeywordTemplateArguments {
                count: Some(21),
                ..KeywordTemplateArguments::default()
            }
        )
        .is_err()
    );
}

#[test]
fn a_keyword_line_reads_its_lexeme_parameter_and_validated_reminder() {
    let bare = line("Battle cry").expect("claimed").expect("read");
    assert_eq!(bare.template.canonical_id, "battle-cry");
    assert!(!bare.reminder);
    assert_eq!(
        bare.abilities,
        [
            "Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn."
        ]
    );
    let reminded = line(
        "Battle cry (Whenever this creature attacks, each other attacking creature gets +1/+0 until end of turn.)",
    )
    .expect("claimed")
    .expect("read");
    assert!(reminded.reminder);
}

#[test]
fn a_claimed_line_with_a_foreign_reminder_or_tail_fails_explicitly() {
    let wrong =
        line("Battle cry (Whenever this creature attacks, it gets +1/+0 until end of turn.)")
            .expect("claimed")
            .expect_err("a reminder that is not the keyword's");
    assert_eq!(wrong.state, "keyword.template.reminder.body");
    let unclosed = line("Battle cry (Whenever this creature attacks")
        .expect("claimed")
        .expect_err("an unclosed reminder");
    assert_eq!(unclosed.state, "keyword.template.reminder.open");
}

#[test]
fn only_the_lexeme_followed_by_its_parameter_start_is_claimed() {
    // A sentence about the keyword, a longer word and a keyword list are not
    // template keyword lines.
    assert!(line("Battle cry creatures get +1/+0.").is_none());
    assert!(line("Battlecry").is_none());
    assert!(line("Flying, battle cry").is_none());
}

#[test]
fn repetition_multiple_and_face_subtype_placeholders_render_their_forms() {
    let one = KeywordTemplateArguments {
        count: Some(1),
        ..KeywordTemplateArguments::default()
    };
    let two = KeywordTemplateArguments {
        count: Some(2),
        subtypes: vec!["Bird".to_string(), "Soldier".to_string()],
        ..KeywordTemplateArguments::default()
    };
    let three = KeywordTemplateArguments {
        count: Some(3),
        subtypes: vec![
            "Human".to_string(),
            "Cleric".to_string(),
            "Rogue".to_string(),
        ],
        ..KeywordTemplateArguments::default()
    };
    // Firebending's mana: N repeated symbols, quoted as `N {R}` in the CR.
    assert_eq!(
        instantiate("add $Nrepeat[{R}].", &two).unwrap(),
        "add {R}{R}."
    );
    assert_eq!(
        render_rules_form("add $Nrepeat[{R}].").unwrap(),
        "add N {R}."
    );
    // Devour's reminder multiple.
    let multiple = "with $Ntimes[that many] counters";
    assert_eq!(
        instantiate(multiple, &one).unwrap(),
        "with that many counters"
    );
    assert_eq!(
        instantiate(multiple, &two).unwrap(),
        "with twice that many counters"
    );
    assert_eq!(
        instantiate(multiple, &three).unwrap(),
        "with three times that many counters"
    );
    // Reminder-only subtype renderings (Embalm, Amplify).
    assert_eq!(
        instantiate("a white Zombie $subtypes", &three).unwrap(),
        "a white Zombie Human Cleric Rogue"
    );
    assert_eq!(
        instantiate("each $subtypeAlternatives card", &two).unwrap(),
        "each Bird and/or Soldier card"
    );
    assert_eq!(
        instantiate("each $subtypeAlternatives card", &three).unwrap(),
        "each Human, Cleric, and/or Rogue card"
    );
    // A definition never names a face's subtypes, a face without subtypes
    // renders no subtype reminder, and a repetition needs its brackets.
    assert!(render_rules_form("a white Zombie $subtypes").is_err());
    assert!(instantiate("$subtypes", &one).is_err());
    assert!(instantiate("$Nrepeat{R}", &two).is_err());
}

#[test]
fn a_reminder_restates_the_definition_with_the_face_subtypes() {
    let text = "Embalm {W} ({W}, Exile this card from your graveyard: Create a token that's a copy of it, except it's a white Zombie Cat with no mana cost. Embalm only as a sorcery.)";
    let tokens = recognize_oracle_entities(text);
    let read = recognize_keyword_template_line(&tokens, text, "Creature \u{2014} Cat")
        .expect("claimed")
        .expect("the reminder names the face's subtype");
    assert_eq!(read.arguments.subtypes, ["Cat"]);
    // Near miss: the same reminder on a face of another subtype.
    let wrong = recognize_keyword_template_line(&tokens, text, "Creature \u{2014} Dog")
        .expect("claimed")
        .expect_err("a reminder naming another subtype");
    assert_eq!(wrong.state, "keyword.template.reminder.body");
}
