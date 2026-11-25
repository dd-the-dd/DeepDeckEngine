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
use crate::oracle::ability_catalog::match_catalog_mechanic;
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

/// Root separators of a keyword line (CR 702.1): `,` `;` and `and` outside a
/// reminder. Each item keeps the reminder that follows it.
fn keyword_line_items(tokens: &[RecognizedEntity], start: usize) -> Vec<(usize, usize)> {
    let mut items = Vec::new();
    let mut item_start = start;
    let mut depth = 0_usize;
    for (index, token) in tokens.iter().enumerate().skip(start) {
        match token.raw.as_str() {
            "(" => depth += 1,
            ")" => depth = depth.saturating_sub(1),
            "," | ";" if depth == 0 => {
                items.push((item_start, index));
                item_start = index + 1;
            }
            raw if depth == 0 && raw.eq_ignore_ascii_case("and") => {
                if item_start < index {
                    items.push((item_start, index));
                }
                item_start = index + 1;
            }
            _ => {}
        }
    }
    let end = if tokens.last().is_some_and(|token| token.raw == ".") {
        tokens.len() - 1
    } else {
        tokens.len()
    };
    items.push((item_start, end.max(item_start)));
    items.retain(|(item_start, item_end)| item_start < item_end);
    items
}

/// The failure of a keyword line names the item that the shared keyword
/// reader rejects, not the first keyword of the line. When every item reads
/// alone, the separator joining them is what the list grammar lacks.
fn keyword_line_failure(tokens: &[RecognizedEntity], start: usize) -> MachineFailure {
    let items = keyword_line_items(tokens, start);
    let failing = items.iter().position(|(item_start, item_end)| {
        let mut cursor = OracleTokenCursor::new(
            "ability-frame-keyword-item",
            &tokens[*item_start..*item_end],
        );
        !matches!(
            try_recognize_standalone_keyword_ability(&mut cursor),
            StandaloneKeywordRecognition::Parsed(_)
        )
    });
    let mut cursor = OracleTokenCursor::new("ability-frame", tokens);
    match failing {
        Some(index) => {
            // An item that does not start with a keyword continues the
            // previous item's parameters (`hexproof from X, Y, and Z`).
            let owner = (0..=index)
                .rev()
                .find(|candidate| {
                    let first = &tokens[items[*candidate].0];
                    *candidate == 0
                        || first.token_type == "keywordWord"
                        || match_catalog_mechanic(tokens, items[*candidate].0).is_some()
                })
                .unwrap_or(0);
            let item_start = items[owner].0;
            let end = keyword_run_end(tokens, item_start);
            cursor.enter("ability-frame.keyword.expansion");
            let found = tokens[item_start..end]
                .iter()
                .map(|token| token.raw.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            cursor.fail_with_span(
                "a keyword ability with a typed, executable rules expansion (CR 702)",
                item_start,
                tokens[item_start].span_start,
                tokens[end - 1].span_end,
                found,
            )
        }
        None => {
            let separator = items
                .first()
                .map_or(start, |(_, separator)| *separator)
                .min(tokens.len().saturating_sub(1));
            cursor.enter("ability-frame.keyword.list-separator");
            cursor.fail_with_span(
                "a keyword-list separator supported by the shared keyword list (CR 702.1)",
                separator,
                tokens[separator].span_start,
                tokens[separator].span_end,
                tokens[separator].raw.clone(),
            )
        }
    }
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

/// `you may ACTION` (CR 609.3, 601.2b): the static permission frame reads
/// the action lemma and its operands with the shared readers. When that
/// frame owns the action and stops, its diagnostic names the missing child
/// (an origin, a condition, a cost or a reminder); otherwise the frame
/// reports the action head it has no reader for.
fn optional_permission_frame_failure(input: &AbilityInput<'_>, start: usize) -> MachineFailure {
    let mut cursor = OracleTokenCursor::new("ability-frame", input.tokens);
    if cursor.advance_to(start).is_ok() {
        match try_recognize_static_player_permission(
            &mut cursor,
            input.name_links,
            input.name_resolution_failure,
        ) {
            // Only a failure the frame owns replaces the head diagnostic; a
            // collection allowance whose envelope rejects a tail keeps it.
            CastingPermissionRecognition::FailedCommitted(failure) => return failure,
            CastingPermissionRecognition::NotClaimed
            | CastingPermissionRecognition::RejectedEnvelope(_)
            | CastingPermissionRecognition::Parsed(_) => {}
        }
    }
    fail_at(
        input,
        "ability-frame.permission.action",
        start + 2,
        "an optional action, permission or alternative cost supported by the shared program (CR 609.3, 601.2b)",
    )
}

/// The outcome of the conditional-static wrapper (CR 611.3a).
pub(in crate::oracle::canonical) enum ConditionalStaticAttempt {
    Parsed(CanonicalRuleDraft),
    /// The leading gate was read and the body, read by the ordinary frames,
    /// failed or cannot carry the gate.
    Committed(MachineFailure),
    NotClaimed,
}

/// `("as long as" CONDITION | "during" TURN | "if" CONDITION) "," BODY`
/// (CR 611.3a, 113.6):
/// the leading gate is read once with the shared static condition readers,
/// then the body is dispatched through the ordinary frame classifier
/// (`parse_body` reads the body as an ability of its own). The gate is
/// attached to the static rule the body produced, so a permission, a
/// replacement, a prevention, a cost change or a continuous effect each
/// keeps its own frame and IR. The wrapper never accepts a body that no
/// frame completed, and a body whose zone of function cannot evaluate the
/// gate fails explicitly.
pub(in crate::oracle::canonical) fn try_parse_conditional_static_frame(
    input: &AbilityInput<'_>,
    parse_body: &mut dyn FnMut(&AbilityInput<'_>) -> CanonicalParseOutcome,
) -> ConditionalStaticAttempt {
    let tokens = input.tokens;
    let start = body_start(tokens);
    // `as long as` / `during` (CR 611.3a), or `if` without a replaced event
    // before its comma (an `if … would` replacement keeps its own frame).
    let frame = recognize_ability_frame(input);
    let allow_if = frame == AbilityFrame::ConditionalBody;
    if !(frame == AbilityFrame::ConditionalStatic || allow_if) {
        return ConditionalStaticAttempt::NotClaimed;
    }
    let mut cursor = OracleTokenCursor::new("conditional-static", tokens);
    if cursor.advance_to(start).is_err() {
        return ConditionalStaticAttempt::NotClaimed;
    }
    let mut committed = false;
    // A gate the shared condition grammar does not read stays with the frame
    // that already reported it.
    let Ok(leading) = read_leading_static_conditions(&mut cursor, input, &mut committed, allow_if)
    else {
        return ConditionalStaticAttempt::NotClaimed;
    };
    let body = cursor.position();
    if (leading.condition.is_none() && leading.zone_of_function.is_none()) || body >= tokens.len() {
        return ConditionalStaticAttempt::NotClaimed;
    }
    let end = tokens.len();
    let name_links = input
        .name_links
        .iter()
        .filter_map(|link| rebase_name_link(link, body, end))
        .collect::<Vec<_>>();
    let name_failure = input
        .name_resolution_failure
        .and_then(|failure| rebase_name_failure(failure, body, end));
    let body_input = AbilityInput {
        face_name: input.face_name,
        face_type_line: input.face_type_line,
        source: input.source,
        tokens: &tokens[body..],
        name_links: &name_links,
        name_resolution_failure: name_failure.as_ref(),
    };
    let mut draft = match parse_body(&body_input) {
        // A leading `if` also introduces conditional spell instructions and
        // other frames: it takes over the ability only for a static body.
        CanonicalParseOutcome::Parsed { draft, .. }
            if allow_if && draft.rule["kind"] != "staticAbility" =>
        {
            return ConditionalStaticAttempt::NotClaimed;
        }
        CanonicalParseOutcome::Parsed { draft, .. } => draft,
        CanonicalParseOutcome::Failed { .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { .. }
        | CanonicalParseOutcome::Unsupported
            if allow_if =>
        {
            return ConditionalStaticAttempt::NotClaimed;
        }
        CanonicalParseOutcome::Failed { failure, .. }
        | CanonicalParseOutcome::FailedWithCandidateEvidence { failure, .. } => {
            return ConditionalStaticAttempt::Committed(rebase_machine_failure(failure, body));
        }
        CanonicalParseOutcome::Unsupported => {
            return ConditionalStaticAttempt::Committed(fail_at(
                input,
                "ability-frame.conditional-static.body",
                body,
                "a body the ordinary frames read after the leading gate (CR 611.3a)",
            ));
        }
    };
    match attach_leading_static_gate(&mut cursor, &mut draft.rule, &leading) {
        Ok(()) => {
            draft.operations.insert(
                0,
                "Read the leading static gate with the shared condition readers, then the body with the ordinary frame classifier (CR 611.3a)".to_string(),
            );
            ConditionalStaticAttempt::Parsed(draft)
        }
        Err(failure) => ConditionalStaticAttempt::Committed(failure),
    }
}

/// The modifiers of a body that the engine gates by the modifier's own
/// condition: a spell's own cost reduction (CR 601.2f) reads it while the
/// card is being cast.
fn modifier_gated_body(rule: &Value) -> bool {
    rule["modifiers"].as_array().is_some_and(|modifiers| {
        !modifiers.is_empty()
            && modifiers
                .iter()
                .all(|modifier| modifier["kind"] == "reduceOwnGenericCastingCost")
    })
}

/// A hand-functioning body the engine gates by the rule's condition: a
/// card's own flash permission (CR 702.8a).
fn hand_rule_gated_body(rule: &Value) -> bool {
    rule["modifiers"].as_array().is_some_and(|modifiers| {
        !modifiers.is_empty()
            && modifiers
                .iter()
                .all(|modifier| modifier["kind"] == "castAsThoughFlash")
    })
}

fn conjoined(existing: Option<&Value>, gate: Value) -> Value {
    match existing {
        None => gate,
        Some(existing) => and(vec![gate, existing.clone()]),
    }
}

/// Attaches the leading gate to the body's static rule where its zone of
/// function evaluates it: the rule's condition on the battlefield, in a
/// graveyard it declares (CR 113.6) or for a card's own flash permission in
/// hand; each modifier's condition for a spell's own cost reduction.
fn attach_leading_static_gate(
    cursor: &mut OracleTokenCursor<'_>,
    rule: &mut Value,
    leading: &LeadingStaticCondition,
) -> MachineResult<()> {
    cursor.enter("ability-frame.conditional-static.body-kind");
    cursor.require(
        rule["kind"] == "staticAbility" && rule["activeWhile"]["object"] == self_ref(),
        "a static body of the source (continuous effect, permission, replacement, prevention or cost change) that the leading gate governs (CR 611.3a)",
    )?;
    let body_zone = rule["activeWhile"]["zone"]["kind"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    if let Some(zone) = leading.zone_of_function {
        cursor.enter("ability-frame.conditional-static.zone-of-function");
        cursor.require(
            body_zone == "battlefield",
            "a battlefield body whose zone of function the leading condition replaces (CR 113.6)",
        )?;
        rule["activeWhile"] = active_while_static_zone(Some(zone));
    }
    let Some(condition) = &leading.condition else {
        return Ok(());
    };
    let gate = lower_condition(condition);
    cursor.enter("ability-frame.conditional-static.gate");
    let rule_gated = leading.zone_of_function.is_some()
        || body_zone == "battlefield"
        || (body_zone == "hand" && hand_rule_gated_body(rule));
    if rule_gated {
        rule["condition"] = conjoined(rule.get("condition"), gate);
        return Ok(());
    }
    cursor.require(
        body_zone == "stackOrCast" && modifier_gated_body(rule),
        "a body whose zone of function evaluates the leading gate (battlefield, a declared graveyard, a card's own flash permission or its own cost reduction)",
    )?;
    for modifier in rule["modifiers"].as_array_mut().into_iter().flatten() {
        modifier["condition"] = conjoined(modifier.get("condition"), gate.clone());
    }
    Ok(())
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
        AbilityFrame::Keyword => keyword_line_failure(tokens, start),
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
        AbilityFrame::OptionalPermission => optional_permission_frame_failure(input, start),
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

#[cfg(test)]
#[path = "ability_frame_keyword_list_tests.rs"]
mod keyword_list_tests;
