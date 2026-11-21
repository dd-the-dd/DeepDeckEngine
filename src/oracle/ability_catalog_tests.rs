use super::ability_catalog::{
    AbilityCategory, AbilitySemanticKind, AbilitySignalPolicy, AbilitySupportLevel,
    CompiledBindingName, CompiledBindingSource, CompiledBindingType, CompiledLoweringAdapterId,
    CompiledPrimitiveId, CompiledRecognizerBindings, CompiledRecognizerId, CompiledUnknownPolicy,
    catalog_version, compiled_integration_accepts_effects, entries, lookup_exact_lexeme,
    lookup_registry, match_longest_exact, registry_entries, rules_effective_date,
};
use super::syntax::recognize_entities;
use serde_json::json;

fn source_catalog() -> &'static serde_json::Value {
    static SOURCE: std::sync::LazyLock<serde_json::Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!(
            "../../data/ability-catalog/ability-catalog-v1.json"
        ))
        .expect("validated source catalog")
    });
    &SOURCE
}

fn source_runtime_identities() -> std::collections::BTreeSet<String> {
    source_catalog()["entries"]
        .as_array()
        .expect("source entries")
        .iter()
        .filter(|entry| {
            entry["category"] != "marker"
                && ["lexical", "parser", "runtime"]
                    .iter()
                    .any(|layer| entry["support"][*layer] != "unsupported")
        })
        .map(|entry| {
            entry["canonicalId"]
                .as_str()
                .expect("source identity")
                .to_string()
        })
        .collect()
}

#[test]
fn split_indexes_share_one_version_and_identity_set() {
    assert_eq!(
        entries()
            .iter()
            .map(|entry| entry.canonical_id.clone())
            .collect::<std::collections::BTreeSet<_>>(),
        source_runtime_identities()
    );
    assert_eq!(registry_entries().len(), entries().len());
    for entry in entries() {
        let registered =
            lookup_registry(&entry.canonical_id).expect("lexicon identity in registry");
        assert_eq!(registered.category, entry.category);
    }
}

#[test]
fn lookup_exposes_identity_semantics_signals_and_support_without_promoting_it() {
    assert_eq!(
        catalog_version(),
        source_catalog()["catalogVersion"]
            .as_str()
            .expect("source version")
    );
    assert_eq!(rules_effective_date(), "2026-11-06");
    assert_eq!(entries().len(), source_runtime_identities().len());

    let surveil = lookup_exact_lexeme("SURVEIL").expect("case-insensitive exact lexeme");
    assert_eq!(surveil.canonical_id, "surveil");
    assert_eq!(surveil.category, AbilityCategory::KeywordAction);
    let surveil = lookup_registry("surveil").expect("surveil registry metadata");
    assert_eq!(
        surveil.semantics.kind,
        AbilitySemanticKind::PrimitiveExpansion
    );
    assert_eq!(
        surveil.semantics.signal.policy,
        AbilitySignalPolicy::AfterActionCompletes
    );
    assert_eq!(
        surveil.semantics.signal.action_id.as_deref(),
        Some("surveil")
    );
    assert_eq!(
        surveil
            .semantics
            .primitive_steps
            .iter()
            .map(|step| step.primitive.as_str())
            .collect::<Vec<_>>(),
        [
            "lookAtTopCards",
            "partitionCards",
            "moveCards",
            "reorderCards"
        ]
    );
    assert_eq!(surveil.support.parser, AbilitySupportLevel::Partial);
    assert_eq!(
        surveil.support.runtime,
        AbilitySupportLevel::SupportedLegacy
    );

    let revolt = lookup_registry("revolt").expect("ability word registry metadata");
    assert_eq!(revolt.category, AbilityCategory::AbilityWord);
    assert_eq!(revolt.semantics.kind, AbilitySemanticKind::MarkerOnly);
    assert!(revolt.semantics.primitive_steps.is_empty());
    assert_eq!(revolt.semantics.signal.policy, AbilitySignalPolicy::Never);
    assert!(revolt.semantics.signal.action_id.is_none());
}

