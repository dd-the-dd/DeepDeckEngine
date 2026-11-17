use serde::Serialize;

/// Canonical, opaque name of one kind of Magic counter.
///
/// Recognition is intentionally open: semantic behavior for a small set of
/// rules-defined counters belongs to the engine and never limits which names
/// the Oracle parser may preserve.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub(crate) struct CounterName(String);

impl CounterName {
    pub(crate) fn from_parts<'a>(parts: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let normalized = parts
            .into_iter()
            .map(str::trim)
            .filter(|part| !part.is_empty())
            .map(|part| part.to_lowercase())
            .collect::<Vec<_>>()
            .join(" ");
        (!normalized.is_empty()).then_some(Self(normalized))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl PartialEq<&str> for CounterName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}
