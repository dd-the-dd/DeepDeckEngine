//! Keyword abilities defined by a versioned rules template (CR 702).
//!
//! A template keyword is one data row of `keyword-templates-v1.json`: its
//! lexeme, the kind of its printed parameter, the CR clause that defines it,
//! and that definition written as ordinary Oracle rules text with parameter
//! placeholders. This child only reads the keyword line: the lexeme, the
//! parameter (with the shared numeric and mana-cost children) and an optional
//! reminder, validated against the row's reminder renderings. It then returns
//! the definition instantiated with the printed parameter; compiling that
//! text belongs to the ordinary ability parser (see
//! `abilities/keyword_template_expansion.rs`).
//!
//! ```text
//! Line      := LEXEME Parameter [ "(" Reminder ")" ]
//! Parameter := ε | N | MANA-COST | N "—" MANA-COST
//! ```
//!
//! Placeholders in a definition or reminder:
//!
//! ```text
//! $N                 the printed count, in digits        (+$N/+$N, loses $N life)
//! $Nw                the printed count as a number word  (two)
//! $N{ONE|MANY}       ONE when the count is 1, MANY otherwise (number agreement)
//! $cost              the printed mana cost, as printed
//! ```
use std::sync::OnceLock;

use serde::Deserialize;

use super::super::{MachineResult, OracleTokenCursor};
use super::*;
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::recognize_oracle_entities;

const EMBEDDED_TEMPLATES: &str =
    include_str!("../../../../data/ability-catalog/keyword-templates-v1.json");

pub(in crate::oracle::canonical) const KEYWORD_TEMPLATE_SCHEMA: &str =
    "deepdeck-keyword-templates/v1";

/// The kind of printed parameter a template keyword takes (CR 702.1).
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(in crate::oracle::canonical) enum KeywordTemplateParameter {
    None,
    Count,
    ManaCost,
    /// `N—[cost]` (CR 702.77a).
    CountThenManaCost,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::oracle::canonical) struct KeywordTemplate {
    pub canonical_id: String,
    pub template_version: u32,
    pub lexeme: String,
    pub parameter: KeywordTemplateParameter,
    /// The cached CR clause (`ability-catalog-v1.json`) the abilities quote.
    pub rules_clause: String,
    pub rules_section: String,
    /// A subtype the face must carry when the definition names the source by
    /// it (`this Equipment`, CR 702.92a).
    #[serde(default)]
    pub face_subtype: Option<String>,
    /// The action the keyword's ability performs, published as an event of
    /// that lemma when it is performed as the ability resolves (exploit,
    /// CR 702.110b). `None` for a keyword that names no action.
    #[serde(default)]
    pub named_action: Option<KeywordNamedAction>,
    /// One Oracle ability per ability the keyword represents.
    pub abilities: Vec<String>,
    /// Accepted reminder renderings, without their parentheses.
    pub reminders: Vec<String>,
}

/// A keyword's named action: its lemma and the engine action performing it.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::oracle::canonical) struct KeywordNamedAction {
    pub lemma: String,
    pub performed_by: KeywordActionPerformer,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(in crate::oracle::canonical) enum KeywordActionPerformer {
    /// The ability's controller sacrifices a permanent as it resolves.
    Sacrifice,
}

impl KeywordActionPerformer {
    pub(in crate::oracle::canonical) fn as_str(self) -> &'static str {
        match self {
            Self::Sacrifice => "sacrifice",
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::oracle::canonical) struct KeywordTemplateDocument {
    pub schema_version: String,
    pub catalog_version: String,
    pub rules_effective_date: String,
    pub templates: Vec<KeywordTemplate>,
}

struct LoadedTemplate {
    template: KeywordTemplate,
    lexeme: Vec<String>,
}

struct LoadedTemplates {
    document: KeywordTemplateDocument,
    templates: Vec<LoadedTemplate>,
}

fn loaded() -> &'static LoadedTemplates {
    static TEMPLATES: OnceLock<LoadedTemplates> = OnceLock::new();
    TEMPLATES.get_or_init(|| {
        let document: KeywordTemplateDocument = serde_json::from_str(EMBEDDED_TEMPLATES)
            .expect("embedded keyword templates must be valid JSON");
        assert_eq!(document.schema_version, KEYWORD_TEMPLATE_SCHEMA);
        let mut templates = document
            .templates
            .iter()
            .map(|template| LoadedTemplate {
                template: template.clone(),
                lexeme: recognize_oracle_entities(&template.lexeme)
                    .into_iter()
                    .map(|token| token.raw.to_ascii_lowercase())
                    .collect(),
            })
            .collect::<Vec<_>>();
        // Longest lexeme first, so a multiword keyword is never shadowed.
        templates.sort_by(|left, right| right.lexeme.len().cmp(&left.lexeme.len()));
        LoadedTemplates {
            document,
            templates,
        }
    })
}

pub(in crate::oracle::canonical) fn keyword_template_document() -> &'static KeywordTemplateDocument
{
    &loaded().document
}

/// The printed parameter of one keyword line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct KeywordTemplateArguments {
    pub count: Option<i64>,
    /// The printed mana cost, exactly as printed.
    pub cost: Option<String>,
}

