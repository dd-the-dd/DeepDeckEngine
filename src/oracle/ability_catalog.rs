use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;
use serde_json::Value;

use super::model::RecognizedEntity;

const EMBEDDED_LEXICON: &str =
    include_str!("../../crates/deepdeck-oracle/generated/ability-lexicon-index-v2.json");
const EMBEDDED_REGISTRY: &str =
    include_str!("../../crates/deepdeck-oracle/generated/ability-registry-index-v2.json");

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum AbilityCategory {
    #[serde(rename = "keywordAbility")]
    KeywordAbility,
    #[serde(rename = "keywordAction")]
    KeywordAction,
    #[serde(rename = "abilityWord")]
    AbilityWord,
    #[serde(rename = "marker")]
    Marker,
    #[serde(rename = "staticOrCostMechanic")]
    StaticOrCostMechanic,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum AbilitySupportLevel {
    #[serde(rename = "supportedNative")]
    SupportedNative,
    #[serde(rename = "supportedLegacy")]
    SupportedLegacy,
    #[serde(rename = "partial")]
    Partial,
    #[serde(rename = "unsupported")]
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilitySupport {
    pub lexical: AbilitySupportLevel,
    pub parser: AbilitySupportLevel,
    pub runtime: AbilitySupportLevel,
    pub notes: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityCatalogEntry {
    pub canonical_id: String,
    pub category: AbilityCategory,
    pub lexemes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityRuleReference {
    pub document: String,
    pub version: String,
    pub section: String,
    pub source_url: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityRuleClause {
    pub clause_id: String,
    pub text: String,
    pub source_section: String,
    pub usage: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityRuleText {
    pub version: String,
    pub clauses: Vec<AbilityRuleClause>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AbilityGrammarRole {
    Primary,
    Reminder,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityGrammarRegistration {
    pub template_id: String,
    pub role: AbilityGrammarRole,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AbilitySemanticKind {
    PrimitiveExpansion,
    IntrinsicRule,
    MarkerOnly,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityPrimitiveStep {
    pub order: usize,
    pub primitive: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub guard: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum AbilitySignalPolicy {
    Never,
    AfterActionCompletes,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilitySignalRegistration {
    pub policy: AbilitySignalPolicy,
    pub action_id: Option<String>,
    pub cardinality: String,
    pub zero_quantity: Option<String>,
    #[serde(default)]
    pub result_bindings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilitySemanticsRegistration {
    pub kind: AbilitySemanticKind,
    pub primitive_steps: Vec<AbilityPrimitiveStep>,
    pub intrinsic_hooks: Vec<String>,
    pub signal: AbilitySignalRegistration,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledRecognizerId {
    #[serde(rename = "keyword.mobilize.v1")]
    KeywordMobilizeV1,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledLoweringAdapterId {
    #[serde(rename = "lower.keyword.mobilize.v1")]
    LowerKeywordMobilizeV1,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledBindingName {
    #[serde(rename = "quantity")]
    Quantity,
    #[serde(rename = "mobilizeTokens")]
    MobilizeTokens,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledBindingType {
    #[serde(rename = "positiveInteger")]
    PositiveInteger,
    #[serde(rename = "objectSet")]
    ObjectSet,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledBindingSource {
    #[serde(rename = "recognizerOutput")]
    RecognizerOutput,
    #[serde(rename = "loweringOutput")]
    LoweringOutput,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledPrimitiveId {
    #[serde(rename = "createTokens")]
    CreateTokens,
    #[serde(rename = "installDelayedStepTrigger")]
    InstallDelayedStepTrigger,
    #[serde(rename = "sacrificePermanents")]
    SacrificePermanents,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub(crate) enum CompiledUnknownPolicy {
    #[serde(rename = "structuredFailure")]
    StructuredFailure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompiledRecognizerBindings {
    MobilizeV1 { quantity: i64 },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompiledBindingContract {
    pub name: CompiledBindingName,
    pub r#type: CompiledBindingType,
    pub source: CompiledBindingSource,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompiledAbilityIntegration {
    pub recognizer_id: CompiledRecognizerId,
    pub lowering_adapter_id: CompiledLoweringAdapterId,
    pub bindings: Vec<CompiledBindingContract>,
    pub emitted_primitives: Vec<CompiledPrimitiveId>,
    pub unknown_policy: CompiledUnknownPolicy,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AbilityRegistryEntry {
    pub canonical_id: String,
    pub category: AbilityCategory,
    pub rules_references: Vec<AbilityRuleReference>,
    pub rules_text: AbilityRuleText,
    pub grammar: Vec<AbilityGrammarRegistration>,
    pub semantics: AbilitySemanticsRegistration,
    pub support: AbilitySupport,
    #[serde(default)]
    pub compiled_integration: Option<CompiledAbilityIntegration>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbilityLexiconDocument {
    lexicon_schema_version: String,
    catalog_version: String,
    rules_effective_date: String,
    entries: Vec<AbilityCatalogEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AbilityRegistryDocument {
    registry_schema_version: String,
    catalog_version: String,
    rules_effective_date: String,
    entries: Vec<AbilityRegistryEntry>,
}

#[derive(Clone, Copy, Debug)]
struct LexemeLocation {
    entry_index: usize,
    lexeme_index: usize,
}

#[derive(Clone, Debug)]
struct TokenLexemeLocation {
    location: LexemeLocation,
    normalized_tokens: Vec<String>,
}

#[derive(Debug)]
struct AbilityIndexes {
    lexicon: AbilityLexiconDocument,
    registry: AbilityRegistryDocument,
    exact_lexemes: HashMap<String, LexemeLocation>,
    lexemes_by_first_token: HashMap<String, Vec<TokenLexemeLocation>>,
    registry_by_id: HashMap<String, usize>,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AbilityLexemeMatch {
    pub entry: &'static AbilityCatalogEntry,
    pub lexeme: &'static str,
    pub start_token: usize,
    pub end_token: usize,
}

fn normalize_lexeme(value: &str) -> String {
    value.trim().to_lowercase()
}

fn build_indexes() -> AbilityIndexes {
    let lexicon: AbilityLexiconDocument = serde_json::from_str(EMBEDDED_LEXICON)
        .expect("embedded ability lexicon index must be valid JSON");
    let registry: AbilityRegistryDocument = serde_json::from_str(EMBEDDED_REGISTRY)
        .expect("embedded ability registry index must be valid JSON");
    assert_eq!(
        lexicon.lexicon_schema_version,
        "deepdeck-ability-lexicon/v2"
    );
    assert_eq!(
        registry.registry_schema_version,
        "deepdeck-ability-registry/v2"
    );
    assert_eq!(lexicon.catalog_version, registry.catalog_version);
    assert_eq!(lexicon.rules_effective_date, registry.rules_effective_date);
    for entry in &registry.entries {
        validate_compiled_integration(entry);
    }

    let mut exact_lexemes = HashMap::new();
    let mut lexemes_by_first_token: HashMap<String, Vec<TokenLexemeLocation>> = HashMap::new();
    for (entry_index, entry) in lexicon.entries.iter().enumerate() {
        for (lexeme_index, lexeme) in entry.lexemes.iter().enumerate() {
            let normalized = normalize_lexeme(lexeme);
            let location = LexemeLocation {
                entry_index,
                lexeme_index,
            };
            assert!(
                exact_lexemes.insert(normalized.clone(), location).is_none(),
                "ambiguous ability lexeme: {lexeme}"
            );
            let normalized_tokens = normalized
                .split_whitespace()
                .map(str::to_owned)
                .collect::<Vec<_>>();
            let first = normalized_tokens
                .first()
                .expect("ability lexemes cannot be empty")
                .clone();
            lexemes_by_first_token
                .entry(first)
                .or_default()
                .push(TokenLexemeLocation {
                    location,
                    normalized_tokens,
                });
        }
    }
    for candidates in lexemes_by_first_token.values_mut() {
        candidates.sort_by(|left, right| {
            right
                .normalized_tokens
                .len()
                .cmp(&left.normalized_tokens.len())
        });
    }

    let registry_by_id = registry
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.canonical_id.clone(), index))
        .collect::<HashMap<_, _>>();
    assert_eq!(
        registry_by_id.len(),
        registry.entries.len(),
        "duplicate ability registry identity"
    );
    for entry in &lexicon.entries {
        let registered = registry_by_id
            .get(&entry.canonical_id)
            .and_then(|index| registry.entries.get(*index))
            .expect("every lexicon entry must have registry metadata");
        assert_eq!(
            entry.category, registered.category,
            "ability category differs between split indexes"
        );
    }

    AbilityIndexes {
        lexicon,
        registry,
        exact_lexemes,
        lexemes_by_first_token,
        registry_by_id,
    }
}

fn validate_compiled_integration(entry: &AbilityRegistryEntry) {
    let Some(integration) = &entry.compiled_integration else {
        return;
    };
    match integration.recognizer_id {
        CompiledRecognizerId::KeywordMobilizeV1 => {
            assert_eq!(entry.canonical_id, "mobilize");
            assert_eq!(
                integration.lowering_adapter_id,
                CompiledLoweringAdapterId::LowerKeywordMobilizeV1
            );
            assert_eq!(
                integration.bindings,
                [
                    CompiledBindingContract {
                        name: CompiledBindingName::Quantity,
                        r#type: CompiledBindingType::PositiveInteger,
                        source: CompiledBindingSource::RecognizerOutput,
                    },
                    CompiledBindingContract {
                        name: CompiledBindingName::MobilizeTokens,
                        r#type: CompiledBindingType::ObjectSet,
                        source: CompiledBindingSource::LoweringOutput,
                    },
                ]
            );
            assert_eq!(
                integration.emitted_primitives,
                [
                    CompiledPrimitiveId::CreateTokens,
                    CompiledPrimitiveId::InstallDelayedStepTrigger,
                    CompiledPrimitiveId::SacrificePermanents,
                ]
            );
            assert_eq!(
                integration.unknown_policy,
                CompiledUnknownPolicy::StructuredFailure
            );
        }
    }
}

fn collect_compiled_primitives(
    effects: &[Value],
    primitives: &mut Vec<CompiledPrimitiveId>,
) -> bool {
    for effect in effects {
        let Some(kind) = effect.get("kind").and_then(Value::as_str) else {
            return false;
        };
        let primitive = match kind {
            "createTokens" => CompiledPrimitiveId::CreateTokens,
            "installDelayedStepTrigger" => CompiledPrimitiveId::InstallDelayedStepTrigger,
            "sacrificePermanents" => CompiledPrimitiveId::SacrificePermanents,
            _ => return false,
        };
        primitives.push(primitive);
        if primitive == CompiledPrimitiveId::InstallDelayedStepTrigger {
            let Some(nested) = effect.get("effects").and_then(Value::as_array) else {
                return false;
            };
            if !collect_compiled_primitives(nested, primitives) {
                return false;
            }
        }
    }
    true
}

fn compiled_binding_name(value: &Value) -> Option<CompiledBindingName> {
    match value.as_str()? {
        "quantity" => Some(CompiledBindingName::Quantity),
        "mobilizeTokens" => Some(CompiledBindingName::MobilizeTokens),
        _ => None,
    }
}

fn mobilize_bindings_are_valid(effects: &[Value], expected_quantity: i64) -> bool {
    let Some(create_tokens) = effects.first() else {
        return false;
    };
    let Some(delayed_trigger) = effects.get(1) else {
        return false;
    };
    let Some(cleanup) = delayed_trigger
        .get("effects")
        .and_then(Value::as_array)
        .and_then(|effects| effects.first())
    else {
        return false;
    };
    expected_quantity > 0
        && create_tokens.get("quantity").is_some_and(|quantity| {
            quantity.get("kind").and_then(Value::as_str) == Some("integer")
                && quantity.get("value").and_then(Value::as_i64) == Some(expected_quantity)
        })
        && create_tokens.get("bind").and_then(compiled_binding_name)
            == Some(CompiledBindingName::MobilizeTokens)
        && delayed_trigger
            .get("captureBindings")
            .and_then(Value::as_array)
            .is_some_and(|bindings| {
                bindings.len() == 1
                    && bindings.first().and_then(compiled_binding_name)
                        == Some(CompiledBindingName::MobilizeTokens)
            })
        && cleanup
            .pointer("/objects/binding")
            .and_then(compiled_binding_name)
            == Some(CompiledBindingName::MobilizeTokens)
}

fn indexes() -> &'static AbilityIndexes {
    static INDEXES: OnceLock<AbilityIndexes> = OnceLock::new();
    INDEXES.get_or_init(build_indexes)
}

pub(crate) fn catalog_version() -> &'static str {
    &indexes().lexicon.catalog_version
}
pub(crate) fn rules_effective_date() -> &'static str {
    &indexes().lexicon.rules_effective_date
}
pub(crate) fn entries() -> &'static [AbilityCatalogEntry] {
    &indexes().lexicon.entries
}
pub(crate) fn registry_entries() -> &'static [AbilityRegistryEntry] {
    &indexes().registry.entries
}

pub(crate) fn lookup_registry(canonical_id: &str) -> Option<&'static AbilityRegistryEntry> {
    let indexes = indexes();
    indexes
        .registry_by_id
        .get(canonical_id)
        .and_then(|index| indexes.registry.entries.get(*index))
}

pub(crate) fn compiled_integration_accepts_effects(
    recognizer_id: CompiledRecognizerId,
    lowering_adapter_id: CompiledLoweringAdapterId,
    recognized_bindings: CompiledRecognizerBindings,
    effects: &[Value],
) -> bool {
    let integration = indexes()
        .registry
        .entries
        .iter()
        .filter_map(|entry| entry.compiled_integration.as_ref())
        .find(|integration| integration.recognizer_id == recognizer_id);
    let Some(integration) = integration else {
        return false;
    };
    if integration.lowering_adapter_id != lowering_adapter_id {
        return false;
    }
    let bindings_are_valid = match (recognizer_id, recognized_bindings) {
        (
            CompiledRecognizerId::KeywordMobilizeV1,
            CompiledRecognizerBindings::MobilizeV1 { quantity },
        ) => mobilize_bindings_are_valid(effects, quantity),
    };
    if !bindings_are_valid {
        return false;
    }
    let mut emitted_primitives = Vec::new();
    collect_compiled_primitives(effects, &mut emitted_primitives)
        && emitted_primitives == integration.emitted_primitives
}

pub(crate) fn lookup_exact_lexeme(lexeme: &str) -> Option<&'static AbilityCatalogEntry> {
    let indexes = indexes();
    let location = indexes.exact_lexemes.get(&normalize_lexeme(lexeme))?;
    indexes.lexicon.entries.get(location.entry_index)
}

pub(crate) fn match_longest_exact(
    tokens: &[RecognizedEntity],
    start_token: usize,
) -> Option<AbilityLexemeMatch> {
    let indexes = indexes();
    let first = tokens.get(start_token)?;
    let candidates = indexes
        .lexemes_by_first_token
        .get(&normalize_lexeme(&first.raw))?;
    let candidate = candidates.iter().find(|candidate| {
        let end = start_token + candidate.normalized_tokens.len();
        let Some(span) = tokens.get(start_token..end) else {
            return false;
        };
        span.iter()
            .zip(&candidate.normalized_tokens)
            .all(|(token, expected)| normalize_lexeme(&token.raw) == *expected)
    })?;
    let entry = &indexes.lexicon.entries[candidate.location.entry_index];
    let lexeme = &entry.lexemes[candidate.location.lexeme_index];
    Some(AbilityLexemeMatch {
        entry,
        lexeme,
        start_token,
        end_token: start_token + candidate.normalized_tokens.len(),
    })
}

/// The complete versioned mechanic catalog (CR 701 keyword actions and CR 702
/// keyword abilities), including mechanics that have no lexicon entry or
/// grammar yet. It only answers "does this text name a mechanic with rules?";
/// it never makes a mechanic parseable. A label naming one of these is not a
/// rules-free ability or flavor word (CR 207.2c-d), and an unknown capitalized
/// word naming one is an unsupported mechanic, not an unresolved card name.
const EMBEDDED_MECHANIC_CATALOG: &str =
    include_str!("../../crates/deepdeck-oracle/generated/ability-catalog-index-v1.json");

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MechanicCatalogEntry {
    canonical_id: String,
    category: AbilityCategory,
    lexemes: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MechanicCatalogDocument {
    catalog_schema_version: String,
    entries: Vec<MechanicCatalogEntry>,
}

struct MechanicCatalogIndex {
    entries: Vec<MechanicCatalogEntry>,
    /// Normalized lexeme words, longest first, keyed by their first word.
    by_first_word: HashMap<String, Vec<(Vec<String>, usize)>>,
}

/// A catalog mechanic named by a token span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CatalogMechanicMatch {
    pub canonical_id: &'static str,
    pub category: AbilityCategory,
    pub start_token: usize,
    pub end_token: usize,
}

/// Lowercase words of a lexeme or token, split at every character that is
/// neither alphanumeric nor an apostrophe (`Power-up` and `Power - up` agree).
fn mechanic_words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|ch: char| !(ch.is_alphanumeric() || ch == '\'' || ch == '\u{2019}'))
        .filter(|word| !word.is_empty())
        .map(|word| word.replace('\u{2019}', "'").to_lowercase())
}

fn mechanic_catalog() -> &'static MechanicCatalogIndex {
    static INDEX: OnceLock<MechanicCatalogIndex> = OnceLock::new();
    INDEX.get_or_init(|| {
        let document: MechanicCatalogDocument = serde_json::from_str(EMBEDDED_MECHANIC_CATALOG)
            .expect("embedded ability catalog index must be valid JSON");
        assert_eq!(
            document.catalog_schema_version,
            "deepdeck-ability-catalog/v1"
        );
        let mut by_first_word: HashMap<String, Vec<(Vec<String>, usize)>> = HashMap::new();
        for (index, entry) in document.entries.iter().enumerate() {
            if !matches!(
                entry.category,
                AbilityCategory::KeywordAbility | AbilityCategory::KeywordAction
            ) {
                continue;
            }
            for lexeme in &entry.lexemes {
                let words = mechanic_words(lexeme).collect::<Vec<_>>();
                if let Some(first) = words.first() {
                    by_first_word
                        .entry(first.clone())
                        .or_default()
                        .push((words, index));
                }
            }
        }
        for candidates in by_first_word.values_mut() {
            candidates.sort_by(|left, right| right.0.len().cmp(&left.0.len()));
        }
        MechanicCatalogIndex {
            entries: document.entries,
            by_first_word,
        }
    })
}

/// The longest catalog keyword ability or keyword action whose name starts at
/// `start_token` and ends on a token boundary.
pub(crate) fn match_catalog_mechanic(
    tokens: &[RecognizedEntity],
    start_token: usize,
) -> Option<CatalogMechanicMatch> {
    let catalog = mechanic_catalog();
    let first = mechanic_words(&tokens.get(start_token)?.raw).next()?;
    catalog
        .by_first_word
        .get(&first)?
        .iter()
        .find_map(|(expected, index)| {
            let mut words = Vec::new();
            for (offset, token) in tokens[start_token..].iter().enumerate() {
                words.extend(mechanic_words(&token.raw));
                if words.len() >= expected.len() {
                    return (words == *expected).then_some((start_token + offset + 1, *index));
                }
            }
            None
        })
        .map(|(end_token, index)| {
            let entry = &catalog.entries[index];
            CatalogMechanicMatch {
                canonical_id: entry.canonical_id.as_str(),
                category: entry.category,
                start_token,
                end_token,
            }
        })
}