#[test]
fn transform_keeps_intrinsic_event_signal_and_counter_identity_separate() {
    let transform = lookup_registry("transform").expect("Transform registry metadata");
    assert_eq!(
        transform.semantics.kind,
        AbilitySemanticKind::PrimitiveExpansion
    );
    assert_eq!(
        transform.semantics.signal.policy,
        AbilitySignalPolicy::Never
    );
    assert!(transform.semantics.signal.action_id.is_none());
    assert_eq!(
        transform
            .semantics
            .primitive_steps
            .iter()
            .map(|step| step.primitive.as_str())
            .collect::<Vec<_>>(),
        ["transformPermanent"]
    );
    assert!(
        transform
            .semantics
            .intrinsic_hooks
            .iter()
            .any(|hook| hook == "events.permanentTransformed")
    );
    assert!(
        transform
            .semantics
            .intrinsic_hooks
            .iter()
            .any(|hook| hook == "counters.preservedBySameObject")
    );
    assert!(
        transform
            .semantics
            .intrinsic_hooks
            .iter()
            .any(|hook| hook == "counters.noIntrinsicCounterName")
    );
    assert_eq!(transform.support.parser, AbilitySupportLevel::Partial);
    assert_eq!(transform.support.runtime, AbilitySupportLevel::Partial);
}

#[test]
fn mobilize_compiled_integration_is_closed_typed_and_checks_the_emitted_ir() {
    let mobilize = lookup_registry("mobilize").expect("Mobilize registry metadata");
    let compiled = mobilize
        .compiled_integration
        .as_ref()
        .expect("certified Mobilize compiled integration");
    assert_eq!(
        compiled.recognizer_id,
        CompiledRecognizerId::KeywordMobilizeV1
    );
    assert_eq!(
        compiled.lowering_adapter_id,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1
    );
    assert_eq!(compiled.bindings[0].name, CompiledBindingName::Quantity);
    assert_eq!(
        compiled.bindings[0].r#type,
        CompiledBindingType::PositiveInteger
    );
    assert_eq!(
        compiled.bindings[0].source,
        CompiledBindingSource::RecognizerOutput
    );
    assert_eq!(
        compiled.bindings[1].name,
        CompiledBindingName::MobilizeTokens
    );
    assert_eq!(
        compiled.unknown_policy,
        CompiledUnknownPolicy::StructuredFailure
    );

    let exact = json!([
        { "kind": "createTokens", "quantity": { "kind": "integer", "value": 2 }, "bind": "mobilizeTokens" },
        {
            "kind": "installDelayedStepTrigger",
            "captureBindings": ["mobilizeTokens"],
            "effects": [{ "kind": "sacrificePermanents", "objects": { "kind": "boundObjects", "binding": "mobilizeTokens" } }]
        }
    ]);
    assert!(compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 2 },
        exact.as_array().unwrap(),
    ));
    assert_eq!(
        compiled.emitted_primitives,
        [
            CompiledPrimitiveId::CreateTokens,
            CompiledPrimitiveId::InstallDelayedStepTrigger,
            CompiledPrimitiveId::SacrificePermanents,
        ]
    );

    let missing_cleanup = json!([
        { "kind": "createTokens", "quantity": { "kind": "integer", "value": 2 }, "bind": "mobilizeTokens" },
        { "kind": "installDelayedStepTrigger", "captureBindings": ["mobilizeTokens"], "effects": [] }
    ]);
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 2 },
        missing_cleanup.as_array().unwrap(),
    ));
    let unknown_primitive = json!([{ "kind": "drawCards" }]);
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 2 },
        unknown_primitive.as_array().unwrap(),
    ));
    let unknown_binding = json!([
        { "kind": "createTokens", "quantity": { "kind": "integer", "value": 2 }, "bind": "unknownTokens" },
        {
            "kind": "installDelayedStepTrigger",
            "captureBindings": ["unknownTokens"],
            "effects": [{ "kind": "sacrificePermanents", "objects": { "kind": "boundObjects", "binding": "unknownTokens" } }]
        }
    ]);
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 2 },
        unknown_binding.as_array().unwrap(),
    ));

    let zero_quantity = json!([
        { "kind": "createTokens", "quantity": { "kind": "integer", "value": 0 }, "bind": "mobilizeTokens" },
        {
            "kind": "installDelayedStepTrigger",
            "captureBindings": ["mobilizeTokens"],
            "effects": [{ "kind": "sacrificePermanents", "objects": { "kind": "boundObjects", "binding": "mobilizeTokens" } }]
        }
    ]);
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 0 },
        zero_quantity.as_array().unwrap(),
    ));
    let invalid_quantity_type = json!([
        { "kind": "createTokens", "quantity": { "kind": "string", "value": "2" }, "bind": "mobilizeTokens" },
        {
            "kind": "installDelayedStepTrigger",
            "captureBindings": ["mobilizeTokens"],
            "effects": [{ "kind": "sacrificePermanents", "objects": { "kind": "boundObjects", "binding": "mobilizeTokens" } }]
        }
    ]);
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 2 },
        invalid_quantity_type.as_array().unwrap(),
    ));
    assert!(!compiled_integration_accepts_effects(
        CompiledRecognizerId::KeywordMobilizeV1,
        CompiledLoweringAdapterId::LowerKeywordMobilizeV1,
        CompiledRecognizerBindings::MobilizeV1 { quantity: 3 },
        exact.as_array().unwrap(),
    ));
}

