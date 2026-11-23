use super::super::{EffectRecognitionContext, recognize_effect_sequence_with_context};
use super::*;
use crate::oracle::syntax::recognize_entities;

#[test]
fn historical_zone_change_wrappers_keep_battlefield_domain_caps() {
    for text in [
        "Return target spell to its owner's hand.",
        "Return target spell or creature to its owner's hand.",
    ] {
        let tokens = recognize_entities(text);
        let mut default = OracleTokenCursor::new("default-zone-domain", &tokens);
        assert!(
            recognize_zone_change_effect(&mut default).is_err(),
            "{text}"
        );
        let mut cardinality = OracleTokenCursor::new("cardinality-is-not-domain-optin", &tokens);
        assert!(
            recognize_zone_change_effect_with_support(
                &mut cardinality,
                ZoneChangeTargetSupport::TargetCollection,
            )
            .is_err(),
            "{text}"
        );
        let mut opted = OracleTokenCursor::new("opted-return-domain", &tokens);
        recognize_zone_change_effect_with_policy(
            &mut opted,
            ZoneChangeTargetSupport::Scalar,
            ZoneChangeDomains::SpellOrBattlefield,
        )
        .expect("only an explicit domain-aware parent enables spell Return");
    }
}

#[test]
fn permanent_nodes_stay_identical_and_destroy_exile_never_expand_domains() {
    let tokens =
        recognize_entities("Return target creature an opponent controls to its owner's hand.");
    let mut old = OracleTokenCursor::new("old-permanent-return", &tokens);
    let mut opted = OracleTokenCursor::new("opted-permanent-return", &tokens);
    assert_eq!(
        recognize_zone_change_effect(&mut old).unwrap(),
        recognize_zone_change_effect_with_policy(
            &mut opted,
            ZoneChangeTargetSupport::Scalar,
            ZoneChangeDomains::SpellOrBattlefield
        )
        .unwrap()
    );
    for text in [
        "Destroy target spell or creature.",
        "Exile target spell or creature.",
    ] {
        let tokens = recognize_entities(text);
        let mut cursor = OracleTokenCursor::new("destroy-exile-domain-cap", &tokens);
        assert!(
            recognize_zone_change_effect_with_policy(
                &mut cursor,
                ZoneChangeTargetSupport::TargetCollection,
                ZoneChangeDomains::SpellOrBattlefield
            )
            .is_err()
        );
    }
}

#[test]
fn activated_and_primitive_modal_parents_cannot_emit_wrong_permanent_ir() {
    // The public dispatcher, not a historical activated family, owns this
    // ability: a spell-or-creature target must never lower to permanents only.
    let parsed = crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: "Domain Probe".to_string(),
        type_line: "Artifact".to_string(),
        mana_cost: Some("{2}".to_string()),
        oracle_text: Some("{U}: Return target spell or creature to its owner's hand.".to_string()),
        layout: None,
        faces: Vec::new(),
    });
    if let Some(rule) = parsed.abilities[0].rule.as_ref() {
        let candidates = &rule["declaration"]["decisions"][0]["candidates"];
        assert_eq!(candidates["kind"], "union", "{rule}");
        assert!(
            candidates["sets"]
                .as_array()
                .is_some_and(|sets| sets.iter().any(|set| set["kind"] == "spells")),
            "{rule}"
        );
    }
}

#[test]
fn shared_effect_sequence_explicitly_receives_domain_support() {
    let tokens =
        recognize_entities("Return target spell or creature to its owner's hand and draw a card.");
    let mut cursor = OracleTokenCursor::new("sequence-domain-optin", &tokens);
    let sequence = recognize_effect_sequence_with_context(
        &mut cursor,
        &[],
        None,
        &EffectRecognitionContext::default()
            .with_zone_change_domains(ZoneChangeDomains::SpellOrBattlefield),
    )
    .unwrap();
    assert!(matches!(
        sequence.effects.as_slice(),
        [
            LocalEffectNode::ZoneChange(_),
            LocalEffectNode::DrawCards(_)
        ]
    ));
}

#[test]
fn historical_effect_sequence_entrypoints_do_not_inherit_a_global_spell_domain() {
    let tokens = recognize_entities("Return target spell or creature to its owner's hand.");
    let mut old = OracleTokenCursor::new("legacy-sequence-domain-cap", &tokens);
    assert!(super::super::recognize_effect_sequence(&mut old, &[], None).is_err());
    for context in [
        EffectRecognitionContext::default(),
        EffectRecognitionContext::for_source(super::super::LocalObjectReference::SourceSelf),
        EffectRecognitionContext::for_source_cast_x(super::super::LocalObjectReference::SourceSelf),
    ] {
        let mut cursor = OracleTokenCursor::new("legacy-context-domain-cap", &tokens);
        assert!(recognize_effect_sequence_with_context(&mut cursor, &[], None, &context).is_err());
    }
}
