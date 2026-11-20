//! Structural frame claim for an ability that no reader completed.
//!
//! Every Oracle ability belongs to one structural frame, recognized from its
//! introducer before any inner grammar (CR 113.3):
//!
//! ```text
//! Ability := [label "—"] Frame
//! Frame   := Keyword [parameters] ["(" reminder ")"]             CR 702
//!          | COSTS ":" EFFECTS                                    CR 602.1
//!          | ("when" | "whenever" | "at") EVENT "," EFFECTS       CR 603.1
//!          | "if" EVENT-SUBJECT "would" EVENT "," REPLACEMENT     CR 614.1a
//!          | "if" CONDITION "," BODY                              CR 614.1, 611.3a
//!          | "as" SOURCE "enters" "," MODIFICATION                CR 614.12
//!          | SUBJECT ("enter" | "enters") MODIFICATION           CR 614.1c-d
//!          | "as an additional cost to cast" ... "," COST         CR 601.2b, 118.8
//!          | "prevent" DAMAGE-SCOPE                               CR 615
//!          | "you may" ACTION                                     CR 609.3, 601.2
//!          | ("during" | "as long as") CONDITION "," STATIC       CR 611.3a
//!          | SUBJECT PREDICATE                                    CR 604.1, 611.3
//!          | spell instructions                                   CR 608.2
//! ```
//!
//! Earlier readers keep their precedence and their exact IR. This claim runs
//! only once none of them owned the ability: the frame then owns it and its
//! diagnostic names where the frame's shared inner grammar stopped, so an
//! ability is never reported as unclaimed. Nothing here accepts a parse; the
//! outcome is always an explicit failure.
use super::super::*;
use crate::oracle::characteristic_vocabulary::type_line_has_permanent_card_type;
use crate::oracle::model::RecognizedEntity;
use crate::oracle::syntax::AbilityInput;

/// The structural frame owning an ability, recognized from its introducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum AbilityFrame {
    Keyword,
    Activated,
    Triggered,
    /// `if … would …` (CR 614.1a).
    Replacement,
    /// `if CONDITION, …` with no replaced event before the comma.
    ConditionalBody,
    /// `as … enters, …` or `… enters with/tapped/as …` (CR 614.1c-d, 614.12).
    EntryReplacement,
    AdditionalCost,
    Prevention,
    OptionalPermission,
    /// `during …` or `as long as …` (CR 611.3a).
    ConditionalStatic,
    StaticContinuous,
    SpellEffect,
    /// Rules text of a face that has neither a permanent nor a spell type.
    RulesText,
}

/// Root positions are outside quotation marks and parentheses: a reminder or
/// a granted quoted ability owns its own punctuation.
#[derive(Default)]
struct RootPunctuation {
    colon: Option<usize>,
    comma: Option<usize>,
    period: Option<usize>,
}

fn root_punctuation(tokens: &[RecognizedEntity], start: usize) -> RootPunctuation {
    let mut punctuation = RootPunctuation::default();
    let mut quoted = false;
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            raw if !quoted && depth == 0 => {
                let slot = match raw {
                    ":" => &mut punctuation.colon,
                    "," => &mut punctuation.comma,
                    "." => &mut punctuation.period,
                    _ => continue,
                };
                slot.get_or_insert(index);
            }
            _ => {}
        }
    }
    punctuation
}

fn word_at(tokens: &[RecognizedEntity], index: usize, expected: &str) -> bool {
    tokens
        .get(index)
        .is_some_and(|token| token.raw.eq_ignore_ascii_case(expected))
}

fn phrase_at(tokens: &[RecognizedEntity], index: usize, phrase: &[&str]) -> bool {
    phrase
        .iter()
        .enumerate()
        .all(|(offset, word)| word_at(tokens, index + offset, word))
}

