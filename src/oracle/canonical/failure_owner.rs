//! Failure ownership (roadmap S0, diagnostics).
//!
//! A reader that fails at a generic child state (`numeric.atom.start`,
//! `selector.predicate.start`, ...) or at the bare entry of the effect frame
//! (`effect.start`) names no missing concept: those states are sinks that
//! collect every unrelated blocker that happened to read furthest. The
//! dispatcher never reports a sink. It reports the owner instead:
//!
//! - for a clause sink, the deepest clause component reached: the subject
//!   shape, the modality and the lemma (`effect.clause/subject.you.may.verb.pay`,
//!   `effect.clause/verb-unknown.look`, `effect.clause/introducer.when`);
//! - for a child sink, the frame that called the child and the class of the
//!   word the child met (`effect.discard.count/numeric.object-phrase`).
//!
//! The original child state stays in the trace and the original expectation
//! stays the prefix of `expected`. Ownership never changes support: an owned
//! failure is still a failure. Engine gaps (`engine_gap.rs`) are never sinks.
//!
//! Composed states are built from bounded parts (frame states, lexicon lemmas,
//! tokenizer classes and sanitized Oracle words) and interned once.
use super::automata::{
    ALL_LEMMAS, VerbForm, diagnostic_word_key, is_unsupported_rules_verb, lookup_verb,
};
use super::machine::{MachineFailure, MachineTransition, OracleTokenCursor, intern_state};
use crate::oracle::ability_catalog::{AbilityCategory, match_catalog_mechanic};
use crate::oracle::model::RecognizedEntity;

/// The historical sink states (roadmap §1.1). Each one is a child's generic
/// entry or exit state, not a missing concept; none leaves the dispatcher.
pub(crate) const SINK_STATES: [&str; 6] = [
    "effect.start",
    "numeric.atom.start",
    "selector.predicate.start",
    "target-list.complete",
    "event.ordinal-card-drawn.action",
    "reference.source.unresolved-name",
];

/// The frame prefix of every effect-clause owner state.
pub(crate) const EFFECT_CLAUSE_FRAME: &str = "effect.clause";
/// The frame prefix of every trigger-event clause owner state.
pub(crate) const EVENT_CLAUSE_FRAME: &str = "event.clause";

/// How many tokens a subject may span before its finite verb. Oracle subjects
/// with their relative clauses stay well inside this bound.
const SUBJECT_SCAN_LIMIT: usize = 16;

pub(crate) fn is_sink_state(state: &str) -> bool {
    SINK_STATES.contains(&state)
}

/// Replaces a sink state by its owner. Every other failure, including every
/// engine gap and every committed frame failure, is returned unchanged.
pub(super) fn assign_failure_owner(
    failure: MachineFailure,
    tokens: &[RecognizedEntity],
) -> MachineFailure {
    let owner = match failure.state {
        "effect.start" => {
            let head = diagnose_clause_head(tokens, failure.token_index);
            Owner {
                state: head.state(EFFECT_CLAUSE_FRAME),
                component: head.component,
            }
        }
        "event.ordinal-card-drawn.action" => event_verb_owner(tokens, failure.token_index),
        "numeric.atom.start" => child_owner(&failure, tokens, ChildSink::Numeric),
        "selector.predicate.start" => child_owner(&failure, tokens, ChildSink::ObjectPredicate),
        "target-list.complete" => child_owner(&failure, tokens, ChildSink::TargetList),
        "reference.source.unresolved-name" => child_owner(&failure, tokens, ChildSink::SourceName),
        _ => return failure,
    };
    reown(failure, owner)
}

