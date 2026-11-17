use std::collections::BTreeMap;

use serde::Serialize;

use super::super::model::RecognizedEntity;
use super::recognize_entities;

const MAX_NAMED_CARD_TOKENS: usize = 20;
const MAX_TRACE_TRANSITIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NameLinkKind {
    CurrentCard,
    CurrentFace,
    NamedCard,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResolvedNameLink {
    pub kind: NameLinkKind,
    pub canonical_name: String,
    pub raw: String,
    pub token_start: usize,
    pub token_end: usize,
    pub span_start: usize,
    pub span_end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UnresolvedName {
    pub raw: String,
    pub token_start: usize,
    pub token_end: usize,
    pub span_start: usize,
    pub span_end: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum NameResolverState {
    ScanUnknown,
    TryContextAlias,
    TryNamedCard,
    Rollback,
    CommitLongest,
    ReportUnresolved,
    Complete,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NameResolverMemory {
    pub position: usize,
    pub checkpoint: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias_tried: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub best_link: Option<ResolvedNameLink>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NameResolverTransition {
    pub state: NameResolverState,
    pub memory: NameResolverMemory,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NameResolution {
    pub state: NameResolverState,
    pub resolved: Vec<ResolvedNameLink>,
    pub trace: Vec<NameResolverTransition>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NameResolutionFailure {
    pub automaton: &'static str,
    pub state: &'static str,
    pub current_state: NameResolverState,
    pub resolved: Vec<ResolvedNameLink>,
    pub unresolved: Vec<UnresolvedName>,
    pub memory: NameResolverMemory,
    pub trace: Vec<NameResolverTransition>,
}

/// Exact external name index used by the name sub-automaton.
///
/// Implementations should build or load their index once, outside a parse.
/// The resolver never owns or reloads a card catalog and never performs fuzzy
/// matching.
pub(crate) trait NamedCardIndex {
    /// Returns every exact card-name end beginning at `start`, in increasing
    /// order. The resolver performs the global longest-match selection.
    ///
    /// The resolver calls this at most once per input token. Implementations
    /// must compare token sequences exactly; fuzzy matching is forbidden.
    fn exact_name_ends(
        &self,
        tokens: &[RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize>;
}

/// Exact token trie shared by every ability parsed for the same card/request.
///
/// Building the contextual alias index once prevents card and transform-face
/// names from being tokenized again for each ability.
pub(crate) struct NameResolutionContext {
    aliases: Vec<Alias>,
}

impl NameResolutionContext {
    pub(crate) fn new<'a>(
        current_card_name: &str,
        face_names: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let mut aliases = Vec::new();
        push_name_aliases(&mut aliases, NameLinkKind::CurrentCard, current_card_name);
        for face_name in face_names {
            if face_name.eq_ignore_ascii_case(current_card_name) {
                continue;
            }
            push_name_aliases(&mut aliases, NameLinkKind::CurrentFace, face_name);
        }
        aliases.sort_by(|left, right| right.tokens.len().cmp(&left.tokens.len()));
        Self { aliases }
    }
}

#[derive(Default)]
struct ExactNameNode {
    children: BTreeMap<String, usize>,
    terminal: bool,
}

/// Token-keyed exact-name index. Construction may tokenize catalog names, but
/// lookup only walks the already-tokenized Oracle ability.
#[derive(Default)]
pub(crate) struct ExactNameTokenIndex {
    nodes: Vec<ExactNameNode>,
}

impl ExactNameTokenIndex {
    pub(crate) fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Self {
        let mut index = Self {
            nodes: vec![ExactNameNode::default()],
        };
        for name in names {
            let name_tokens = recognize_entities(name);
            if name_tokens.is_empty() {
                continue;
            }
            let mut node_index = 0;
            for token in name_tokens {
                let key = token.raw.to_lowercase();
                let next = if let Some(next) = index.nodes[node_index].children.get(&key) {
                    *next
                } else {
                    let next = index.nodes.len();
                    index.nodes.push(ExactNameNode::default());
                    index.nodes[node_index].children.insert(key, next);
                    next
                };
                node_index = next;
            }
            index.nodes[node_index].terminal = true;
        }
        index
    }
}

impl NamedCardIndex for ExactNameTokenIndex {
    fn exact_name_ends(
        &self,
        tokens: &[RecognizedEntity],
        start: usize,
        maximum_end: usize,
    ) -> Vec<usize> {
        if self.nodes.is_empty() || start >= maximum_end || maximum_end > tokens.len() {
            return Vec::new();
        }
        let mut node_index = 0;
        let mut matches = Vec::new();
        for (offset, token) in tokens[start..maximum_end].iter().enumerate() {
            let key = token.raw.to_lowercase();
            let Some(next) = self.nodes[node_index].children.get(&key) else {
                break;
            };
            node_index = *next;
            if self.nodes[node_index].terminal {
                matches.push(start + offset + 1);
            }
        }
        matches
    }
}

#[derive(Clone)]
struct Alias {
    kind: NameLinkKind,
    canonical_name: String,
    tokens: Vec<String>,
}

#[derive(Clone)]
struct Candidate {
    link: ResolvedNameLink,
    token_count: usize,
}

#[derive(Default)]
struct TraceRecorder {
    memory: NameResolverMemory,
    transitions: Vec<NameResolverTransition>,
}

impl TraceRecorder {
    fn enter(&mut self, state: NameResolverState) {
        let transition = NameResolverTransition {
            state,
            memory: self.memory.clone(),
        };
        if self.transitions.len() < MAX_TRACE_TRANSITIONS {
            self.transitions.push(transition);
        } else if matches!(
            state,
            NameResolverState::CommitLongest
                | NameResolverState::ReportUnresolved
                | NameResolverState::Complete
        ) && let Some(last) = self.transitions.last_mut()
        {
            *last = transition;
        }
    }

    fn begin_attempt(
        &mut self,
        state: NameResolverState,
        checkpoint: usize,
        end: usize,
        candidate: &str,
        alias_tried: Option<&str>,
    ) {
        self.memory.checkpoint = checkpoint;
        self.memory.position = end;
        self.memory.candidate = Some(candidate.to_string());
        self.memory.alias_tried = alias_tried.map(ToOwned::to_owned);
        self.enter(state);
    }

    fn reject_and_rollback(&mut self) {
        self.memory.position = self.memory.checkpoint;
        self.enter(NameResolverState::Rollback);
    }

    fn accept_candidate(&mut self, candidate: &Candidate) {
        let is_better = self.memory.best_link.as_ref().is_none_or(|best| {
            candidate.token_count > best.token_end.saturating_sub(best.token_start)
        });
        if is_better {
            self.memory.best_link = Some(candidate.link.clone());
        }
    }
}

/// Offers exact contextual card/face aliases independently of lexical class,
/// and resolves remaining UNKNOWN ranges through the bounded external index.
///
/// Links are side annotations, not token replacements: a typed source-reference
/// consumer may select the contextual alias while an object query still reads
/// the same lexical noun as a subtype. Longest non-overlapping matches, original
/// spans and contextual possessives are preserved. External catalog candidates
/// still require UNKNOWN evidence; unrelated unknowns remain structured errors.
pub(crate) fn resolve_unknown_names(
    source_text: &str,
    tokens: &[RecognizedEntity],
    context: &NameResolutionContext,
    named_cards: &dyn NamedCardIndex,
) -> Result<NameResolution, NameResolutionFailure> {
    let mut trace = TraceRecorder::default();
    trace.enter(NameResolverState::ScanUnknown);
    let mut candidates = contextual_candidates(source_text, tokens, context, &mut trace);
    let mut reserved = vec![false; tokens.len()];
    for candidate in &candidates {
        reserved[candidate.link.token_start..candidate.link.token_end].fill(true);
    }
    candidates.extend(named_card_candidates(
        source_text,
        tokens,
        named_cards,
        &reserved,
        &mut trace,
    ));
    candidates.sort_by(|left, right| {
        right
            .token_count
            .cmp(&left.token_count)
            .then_with(|| kind_priority(right.link.kind).cmp(&kind_priority(left.link.kind)))
            .then_with(|| left.link.token_start.cmp(&right.link.token_start))
    });

    let mut linked = vec![false; tokens.len()];
    let mut resolved = Vec::new();
    for candidate in candidates {
        if linked[candidate.link.token_start..candidate.link.token_end]
            .iter()
            .any(|is_linked| *is_linked)
        {
            continue;
        }
        linked[candidate.link.token_start..candidate.link.token_end].fill(true);
        trace.memory.position = candidate.link.token_end;
        trace.memory.checkpoint = candidate.link.token_start;
        trace.memory.candidate = Some(candidate.link.raw.clone());
        trace.memory.alias_tried = Some(candidate.link.canonical_name.clone());
        trace.memory.best_link = Some(candidate.link.clone());
        trace.enter(NameResolverState::CommitLongest);
        resolved.push(candidate.link);
    }
    resolved.sort_by_key(|link| link.token_start);

    let unresolved = expand_unresolved_context_ranges(
        source_text,
        tokens,
        &linked,
        context,
        unresolved_groups(source_text, tokens, &linked),
    );
    if unresolved.is_empty() {
        trace.memory.position = tokens.len();
        trace.memory.checkpoint = tokens.len();
        trace.memory.candidate = None;
        trace.memory.alias_tried = None;
        trace.enter(NameResolverState::Complete);
        Ok(NameResolution {
            state: NameResolverState::Complete,
            resolved,
            trace: trace.transitions,
        })
    } else {
        trace.memory.position = unresolved[0].token_start;
        trace.memory.checkpoint = unresolved[0].token_start;
        trace.memory.candidate = Some(unresolved[0].raw.clone());
        trace.memory.alias_tried = None;
        trace.enter(NameResolverState::ReportUnresolved);
        Err(NameResolutionFailure {
            automaton: "lexical-name-resolver",
            state: "unresolved-name",
            current_state: NameResolverState::ReportUnresolved,
            resolved,
            unresolved,
            memory: trace.memory,
            trace: trace.transitions,
        })
    }
}

fn contextual_candidates(
    source_text: &str,
    tokens: &[RecognizedEntity],
    context: &NameResolutionContext,
    trace: &mut TraceRecorder,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for alias in &context.aliases {
        if alias.tokens.is_empty() || alias.tokens.len() > tokens.len() {
            continue;
        }
        for start in 0..=(tokens.len() - alias.tokens.len()) {
            let end = start + alias.tokens.len();
            let matched = contextual_tokens_match(&tokens[start..end], &alias.tokens);
            // Known nonmatches need no diagnostic attempt or trace allocation.
            // UNKNOWN failed attempts retain their existing evidence/rollback.
            if matched.is_none() && !contains_unknown(&tokens[start..end]) {
                continue;
            }
            let raw = source_slice(source_text, tokens, start, end);
            trace.begin_attempt(
                NameResolverState::TryContextAlias,
                start,
                end,
                raw,
                Some(&alias.canonical_name),
            );
            let Some(possessive_suffix_len) = matched else {
                trace.reject_and_rollback();
                continue;
            };
            let link = if possessive_suffix_len == 0 {
                link_from_range(
                    source_text,
                    tokens,
                    start,
                    end,
                    alias.kind,
                    alias.canonical_name.clone(),
                )
            } else {
                link_from_possessive_range(
                    source_text,
                    tokens,
                    start,
                    end,
                    possessive_suffix_len,
                    alias.kind,
                    alias.canonical_name.clone(),
                )
            };
            let candidate = Candidate {
                link,
                token_count: alias.tokens.len(),
            };
            trace.accept_candidate(&candidate);
            candidates.push(candidate);
        }
    }
    candidates
}

fn named_card_candidates(
    source_text: &str,
    tokens: &[RecognizedEntity],
    named_cards: &dyn NamedCardIndex,
    reserved: &[bool],
    trace: &mut TraceRecorder,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    for start in 0..tokens.len() {
        if reserved[start] {
            continue;
        }
        let hard_limit = tokens.len().min(start + MAX_NAMED_CARD_TOKENS);
        let maximum_end = (start..hard_limit)
            .find(|position| {
                reserved[*position]
                    || matches!(tokens[*position].raw.as_str(), "." | ";" | "\u{2022}")
            })
            .unwrap_or(hard_limit);
        if start == maximum_end {
            continue;
        }
        if !contains_unknown(&tokens[start..maximum_end]) {
            continue;
        }
        trace.begin_attempt(
            NameResolverState::TryNamedCard,
            start,
            maximum_end,
            source_slice(source_text, tokens, start, maximum_end),
            None,
        );
        let exact_ends = named_cards.exact_name_ends(tokens, start, maximum_end);
        if exact_ends.is_empty() {
            trace.reject_and_rollback();
            continue;
        }
        let mut accepted = false;
        for end in exact_ends {
            if !contains_unknown(&tokens[start..end]) || crosses_hard_boundary(&tokens[start..end])
            {
                continue;
            }
            let canonical_name = source_slice(source_text, tokens, start, end)
                .trim()
                .to_string();
            let candidate = Candidate {
                link: link_from_range(
                    source_text,
                    tokens,
                    start,
                    end,
                    NameLinkKind::NamedCard,
                    canonical_name,
                ),
                token_count: end - start,
            };
            trace.accept_candidate(&candidate);
            candidates.push(candidate);
            accepted = true;
        }
        if !accepted {
            trace.reject_and_rollback();
        }
    }
    candidates
}

fn push_name_aliases(aliases: &mut Vec<Alias>, kind: NameLinkKind, canonical_name: &str) {
    push_alias(aliases, kind, canonical_name, canonical_name);
    if let Some((short_name, _)) = canonical_name.split_once(',') {
        let short_name = short_name.trim();
        if !short_name.is_empty() {
            push_alias(aliases, kind, canonical_name, short_name);
        }
    }
}

fn push_alias(aliases: &mut Vec<Alias>, kind: NameLinkKind, canonical_name: &str, alias: &str) {
    let tokens = recognize_entities(alias)
        .into_iter()
        .map(|token| token.raw.to_lowercase())
        .collect::<Vec<_>>();
    if aliases.iter().any(|existing| {
        existing.kind == kind
            && existing.canonical_name.eq_ignore_ascii_case(canonical_name)
            && existing.tokens == tokens
    }) {
        return;
    }
    aliases.push(Alias {
        kind,
        canonical_name: canonical_name.to_string(),
        tokens,
    });
}

fn tokens_match(actual: &[RecognizedEntity], expected: &[String]) -> bool {
    actual
        .iter()
        .zip(expected)
        .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
}

/// Matches an exact contextual card or face name, optionally followed by the
/// Oracle possessive suffix on the final token. The returned byte length is
/// the suffix width to exclude from the source link. Catalog names deliberately
/// do not use this helper: a possessive is only a safe self-reference when the
/// name is already established by the current-card context.
fn contextual_tokens_match(actual: &[RecognizedEntity], expected: &[String]) -> Option<usize> {
    if actual.len() != expected.len() || actual.is_empty() {
        return None;
    }
    let last = actual.len() - 1;
    if !actual[..last]
        .iter()
        .zip(&expected[..last])
        .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected))
    {
        return None;
    }
    if actual[last].raw.eq_ignore_ascii_case(&expected[last]) {
        return Some(0);
    }
    let (stem, suffix_len) = strip_oracle_possessive(&actual[last].raw)?;
    stem.eq_ignore_ascii_case(&expected[last])
        .then_some(suffix_len)
}

fn strip_oracle_possessive(raw: &str) -> Option<(&str, usize)> {
    ["'s", "'S", "’s", "’S"]
        .into_iter()
        .find_map(|suffix| raw.strip_suffix(suffix).map(|stem| (stem, suffix.len())))
}

fn contains_unknown(tokens: &[RecognizedEntity]) -> bool {
    tokens.iter().any(is_unknown)
}

fn is_unknown(token: &RecognizedEntity) -> bool {
    token.category == "unknown" || token.token_type == "unknown"
}

fn crosses_hard_boundary(tokens: &[RecognizedEntity]) -> bool {
    tokens
        .iter()
        .any(|token| matches!(token.raw.as_str(), "." | ";" | "\u{2022}"))
}

fn kind_priority(kind: NameLinkKind) -> usize {
    match kind {
        NameLinkKind::CurrentFace => 3,
        NameLinkKind::CurrentCard => 2,
        NameLinkKind::NamedCard => 1,
    }
}

fn link_from_range(
    source_text: &str,
    tokens: &[RecognizedEntity],
    start: usize,
    end: usize,
    kind: NameLinkKind,
    canonical_name: String,
) -> ResolvedNameLink {
    let span_start = tokens[start].span_start;
    let span_end = tokens[end - 1].span_end;
    ResolvedNameLink {
        kind,
        canonical_name,
        raw: source_text[span_start..span_end].to_string(),
        token_start: start,
        token_end: end,
        span_start,
        span_end,
    }
}

fn link_from_possessive_range(
    source_text: &str,
    tokens: &[RecognizedEntity],
    start: usize,
    end: usize,
    possessive_suffix_len: usize,
    kind: NameLinkKind,
    canonical_name: String,
) -> ResolvedNameLink {
    let span_start = tokens[start].span_start;
    let span_end = tokens[end - 1].span_end - possessive_suffix_len;
    ResolvedNameLink {
        kind,
        canonical_name,
        raw: source_text[span_start..span_end].to_string(),
        token_start: start,
        token_end: end,
        span_start,
        span_end,
    }
}

fn source_slice<'a>(
    source_text: &'a str,
    tokens: &[RecognizedEntity],
    start: usize,
    end: usize,
) -> &'a str {
    &source_text[tokens[start].span_start..tokens[end - 1].span_end]
}

fn unresolved_groups(
    source_text: &str,
    tokens: &[RecognizedEntity],
    linked: &[bool],
) -> Vec<UnresolvedName> {
    let mut failures = Vec::new();
    let mut position = 0;
    while position < tokens.len() {
        if linked[position] || !is_unknown(&tokens[position]) {
            position += 1;
            continue;
        }
        let start = position;
        position += 1;
        while position < tokens.len() && !linked[position] && is_unknown(&tokens[position]) {
            position += 1;
        }
        let end = position;
        let span_start = tokens[start].span_start;
        let span_end = tokens[end - 1].span_end;
        failures.push(UnresolvedName {
            raw: source_text[span_start..span_end].to_string(),
            token_start: start,
            token_end: end,
            span_start,
            span_end,
        });
    }
    failures
}

/// Expands an UNKNOWN-only failure to the exact-width contextual-name range
/// that was attempted around it, without ever accepting that range as a
/// match. This matters when ordinary Oracle vocabulary occurs inside a name:
/// in `Other Spark`, for example, `Other` is classified vocabulary while
/// `Spark` is UNKNOWN. If the current card is `Test Spark`, the failed
/// two-token context-name attempt must be reported as `Other Spark`; otherwise
/// a semantic recognizer can miss that the source position is unresolved and
/// incorrectly fall through to a legacy whole-text parser.
///
/// Requiring the attempted range to end on UNKNOWN keeps a shorter partial
/// name such as `Lightning deals ...` from swallowing the following action
/// merely because the current card name has two tokens. Exact links always win
/// because ranges containing already-linked tokens are excluded.
fn expand_unresolved_context_ranges(
    source_text: &str,
    tokens: &[RecognizedEntity],
    linked: &[bool],
    context: &NameResolutionContext,
    unresolved: Vec<UnresolvedName>,
) -> Vec<UnresolvedName> {
    let mut expanded = Vec::new();
    for failure in unresolved {
        let mut best = (failure.token_start, failure.token_end);
        for alias in &context.aliases {
            let width = alias.tokens.len();
            if width == 0 || width > tokens.len() {
                continue;
            }
            let minimum_start = failure.token_end.saturating_sub(width);
            let maximum_start = failure.token_start.min(tokens.len() - width);
            if minimum_start > maximum_start {
                continue;
            }
            for start in minimum_start..=maximum_start {
                let end = start + width;
                let candidate = &tokens[start..end];
                let unknown_tokens_match_alias = candidate
                    .iter()
                    .zip(&alias.tokens)
                    .filter(|(token, _)| is_unknown(token))
                    .all(|(token, expected)| token.raw.eq_ignore_ascii_case(expected));
                if start > failure.token_start
                    || end < failure.token_end
                    || linked[start..end].iter().any(|is_linked| *is_linked)
                    || !is_unknown(&tokens[end - 1])
                    || !unknown_tokens_match_alias
                    || tokens_match(candidate, &alias.tokens)
                {
                    continue;
                }
                let candidate_width = end - start;
                let best_width = best.1 - best.0;
                if candidate_width > best_width || (candidate_width == best_width && start < best.0)
                {
                    best = (start, end);
                }
            }
        }

        let span_start = tokens[best.0].span_start;
        let span_end = tokens[best.1 - 1].span_end;
        let expanded_failure = UnresolvedName {
            raw: source_text[span_start..span_end].to_string(),
            token_start: best.0,
            token_end: best.1,
            span_start,
            span_end,
        };
        if !expanded.iter().any(|existing: &UnresolvedName| {
            existing.token_start == expanded_failure.token_start
                && existing.token_end == expanded_failure.token_end
        }) {
            expanded.push(expanded_failure);
        }
    }
    expanded.sort_by_key(|failure| failure.token_start);
    expanded
}
