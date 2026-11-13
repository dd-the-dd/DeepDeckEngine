use crate::oracle::model::AbilitySource;

use super::{
    AbilityInput, NameLinkKind, ResolvedNameLink, classify_promoted_ability_from_tokens,
    recognize_entities,
};

fn promoted_kind(text: &str) -> Option<&'static str> {
    promoted_kind_with_links(text, &[])
}

fn promoted_kind_with_links(text: &str, name_links: &[ResolvedNameLink]) -> Option<&'static str> {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    let input = AbilityInput {
        face_name: "Generalized Test Permanent",
        face_type_line: "Land",
        source: &source,
        tokens: &tokens,
        name_links,
        name_resolution_failure: None,
    };
    classify_promoted_ability_from_tokens(&input)
}

#[test]
fn resolved_current_name_is_a_valid_entering_source() {
    let text = "Test Marsh enters tapped unless you control two or fewer other lands.";
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Test Marsh".to_string(),
        raw: "Test Marsh".to_string(),
        token_start: 0,
        token_end: 2,
        span_start: 0,
        span_end: "Test Marsh".len(),
    }];
    assert_eq!(
        promoted_kind_with_links(text, &links),
        Some("replacementEffect")
    );
}

#[test]
fn resolved_current_name_uses_the_token_only_unconditional_entry_envelope() {
    let text = "Timeless Lotus enters tapped.";
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Timeless Lotus".to_string(),
        raw: "Timeless Lotus".to_string(),
        token_start: 0,
        token_end: 2,
        span_start: 0,
        span_end: "Timeless Lotus".len(),
    }];

    assert_eq!(
        promoted_kind_with_links(text, &links),
        Some("replacementEffect")
    );
}

#[test]
fn named_conditional_entry_is_left_to_its_own_parent_family() {
    let text = "Port Town enters tapped unless you reveal a Plains or Island card from your hand.";
    let links = [ResolvedNameLink {
        kind: NameLinkKind::CurrentCard,
        canonical_name: "Port Town".to_string(),
        raw: "Port Town".to_string(),
        token_start: 0,
        token_end: 2,
        span_start: 0,
        span_end: "Port Town".len(),
    }];

    assert_eq!(promoted_kind_with_links(text, &links), None);
}

#[test]
fn tapped_sentence_followed_by_another_replacement_sentence_is_not_claimed() {
    assert_eq!(
        promoted_kind("This land enters tapped. As it enters, choose a color."),
        None
    );
}

#[test]
fn controlled_count_entry_uses_the_token_only_replacement_envelope() {
    assert_eq!(
        promoted_kind("This land enters tapped unless you control four or fewer other lands."),
        Some("replacementEffect")
    );
}

#[test]
fn attack_tax_uses_the_token_only_static_envelope() {
    assert_eq!(
        promoted_kind(
            "Creatures can't attack you unless their controller pays {2} for each creature they control that's attacking you."
        ),
        Some("staticAbility")
    );
}

#[test]
fn spell_cost_increase_uses_the_token_only_static_envelope() {
    for text in [
        "Noncreature spells cost {1} more to cast.",
        "Creature spells cost {2} more to cast.",
        "Red creature spells and green creature spells cost {1} more to cast.",
    ] {
        assert_eq!(promoted_kind(text), Some("staticAbility"), "{text}");
    }
}

#[test]
fn activation_reduction_and_conditional_costs_stay_outside_spell_tax_envelope() {
    for text in [
        "Activated abilities of white enchantments cost {3} more to activate.",
        "Noncreature spells cost {1} less to cast.",
        "Noncreature spells cost {1} more to cast if it is your turn.",
    ] {
        assert_eq!(promoted_kind(text), None, "{text}");
    }
}

#[test]
fn fixed_spell_reduction_envelope_is_structural_across_caster_scopes() {
    for text in [
        "Artifact spells you cast cost {2} less to cast.",
        "Each spell you cast that's red or green costs {1} less to cast.",
        "Red spells your opponents cast cost {1} less to cast.",
    ] {
        assert_eq!(promoted_kind(text), Some("staticAbility"), "{text}");
    }
}

#[test]
fn different_attack_tax_multiplicity_stays_outside_the_fixed_per_attacker_family() {
    assert_eq!(
        promoted_kind(
            "Creatures can't attack you unless their controller pays {X}, where X is the number of basic land types among lands you control."
        ),
        None
    );
}

#[test]
fn malformed_comparator_still_reaches_the_committed_native_family() {
    assert_eq!(
        promoted_kind("This land enters tapped unless you control two or exactly other lands."),
        Some("replacementEffect")
    );
}

#[test]
fn simple_compound_and_whole_parenthetical_mana_costs_use_the_token_envelope() {
    for text in [
        "{T}: Add {B} or {G}.",
        "{1}, {T}: Add {U}.",
        "({T}: Add {R}.)",
    ] {
        assert_eq!(promoted_kind(text), Some("activatedAbility"), "{text}");
    }
}

#[test]
fn generic_root_activation_envelope_classifies_semantic_near_misses() {
    for text in [
        "{4}, {T}: Target creature can't be blocked this combat.",
        "Sacrifice a creature: Target creature can't attack this turn.",
        "(Discard a card: Target creature can't block this turn.)",
    ] {
        assert_eq!(promoted_kind(text), Some("activatedAbility"), "{text}");
    }
}