/// The effect-clause diagnostic at `start`, as a failure of `cursor`. A
/// transactional child that returned `NoMatch` hands its clause to this
/// function instead of reporting the requirement it did not meet. The failure
/// points at the deepest component (the verb when one was found).
pub(in crate::oracle::canonical) fn clause_head_failure(
    cursor: &OracleTokenCursor<'_>,
    start: usize,
) -> MachineFailure {
    let tokens = cursor.token_slice(0, usize::MAX);
    let head = diagnose_clause_head(tokens, start);
    let index = head.verb_index.unwrap_or(start);
    let state = head.state(EFFECT_CLAUSE_FRAME);
    let expected = format!(
        "a supported effect clause; deepest component: {}",
        head.component
    );
    let mut at = cursor.clone();
    at.enter(state);
    match tokens.get(index) {
        Some(token) => at.fail_with_span(
            &expected,
            index,
            token.span_start,
            token.span_end,
            token.raw.clone(),
        ),
        None => at.fail(&expected),
    }
}

struct Owner {
    state: &'static str,
    component: String,
}

fn reown(mut failure: MachineFailure, owner: Owner) -> MachineFailure {
    let child_state = failure.state;
    failure.trace.push(MachineTransition {
        state: owner.state,
        token_index: failure.token_index,
    });
    failure.expected = format!(
        "{} (owner {}: {} reached from {child_state})",
        failure.expected, owner.state, owner.component
    );
    failure.state = owner.state;
    failure
}

/// The deepest component of an effect clause starting at a token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ClauseHead {
    /// `introducer.W`, `keyword.K`, `verb.L`, `verb-unknown.W`,
    /// `subject.S[.MODALITY].verb.L`, `subject.S.no-verb`, `head.CLASS`.
    pub component: String,
    /// The token of the finite verb, when the head reached one.
    pub verb_index: Option<usize>,
}

impl ClauseHead {
    fn new(component: impl Into<String>, verb_index: Option<usize>) -> Self {
        Self {
            component: component.into(),
            verb_index,
        }
    }

    pub(crate) fn state(&self, frame: &str) -> &'static str {
        intern_state(format!("{frame}/{}", self.component))
    }
}

/// Reads the head of an effect clause only as far as diagnosis needs: a
/// structural introducer, a keyword, an imperative verb, or a subject shape
/// followed by its modality and finite verb. It never claims the clause.
pub(crate) fn diagnose_clause_head(tokens: &[RecognizedEntity], start: usize) -> ClauseHead {
    let Some(head) = tokens.get(start) else {
        return ClauseHead::new("end-of-input", None);
    };
    let lower = head.raw.to_lowercase();
    if let Some(introducer) = clause_introducer(&lower) {
        return ClauseHead::new(format!("introducer.{introducer}"), None);
    }
    if let Some(mechanic) = match_catalog_mechanic(tokens, start)
        && !matches!(mechanic.category, AbilityCategory::KeywordAction)
    {
        return ClauseHead::new(
            format!("keyword.{}", diagnostic_word_key(mechanic.canonical_id)),
            None,
        );
    }
    if let Some(subject) = subject_class(head) {
        if fused_copula(&lower) {
            return ClauseHead::new(format!("subject.{subject}.verb-unknown.be"), Some(start));
        }
        return match finite_verb_after(tokens, start) {
            Some((index, modality, verb)) => {
                ClauseHead::new(format!("subject.{subject}{modality}.{verb}"), Some(index))
            }
            None => ClauseHead::new(format!("subject.{subject}.no-verb"), None),
        };
    }
    if let Some(verb) = finite_verb(tokens, start) {
        return ClauseHead::new(verb, Some(start));
    }
    if let Some(suffix) = modality(&lower) {
        return ClauseHead::new(format!("modality{suffix}"), None);
    }
    ClauseHead::new(format!("head.{}", token_class(Some(head))), None)
}

fn clause_introducer(lower: &str) -> Option<&'static str> {
    Some(match lower {
        "when" => "when",
        "whenever" => "whenever",
        "at" => "at",
        "if" => "if",
        "unless" => "unless",
        "as" => "as",
        "until" => "until",
        "for" => "for",
        "then" => "then",
        "rather" => "rather",
        "instead" => "instead",
        "otherwise" => "otherwise",
        "after" => "after",
        "before" => "before",
        "during" => "during",
        "once" => "once",
        "while" => "while",
        "where" => "where",
        "except" => "except",
        "only" => "only",
        _ => return None,
    })
}