/// A registered ability word, or a short flavor label made only of words,
/// closed by an em dash (CR 207.2c, 207.2d). The label never selects a frame.
fn body_start(tokens: &[RecognizedEntity]) -> usize {
    const MAXIMUM_LABEL_TOKENS: usize = 8;
    if let Some(start) = ability_word_body_start(tokens).filter(|start| *start > 0) {
        return start;
    }
    let Some(dash) = tokens
        .iter()
        .take(MAXIMUM_LABEL_TOKENS + 1)
        .position(|token| token.raw == "\u{2014}")
    else {
        return 0;
    };
    let plain_label = dash > 0
        && tokens[..dash].iter().all(|token| {
            token.token_type != "keywordWord"
                && token
                    .raw
                    .chars()
                    .all(|ch| ch.is_alphanumeric() || matches!(ch, '\'' | '\u{2019}' | '-' | '!'))
        });
    if plain_label { dash + 1 } else { 0 }
}

/// The token after a keyword name: a keyword line carries only parameters,
/// a reminder or another keyword (CR 702.1). A keyword noun that heads a
/// sentence (`Flying creatures can't block`) is not a keyword line.
fn keyword_line_continues(tokens: &[RecognizedEntity], index: usize) -> bool {
    let Some(token) = tokens.get(index) else {
        return true;
    };
    matches!(token.raw.as_str(), "(" | "," | "." | "\u{2014}" | ";")
        || matches!(
            token.token_type.as_str(),
            "manaSymbol" | "quantity" | "keywordWord" | "number"
        )
        || token.raw.parse::<i64>().is_ok()
        || ["from", "for", "with", "x", "n", "and"]
            .iter()
            .any(|word| token.raw.eq_ignore_ascii_case(word))
}

/// A keyword parameter: a number, a mana or cost symbol, or `X` (CR 702.1).
fn keyword_parameter(token: &RecognizedEntity) -> bool {
    matches!(
        token.token_type.as_str(),
        "manaSymbol" | "quantity" | "number"
    ) || token.raw.parse::<i64>().is_ok()
        || token.raw.eq_ignore_ascii_case("x")
}

/// The keyword name: catalog keyword words, or the words before the first
/// parameter, reminder or separator of a structural keyword line.
fn keyword_run_end(tokens: &[RecognizedEntity], start: usize) -> usize {
    let catalog = tokens[start..]
        .iter()
        .take_while(|token| token.token_type == "keywordWord")
        .count();
    if catalog > 0 {
        return start + catalog;
    }
    start
        + tokens[start..]
            .iter()
            .take_while(|token| {
                !keyword_parameter(token)
                    && !matches!(token.raw.as_str(), "(" | "," | "." | "\u{2014}" | ":")
            })
            .count()
            .max(1)
}

/// The structural shape of a keyword line whose name the vocabulary does not
/// register yet (CR 702.1, 207.2): a short capitalized head without root
/// punctuation, optional parameters, and either nothing or one reminder that
/// closes the ability. A sentence before a reminder ends with its period.
fn structural_keyword_line(tokens: &[RecognizedEntity], start: usize) -> bool {
    const MAXIMUM_HEAD_TOKENS: usize = 5;
    let Some(first) = tokens.get(start) else {
        return false;
    };
    if !first.raw.starts_with(|ch: char| ch.is_ascii_uppercase())
        || !matches!(first.token_type.as_str(), "unknown" | "keywordWord")
    {
        return false;
    }
    let head_end = tokens[start..]
        .iter()
        .position(|token| token.raw == "(")
        .map_or(tokens.len(), |offset| start + offset);
    let head = &tokens[start..head_end];
    let head_is_name_and_parameters = head.len() <= MAXIMUM_HEAD_TOKENS
        && head.iter().all(|token| {
            !matches!(token.raw.as_str(), "," | "." | ":" | ";" | "\"")
                && !matches!(token.token_type.as_str(), "triggerWord" | "modalityWord")
        });
    if !head_is_name_and_parameters {
        return false;
    }
    if head_end == tokens.len() {
        // Without a reminder, only a name followed by parameters qualifies.
        return head.len() >= 2 && head[1..].iter().all(keyword_parameter);
    }
    let mut depth = 0_usize;
    for (offset, token) in tokens[head_end..].iter().enumerate() {
        match token.raw.as_str() {
            "(" => depth += 1,
            ")" => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return tokens[head_end + offset + 1..].is_empty();
                }
            }
            _ => {}
        }
    }
    false
}

