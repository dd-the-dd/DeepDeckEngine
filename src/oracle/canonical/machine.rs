use std::fmt;

#[cfg(feature = "automata-trace")]
use crate::oracle::automata_trace::{self, AutomataTraceEventKind, CursorTraceHandle};
use crate::oracle::model::RecognizedEntity;

use super::automata::{BindingEnvironment, EventNumericBinding};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MachineTransition {
    pub state: &'static str,
    pub token_index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MachineCheckpoint {
    position: usize,
    state: &'static str,
    transition_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MachineFailure {
    pub automaton: &'static str,
    pub state: &'static str,
    pub token_index: usize,
    pub span_start: usize,
    pub span_end: usize,
    pub expected: String,
    pub actual: Option<String>,
    pub trace: Vec<MachineTransition>,
}

impl fmt::Display for MachineFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} stopped in {} at token {} [{}..{}]: expected {}, got {}",
            self.automaton,
            self.state,
            self.token_index,
            self.span_start,
            self.span_end,
            self.expected,
            self.actual.as_deref().unwrap_or("end of input")
        )
    }
}

pub(super) type MachineResult<T> = Result<T, MachineFailure>;

/// Token-driven cursor shared by the canonical sub-automata.
///
/// The pipeline tokenizes an Oracle ability exactly once. Local recognizers
/// borrow that immutable stream and move only their own transactional cursor.
#[cfg_attr(not(feature = "automata-trace"), derive(Clone))]
#[derive(Debug)]
pub(super) struct OracleTokenCursor<'a> {
    automaton: &'static str,
    state: &'static str,
    tokens: &'a [RecognizedEntity],
    position: usize,
    transitions: Vec<MachineTransition>,
    /// Quantity the enclosing triggered ability's event exports to its body
    /// (`that many`/`that much`). The parent installs it; no child infers it.
    event_numeric_binding: Option<EventNumericBinding>,
    /// The references the enclosing ability frame and program position bind
    /// (`binding_environment.rs`). The owning parent installs it for the
    /// child it calls; a child never infers or extends it.
    binding_environment: Option<std::sync::Arc<BindingEnvironment>>,
    #[cfg(feature = "automata-trace")]
    trace_cursor: Option<CursorTraceHandle>,
}

#[cfg(feature = "automata-trace")]
impl Clone for OracleTokenCursor<'_> {
    fn clone(&self) -> Self {
        Self {
            automaton: self.automaton,
            state: self.state,
            tokens: self.tokens,
            position: self.position,
            transitions: self.transitions.clone(),
            event_numeric_binding: self.event_numeric_binding,
            binding_environment: self.binding_environment.clone(),
            trace_cursor: automata_trace::new_cursor(
                self.trace_cursor,
                self.automaton,
                self.state,
                self.position,
                self.trace_span(self.position),
                self.peek(),
                AutomataTraceEventKind::Clone,
            ),
        }
    }
}

impl<'a> OracleTokenCursor<'a> {
    pub fn new(automaton: &'static str, tokens: &'a [RecognizedEntity]) -> Self {
        Self {
            automaton,
            state: "start",
            tokens,
            position: 0,
            transitions: vec![MachineTransition {
                state: "start",
                token_index: 0,
            }],
            event_numeric_binding: None,
            binding_environment: None,
            #[cfg(feature = "automata-trace")]
            trace_cursor: automata_trace::new_cursor(
                None,
                automaton,
                "start",
                0,
                tokens
                    .first()
                    .map(|token| (token.span_start, token.span_end)),
                tokens.first().map(|token| token.raw.as_str()),
                AutomataTraceEventKind::New,
            ),
        }
    }

    #[cfg(feature = "automata-trace")]
    fn trace_span(&self, position: usize) -> Option<(usize, usize)> {
        self.tokens
            .get(position)
            .map(|token| (token.span_start, token.span_end))
            .or_else(|| {
                self.tokens
                    .last()
                    .map(|token| (token.span_end, token.span_end))
            })
    }

    #[cfg(feature = "automata-trace")]
    fn record_trace(&self, kind: AutomataTraceEventKind, position: usize) {
        if self.trace_cursor.is_some() {
            automata_trace::record(
                self.trace_cursor,
                self.automaton,
                self.state,
                position,
                self.trace_span(position),
                self.tokens.get(position).map(|token| token.raw.as_str()),
                kind,
            );
        }
    }