/// The shape of a clause subject, from its first word.
fn subject_class(token: &RecognizedEntity) -> Option<&'static str> {
    let lower = token.raw.to_lowercase();
    Some(match lower.as_str() {
        "you" => "you",
        "it" | "they" | "he" | "she" | "it's" | "it\u{2019}s" | "they're" | "they\u{2019}re" => {
            "pronoun"
        }
        "target" => "target",
        "each" | "all" | "any" | "another" | "every" | "a" | "an" | "no" => "quantified",
        "the" => "definite",
        "that" | "those" | "this" | "these" | "its" | "their" | "your" | "his" | "her"
        | "that's" | "that\u{2019}s" => "reference",
        "other" | "attacking" | "blocking" | "tapped" | "untapped" | "enchanted" | "equipped" => {
            "object"
        }
        "player" | "players" | "opponent" | "opponents" => "player",
        "up" | "x" => "counted",
        "damage" | "life" => "resource",
        _ => match token.token_type.as_str() {
            "quantity" => "counted",
            "typeWord"
            | "subtypeWord"
            | "supertypeWord"
            | "negativeTypeWord"
            | "negativeSupertypeWord"
            | "negativeCharacteristicWord"
            | "objectCharacteristicWord"
            | "objectWord"
            | "colorWord"
            | "stateWord"
            | "combatStateWord" => "object",
            "possessivePlayerRoleWord" | "possessiveTypeWord" => "possessive",
            _ => return None,
        },
    })
}

/// `it's`, `that's`, `they're`: the subject and the copula in one token.
fn fused_copula(lower: &str) -> bool {
    matches!(
        lower,
        "it's" | "it\u{2019}s" | "that's" | "that\u{2019}s" | "they're" | "they\u{2019}re"
    )
}

/// The modality written between a subject and its verb, as a state suffix.
fn modality(lower: &str) -> Option<&'static str> {
    Some(match lower {
        "may" => ".may",
        "can" => ".can",
        "can't" | "can\u{2019}t" | "cannot" => ".cannot",
        "don't" | "don\u{2019}t" | "doesn't" | "doesn\u{2019}t" | "didn't" | "didn\u{2019}t" => {
            ".negated"
        }
        "must" => ".must",
        "would" => ".would",
        "could" => ".could",
        "will" => ".will",
        _ => return None,
    })
}

/// The first finite verb after a subject head. A candidate right after a
/// determiner, quantity, possessive or relation word is a noun (`a copy`,
/// `two counters`); a relation verb after a player inside the subject is a
/// relative clause (`spells you cast`, `creatures your opponents control`).
fn finite_verb_after(
    tokens: &[RecognizedEntity],
    start: usize,
) -> Option<(usize, &'static str, String)> {
    let mut found_modality = "";
    let end = tokens.len().min(start + 1 + SUBJECT_SCAN_LIMIT);
    for index in start + 1..end {
        let token = &tokens[index];
        if token.token_type == "separator" {
            return None;
        }
        let lower = token.raw.to_lowercase();
        if let Some(suffix) = modality(&lower) {
            found_modality = suffix;
            continue;
        }
        let previous = &tokens[index - 1];
        if blocks_finite_verb(previous) {
            continue;
        }
        if index - 1 > start && relative_clause_verb(previous, &lower) {
            continue;
        }
        if let Some(verb) = finite_verb(tokens, index) {
            return Some((index, found_modality, verb));
        }
    }
    None
}

fn blocks_finite_verb(previous: &RecognizedEntity) -> bool {
    matches!(
        previous.token_type.as_str(),
        "determinerWord"
            | "quantifierWord"
            | "quantity"
            | "counterValue"
            | "ordinalWord"
            | "possessiveTypeWord"
            | "possessivePlayerRoleWord"
            | "targetMarker"
            | "relationWord"
    ) || matches!(
        previous.raw.to_lowercase().as_str(),
        "its"
            | "their"
            | "your"
            | "his"
            | "her"
            | "this"
            | "that"
            | "those"
            | "these"
            | "which"
            | "who"
            | "whose"
            | "than"
            | "mana"
            | "additional"
            | "total"
    )
}