/// The first root `enter`/`enters` before the first root comma, when it is
/// followed by an entry modification rather than ending a clause.
fn entry_modification_verb(tokens: &[RecognizedEntity], start: usize, end: usize) -> Option<usize> {
    let mut quoted = false;
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().take(end).skip(start) {
        match token.raw.as_str() {
            "\"" => quoted = !quoted,
            "(" if !quoted => depth += 1,
            ")" if !quoted => depth = depth.saturating_sub(1),
            raw if !quoted
                && depth == 0
                && (raw.eq_ignore_ascii_case("enters") || raw.eq_ignore_ascii_case("enter")) =>
            {
                return Some(index);
            }
            _ => {}
        }
    }
    None
}

pub(in crate::oracle::canonical) fn recognize_ability_frame(
    input: &AbilityInput<'_>,
) -> AbilityFrame {
    let tokens = input.tokens;
    let start = body_start(tokens);
    let root = root_punctuation(tokens, start);
    let clause_end = root
        .comma
        .into_iter()
        .chain(root.period)
        .min()
        .unwrap_or(tokens.len());
    let first = tokens.get(start);
    if first.is_some_and(|token| {
        token.token_type == "triggerWord"
            && ["when", "whenever", "at"]
                .iter()
                .any(|word| token.raw.eq_ignore_ascii_case(word))
    }) {
        return AbilityFrame::Triggered;
    }
    if (first.is_some_and(|token| token.token_type == "keywordWord")
        && keyword_line_continues(tokens, keyword_run_end(tokens, start)))
        || structural_keyword_line(tokens, start)
    {
        return AbilityFrame::Keyword;
    }
    if root
        .colon
        .is_some_and(|colon| colon > start && root.period.is_none_or(|period| colon < period))
    {
        return AbilityFrame::Activated;
    }
    if word_at(tokens, start, "if") {
        let would = (start..clause_end).any(|index| word_at(tokens, index, "would"));
        return if would {
            AbilityFrame::Replacement
        } else {
            AbilityFrame::ConditionalBody
        };
    }
    if phrase_at(tokens, start, &["as", "an", "additional", "cost"]) {
        return AbilityFrame::AdditionalCost;
    }
    if phrase_at(tokens, start, &["as", "long", "as"]) || word_at(tokens, start, "during") {
        return AbilityFrame::ConditionalStatic;
    }
    if entry_modification_verb(tokens, start, clause_end).is_some()
        && (word_at(tokens, start, "as") || root.comma.is_none_or(|comma| clause_end < comma))
    {
        return AbilityFrame::EntryReplacement;
    }
    if word_at(tokens, start, "prevent") {
        return AbilityFrame::Prevention;
    }
    if phrase_at(tokens, start, &["you", "may"]) {
        return AbilityFrame::OptionalPermission;
    }
    if spell_face(input.face_type_line) {
        return AbilityFrame::SpellEffect;
    }
    if type_line_has_permanent_card_type(input.face_type_line) {
        return AbilityFrame::StaticContinuous;
    }
    AbilityFrame::RulesText
}

/// A committed failure at `index`: the frame owns the ability up to there.
fn fail_at(
    input: &AbilityInput<'_>,
    state: &'static str,
    index: usize,
    expected: &str,
) -> MachineFailure {
    let mut cursor = OracleTokenCursor::new("ability-frame", input.tokens);
    cursor.enter(state);
    let index = index.min(input.tokens.len());
    if cursor.advance_to(index).is_err() {
        return cursor.fail(expected);
    }
    cursor.fail(expected)
}

