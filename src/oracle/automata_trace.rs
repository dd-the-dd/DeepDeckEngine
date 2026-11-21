//! Opt-in observations of real OracleTokenCursor operations only.
//! Lexical/name recognizers outside this cursor and reporting projections are
//! deliberately absent. All spans are half-open UTF-8 bytes in ability text.
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

use serde::Serialize;

use super::{OracleCardParseRequest, OracleCardParseResult};

const EVENT_LIMIT: usize = 20_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AutomataTraceEventKind {
    New,
    Clone,
    Enter,
    Checkpoint,
    Rollback,
    Consume,
    Advance,
    Finish,
    Failure,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomataTraceEvent {
    pub cursor_id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_cursor_id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ability_index: Option<usize>,
    pub automaton: &'static str,
    pub state: &'static str,
    /// Local to this cursor, including cursors over token subslices.
    pub token_index: usize,
    pub kind: AutomataTraceEventKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_start: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span_end: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_text: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomataTrace {
    pub events: Vec<AutomataTraceEvent>,
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TracedOracleCardParseResult {
    pub result: OracleCardParseResult,
    pub trace: AutomataTrace,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CursorTraceHandle {
    session_id: u64,
    cursor_id: u64,
}

struct Collector {
    session_id: u64,
    next_cursor_id: u64,
    ability_index: Option<usize>,
    trace: AutomataTrace,
}
impl Collector {
    fn push(&mut self, event: AutomataTraceEvent) {
        if self.trace.events.len() < EVENT_LIMIT {
            self.trace.events.push(event);
        } else {
            self.trace.truncated = true;
        }
    }
}

thread_local! {
    static ACTIVE: RefCell<Option<Collector>> = const { RefCell::new(None) };
    static NEXT_SESSION: Cell<u64> = const { Cell::new(1) };
}

/// A session never moves between threads. Drop restores a nested outer session,
/// including during unwinding; a failed parse cannot contaminate another card.
struct Session {
    previous: Option<Collector>,
    restored: bool,
    _thread: PhantomData<Rc<()>>,
}
impl Session {
    fn begin() -> Self {
        let session_id = NEXT_SESSION.with(|next| {
            let value = next.get();
            next.set(
                value
                    .checked_add(1)
                    .expect("trace session identity exhausted"),
            );
            value
        });
        let previous = ACTIVE.with(|active| {
            active.replace(Some(Collector {
                session_id,
                next_cursor_id: 1,
                ability_index: None,
                trace: AutomataTrace::default(),
            }))
        });
        Self {
            previous,
            restored: false,
            _thread: PhantomData,
        }
    }

    fn finish(mut self) -> AutomataTrace {
        let current = ACTIVE.with(|active| active.replace(self.previous.take()));
        self.restored = true;
        current.expect("active scoped trace collector").trace
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        if !self.restored {
            ACTIVE.with(|active| active.replace(self.previous.take()));
        }
    }
}

pub(crate) fn collect_trace<T>(parse: impl FnOnce() -> T) -> (T, AutomataTrace) {
    let session = Session::begin();
    let result = parse();
    (result, session.finish())
}

pub(crate) struct AbilityScope {
    session_id: Option<u64>,
    previous: Option<usize>,
    _thread: PhantomData<Rc<()>>,
}
pub(crate) fn scope_ability(index: usize) -> AbilityScope {
    ACTIVE.with(|active| {
        let mut slot = active.borrow_mut();
        let (session_id, previous) = match slot.as_mut() {
            Some(collector) => {
                let previous = collector.ability_index.replace(index);
                (Some(collector.session_id), previous)
            }
            None => (None, None),
        };
        AbilityScope {
            session_id,
            previous,
            _thread: PhantomData,
        }
    })
}
impl Drop for AbilityScope {
    fn drop(&mut self) {
        ACTIVE.with(|active| {
            if let Some(collector) = active.borrow_mut().as_mut()
                && Some(collector.session_id) == self.session_id
            {
                collector.ability_index = self.previous;
            }
        });
    }
}

pub(crate) fn new_cursor(
    parent: Option<CursorTraceHandle>,
    automaton: &'static str,
    state: &'static str,
    token_index: usize,
    span: Option<(usize, usize)>,
    token_text: Option<&str>,
    kind: AutomataTraceEventKind,
) -> Option<CursorTraceHandle> {
    ACTIVE.with(|active| {
        let mut slot = active.borrow_mut();
        let collector = slot.as_mut()?;
        let handle = CursorTraceHandle {
            session_id: collector.session_id,
            cursor_id: collector.next_cursor_id,
        };
        collector.next_cursor_id = collector
            .next_cursor_id
            .checked_add(1)
            .expect("trace cursor identity exhausted");
        if collector.trace.events.len() >= EVENT_LIMIT {
            collector.trace.truncated = true;
            return Some(handle);
        }
        let parent_cursor_id = parent
            .filter(|parent| parent.session_id == collector.session_id)
            .map(|parent| parent.cursor_id);
        collector.push(AutomataTraceEvent {
            cursor_id: handle.cursor_id,
            parent_cursor_id,
            ability_index: collector.ability_index,
            automaton,
            state,
            token_index,
            kind,
            span_start: span.map(|span| span.0),
            span_end: span.map(|span| span.1),
            token_text: token_text.map(str::to_string),
        });
        Some(handle)
    })
}

pub(crate) fn record(
    handle: Option<CursorTraceHandle>,
    automaton: &'static str,
    state: &'static str,
    token_index: usize,
    span: Option<(usize, usize)>,
    token_text: Option<&str>,
    kind: AutomataTraceEventKind,
) {
    let Some(handle) = handle else { return };
    ACTIVE.with(|active| {
        let mut slot = active.borrow_mut();
        let Some(collector) = slot.as_mut() else {
            return;
        };
        if collector.session_id != handle.session_id {
            return;
        }
        if collector.trace.events.len() >= EVENT_LIMIT {
            collector.trace.truncated = true;
            return;
        }
        collector.push(AutomataTraceEvent {
            cursor_id: handle.cursor_id,
            parent_cursor_id: None,
            ability_index: collector.ability_index,
            automaton,
            state,
            token_index,
            kind,
            span_start: span.map(|span| span.0),
            span_end: span.map(|span| span.1),
            token_text: token_text.map(str::to_string),
        });
    });
}

/// Returns the unchanged native parser result plus bounded, real cursor events.
/// This feature does not instrument lexical/name readers or report projections.
pub fn parse_oracle_card_native_traced(
    request: OracleCardParseRequest,
) -> TracedOracleCardParseResult {
    let (result, trace) = collect_trace(|| super::pipeline::parse_oracle_card_native(request));
    TracedOracleCardParseResult { result, trace }
}

#[cfg(test)]
#[path = "automata_trace_tests.rs"]
mod tests;
