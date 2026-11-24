//! The status-predicate registry: every adjective that names a status of an
//! object rather than a characteristic.
//!
//! - Designations (CR 701.37b monstrous, 702.112b renowned, 701.60b
//!   suspected), the kicked status a permanent keeps from its kicked spell
//!   (CR 702.33d) and the face-down status (CR 708.2): flags of the object,
//!   kept until it leaves the battlefield.
//! - Battlefield statuses (CR 110.5 tapped/untapped, 506.4 attacking/
//!   blocking) and attachment-derived statuses (CR 700.9 modified, 301.5/
//!   303.4 equipped/enchanted), read from the permanent's current state.
//!
//! One registry is read by every consumer: the object predicate
//! (`a renowned creature`, `if it was kicked`, `target enchanted
//! permanent`), the source-state condition (`as long as this creature is
//! equipped`) and the designating action (`it becomes renowned`).

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PermanentDesignation {
    Monstrous,
    Renowned,
    Suspected,
    /// A permanent whose spell was kicked (CR 702.33d).
    Kicked,
    /// A face-down permanent (CR 708.2).
    FaceDown,
    /// A prepared permanent (the prepare mechanic): while it is prepared,
    /// its controller may cast a copy of its prepare spell; doing so
    /// unprepares it. The engine's `setPrepared` owns that permission.
    Prepared,
}

impl PermanentDesignation {
    pub(in crate::oracle::canonical) fn from_adjective(raw: &str) -> Option<Self> {
        match raw.to_ascii_lowercase().as_str() {
            "monstrous" => Some(Self::Monstrous),
            "renowned" => Some(Self::Renowned),
            "suspected" => Some(Self::Suspected),
            "kicked" => Some(Self::Kicked),
            "face-down" => Some(Self::FaceDown),
            "prepared" => Some(Self::Prepared),
            _ => None,
        }
    }

    /// The runtime designation flag (`setPermanentFlag`, `hasDesignation`).
    pub(in crate::oracle::canonical) const fn flag(self) -> &'static str {
        match self {
            Self::Monstrous => "monstrous",
            Self::Renowned => "renowned",
            Self::Suspected => "suspected",
            Self::Kicked => "kicked",
            Self::FaceDown => "faceDown",
            Self::Prepared => "prepared",
        }
    }

    /// Whether `becomes DESIGNATION` gives it. Suspected is given only by
    /// the suspect action, which also grants its rules (CR 701.60c).
    pub(in crate::oracle::canonical) const fn given_by_becomes(self) -> bool {
        matches!(self, Self::Monstrous | Self::Renowned | Self::Prepared)
    }
}

/// The object predicate one status adjective names, if it is registered.
pub(in crate::oracle::canonical) fn lookup_status_predicate(
    raw: &str,
) -> Option<super::ObjectPredicate> {
    super::ObjectStatus::from_adjective(raw)
        .map(super::ObjectPredicate::Status)
        .or_else(|| {
            PermanentDesignation::from_adjective(raw).map(super::ObjectPredicate::Designation)
        })
}
