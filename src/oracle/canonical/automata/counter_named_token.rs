use super::super::{MachineResult, OracleTokenCursor};
use super::{
    CounterEffectNode, LocalEffectNode, LocalPlayerReference, NamedTokenCreationPattern,
    NumericAtom, TargetBinding, recognize_counter_effect, recognize_named_token_coordination,
    recognize_named_token_spec, recognize_player_reference,
};
use crate::card_catalog::named_token_printing;
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};
use crate::oracle::model::RecognizedEntity;
use std::cell::RefCell;
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CreateNamedTokensEffectNode {
    pub player: LocalPlayerReference,
    pub creation: NamedTokenCreationPattern,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct CounterNamedTokenSequenceNode {
    pub counter: CounterEffectNode,
    pub token_creation: CreateNamedTokensEffectNode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NamedTokenCreationState {
    Player,
    Action,
    Complete,
}

pub(super) fn recognize_named_token_identity(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<String> {
    cursor.enter("token.named.catalog-identity");
    let mut parts = Vec::new();
    while cursor
        .peek_token()
        .is_some_and(|token| token.token_type == "namedTokenWord")
    {
        parts.push(cursor.consume_type("namedTokenWord")?.raw.clone());
    }
    cursor.require(!parts.is_empty(), "an exact catalog-backed named token")?;
    let name = parts.join(" ");
    cursor.require(
        named_token_printing(&name).ok().flatten().is_some(),
        "an exact catalog-backed named token",
    )?;
    Ok(name)
}

/// Recognizes `[<player>] create(s) <quantity> <named artifact> token(s)` while
/// resolving controller anaphora through the parent's target binding. An
/// omitted subject is the ability controller, as in ordinary imperative
/// spell and triggered-ability instructions.
pub(in crate::oracle::canonical) fn recognize_create_named_tokens_effect(
    cursor: &mut OracleTokenCursor<'_>,
    antecedent: Option<TargetBinding>,
) -> MachineResult<CreateNamedTokensEffectNode> {
    let mut state = NamedTokenCreationState::Player;
    let mut player = None;
    let mut creation = None;
    loop {
        state = match state {
            NamedTokenCreationState::Player => {
                cursor.enter("effect.create-named-token.player");
                player = Some(
                    if crate::oracle::canonical::automata::lemma_at(
                        &cursor,
                        VerbLemma::Create,
                        crate::oracle::canonical::automata::VerbAgreement::Base,
                    ) {
                        LocalPlayerReference::Controller
                    } else {
                        recognize_player_reference(cursor, antecedent)?
                    },
                );
                NamedTokenCreationState::Action
            }
            NamedTokenCreationState::Action => {
                cursor.enter("effect.create-named-token.action");
                let action = cursor.consume_type("actionWord")?;
                let player = player
                    .as_ref()
                    .ok_or_else(|| cursor.fail("a populated token controller"))?;
                match player {
                    LocalPlayerReference::Controller => cursor.require(
                        is_verb_form(&action.raw, VerbLemma::Create, VerbForm::Base),
                        "the second-person `create` token action",
                    )?,
                    LocalPlayerReference::ControllerOf(_)
                    | LocalPlayerReference::ControllerOfQualified(_)
                    | LocalPlayerReference::Bound(_) => cursor.require(
                        is_verb_form(
                            &action.raw,
                            VerbLemma::Create,
                            VerbForm::ThirdPersonSingular,
                        ),
                        "the third-person `creates` token action",
                    )?,
                }
                creation = Some(recognize_named_token_spec(cursor)?);
                NamedTokenCreationState::Complete
            }
            NamedTokenCreationState::Complete => {
                cursor.enter("effect.create-named-token.complete");
                let first = cursor.require_value(creation, "a named-token specification")?;
                let creation = recognize_named_token_coordination(cursor, first)?;
                return Ok(CreateNamedTokensEffectNode {
                    player: cursor.require_value(player, "a named-token controller")?,
                    creation,
                });
            }
        };
    }
}

thread_local! {
    static REMINDER_COMPILATION_STACK: RefCell<BTreeSet<String>> = RefCell::new(BTreeSet::new());
}

struct ReminderCompilationGuard(String);

impl ReminderCompilationGuard {
    fn enter(name: &str) -> Option<Self> {
        let key = name.trim().to_ascii_lowercase();
        REMINDER_COMPILATION_STACK
            .with(|stack| stack.borrow_mut().insert(key.clone()).then_some(Self(key)))
    }
}

impl Drop for ReminderCompilationGuard {
    fn drop(&mut self) {
        REMINDER_COMPILATION_STACK.with(|stack| {
            stack.borrow_mut().remove(&self.0);
        });
    }
}

/// Validates a named-token reminder as a semantic projection of the catalog
/// definition. Both the embedded Oracle and the catalog Oracle are compiled by
/// the shared ability grammar (the token spec's quoted-ability slot), so
/// equivalent grammar compares equal as primitive IR without sentence
/// matching. The card effect still stores only the token identity.
pub(in crate::oracle::canonical) fn recognize_named_token_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    quantity: &NumericAtom,
    token_name: &str,
) -> MachineResult<()> {
    cursor.enter("reminder.named-token.open");
    cursor.consume("(")?;
    cursor.enter("reminder.named-token.subject");
    if matches!(quantity, NumericAtom::Integer(1)) {
        cursor.consume("it's")?;
        if cursor
            .peek()
            .is_some_and(|raw| matches!(raw.to_ascii_lowercase().as_str(), "a" | "an"))
        {
            cursor.advance_to(cursor.position() + 1)?;
        }
    } else {
        cursor.consume("they're")?;
    }

    recognize_named_token_reminder_body(
        cursor,
        token_name,
        NamedTokenReminderBoundary::FinalParenthesis,
    )
}

enum NamedTokenReminderBoundary {
    FinalParenthesis,
    NextDescription,
}

fn recognize_named_token_reminder_body(
    cursor: &mut OracleTokenCursor<'_>,
    token_name: &str,
    boundary: NamedTokenReminderBoundary,
) -> MachineResult<()> {
    cursor.enter("reminder.named-token.semantic-match");
    cursor.enter("reminder.named-token.semantic-match");
    let descriptor_start = cursor.position();
    while cursor
        .peek()
        .is_some_and(|raw| !raw.eq_ignore_ascii_case("with"))
    {
        cursor.advance_to(cursor.position() + 1)?;
    }
    let descriptor_end = cursor.position();
    cursor.require(
        descriptor_end > descriptor_start,
        "a token type description",
    )?;
    cursor.consume("with")?;
    // `with indestructible and "..."`: keyword abilities listed before the
    // quoted ones are lines of the same printed Oracle text (CR 111.4).
    cursor.enter("reminder.named-token.keyword-lines");
    let keyword_start = cursor.position();
    while cursor.peek().is_some_and(|raw| raw != "\"") {
        cursor.advance_to(cursor.position() + 1)?;
    }
    let keyword_lines =
        keyword_reminder_lines(cursor.token_slice(keyword_start, cursor.position()));
    cursor.require(
        keyword_lines.is_some(),
        "keyword abilities joined by `,` and a closing `and` before the quoted abilities",
    )?;
    cursor.enter("reminder.named-token.semantic-match");
    cursor.consume("\"")?;
    let oracle_start = cursor.position();
    while cursor.peek().is_some_and(|raw| raw != "\"") {
        cursor.advance_to(cursor.position() + 1)?;
    }
    let oracle_end = cursor.position();
    cursor.require(oracle_end > oracle_start, "embedded token Oracle")?;
    cursor.consume("\"")?;
    if matches!(boundary, NamedTokenReminderBoundary::FinalParenthesis) {
        cursor.consume(")")?;
    }

    cursor.enter("reminder.named-token.semantic-match");
    let Some(_guard) = ReminderCompilationGuard::enter(token_name) else {
        return Err(cursor.fail("an acyclic named-token reminder reference"));
    };
    let printing = named_token_printing(token_name)
        .ok()
        .flatten()
        .ok_or_else(|| cursor.fail("a catalog-backed named-token definition"))?;
    let type_line = printing
        .type_line
        .as_deref()
        .ok_or_else(|| cursor.fail("a catalog token type line"))?;
    let type_words = type_line
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(|word| word.trim_end_matches('s').to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let catalog_colors = printing.color_names();
    let descriptors = cursor.token_slice(descriptor_start, descriptor_end);
    let mut described_colors = BTreeSet::new();
    let mut described_colorless = false;
    let mut descriptors_compatible = true;
    for token in descriptors {
        let raw = token.raw.to_ascii_lowercase();
        if raw == "colorless" {
            described_colorless = true;
        } else if let Some(color) = catalog_colors.iter().find(|color| **color == raw) {
            described_colors.insert(*color);
        } else if raw == "and" && !catalog_colors.is_empty() {
        } else {
            descriptors_compatible &= type_words.contains(raw.trim_end_matches('s'));
        }
    }
    // A colored token must be described by exactly its printed colors; a
    // colorless description is valid only for a token without colors.
    descriptors_compatible &= described_colors.len() == catalog_colors.len()
        && (!described_colorless || catalog_colors.is_empty());
    cursor.require(
        descriptors_compatible,
        "a reminder type description compatible with the catalog token",
    )?;
    let catalog_oracle = printing
        .oracle_text
        .as_deref()
        .ok_or_else(|| cursor.fail("catalog token Oracle"))?;
    let embedded_tokens = cursor.token_slice(oracle_start, oracle_end);
    let embedded_oracle = keyword_lines
        .unwrap_or_default()
        .into_iter()
        .chain(std::iter::once(source_text(embedded_tokens)))
        .collect::<Vec<_>>()
        .join("\n");
    let embedded_rules =
        compile_token_rules_with_shared_grammar(token_name, type_line, &embedded_oracle);
    let matches = embedded_rules.is_some()
        && embedded_rules
            == compile_token_rules_with_shared_grammar(token_name, type_line, catalog_oracle);
    cursor.require(matches, "the catalog token type line and canonical rules")
}

/// The keyword abilities a reminder lists before its quoted abilities, as
/// printed Oracle lines (`indestructible and` -> `Indestructible`). Empty
/// when the quote follows `with` directly; `None` when the words are not a
/// `,`/`and` list closed by `and`.
fn keyword_reminder_lines(tokens: &[RecognizedEntity]) -> Option<Vec<String>> {
    if tokens.is_empty() {
        return Some(Vec::new());
    }
    let (last, items) = tokens.split_last()?;
    if !last.raw.eq_ignore_ascii_case("and") {
        return None;
    }
    let mut lines = Vec::new();
    for item in items.split(|token| token.raw == "," || token.raw.eq_ignore_ascii_case("and")) {
        if item.is_empty() {
            continue;
        }
        let text = source_text(item);
        let mut characters = text.chars();
        let first = characters.next()?;
        lines.push(first.to_uppercase().chain(characters).collect());
    }
    (!lines.is_empty()).then_some(lines)
}

/// Rebuilds source text from token spans; gaps become single spaces.
pub(in crate::oracle::canonical) fn source_text(tokens: &[RecognizedEntity]) -> String {
    let mut text = String::new();
    let mut previous_end = None;
    for token in tokens {
        if previous_end.is_some_and(|end| end < token.span_start) {
            text.push(' ');
        }
        text.push_str(&token.raw);
        previous_end = Some(token.span_end);
    }
    text
}

/// Compiles token Oracle text as an ordinary card face. Every ability must be
/// canonical; a partially parsed token has no definition.
fn compile_token_rules_with_shared_grammar(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> Option<Vec<serde_json::Value>> {
    let result = crate::oracle::parse_oracle_card(crate::oracle::OracleCardParseRequest {
        card_name: token_name.to_string(),
        type_line: type_line.to_string(),
        mana_cost: None,
        oracle_text: Some(oracle.to_string()),
        layout: Some("token".to_string()),
        faces: Vec::new(),
    });
    let rules = result
        .abilities
        .iter()
        .map(|ability| {
            (ability.status == "canonical")
                .then(|| ability.rule.clone())
                .flatten()
        })
        .collect::<Option<Vec<_>>>()?;
    (!rules.is_empty()).then_some(rules)
}

/// Executable rules of a built-in catalog token: its Oracle text compiled by
/// the shared ability grammar, so no per-token compiler or closed token
/// vocabulary exists.
pub(in crate::oracle::canonical) fn compile_named_token_catalog_rules(
    token_name: &str,
    type_line: &str,
    oracle: &str,
) -> Option<Vec<serde_json::Value>> {
    compile_token_rules_with_shared_grammar(token_name, type_line, oracle)
}

pub(in crate::oracle::canonical) fn recognize_named_token_creation_reminder(
    cursor: &mut OracleTokenCursor<'_>,
    creation: &NamedTokenCreationPattern,
) -> MachineResult<()> {
    let specs = match creation {
        NamedTokenCreationPattern::Single(spec) => {
            return recognize_named_token_reminder(cursor, &spec.quantity, &spec.token_name);
        }
        NamedTokenCreationPattern::OneOf(specs) => specs,
    };
    cursor.enter("reminder.named-token-list.open");
    cursor.consume("(")?;
    let expected = specs
        .iter()
        .map(|s| s.token_name.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    while cursor.peek().is_some_and(|s| s != ")") {
        cursor.enter("reminder.named-token-list.subject");
        let article = cursor
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an"));
        if article {
            cursor.advance_to(cursor.position() + 1)?;
        }
        let name = recognize_named_token_identity(cursor)?;
        let key = name.to_ascii_lowercase();
        cursor.require(
            expected.contains(&key) && seen.insert(key),
            "one description per selected catalog token identity",
        )?;
        if article {
            cursor.consume("token")?;
        }
        cursor.consume("is")?;
        if cursor
            .peek()
            .is_some_and(|s| s.eq_ignore_ascii_case("a") || s.eq_ignore_ascii_case("an"))
        {
            cursor.advance_to(cursor.position() + 1)?;
        }
        recognize_named_token_reminder_body(
            cursor,
            &name,
            NamedTokenReminderBoundary::NextDescription,
        )?;
        if cursor.peek() == Some(".") {
            cursor.consume(".")?;
        }
    }
    cursor.enter("reminder.named-token-list.coverage");
    cursor.require(
        seen == expected,
        "descriptions for every alternative token identity",
    )?;
    cursor.consume(")")?;
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CounterNamedTokenState {
    Counter,
    SentenceBoundary,
    TokenCreation,
    TokenTerminator,
    ReminderOrComplete,
    Complete,
}

/// Composes a counter effect with an independently recognized named-token
/// creation linked to the countered object's controller.
pub(in crate::oracle::canonical) fn recognize_counter_named_token_sequence(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<CounterNamedTokenSequenceNode> {
    let mut state = CounterNamedTokenState::Counter;
    let mut counter = None;
    let mut token_creation = None;
    loop {
        state = match state {
            CounterNamedTokenState::Counter => {
                cursor.enter("sequence.counter-named-token.counter");
                let parsed = recognize_counter_effect(cursor)?;
                let LocalEffectNode::CounterSpell(effect) = parsed else {
                    return Err(cursor.fail("a counter-spell effect"));
                };
                counter = Some(effect);
                CounterNamedTokenState::SentenceBoundary
            }
            CounterNamedTokenState::SentenceBoundary => {
                cursor.enter("sequence.counter-named-token.sentence-boundary");
                cursor.consume(".")?;
                CounterNamedTokenState::TokenCreation
            }
            CounterNamedTokenState::TokenCreation => {
                cursor.enter("sequence.counter-named-token.creation");
                token_creation = Some(recognize_create_named_tokens_effect(
                    cursor,
                    Some(TargetBinding {
                        effect_index: 0,
                        slot_index: 0,
                    }),
                )?);
                CounterNamedTokenState::TokenTerminator
            }
            CounterNamedTokenState::TokenTerminator => {
                cursor.enter("sequence.counter-named-token.creation-terminator");
                cursor.consume(".")?;
                CounterNamedTokenState::ReminderOrComplete
            }
            CounterNamedTokenState::ReminderOrComplete => {
                cursor.enter("sequence.counter-named-token.reminder-or-complete");
                if cursor.peek().is_some_and(|raw| raw == "(") {
                    let creation = token_creation
                        .as_ref()
                        .ok_or_else(|| cursor.fail("a completed named-token creation"))?;
                    recognize_named_token_creation_reminder(cursor, &creation.creation)?;
                }
                CounterNamedTokenState::Complete
            }
            CounterNamedTokenState::Complete => {
                cursor.enter("sequence.counter-named-token.complete");
                cursor.finish()?;
                return Ok(CounterNamedTokenSequenceNode {
                    counter: cursor.require_value(counter, "a counter effect")?,
                    token_creation: cursor
                        .require_value(token_creation, "a linked named-token creation")?,
                });
            }
        };
    }
}

#[cfg(test)]
#[path = "counter_named_token_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "named_token_catalog_tests.rs"]
mod catalog_tests;