#[test]
fn longest_exact_span_resolves_multi_token_aliases_without_substring_matching() {
    let tokens = recognize_entities("Umbra armor protects this creature.");
    let matched = match_longest_exact(&tokens, 0).expect("multi-token keyword");
    assert_eq!(matched.entry.canonical_id, "umbra-armor");
    assert_eq!(matched.lexeme, "Umbra Armor");
    assert_eq!((matched.start_token, matched.end_token), (0, 2));

    let alias_tokens = recognize_entities("Totem armor protects this creature.");
    let alias = match_longest_exact(&alias_tokens, 0).expect("multi-token alias");
    assert_eq!(alias.entry.canonical_id, "umbra-armor");
    assert_eq!(alias.lexeme, "totem armor");
    assert_eq!((alias.start_token, alias.end_token), (0, 2));

    let near_miss = recognize_entities("Umbra armored creature");
    assert!(match_longest_exact(&near_miss, 0).is_none());
    assert!(lookup_exact_lexeme("sur").is_none());
}

#[test]
fn tokenizer_uses_catalog_categories_only_in_ability_positions() {
    let keyword_list = recognize_entities("Flying, vigilance");
    assert_eq!(keyword_list[0].token_type, "keywordWord");
    assert_eq!(keyword_list[2].token_type, "keywordWord");

    let granted_keyword = recognize_entities("This creature has deathtouch.");
    assert_eq!(granted_keyword[3].token_type, "keywordWord");

    let action_alias = recognize_entities("Target player surveils 2.");
    assert_eq!(action_alias[2].token_type, "actionWord");
    let participial_adjective = recognize_entities("This blighted creature attacks.");
    assert_eq!(participial_adjective[1].token_type, "unknown");

    let ability_word = recognize_entities("Revolt \u{2014} Draw a card.");
    assert_eq!(ability_word[0].token_type, "abilityWord");
    let ordinary_noun = recognize_entities("This revolt matters.");
    assert_eq!(ordinary_noun[1].token_type, "unknown");

    let contextual_keyword_noun = recognize_entities("You create a Gift token.");
    assert_eq!(contextual_keyword_noun[3].token_type, "unknown");
}

#[test]
fn every_official_ability_word_is_a_contextual_marker_without_semantics() {
    let ability_words = entries()
        .iter()
        .filter(|entry| entry.category == AbilityCategory::AbilityWord)
        .collect::<Vec<_>>();
    assert_eq!(ability_words.len(), 66);

    for entry in ability_words {
        let text = format!("{} — Draw a card.", entry.lexemes[0]);
        let tokens = recognize_entities(&text);
        let matched = match_longest_exact(&tokens, 0).expect("official ability-word lexeme");
        assert_eq!(matched.entry.canonical_id, entry.canonical_id);
        assert!(
            tokens[..matched.end_token]
                .iter()
                .all(|token| token.token_type == "abilityWord"),
            "{} was not marked across its complete contextual span",
            entry.canonical_id
        );

        let registered = lookup_registry(&entry.canonical_id).expect("ability-word registry entry");
        assert_eq!(registered.semantics.kind, AbilitySemanticKind::MarkerOnly);
        assert!(registered.semantics.primitive_steps.is_empty());
        assert!(registered.semantics.intrinsic_hooks.is_empty());
        assert_eq!(
            registered.semantics.signal.policy,
            AbilitySignalPolicy::Never
        );
    }

    let without_dash = recognize_entities("will of the council decides this vote");
    assert!(
        without_dash
            .iter()
            .all(|token| token.token_type != "abilityWord")
    );
}

#[test]
fn specialized_token_classes_win_over_catalog_annotations() {
    let tokens = recognize_entities("Basic landcycling {2}");
    assert_eq!(tokens[0].token_type, "supertypeWord");
    let matched = match_longest_exact(&tokens, 0).expect("catalog still exposes the full span");
    assert_eq!(matched.entry.canonical_id, "basic-landcycling");
    assert_eq!(matched.end_token, 2);
}