/// One keyword line read against its template, with the definition already
/// instantiated with the printed parameter.
#[derive(Clone, Debug)]
pub(in crate::oracle::canonical) struct KeywordTemplateInvocation {
    pub template: &'static KeywordTemplate,
    pub arguments: KeywordTemplateArguments,
    pub reminder: bool,
    pub abilities: Vec<String>,
    pub trace: String,
}

fn is_numeral(raw: &str) -> bool {
    raw.eq_ignore_ascii_case("x") || (!raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()))
}

fn parameter_starts(kind: KeywordTemplateParameter, next: Option<&RecognizedEntity>) -> bool {
    match kind {
        KeywordTemplateParameter::None => next.is_none_or(|token| token.raw == "("),
        KeywordTemplateParameter::Count | KeywordTemplateParameter::CountThenManaCost => {
            next.is_some_and(|token| is_numeral(&token.raw))
        }
        KeywordTemplateParameter::ManaCost => {
            next.is_some_and(|token| token.token_type == "manaSymbol")
        }
    }
}

/// Structural claim: the ability starts with a template keyword's lexeme
/// immediately followed by the start of its parameter kind.
fn claimed_template(tokens: &[RecognizedEntity]) -> Option<&'static LoadedTemplate> {
    loaded().templates.iter().find(|loaded| {
        tokens.len() >= loaded.lexeme.len()
            && loaded
                .lexeme
                .iter()
                .zip(tokens)
                .all(|(word, token)| token.raw.eq_ignore_ascii_case(word))
            && parameter_starts(loaded.template.parameter, tokens.get(loaded.lexeme.len()))
    })
}

/// Reads a keyword line owned by a template keyword. `None` when no template
/// keyword claims the line; a claimed line that fails later fails explicitly.
pub(in crate::oracle::canonical) fn recognize_keyword_template_line(
    tokens: &[RecognizedEntity],
    text: &str,
    face_type_line: &str,
) -> Option<MachineResult<KeywordTemplateInvocation>> {
    let loaded = claimed_template(tokens)?;
    let mut cursor = OracleTokenCursor::new("keyword-template", tokens);
    Some(read_line(&mut cursor, loaded, text, face_type_line))
}

fn read_count(cursor: &mut OracleTokenCursor<'_>) -> MachineResult<i64> {
    match recognize_numeric_atom(cursor)? {
        NumericAtom::Integer(value) if value > 0 => Ok(value),
        _ => Err(cursor.fail("a positive literal keyword count")),
    }
}

fn read_mana_cost(cursor: &mut OracleTokenCursor<'_>, text: &str) -> MachineResult<String> {
    cursor.enter("keyword.template.parameter.mana-cost");
    let start = cursor.position();
    let cost = recognize_mana_cost(cursor)?;
    cursor.enter("keyword.template.parameter.mana-support");
    cost.fixed_runtime_mana_value()
        .map_err(|error| cursor.fail(error.expected()))?;
    let printed = cursor.token_slice(start, cursor.position());
    let (Some(first), Some(last)) = (printed.first(), printed.last()) else {
        return Err(cursor.fail("a printed mana cost"));
    };
    text.get(first.span_start..last.span_end)
        .map(str::to_string)
        .ok_or_else(|| cursor.fail("a printed mana cost inside the ability text"))
}

fn read_line(
    cursor: &mut OracleTokenCursor<'_>,
    loaded: &'static LoadedTemplate,
    text: &str,
    face_type_line: &str,
) -> MachineResult<KeywordTemplateInvocation> {
    let template = &loaded.template;
    cursor.enter("keyword.template.lexeme");
    for word in &loaded.lexeme {
        cursor.consume(word)?;
    }
    cursor.enter("keyword.template.parameter");
    let mut arguments = KeywordTemplateArguments::default();
    match template.parameter {
        KeywordTemplateParameter::None => {}
        KeywordTemplateParameter::Count => arguments.count = Some(read_count(cursor)?),
        KeywordTemplateParameter::ManaCost => arguments.cost = Some(read_mana_cost(cursor, text)?),
        KeywordTemplateParameter::CountThenManaCost => {
            arguments.count = Some(read_count(cursor)?);
            cursor.enter("keyword.template.parameter.dash");
            cursor.consume("\u{2014}")?;
            arguments.cost = Some(read_mana_cost(cursor, text)?);
        }
    }
    cursor.enter("keyword.template.reminder-or-complete");
    let reminder = cursor.peek() == Some("(");
    if reminder {
        read_reminder(cursor, template, &arguments)?;
    }
    cursor.enter("keyword.template.complete");
    cursor.finish()?;
    if let Some(subtype) = &template.face_subtype {
        cursor.enter("keyword.template.face-subtype");
        let carries = face_type_line
            .split(|character: char| !character.is_alphanumeric())
            .any(|word| word.eq_ignore_ascii_case(subtype));
        cursor.require(
            carries,
            "a face carrying the subtype its keyword definition names",
        )?;
    }
    cursor.enter("keyword.template.instantiate");
    let abilities = template
        .abilities
        .iter()
        .map(|ability| instantiate(ability, &arguments))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|expected| cursor.fail(&expected))?;
    let trace = cursor
        .transitions()
        .iter()
        .map(|transition| transition.state)
        .collect::<Vec<_>>()
        .join(" -> ");
    Ok(KeywordTemplateInvocation {
        template,
        arguments,
        reminder,
        abilities,
        trace,
    })
}

