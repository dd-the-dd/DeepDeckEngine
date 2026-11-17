use super::*;
use crate::oracle::{model::AbilitySource, syntax::recognize_entities};

fn claims(text: &str) -> bool {
    let source = AbilitySource {
        face_id: None,
        text: text.to_string(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    claims_promoted_player_spell_tax_draw_family(&AbilityInput {
        face_name: "Synthetic cast watcher",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
}

#[test]
fn an_optional_draw_without_a_tax_marker_belongs_to_the_generic_program() {
    for text in [
        "Whenever a player casts a spell, you may draw a card.",
        "Whenever an opponent casts a spell, you may draw two cards.",
        "Whenever an opponent casts a spell, you may draw a card. Unless malformed suffix.",
    ] {
        assert!(
            !claims(text),
            "a tax cannot be inferred from may draw: {text}"
        );
    }
}

#[test]
fn a_tax_marker_preserves_the_familys_committed_failures() {
    for text in [
        "Whenever an opponent casts a spell, you may draw a card unless that player pays {1}.",
        "Whenever a player casts a spell, you may draw nonsense unless that player pays {1}.",
        "Whenever a player casts a spell, you may draw a card unless an opponent pays {1}.",
        "Whenever a player casts a spell, you may draw a card unless that player pays.",
    ] {
        assert!(
            claims(text),
            "the explicit tax marker must retain its diagnostics: {text}"
        );
    }
}

#[test]
fn specialized_tax_child_rejects_a_payer_without_the_event_player_binding() {
    let text = "Whenever a player casts a spell, you may draw a card unless an opponent pays {1}.";
    let source = AbilitySource {
        face_id: None,
        text: text.into(),
        line_start: 0,
        line_end: 0,
    };
    let tokens = recognize_entities(text);
    let failure = parse_promoted_player_spell_tax_draw(&AbilityInput {
        face_name: "Synthetic cast watcher",
        face_type_line: "Enchantment",
        source: &source,
        tokens: &tokens,
        name_links: &[],
        name_resolution_failure: None,
    })
    .err()
    .expect("the specialized child must reject an unrelated payer");
    assert_eq!(failure.state, "reference.triggering-player.determiner");
    assert_eq!(failure.actual.as_deref(), Some("an"));
    assert!(
        failure
            .trace
            .iter()
            .any(|transition| transition.state == "effect.optional-draw-unless-payment.payer")
    );
}
