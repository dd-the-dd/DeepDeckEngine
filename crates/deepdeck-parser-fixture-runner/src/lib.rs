use deepdeck_oracle::oracle::{
    OracleCardFace, OracleCardParseRequest, RecognizerProvenance, parse_oracle_card,
    parse_oracle_card_native,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

pub const FIXTURE_SCHEMA_VERSION: &str = "deepdeck-parser-fixture/v1";
pub const RESULT_SCHEMA_VERSION: &str = "deepdeck-parser-fixture-result/v1";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ParserMode {
    #[default]
    Full,
    NativeOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FixtureStatus {
    Passed,
    Failed,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FixtureEvent {
    CaseResult {
        #[serde(rename = "schemaVersion")]
        schema_version: &'static str,
        #[serde(rename = "fixturePath")]
        fixture_path: String,
        #[serde(rename = "fixtureId")]
        fixture_id: String,
        #[serde(rename = "caseId")]
        case_id: String,
        #[serde(rename = "cardName", skip_serializing_if = "Option::is_none")]
        card_name: Option<String>,
        #[serde(rename = "oracleId", skip_serializing_if = "Option::is_none")]
        oracle_id: Option<String>,
        #[serde(rename = "parseStatus", skip_serializing_if = "Option::is_none")]
        parse_status: Option<String>,
        #[serde(rename = "elapsedMilliseconds")]
        elapsed_milliseconds: f64,
        status: FixtureStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    FixtureError {
        #[serde(rename = "schemaVersion")]
        schema_version: &'static str,
        #[serde(rename = "fixturePath")]
        fixture_path: String,
        error: String,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureSummary {
    #[serde(rename = "type")]
    event_type: &'static str,
    pub schema_version: &'static str,
    pub parser_mode: ParserMode,
    pub fixture_count: usize,
    pub case_count: usize,
    pub passed: usize,
    pub failed: usize,
    pub invalid_fixtures: usize,
    pub elapsed_milliseconds: f64,
    pub status: FixtureStatus,
}

pub struct FixtureRunReport {
    pub events: Vec<FixtureEvent>,
    pub summary: FixtureSummary,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ParserFixture {
    schema_version: String,
    id: String,
    #[serde(default)]
    card_name: Option<String>,
    #[serde(default)]
    oracle_id: Option<String>,
    cases: Vec<ParserFixtureCase>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ParserFixtureCase {
    id: String,
    request: FixtureParseRequest,
    expect: CaseExpectation,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FixtureParseRequest {
    card_name: String,
    type_line: String,
    mana_cost: Option<String>,
    oracle_text: Option<String>,
    #[serde(default)]
    layout: Option<String>,
    #[serde(default)]
    faces: Vec<FixtureCardFace>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FixtureCardFace {
    id: String,
    name: String,
    type_line: String,
    mana_cost: Option<String>,
    oracle_text: String,
    #[serde(default)]
    power: Option<String>,
    #[serde(default)]
    toughness: Option<String>,
    #[serde(default)]
    loyalty: Option<String>,
}

impl From<FixtureParseRequest> for OracleCardParseRequest {
    fn from(request: FixtureParseRequest) -> Self {
        Self {
            card_name: request.card_name,
            type_line: request.type_line,
            mana_cost: request.mana_cost,
            oracle_text: request.oracle_text,
            layout: request.layout,
            faces: request
                .faces
                .into_iter()
                .map(|face| OracleCardFace {
                    id: face.id,
                    name: face.name,
                    type_line: face.type_line,
                    mana_cost: face.mana_cost,
                    oracle_text: face.oracle_text,
                    power: face.power,
                    toughness: face.toughness,
                    loyalty: face.loyalty,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "outcome", rename_all = "camelCase", deny_unknown_fields)]
enum CaseExpectation {
    Success(SuccessExpectation),
    Failure(FailureExpectation),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct SuccessExpectation {
    card_status: String,
    #[serde(default)]
    ability_count: Option<usize>,
    #[serde(default)]
    abilities: Vec<ExpectedAbility>,
    #[serde(default)]
    allow_diagnostics: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct FailureExpectation {
    card_status: String,
    #[serde(default)]
    ability_count: Option<usize>,
    #[serde(default)]
    abilities: Vec<ExpectedAbility>,
    diagnostics: Vec<ExpectedDiagnostic>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExpectedAbility {
    index: usize,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    recognizer: Option<ExpectedRecognizer>,
    #[serde(default)]
    ability_type: Option<String>,
    #[serde(default)]
    rule: Option<Value>,
    #[serde(default)]
    rule_assertions: Vec<JsonPointerAssertion>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum ExpectedRecognizer {
    Native,
    Legacy,
    Unclaimed,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct JsonPointerAssertion {
    pointer: String,
    equals: Value,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct ExpectedDiagnostic {
    #[serde(default)]
    ability_index: Option<usize>,
    #[serde(default)]
    code: Option<String>,
    #[serde(default)]
    automaton: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    expected: Option<String>,
    #[serde(default)]
    expected_contains: Option<String>,
    #[serde(default)]
    found: Option<String>,
    #[serde(default)]
    message_contains: Option<String>,
    #[serde(default)]
    token_index: Option<usize>,
    #[serde(default)]
    trace_contains_state: Option<String>,
}

pub fn run_fixture_paths(inputs: &[PathBuf]) -> FixtureRunReport {
    run_fixture_paths_with_mode(inputs, ParserMode::Full)
}

pub fn run_fixture_paths_with_mode(
    inputs: &[PathBuf],
    parser_mode: ParserMode,
) -> FixtureRunReport {
    let started = Instant::now();
    let (paths, discovery_errors) = discover_fixture_paths(inputs);
    let mut events = Vec::new();
    let mut fixture_count = 0;
    let mut case_count = 0;
    let mut passed = 0;
    let mut failed = 0;
    let mut invalid_fixtures = 0;
    let mut fixture_ids = HashSet::new();

    for (path, error) in discovery_errors {
        invalid_fixtures += 1;
        events.push(fixture_error(&path, error));
    }
    if paths.is_empty() && invalid_fixtures == 0 {
        invalid_fixtures += 1;
        events.push(fixture_error(
            Path::new(""),
            "no JSON fixture was found in the supplied inputs".to_string(),
        ));
    }

    for path in paths {
        let source = match fs::read_to_string(&path) {
            Ok(source) => source,
            Err(error) => {
                invalid_fixtures += 1;
                events.push(fixture_error(
                    &path,
                    format!("cannot read fixture: {error}"),
                ));
                continue;
            }
        };
        let fixture: ParserFixture = match serde_json::from_str(&source) {
            Ok(fixture) => fixture,
            Err(error) => {
                invalid_fixtures += 1;
                events.push(fixture_error(
                    &path,
                    format!("invalid fixture JSON: {error}"),
                ));
                continue;
            }
        };
        if let Err(error) = validate_fixture(&fixture, &mut fixture_ids) {
            invalid_fixtures += 1;
            events.push(fixture_error(&path, error));
            continue;
        }

        fixture_count += 1;
        for case in fixture.cases {
            case_count += 1;
            let case_started = Instant::now();
            let request = case.request.into();
            let parsed = match parser_mode {
                ParserMode::Full => parse_oracle_card(request),
                ParserMode::NativeOnly => parse_oracle_card_native(request),
            };
            let error = evaluate_case(&parsed, &case.expect).err();
            let status = if error.is_none() {
                passed += 1;
                FixtureStatus::Passed
            } else {
                failed += 1;
                FixtureStatus::Failed
            };
            events.push(FixtureEvent::CaseResult {
                schema_version: RESULT_SCHEMA_VERSION,
                fixture_path: path.display().to_string(),
                fixture_id: fixture.id.clone(),
                case_id: case.id,
                card_name: fixture.card_name.clone(),
                oracle_id: fixture.oracle_id.clone(),
                parse_status: Some(parsed.status),
                elapsed_milliseconds: rounded_milliseconds(case_started.elapsed().as_secs_f64()),
                status,
                error,
            });
        }
    }

    let status = if failed == 0 && invalid_fixtures == 0 {
        FixtureStatus::Passed
    } else {
        FixtureStatus::Failed
    };
    FixtureRunReport {
        events,
        summary: FixtureSummary {
            event_type: "summary",
            schema_version: RESULT_SCHEMA_VERSION,
            parser_mode,
            fixture_count,
            case_count,
            passed,
            failed,
            invalid_fixtures,
            elapsed_milliseconds: rounded_milliseconds(started.elapsed().as_secs_f64()),
            status,
        },
    }
}

fn evaluate_case(
    parsed: &deepdeck_oracle::oracle::OracleCardParseResult,
    expected: &CaseExpectation,
) -> Result<(), String> {
    match expected {
        CaseExpectation::Success(expected) => {
            compare_card_shape(
                parsed,
                &expected.card_status,
                expected.ability_count,
                &expected.abilities,
            )?;
            if !expected.allow_diagnostics && !parsed.diagnostics.is_empty() {
                return Err(format!(
                    "expected no parser diagnostics, found {}",
                    parsed.diagnostics.len()
                ));
            }
            Ok(())
        }
        CaseExpectation::Failure(expected) => {
            compare_card_shape(
                parsed,
                &expected.card_status,
                expected.ability_count,
                &expected.abilities,
            )?;
            let parsed_json = serde_json::to_value(parsed)
                .map_err(|error| format!("cannot inspect parser result: {error}"))?;
            for diagnostic in &expected.diagnostics {
                let diagnostics = diagnostics_for_expectation(&parsed_json, diagnostic)?;
                if !diagnostics
                    .iter()
                    .any(|actual| diagnostic_matches(diagnostic, actual))
                {
                    return Err(format!("no diagnostic matched expectation {diagnostic:?}"));
                }
            }
            Ok(())
        }
    }
}

fn compare_card_shape(
    parsed: &deepdeck_oracle::oracle::OracleCardParseResult,
    card_status: &str,
    ability_count: Option<usize>,
    abilities: &[ExpectedAbility],
) -> Result<(), String> {
    if parsed.status != card_status {
        return Err(format!(
            "card status mismatch: expected {card_status:?}, found {:?}",
            parsed.status
        ));
    }
    if ability_count.is_some_and(|expected| parsed.abilities.len() != expected) {
        return Err(format!(
            "ability count mismatch: expected {}, found {}",
            ability_count.unwrap_or_default(),
            parsed.abilities.len()
        ));
    }
    for expected in abilities {
        let actual = parsed.abilities.get(expected.index).ok_or_else(|| {
            format!(
                "ability index {} is absent; parsed {} abilities",
                expected.index,
                parsed.abilities.len()
            )
        })?;
        if expected
            .status
            .as_ref()
            .is_some_and(|value| &actual.status != value)
        {
            return Err(format!(
                "ability {} status mismatch: expected {:?}, found {:?}",
                expected.index, expected.status, actual.status
            ));
        }
        if expected
            .ability_type
            .as_ref()
            .is_some_and(|value| &actual.ability_type != value)
        {
            return Err(format!(
                "ability {} type mismatch: expected {:?}, found {:?}",
                expected.index, expected.ability_type, actual.ability_type
            ));
        }
        if let Some(recognizer) = expected.recognizer {
            let matches = matches!(
                (recognizer, actual.recognizer),
                (ExpectedRecognizer::Native, RecognizerProvenance::Native)
                    | (ExpectedRecognizer::Legacy, RecognizerProvenance::Legacy)
                    | (
                        ExpectedRecognizer::Unclaimed,
                        RecognizerProvenance::Unclaimed
                    )
            );
            if !matches {
                return Err(format!(
                    "ability {} recognizer mismatch: expected {:?}, found {:?}",
                    expected.index, recognizer, actual.recognizer
                ));
            }
        }
        if let Some(expected_rule) = &expected.rule {
            if actual.rule.as_ref() != Some(expected_rule) {
                return Err(format!(
                    "ability {} rule mismatch: expected {}, found {}",
                    expected.index,
                    expected_rule,
                    actual
                        .rule
                        .as_ref()
                        .map(Value::to_string)
                        .unwrap_or_else(|| "null".to_string())
                ));
            }
        }
        for assertion in &expected.rule_assertions {
            let rule = actual.rule.as_ref().ok_or_else(|| {
                format!(
                    "ability {} has no rule for JSON pointer {:?}",
                    expected.index, assertion.pointer
                )
            })?;
            let found = rule.pointer(&assertion.pointer).ok_or_else(|| {
                format!(
                    "ability {} rule has no JSON pointer {:?}",
                    expected.index, assertion.pointer
                )
            })?;
            if found != &assertion.equals {
                return Err(format!(
                    "ability {} rule pointer {:?} mismatch: expected {}, found {}",
                    expected.index, assertion.pointer, assertion.equals, found
                ));
            }
        }
    }
    Ok(())
}

fn diagnostics_for_expectation<'a>(
    parsed: &'a Value,
    expected: &ExpectedDiagnostic,
) -> Result<Vec<&'a Value>, String> {
    let container = if let Some(index) = expected.ability_index {
        parsed
            .pointer(&format!("/abilities/{index}/diagnostics"))
            .ok_or_else(|| format!("ability {index} diagnostics are absent"))?
    } else {
        parsed
            .get("diagnostics")
            .ok_or_else(|| "card diagnostics are absent".to_string())?
    };
    container
        .as_array()
        .map(|values| values.iter().collect())
        .ok_or_else(|| "diagnostics must serialize as an array".to_string())
}

fn diagnostic_matches(expected: &ExpectedDiagnostic, actual: &Value) -> bool {
    string_field_matches(actual, "code", expected.code.as_deref())
        && string_field_matches(actual, "automaton", expected.automaton.as_deref())
        && string_field_matches(actual, "state", expected.state.as_deref())
        && string_field_matches(actual, "expected", expected.expected.as_deref())
        && string_field_contains(actual, "expected", expected.expected_contains.as_deref())
        && string_field_matches(actual, "found", expected.found.as_deref())
        && string_field_contains(actual, "message", expected.message_contains.as_deref())
        && expected.token_index.is_none_or(|value| {
            actual.get("tokenIndex").and_then(Value::as_u64) == Some(value as u64)
        })
        && expected.trace_contains_state.as_ref().is_none_or(|state| {
            actual
                .get("trace")
                .and_then(Value::as_array)
                .is_some_and(|trace| {
                    trace
                        .iter()
                        .any(|step| step.get("state").and_then(Value::as_str) == Some(state))
                })
        })
}

fn string_field_matches(actual: &Value, field: &str, expected: Option<&str>) -> bool {
    expected.is_none_or(|value| actual.get(field).and_then(Value::as_str) == Some(value))
}

fn string_field_contains(actual: &Value, field: &str, expected: Option<&str>) -> bool {
    expected.is_none_or(|value| {
        actual
            .get(field)
            .and_then(Value::as_str)
            .is_some_and(|actual| actual.contains(value))
    })
}

fn validate_fixture(
    fixture: &ParserFixture,
    fixture_ids: &mut HashSet<String>,
) -> Result<(), String> {
    if fixture.schema_version != FIXTURE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported schemaVersion {:?}; expected {FIXTURE_SCHEMA_VERSION:?}",
            fixture.schema_version
        ));
    }
    if fixture.id.trim().is_empty() {
        return Err("fixture id must not be empty".to_string());
    }
    if !fixture_ids.insert(fixture.id.clone()) {
        return Err(format!("duplicate fixture id {:?}", fixture.id));
    }
    if fixture.cases.is_empty() {
        return Err("fixture must contain at least one case".to_string());
    }
    let mut case_ids = HashSet::new();
    for case in &fixture.cases {
        if case.id.trim().is_empty() {
            return Err("case id must not be empty".to_string());
        }
        if !case_ids.insert(case.id.clone()) {
            return Err(format!("duplicate case id {:?}", case.id));
        }
        validate_expectation(&case.expect)?;
    }
    Ok(())
}

fn validate_expectation(expectation: &CaseExpectation) -> Result<(), String> {
    let (abilities, diagnostics) = match expectation {
        CaseExpectation::Success(expected) => (&expected.abilities, None),
        CaseExpectation::Failure(expected) => {
            if expected.diagnostics.is_empty() {
                return Err("failure expectation must contain a diagnostic matcher".to_string());
            }
            (&expected.abilities, Some(&expected.diagnostics))
        }
    };
    let mut ability_indices = HashSet::new();
    for ability in abilities {
        if !ability_indices.insert(ability.index) {
            return Err(format!(
                "duplicate expected ability index {}",
                ability.index
            ));
        }
        if ability.status.is_none()
            && ability.recognizer.is_none()
            && ability.ability_type.is_none()
            && ability.rule.is_none()
            && ability.rule_assertions.is_empty()
        {
            return Err(format!(
                "ability {} expectation must assert at least one field",
                ability.index
            ));
        }
        for assertion in &ability.rule_assertions {
            if !assertion.pointer.starts_with('/') {
                return Err(format!(
                    "rule assertion pointer {:?} must start with `/`",
                    assertion.pointer
                ));
            }
        }
    }
    if let Some(diagnostics) = diagnostics {
        for diagnostic in diagnostics {
            if diagnostic.code.is_none()
                && diagnostic.automaton.is_none()
                && diagnostic.state.is_none()
                && diagnostic.expected.is_none()
                && diagnostic.expected_contains.is_none()
                && diagnostic.found.is_none()
                && diagnostic.message_contains.is_none()
                && diagnostic.token_index.is_none()
                && diagnostic.trace_contains_state.is_none()
            {
                return Err(
                    "diagnostic expectation must assert at least one diagnostic field".to_string(),
                );
            }
        }
    }
    Ok(())
}

fn discover_fixture_paths(inputs: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut fixtures = Vec::new();
    let mut errors = Vec::new();
    for input in inputs {
        collect_fixture_paths(input, &mut fixtures, &mut errors);
    }
    fixtures.sort();
    fixtures.dedup();
    (fixtures, errors)
}

fn collect_fixture_paths(
    path: &Path,
    fixtures: &mut Vec<PathBuf>,
    errors: &mut Vec<(PathBuf, String)>,
) {
    if path.is_file() {
        if path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        {
            fixtures.push(path.to_path_buf());
        } else {
            errors.push((
                path.to_path_buf(),
                "fixture file must use .json".to_string(),
            ));
        }
        return;
    }
    if !path.is_dir() {
        errors.push((
            path.to_path_buf(),
            "fixture path does not exist".to_string(),
        ));
        return;
    }
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) => {
            errors.push((
                path.to_path_buf(),
                format!("cannot read directory: {error}"),
            ));
            return;
        }
    };
    for entry in entries {
        match entry {
            Ok(entry) => {
                let entry_path = entry.path();
                if entry_path.is_dir()
                    || entry_path
                        .extension()
                        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
                {
                    collect_fixture_paths(&entry_path, fixtures, errors);
                }
            }
            Err(error) => errors.push((
                path.to_path_buf(),
                format!("cannot read directory entry: {error}"),
            )),
        }
    }
}

fn fixture_error(path: &Path, error: String) -> FixtureEvent {
    FixtureEvent::FixtureError {
        schema_version: RESULT_SCHEMA_VERSION,
        fixture_path: path.display().to_string(),
        error,
    }
}

fn rounded_milliseconds(seconds: f64) -> f64 {
    (seconds * 100_000.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(source: &str) -> ParserFixture {
        serde_json::from_str(source).expect("test fixture deserializes")
    }

    #[test]
    fn success_case_checks_native_rule_pointers_without_a_card_test_harness() {
        let fixture = fixture(include_str!("../fixtures/example.json"));
        let case = &fixture.cases[0];
        let parsed = parse_oracle_card(case.request.clone().into());
        evaluate_case(&parsed, &case.expect).expect("native success case passes");
    }

    #[test]
    fn failure_case_matches_structured_machine_diagnostic() {
        let fixture = fixture(include_str!("../fixtures/example.json"));
        let case = &fixture.cases[1];
        let parsed = parse_oracle_card(case.request.clone().into());
        evaluate_case(&parsed, &case.expect).expect("structured failure case passes");
    }

    #[test]
    fn unknown_fixture_fields_are_rejected() {
        let source = include_str!("../fixtures/example.json").replacen(
            "\"cardName\": \"Fixture Mana Source\"",
            "\"cardName\": \"Fixture Mana Source\", \"typo\": true",
            1,
        );
        assert!(serde_json::from_str::<ParserFixture>(&source).is_err());
    }
}