fn relative_clause_verb(previous: &RecognizedEntity, lower: &str) -> bool {
    matches!(
        previous.raw.to_lowercase().as_str(),
        "you" | "they" | "opponent" | "opponents" | "player" | "players"
    ) && matches!(
        lower,
        "cast" | "casts" | "control" | "controls" | "controlled" | "own" | "owns" | "owned"
    )
}

/// The finite verb at `index`: a lexicon lemma in its base or third-person
/// form (`verb.L`), or a word the tokenizer or the rules-verb list marks as a
/// verb without a lexicon entry (`verb-unknown.W`, "lemma W unknown").
fn finite_verb(tokens: &[RecognizedEntity], index: usize) -> Option<String> {
    let token = tokens.get(index)?;
    let lower = token.raw.to_lowercase();
    if let Some((entry, forms)) = lookup_verb(&lower, &ALL_LEMMAS) {
        return forms
            .iter()
            .any(|form| matches!(form, VerbForm::Base | VerbForm::ThirdPersonSingular))
            .then(|| format!("verb.{}", entry.surface(VerbForm::Base)));
    }
    let verb_typed = matches!(
        token.token_type.as_str(),
        "actionWord" | "costActionWord" | "eventWord" | "predicateWord" | "copulaWord"
    );
    let cost_verb = matches!(lower.as_str(), "cost" | "costs")
        && index
            .checked_sub(1)
            .and_then(|previous| tokens.get(previous))
            .is_none_or(|previous| {
                !matches!(
                    previous.raw.to_lowercase().as_str(),
                    "mana" | "additional" | "total" | "its" | "their" | "the" | "a" | "an"
                )
            });
    (verb_typed || cost_verb || is_unsupported_rules_verb(&lower))
        .then(|| format!("verb-unknown.{}", unknown_verb_base(&lower)))
}

/// A bounded base form of a verb without a lexicon entry, so `looks` and
/// `look` share one owner. Irregular copulas and auxiliaries are explicit.
fn unknown_verb_base(lower: &str) -> String {
    let base = match lower {
        "is" | "are" | "was" | "were" | "be" | "been" | "being" => "be".to_string(),
        "has" | "had" | "have" => "have".to_string(),
        "does" | "did" | "do" => "do".to_string(),
        _ => {
            if let Some(stem) = lower.strip_suffix("ies") {
                format!("{stem}y")
            } else if ["ches", "shes", "sses", "xes"]
                .iter()
                .any(|suffix| lower.ends_with(suffix))
            {
                lower[..lower.len() - 2].to_string()
            } else if lower.ends_with('s') && !lower.ends_with("ss") && lower.len() > 3 {
                lower[..lower.len() - 1].to_string()
            } else {
                lower.to_string()
            }
        }
    };
    diagnostic_word_key(&base)
}

/// `you commit a crime`, `you tap a land`: the player event reader read its
/// subject and met a verb it does not observe.
fn event_verb_owner(tokens: &[RecognizedEntity], index: usize) -> Owner {
    let subject = index
        .checked_sub(1)
        .and_then(|previous| tokens.get(previous))
        .and_then(subject_class)
        .unwrap_or("unknown");
    let verb = match tokens.get(index) {
        None => "end-of-input".to_string(),
        Some(token) => finite_verb(tokens, index).unwrap_or_else(|| {
            format!(
                "verb-unknown.{}",
                unknown_verb_base(&token.raw.to_lowercase())
            )
        }),
    };
    let component = format!("subject.{subject}.{verb}");
    Owner {
        state: intern_state(format!("{EVENT_CLAUSE_FRAME}/{component}")),
        component,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChildSink {
    Numeric,
    ObjectPredicate,
    TargetList,
    SourceName,
}

impl ChildSink {
    fn component(self) -> &'static str {
        match self {
            Self::Numeric => "numeric",
            Self::ObjectPredicate => "object-predicate",
            Self::TargetList => "target-list",
            Self::SourceName => "source-name",
        }
    }

    /// State prefixes that belong to the child itself, never to its caller.
    fn family(self) -> &'static [&'static str] {
        match self {
            Self::Numeric => &["numeric.", "arithmetic."],
            Self::ObjectPredicate | Self::TargetList => &["selector.", "target-list."],
            Self::SourceName => &["reference."],
        }
    }
}

