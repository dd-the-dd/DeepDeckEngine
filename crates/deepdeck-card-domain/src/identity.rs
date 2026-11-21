use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SubtypeFamily {
    Creature,
    Artifact,
    Enchantment,
    Land,
    Planeswalker,
    Spell,
    Battle,
    Plane,
    Dungeon,
}

pub(crate) const FAMILIES: [SubtypeFamily; 9] = [
    SubtypeFamily::Creature,
    SubtypeFamily::Artifact,
    SubtypeFamily::Enchantment,
    SubtypeFamily::Land,
    SubtypeFamily::Planeswalker,
    SubtypeFamily::Spell,
    SubtypeFamily::Battle,
    SubtypeFamily::Plane,
    SubtypeFamily::Dungeon,
];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubtypeFamilies(pub(crate) u16);

impl SubtypeFamilies {
    pub const fn single(family: SubtypeFamily) -> Self {
        Self(1 << family as u16)
    }

    pub const fn contains(self, family: SubtypeFamily) -> bool {
        self.0 & Self::single(family).0 != 0
    }

    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn iter(self) -> impl Iterator<Item = SubtypeFamily> {
        FAMILIES
            .into_iter()
            .filter(move |family| self.contains(*family))
    }

    pub fn only(self) -> Option<SubtypeFamily> {
        let mut families = self.iter();
        let first = families.next()?;
        families.next().is_none().then_some(first)
    }

    /// A nominal-family fact, not an implicit card type or a claim that an
    /// object is currently on the battlefield. Creature includes Kindred and
    /// cannot establish this domain. Spacecraft also has the Plane family.
    pub fn establishes_permanent_domain(self) -> bool {
        !self.is_empty()
            && self.iter().all(|family| {
                matches!(
                    family,
                    SubtypeFamily::Artifact
                        | SubtypeFamily::Enchantment
                        | SubtypeFamily::Land
                        | SubtypeFamily::Planeswalker
                        | SubtypeFamily::Battle
                )
            })
    }
}

/// Values are issued only by the validated registry. Names, IR spellings and
/// family sets are immutable; the identity is not a roster-position integer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SubtypeId {
    pub(crate) canonical: &'static str,
    pub(crate) ir_name: &'static str,
    pub(crate) families: SubtypeFamilies,
}

impl SubtypeId {
    pub const fn canonical_name(self) -> &'static str {
        self.canonical
    }

    pub const fn ir_name(self) -> &'static str {
        self.ir_name
    }

    pub const fn families(self) -> SubtypeFamilies {
        self.families
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptedForms {
    SingularOnly,
    SingularOrPlural,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NominalError {
    RegistryData(String),
    EmptySurface,
    NonCanonicalWhitespace(String),
    UnknownQuery(String),
    PluralQueryNotAllowed(String),
    UnknownSurface(String),
    WrongChoiceFamily {
        surface: String,
        families: SubtypeFamilies,
    },
    EmptyTypeLine,
    UnknownTypeWord(String),
    MissingCardType,
    UnsupportedPrintedFamily(SubtypeFamily),
    MalformedTypeLine(String),
    UnknownPrintedSubtype {
        surface: String,
        token_index: usize,
    },
    PrintedPluralNotAllowed {
        surface: String,
        token_index: usize,
    },
    PrintedFamilyMismatch {
        surface: String,
        nominal_families: SubtypeFamilies,
        type_families: SubtypeFamilies,
    },
    AmbiguousPrintedSubtype {
        surface: String,
        families: SubtypeFamilies,
    },
    DuplicatePrintedSubtype(String),
}

impl fmt::Display for NominalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RegistryData(message) => write!(f, "invalid nominal registry: {message}"),
            Self::EmptySurface => f.write_str("empty nominal surface"),
            Self::NonCanonicalWhitespace(value) => {
                write!(f, "noncanonical nominal whitespace: {value:?}")
            }
            Self::UnknownQuery(value) => write!(f, "unknown canonical subtype query: {value}"),
            Self::PluralQueryNotAllowed(value) => write!(
                f,
                "plural surface is not a canonical subtype query: {value}"
            ),
            Self::UnknownSurface(value) => write!(f, "unregistered subtype surface: {value}"),
            Self::WrongChoiceFamily { surface, families } => write!(
                f,
                "subtype choice {surface} is not a creature type: {families:?}"
            ),
            Self::EmptyTypeLine => f.write_str("empty printed type line"),
            Self::UnknownTypeWord(value) => {
                write!(f, "unsupported printed type/supertype: {value}")
            }
            Self::MissingCardType => f.write_str("printed type line has no card type"),
            Self::UnsupportedPrintedFamily(family) => {
                write!(f, "unsupported printed subtype family: {family:?}")
            }
            Self::MalformedTypeLine(value) => write!(f, "malformed printed type line: {value}"),
            Self::UnknownPrintedSubtype {
                surface,
                token_index,
            } => write!(
                f,
                "unregistered printed subtype at token {token_index}: {surface}"
            ),
            Self::PrintedPluralNotAllowed {
                surface,
                token_index,
            } => write!(
                f,
                "printed subtype at token {token_index} is a plural alias: {surface}"
            ),
            Self::PrintedFamilyMismatch {
                surface,
                nominal_families,
                type_families,
            } => write!(
                f,
                "printed subtype {surface} has families {nominal_families:?}, incompatible with {type_families:?}"
            ),
            Self::AmbiguousPrintedSubtype { surface, families } => write!(
                f,
                "ambiguous printed subtype family for {surface}: {families:?}"
            ),
            Self::DuplicatePrintedSubtype(value) => write!(f, "duplicate printed subtype: {value}"),
        }
    }
}

impl std::error::Error for NominalError {}