fn read_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    template: &KeywordTemplate,
    arguments: &KeywordTemplateArguments,
) -> MachineResult<()> {
    cursor.enter("keyword.template.reminder.open");
    cursor.consume("(")?;
    let body_start = cursor.position();
    let close = body_start + cursor.token_slice(body_start, usize::MAX).len();
    cursor.require(
        close > body_start
            && cursor
                .token_slice(close - 1, close)
                .first()
                .is_some_and(|token| token.raw == ")"),
        "a reminder closing the keyword ability",
    )?;
    let body_end = close - 1;
    cursor.enter("keyword.template.reminder.body");
    let printed = cursor.token_slice(body_start, body_end);
    let matches = template.reminders.iter().any(|reminder| {
        instantiate(reminder, arguments).is_ok_and(|rendering| {
            let expected = recognize_oracle_entities(&rendering);
            expected.len() == printed.len()
                && expected
                    .iter()
                    .zip(printed)
                    .all(|(expected, token)| token.raw.eq_ignore_ascii_case(&expected.raw))
        })
    });
    cursor.require(
        matches,
        "the keyword's reminder instantiated with the printed parameter",
    )?;
    cursor.advance_to(body_end)?;
    cursor.enter("keyword.template.reminder.close");
    cursor.consume(")")
}

/// Instantiates a definition or reminder with the printed parameter. Every
/// placeholder must have its argument; an unknown placeholder is an error.
pub(in crate::oracle::canonical) fn instantiate(
    template: &str,
    arguments: &KeywordTemplateArguments,
) -> Result<String, String> {
    render(template, &mut |placeholder| match placeholder {
        Placeholder::Count => arguments
            .count
            .map(|count| count.to_string())
            .ok_or_else(|| "a printed keyword count for `$N`".to_string()),
        Placeholder::CountWord => {
            let count = arguments
                .count
                .ok_or_else(|| "a printed keyword count for `$Nw`".to_string())?;
            number_word(count)
                .map(str::to_string)
                .ok_or_else(|| format!("an Oracle number word for the keyword count {count}"))
        }
        Placeholder::CountAgreement { one, many } => {
            let count = arguments
                .count
                .ok_or_else(|| "a printed keyword count for `$N{...}`".to_string())?;
            Ok(if count == 1 { one } else { many }.to_string())
        }
        Placeholder::Cost => arguments
            .cost
            .clone()
            .ok_or_else(|| "a printed keyword cost for `$cost`".to_string()),
    })
}

/// Renders a template with the symbolic parameters of its CR definition
/// (`N`, `[cost]`, the plural agreement branch). Used to check that a row
/// quotes its cached rules clause.
pub(in crate::oracle::canonical) fn render_rules_form(template: &str) -> Result<String, String> {
    render(template, &mut |placeholder| {
        Ok(match placeholder {
            Placeholder::Count | Placeholder::CountWord => "N".to_string(),
            Placeholder::CountAgreement { many, .. } => many.to_string(),
            Placeholder::Cost => "[cost]".to_string(),
        })
    })
}

enum Placeholder<'a> {
    Count,
    CountWord,
    CountAgreement { one: &'a str, many: &'a str },
    Cost,
}

fn render(
    template: &str,
    resolve: &mut dyn FnMut(Placeholder<'_>) -> Result<String, String>,
) -> Result<String, String> {
    let mut output = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(dollar) = rest.find('$') {
        output.push_str(&rest[..dollar]);
        let after = &rest[dollar + 1..];
        let name_length = after
            .find(|character: char| !character.is_ascii_alphabetic())
            .unwrap_or(after.len());
        let (name, tail) = after.split_at(name_length);
        rest = match name {
            "N" if tail.starts_with('{') => {
                let close = tail
                    .find('}')
                    .ok_or_else(|| "a closing `}` after `$N{`".to_string())?;
                let (one, many) = tail[1..close]
                    .split_once('|')
                    .ok_or_else(|| "`$N{ONE|MANY}` with two branches".to_string())?;
                let one = render(one, resolve)?;
                let many = render(many, resolve)?;
                output.push_str(&resolve(Placeholder::CountAgreement {
                    one: &one,
                    many: &many,
                })?);
                &tail[close + 1..]
            }
            "N" => {
                output.push_str(&resolve(Placeholder::Count)?);
                tail
            }
            "Nw" => {
                output.push_str(&resolve(Placeholder::CountWord)?);
                tail
            }
            "cost" => {
                output.push_str(&resolve(Placeholder::Cost)?);
                tail
            }
            other => {
                return Err(format!(
                    "a known keyword template placeholder, got `${other}`"
                ));
            }
        };
    }
    output.push_str(rest);
    Ok(output)
}

#[cfg(test)]
#[path = "keyword_template_tests.rs"]
mod tests;