    /// The same reading state over another view of the same tokens (equal
    /// length, indices and spans), such as a predicate read in another
    /// grammatical person.
    pub fn with_tokens<'b>(&self, tokens: &'b [RecognizedEntity]) -> OracleTokenCursor<'b> {
        OracleTokenCursor {
            automaton: self.automaton,
            state: self.state,
            tokens,
            position: self.position,
            transitions: self.transitions.clone(),
            event_numeric_binding: self.event_numeric_binding,
            binding_environment: self.binding_environment.clone(),
            #[cfg(feature = "automata-trace")]
            trace_cursor: None,
        }
    }

    /// Moves back to an earlier position of the same tokens (a parent that
    /// re-reads a word it already examined, such as a subject's last word).
    pub fn rollback_to_position(&mut self, position: usize) {
        self.position = position.min(self.position);
    }

    pub fn set_event_numeric_binding(&mut self, binding: Option<EventNumericBinding>) {
        self.event_numeric_binding = binding;
    }

    pub fn event_numeric_binding(&self) -> Option<EventNumericBinding> {
        self.event_numeric_binding
    }

    /// Installs the binding environment for the children read next and
    /// returns the previous one, which the installing parent restores.
    pub fn replace_binding_environment(
        &mut self,
        environment: Option<std::sync::Arc<BindingEnvironment>>,
    ) -> Option<std::sync::Arc<BindingEnvironment>> {
        std::mem::replace(&mut self.binding_environment, environment)
    }

    pub fn binding_environment(&self) -> Option<&BindingEnvironment> {
        self.binding_environment.as_deref()
    }

    pub fn enter(&mut self, state: &'static str) {
        self.state = state;
        self.transitions.push(MachineTransition {
            state,
            token_index: self.position,
        });
        #[cfg(feature = "automata-trace")]
        self.record_trace(AutomataTraceEventKind::Enter, self.position);
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn checkpoint(&self) -> MachineCheckpoint {
        #[cfg(feature = "automata-trace")]
        self.record_trace(AutomataTraceEventKind::Checkpoint, self.position);
        MachineCheckpoint {
            position: self.position,
            state: self.state,
            transition_count: self.transitions.len(),
        }
    }

    pub fn rollback(&mut self, checkpoint: MachineCheckpoint) {
        self.position = checkpoint.position.min(self.tokens.len());
        self.state = checkpoint.state;
        self.transitions.truncate(checkpoint.transition_count);
        self.transitions.push(MachineTransition {
            state: "rollback",
            token_index: self.position,
        });
        #[cfg(feature = "automata-trace")]
        self.record_trace(AutomataTraceEventKind::Rollback, self.position);
    }

    pub fn attempt<T>(
        &mut self,
        recognizer: impl FnOnce(&mut Self) -> MachineResult<T>,
    ) -> MachineResult<T> {
        let checkpoint = self.checkpoint();
        match recognizer(self) {
            Ok(value) => Ok(value),
            Err(failure) => {
                self.rollback(checkpoint);
                Err(failure)
            }
        }
    }

    pub fn peek_token(&self) -> Option<&RecognizedEntity> {
        self.tokens.get(self.position)
    }

    pub fn peek(&self) -> Option<&str> {
        self.peek_token().map(|token| token.raw.as_str())
    }

    pub fn peek_ahead(&self, offset: usize) -> Option<&str> {
        self.tokens
            .get(self.position.saturating_add(offset))
            .map(|token| token.raw.as_str())
    }

    pub fn consume(&mut self, expected: &str) -> MachineResult<()> {
        if self
            .peek()
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
        {
            #[cfg(feature = "automata-trace")]
            self.record_trace(AutomataTraceEventKind::Consume, self.position);
            self.position += 1;
            return Ok(());
        }
        Err(self.failure(expected))
    }

    pub fn consume_phrase(&mut self, expected: &[&str]) -> MachineResult<()> {
        self.attempt(|cursor| {
            for token in expected {
                cursor.consume(token)?;
            }
            Ok(())
        })
    }

    pub fn consume_type(&mut self, expected: &str) -> MachineResult<&'a RecognizedEntity> {
        let Some(token) = self.tokens.get(self.position) else {
            return Err(self.fail(expected));
        };
        if token.token_type != expected {
            return Err(self.fail(expected));
        }
        #[cfg(feature = "automata-trace")]
        self.record_trace(AutomataTraceEventKind::Consume, self.position);
        self.position += 1;
        Ok(token)
    }

    pub fn advance_to(&mut self, end: usize) -> MachineResult<()> {
        if end < self.position || end > self.tokens.len() {
            return Err(self.fail("a valid forward token range"));
        }
        self.position = end;
        #[cfg(feature = "automata-trace")]
        self.record_trace(AutomataTraceEventKind::Advance, self.position);
        Ok(())
    }

    pub fn finish(&self) -> MachineResult<()> {
        if self.position == self.tokens.len() {
            #[cfg(feature = "automata-trace")]
            self.record_trace(AutomataTraceEventKind::Finish, self.position);
            Ok(())
        } else {
            Err(self.failure("end of input"))
        }
    }

    /// Run a transactional child over a bounded view of the original tokens.
    /// Indices/spans stay absolute; no text or lexical entities are recreated.
    pub fn bounded<T>(
        &mut self,
        end: usize,
        child: impl FnOnce(&mut Self) -> MachineResult<T>,
    ) -> MachineResult<T> {
        self.require(
            end >= self.position && end <= self.tokens.len(),
            "a valid bounded child token range",
        )?;
        let mut trial = self.clone();
        trial.tokens = &self.tokens[..end];
        let value = child(&mut trial)?;
        trial.finish()?;
        trial.tokens = self.tokens;
        *self = trial;
        Ok(value)
    }

    pub fn require(&self, condition: bool, expected: &str) -> MachineResult<()> {
        condition
            .then_some(())
            .ok_or_else(|| self.failure(expected))
    }

    pub fn require_value<T>(&self, value: Option<T>, expected: &str) -> MachineResult<T> {
        value.ok_or_else(|| self.failure(expected))
    }

    pub fn transitions(&self) -> &[MachineTransition] {
        &self.transitions
    }

    pub fn token_slice(&self, start: usize, end: usize) -> &'a [RecognizedEntity] {
        &self.tokens[start.min(self.tokens.len())..end.min(self.tokens.len())]
    }

    pub fn fail(&self, expected: &str) -> MachineFailure {
        self.failure(expected)
    }

    pub fn fail_with_span(
        &self,
        expected: &str,
        token_index: usize,
        span_start: usize,
        span_end: usize,
        found: impl Into<String>,
    ) -> MachineFailure {
        #[cfg(feature = "automata-trace")]
        automata_trace::record(
            self.trace_cursor,
            self.automaton,
            self.state,
            token_index,
            Some((span_start, span_end)),
            self.tokens.get(token_index).map(|token| token.raw.as_str()),
            AutomataTraceEventKind::Failure,
        );
        MachineFailure {
            automaton: self.automaton,
            state: self.state,
            token_index,
            span_start,
            span_end,
            expected: expected.to_string(),
            actual: Some(found.into()),
            trace: self.transitions.clone(),
        }
    }

    fn failure(&self, expected: &str) -> MachineFailure {
        let (span_start, span_end) = self
            .peek_token()
            .map(|token| (token.span_start, token.span_end))
            .unwrap_or_else(|| {
                let end = self.tokens.last().map_or(0, |token| token.span_end);
                (end, end)
            });
        #[cfg(feature = "automata-trace")]
        automata_trace::record(
            self.trace_cursor,
            self.automaton,
            self.state,
            self.position,
            Some((span_start, span_end)),
            self.peek(),
            AutomataTraceEventKind::Failure,
        );
        MachineFailure {
            automaton: self.automaton,
            state: self.state,
            token_index: self.position,
            span_start,
            span_end,
            expected: expected.to_string(),
            actual: self.peek().map(ToOwned::to_owned),
            trace: self.transitions.clone(),
        }
    }
}

#[cfg(all(test, feature = "automata-trace"))]
#[path = "machine_trace_tests.rs"]
mod trace_tests;
