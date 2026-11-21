use super::*;
use crate::oracle::syntax::recognize_entities;
#[test]
fn compact_mana_envelope_reuses_sequence_connectors_for_followups() {
    for separator in [".", ",", ", then", ", and", " then"] {
        let text = format!("{{T}}: Add {{G}}{separator} discard a card.");
        let tokens = recognize_entities(&text);
        let mut cursor = OracleTokenCursor::new("test.mana-followup-boundary", &tokens);
        let node = recognize_activated_mana_ability(&mut cursor, &[], None).unwrap();
        cursor.finish().unwrap();
        assert!(matches!(node.effect, AddManaEffectNode::Fixed(_)));
        assert!(matches!(
            node.followup.unwrap().effects.as_slice(),
            [super::super::LocalEffectNode::DiscardCards(_)]
        ));
    }
}
#[test]
fn an_effect_connector_cannot_hide_a_missing_or_unsupported_followup() {
    for text in [
        "{T}: Add {G},.",
        "{T}: Add {G},, discard a card.",
        "{T}: Add {G}, or discard a card.",
        "{T}: Add {G}, teleport a card.",
        "{T}: Add {G}, then.",
        "{T}: Add {G}, and.",
        "{T}: Add {G}, then teleport a card.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("test.mana-followup-near-miss", &tokens);
        assert!(
            recognize_activated_mana_ability(&mut cursor, &[], None).is_err(),
            "{text}"
        );
    }
}