#[test]
fn nested_or_quoted_activation_envelopes_do_not_reclassify_the_parent() {
    assert_eq!(
        promoted_kind("Creatures you control have \"{T}: Add {G}.\""),
        None
    );
    assert_eq!(
        promoted_kind("Create a token. (It has \"{T}: Add {G}.\")"),
        None
    );
    assert_eq!(
        promoted_kind("Create a token. Choose a color: Draw a card."),
        None
    );
    assert_eq!(
        promoted_kind("Whenever a creature enters, it gains \"{T}: Add {G}.\""),
        Some("triggeredAbility")
    );
}

#[test]
fn activated_library_search_uses_the_token_only_envelope() {
    for text in [
        "{T}, Pay 1 life, Sacrifice this land: Search your library for a Mountain or Plains card, put it onto the battlefield, then shuffle.",
        "{T}, Pay 1 life, Sacrifice this land: Search your library for an Island or Swamp card, put it onto the battlefield, then shuffle.",
    ] {
        assert_eq!(promoted_kind(text), Some("activatedAbility"), "{text}");
    }
}

#[test]
fn malformed_library_search_cost_still_reaches_the_committed_native_family() {
    let text = "{T}, Pay 1 life Sacrifice this land: Search your library for a Mountain or Plains card, put it onto the battlefield, then shuffle.";
    assert_eq!(promoted_kind(text), Some("activatedAbility"));
}

#[test]
fn quoted_and_reminder_library_search_colons_do_not_classify_the_enclosing_ability() {
    for text in [
        "Creatures you control have \"{T}: Search your library for a land card.\"",
        "Create a token. (It has \"{T}: Search your library for a land card.\")",
        "Create a token. ({T}: Search your library for a land card.)",
    ] {
        assert_eq!(promoted_kind(text), None, "{text}");
    }
}

#[test]
fn quoted_and_reminder_colons_do_not_classify_the_enclosing_ability() {
    for text in [
        "Creatures you control have \"{T}: Add {G}.\"",
        "Create a token. (It has \"{T}: Add {C}.\")",
    ] {
        assert_eq!(promoted_kind(text), None, "{text}");
    }
}

#[test]
fn triggered_entry_sentence_stays_triggered_instead_of_becoming_a_replacement_effect() {
    assert_eq!(
        promoted_kind(
            "Whenever a land enters tapped unless a player pays {1}, that player draws a card."
        ),
        Some("triggeredAbility")
    );
}

#[test]
fn unrelated_subject_is_not_promoted_as_the_current_entering_source() {
    assert_eq!(
        promoted_kind("A land enters tapped unless you control two or fewer other lands."),
        None
    );
}

#[test]
fn non_mana_activation_has_a_token_driven_ability_envelope() {
    assert_eq!(promoted_kind("{T}: Draw a card."), Some("activatedAbility"));
}

#[test]
fn destroy_regeneration_sequence_uses_the_token_only_spell_envelope() {
    assert_eq!(
        promoted_kind("Destroy target creature. It can't be regenerated."),
        Some("spellAbility")
    );
    assert_eq!(
        promoted_kind("Destroy target creature. It can't be restored."),
        Some("spellAbility"),
        "a malformed amendment must still reach the committed semantic automaton"
    );
}

#[test]
fn collection_destruction_uses_the_token_only_spell_envelope() {
    for text in [
        "Destroy all creatures.",
        "Destroy each noncreature permanent you control.",
        "Destroy all nonland permanents your opponents own.",
    ] {
        assert_eq!(
            promoted_kind(text),
            Some("spellAbility"),
            "malformed claimed members must still reach the committed semantic automaton: {text}"
        );
    }
}

#[test]
fn counter_sequences_use_the_token_only_spell_envelope() {
    for text in [
        "Counter target enchantment, instant, or sorcery spell. Its controller creates a 2/2 blue Bird creature token with flying.",
        "Counter target artifact spell. Its controller creates a 3/3 red Goblin creature token with haste.",
    ] {
        assert_eq!(promoted_kind(text), Some("spellAbility"), "{text}");
    }
}

#[test]
fn trigger_heads_are_classified_from_the_shared_token_stream() {
    for text in [
        "Whenever this creature attacks, draw a card.",
        "When this artifact enters, gain 1 life.",
        "At the beginning of your upkeep, scry 1.",
    ] {
        assert_eq!(promoted_kind(text), Some("triggeredAbility"), "{text}");
    }
}

#[test]
fn ability_word_labeled_trigger_heads_use_the_token_only_envelope() {
    for text in [
        "Landfall \u{2014} Whenever a land you control enters, draw a card.",
        "Revolt \u{2014} When this artifact enters, draw a card.",
        "Will of the council \u{2014} At the beginning of your upkeep, draw a card.",
    ] {
        assert_eq!(promoted_kind(text), Some("triggeredAbility"), "{text}");
    }
}

#[test]
fn ability_word_without_a_trigger_head_is_not_promoted_as_triggered() {
    assert_eq!(
        promoted_kind("Landfall \u{2014} Put a quest counter on this enchantment."),
        None
    );
}
