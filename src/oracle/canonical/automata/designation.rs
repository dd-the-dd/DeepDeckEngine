//! Permanent designations (CR 701.37b monstrous, 702.112b renowned, 701.60b
//! suspected): a designation is not a characteristic; it stays on the
//! permanent until it leaves the battlefield (or an effect removes it).
//!
//! One registry is read by every consumer: the object predicate
//! (`a renowned creature`, `if it isn't renowned`), the source-state
//! condition (`as long as this creature is monstrous`) and the designating
//! action (`it becomes renowned`). The runtime representation is the
//! permanent's designation flag of the same name.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentDesignation {
    Monstrous,
    Renowned,
    Suspected,
}

impl PermanentDesignation {
    pub(in crate::oracle::canonical) fn from_adjective(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "monstrous" => Some(Self::Monstrous),
            "renowned" => Some(Self::Renowned),
            "suspected" => Some(Self::Suspected),
            _ => None,
        }
    }

    /// The runtime designation flag (`setPermanentFlag`, `hasDesignation`).
    pub(in crate::oracle::canonical) const fn flag(self) -> &'static str {
        match self {
            Self::Monstrous => "monstrous",
            Self::Renowned => "renowned",
            Self::Suspected => "suspected",
        }
    }

    /// Whether `becomes DESIGNATION` gives it. Suspected is given only by
    /// the suspect action, which also grants its rules (CR 701.60c).
    pub(in crate::oracle::canonical) const fn given_by_becomes(self) -> bool {
        !matches!(self, Self::Suspected)
    }
}