/// The frame that called a generic child: the latest state on the failure's
/// path that is neither the child's own state nor a rollback marker.
fn calling_frame(failure: &MachineFailure, sink: ChildSink) -> &'static str {
    failure
        .trace
        .iter()
        .rev()
        .map(|transition| transition.state)
        .find(|state| {
            *state != "rollback"
                && *state != "start"
                && !is_sink_state(state)
                && !sink.family().iter().any(|prefix| state.starts_with(prefix))
        })
        .unwrap_or(failure.automaton)
}

fn child_owner(failure: &MachineFailure, tokens: &[RecognizedEntity], sink: ChildSink) -> Owner {
    let frame = calling_frame(failure, sink);
    let class = match (sink, token_class(tokens.get(failure.token_index))) {
        (ChildSink::SourceName, "capitalized-word") => "unlinked-name",
        (_, class) => class,
    };
    let component = format!("{}.{class}", sink.component());
    Owner {
        state: intern_state(format!("{frame}/{component}")),
        component,
    }
}

/// A bounded word class from the tokenizer's lexical type.
pub(crate) fn token_class(token: Option<&RecognizedEntity>) -> &'static str {
    let Some(token) = token else {
        return "end";
    };
    let lower = token.raw.to_lowercase();
    if matches!(
        lower.as_str(),
        "player" | "players" | "opponent" | "opponents"
    ) {
        return "player-noun";
    }
    match token.token_type.as_str() {
        "determinerWord"
        | "quantifierWord"
        | "referenceWord"
        | "targetMarker"
        | "possessivePlayerRoleWord"
        | "possessiveTypeWord" => "object-phrase",
        "typeWord"
        | "subtypeWord"
        | "supertypeWord"
        | "negativeTypeWord"
        | "negativeSupertypeWord"
        | "negativeCharacteristicWord"
        | "objectCharacteristicWord"
        | "objectWord"
        | "colorWord"
        | "stateWord"
        | "combatStateWord"
        | "attachmentStateWord" => "object-noun",
        "playerQualifierWord" | "playerRoleWord" => "player-noun",
        "numericOperatorWord" | "roundingWord" => "arithmetic",
        "comparisonWord" | "cardinalityWord" => "comparison",
        "logicWord" | "exceptionWord" | "substitutionWord" => "connector",
        "relationWord" | "temporalConnectorWord" => "relation",
        "separator" | "statPairSeparator" => "boundary",
        "manaSymbol" | "tapSymbol" | "counterSymbol" => "symbol",
        "quantity" | "ordinalWord" | "counterValue" | "quantityAnaphorWord" | "quantityNoun" => {
            "quantity"
        }
        "actionWord"
        | "costActionWord"
        | "eventWord"
        | "predicateWord"
        | "copulaWord"
        | "negativePredicateWord" => "verb",
        "modalityWord" | "modalWord" | "replacementModalWord" => "modality",
        "triggerWord" => "trigger-word",
        "zoneWord" | "zonePositionWord" | "zoneScopeWord" => "zone",
        "resourceWord" | "propertyWord" | "colorNoun" | "identityWord" => "property",
        "repetitionUnitWord"
        | "turnStepWord"
        | "durationBoundaryWord"
        | "durationPointWord"
        | "temporalMarkerWord" => "time",
        "keywordWord" | "abilityWord" => "keyword",
        _ if token
            .raw
            .chars()
            .next()
            .is_some_and(|character| character.is_uppercase()) =>
        {
            "capitalized-word"
        }
        _ => "word",
    }
}

#[cfg(test)]
#[path = "failure_owner_tests.rs"]
mod tests;