fn first_root_index(
    tokens: &[RecognizedEntity],
    start: usize,
    end: usize,
    word: &str,
) -> Option<usize> {
    (start..end).find(|index| word_at(tokens, *index, word))
}

/// The frame owning an unclaimed ability and the failure inside that frame.
pub(in crate::oracle::canonical) fn claim_unclaimed_ability_frame(
    input: &AbilityInput<'_>,
) -> MachineFailure {
    let tokens = input.tokens;
    let start = body_start(tokens);
    let root = root_punctuation(tokens, start);
    let clause_end = root
        .comma
        .into_iter()
        .chain(root.period)
        .min()
        .unwrap_or(tokens.len());
    let after_comma = root.comma.map_or(tokens.len(), |comma| comma + 1);
    match recognize_ability_frame(input) {
        AbilityFrame::Keyword => {
            let end = keyword_run_end(tokens, start);
            let mut cursor = OracleTokenCursor::new("ability-frame", tokens);
            cursor.enter("ability-frame.keyword.expansion");
            let found = tokens[start..end]
                .iter()
                .map(|token| token.raw.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            cursor.fail_with_span(
                "a keyword ability with a typed, executable rules expansion (CR 702)",
                start,
                tokens[start].span_start,
                tokens[end - 1].span_end,
                found,
            )
        }
        AbilityFrame::Activated => fail_at(
            input,
            "ability-frame.activated.program",
            root.colon.map_or(start, |colon| colon + 1),
            "an activated ability whose costs and effects the shared activation program reads (CR 602.1)",
        ),
        AbilityFrame::Triggered => fail_at(
            input,
            "ability-frame.trigger.event",
            start + 1,
            "a supported trigger event after the trigger word (CR 603.1)",
        ),
        AbilityFrame::Replacement => fail_at(
            input,
            "ability-frame.replacement.event",
            first_root_index(tokens, start, clause_end, "would").map_or(start, |would| would + 1),
            "a replaceable event supported by the shared replacement frame (CR 614.1a)",
        ),
        AbilityFrame::ConditionalBody => fail_at(
            input,
            "ability-frame.conditional.body",
            after_comma,
            "a static or replacement body governed by the leading condition (CR 614.1, 611.3a)",
        ),
        AbilityFrame::EntryReplacement => {
            let index = if word_at(tokens, start, "as") {
                after_comma
            } else {
                entry_modification_verb(tokens, start, clause_end)
                    .map_or(start, |verb| verb + 1)
            };
            fail_at(
                input,
                "ability-frame.entry-replacement.modification",
                index,
                "an entry modification supported by the shared entry-replacement reader (CR 614.1c, 614.12)",
            )
        }
        AbilityFrame::AdditionalCost => fail_at(
            input,
            "ability-frame.additional-cost.action",
            after_comma,
            "an additional cost action supported by the shared cost reader (CR 601.2b)",
        ),
        AbilityFrame::Prevention => static_prevention_frame_failure(input, start),
        AbilityFrame::OptionalPermission => fail_at(
            input,
            "ability-frame.permission.action",
            start + 2,
            "an optional action, permission or alternative cost supported by the shared program (CR 609.3, 601.2b)",
        ),
        AbilityFrame::ConditionalStatic | AbilityFrame::StaticContinuous => {
            static_continuous_frame_failure(input).unwrap_or_else(|| {
                fail_at(
                    input,
                    "ability-frame.static.subject",
                    start,
                    "a static subject and continuous predicate supported by the shared static frame (CR 604.1, 611.3)",
                )
            })
        }
        AbilityFrame::SpellEffect => fail_at(
            input,
            "ability-frame.spell.program",
            start,
            "spell instructions supported by the shared effect program (CR 608.2)",
        ),
        AbilityFrame::RulesText => fail_at(
            input,
            "ability-frame.rules-text",
            start,
            "a keyword, activated, triggered, static, replacement or spell frame",
        ),
    }
}

#[cfg(test)]
#[path = "ability_frame_claim_tests.rs"]
mod tests;
