use std::fmt;

use crate::oracle::model::RecognizedEntity;

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
#[derive(Clone, Debug)]
pub(super) struct OracleTokenCursor<'a> {
    automaton: &'static str,
    state: &'static str,
    tokens: &'a [RecognizedEntity],
    position: usize,
    transitions: Vec<MachineTransition>,
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
        }
    }

    pub fn enter(&mut self, state: &'static str) {
        self.state = state;
        self.transitions.push(MachineTransition {
            state,
            token_index: self.position,
        });
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn checkpoint(&self) -> MachineCheckpoint {
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
        self.position += 1;
        Ok(token)
    }

    pub fn advance_to(&mut self, end: usize) -> MachineResult<()> {
        if end < self.position || end > self.tokens.len() {
            return Err(self.fail("a valid forward token range"));
        }
        self.position = end;
        Ok(())
    }

    pub fn finish(&self) -> MachineResult<()> {
        if self.position == self.tokens.len() {
            Ok(())
        } else {
            Err(self.failure("end of input"))
        }
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
