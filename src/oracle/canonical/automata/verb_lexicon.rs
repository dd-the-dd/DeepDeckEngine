//! Verb lexicon (data): one entry per lemma with every surface form, the
//! set of typed argument frames the lemma takes and the voices it admits.
//! The clause reader picks the frame from the tokens after the verb.
//!
//! The lexicon supplies forms and categories; the clause grammar
//! (`clause.rs`) decides their role. A parent never tests a verb word: it
//! asks the clause reader for a lemma in the frame it expects. Agreement and
//! inflection have one mechanism (`VerbAgreement` -> `VerbForm` ->
//! `VerbEntry::surface`).
//!
//! Lemmas migrate one at a time (`docs/oracle-clause-layer.md`). A verb that
//! has no entry here keeps its established reader until its lemma migrates.

/// A verb lemma with an entry in this lexicon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbLemma {
    /// `gain` (CR 119.3, 119.10 a player gains life).
    Gain,
    /// `lose` (CR 119.3 a player loses life).
    Lose,
    /// `deal` (CR 120.1 an object deals damage).
    Deal,
    /// `sacrifice` (CR 701.21).
    Sacrifice,
    /// `discard` (CR 701.9).
    Discard,
    /// `tap` (CR 701.26).
    Tap,
    /// `untap` (CR 701.26).
    Untap,
    /// `exile` (CR 701.13).
    Exile,
    /// `return` (a zone change to a named zone).
    Return,
    /// `put` (counters on an object, or an object into a zone).
    Put,
    /// `reveal` (CR 701.20).
    Reveal,
    /// `mill` (CR 701.17).
    Mill,
    /// `exert` (CR 701.43).
    Exert,
    /// `remove` (counters from an object).
    Remove,
    /// `move` (counters from one object onto another, CR 122.5).
    Move,
    /// `pay` (CR 118: mana, life or another resource).
    Pay,
    /// `blight` (the keyword action: -1/-1 counters on a creature you control).
    Blight,
    /// `create` (CR 111.1 tokens).
    Create,
    /// `cycle` (CR 702.29: activating a cycling ability of a card).
    Cycle,
    /// `attack` (CR 508.1).
    Attack,
    /// `block` (CR 509.1).
    Block,
    /// `become` (a state change: tapped, untapped, blocked, the target of).
    Become,
    /// `die` (CR 700.4).
    Die,
    /// `enter` (the battlefield, CR 603.6a).
    Enter,
    /// `leave` (a zone, CR 603.6c).
    Leave,
    /// `cast` (CR 601).
    Cast,
    /// `draw` (CR 121).
    Draw,
    /// `exploit` (CR 702.110b: the named action of the exploit keyword).
    Exploit,
    /// `get` (CR 122.1: a player gets counters; `{E}` is an energy counter,
    /// CR 107.14).
    Get,
    /// `choose` (CR 608.2d: a choice made while the instruction resolves).
    Choose,
    /// `shuffle` (CR 701.24).
    Shuffle,
    /// `search` (CR 701.23).
    Search,
}

/// One inflected form of a lemma.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbForm {
    /// `gain`: imperative, first/second person, plural, bare infinitive.
    Base,
    /// `gains`.
    ThirdPersonSingular,
    /// `gained` (simple past, `you gained life this turn`).
    Past,
    /// `gained` (perfect and passive).
    PastParticiple,
    /// `gaining`.
    PresentParticiple,
}

/// Agreement of a finite present verb with its grammatical subject. This is
/// the only conjugation choice a subject makes: the subject decides the
/// agreement, the lexicon decides the surface.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum VerbAgreement {
    /// Imperative, `you`, plural subjects, `each` after coordinated subjects,
    /// the bare infinitive after a causative `have` or a modal.
    Base,
    /// A singular third-person subject (`target player`, `each opponent`).
    ThirdPersonSingular,
}

impl VerbAgreement {
    pub(in crate::oracle::canonical) fn form(self) -> VerbForm {
        match self {
            Self::Base => VerbForm::Base,
            Self::ThirdPersonSingular => VerbForm::ThirdPersonSingular,
        }
    }
}

/// One typed argument frame a lemma can take after its verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum ArgumentFrame {
    /// `AMOUNT life` | `life equal to AMOUNT` | bare `life` in patterns: a
    /// change of the acting player's life total (CR 119.3). The slot is the
    /// amount; the resource noun `life` marks the frame.
    LifeTotalChange,
    /// `[AMOUNT] [combat|noncombat] damage [to RECIPIENT]` (CR 120.1); the
    /// passive takes the recipient as subject (`is dealt damage`) or the
    /// damage as subject (`damage would be dealt to RECIPIENT`).
    DamageDealt,
    /// `KEYWORD[, KEYWORD and KEYWORD]` (`gains flying`): declared, not yet
    /// read by the clause (the keyword-grant readers still own it).
    KeywordList,
    /// `control of OBJECT` (`gain control of`): declared, not yet read.
    Control,
    /// `all abilities` (`loses all abilities`): declared, not yet read.
    Abilities,
    /// `OBJECTS`: the objects the action is performed on (`sacrifice a
    /// creature`, `discard two cards`, `tap an untapped Wall you control`).
    ObjectSelection,
    /// `OBJECTS to|into|onto|on ZONE` (`return a land you control to its
    /// owner's hand`, `put a card from your hand on top of your library`).
    ObjectToZone,
    /// `N [KIND] counter(s) on HOLDER` (`put a -1/-1 counter on this creature`).
    CountersOn,
    /// `N [KIND] counter(s) from HOLDER` (`remove a charge counter from ...`).
    CountersFrom,
    /// `N card(s)` from the actor's library (`mill a card`).
    CardCount,
    /// `MANA` | `N life` (`pay {2}`, `pay 3 life`).
    ResourcePayment,
    /// `N` for a numbered keyword action (`blight 2`).
    KeywordAmount,
    /// `TOKENS` (`create a 1/1 white Soldier creature token`).
    TokenCreation,
    /// No object, or an optional counterpart (`attacks [alone]`, `dies`,
    /// `enters [the battlefield]`, `blocks [a creature]`).
    Intransitive,
    /// `STATE` after `become` (`tapped`, `blocked [by ...]`, `the target of ...`).
    StateChange,
    /// `ZONE` after `leave` (`the battlefield`, `your graveyard`).
    ZoneDeparture,
    /// `SPELL` after `cast`.
    SpellObject,
    /// `N KIND counter(s)` | `{E}...` given to the acting player (CR 122.1).
    PlayerCounters,
}

impl ArgumentFrame {
    /// The noun that marks the frame after the verb, when it has one.
    pub(in crate::oracle::canonical) fn marker_noun(self) -> Option<&'static str> {
        match self {
            Self::LifeTotalChange => Some("life"),
            Self::DamageDealt => Some("damage"),
            Self::Control => Some("control"),
            Self::Abilities => Some("abilities"),
            Self::CountersOn | Self::CountersFrom => Some("counter"),
            Self::KeywordList
            | Self::ObjectSelection
            | Self::ObjectToZone
            | Self::CardCount
            | Self::ResourcePayment
            | Self::KeywordAmount
            | Self::TokenCreation
            | Self::Intransitive
            | Self::StateChange
            | Self::ZoneDeparture
            | Self::SpellObject
            | Self::PlayerCounters => None,
        }
    }
}

/// Voices a lemma admits. The passive reads `be` agreeing with the subject
/// (or bare after a modal) and the past participle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum Voice {
    Active,
    Passive,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) struct VerbForms {
    pub base: &'static str,
    pub third_person_singular: &'static str,
    pub past: &'static str,
    pub past_participle: &'static str,
    pub present_participle: &'static str,
}

#[derive(Debug)]
pub(in crate::oracle::canonical) struct VerbEntry {
    pub lemma: VerbLemma,
    pub forms: VerbForms,
    pub frames: &'static [ArgumentFrame],
    pub voices: &'static [Voice],
}

impl VerbEntry {
    pub(in crate::oracle::canonical) fn surface(&self, form: VerbForm) -> &'static str {
        match form {
            VerbForm::Base => self.forms.base,
            VerbForm::ThirdPersonSingular => self.forms.third_person_singular,
            VerbForm::Past => self.forms.past,
            VerbForm::PastParticiple => self.forms.past_participle,
            VerbForm::PresentParticiple => self.forms.present_participle,
        }
    }

    /// Every form whose surface is `raw` (`gained` is both past and participle).
    pub(in crate::oracle::canonical) fn forms_of(&self, raw: &str) -> Vec<VerbForm> {
        const ALL: [VerbForm; 5] = [
            VerbForm::Base,
            VerbForm::ThirdPersonSingular,
            VerbForm::Past,
            VerbForm::PastParticiple,
            VerbForm::PresentParticiple,
        ];
        ALL.into_iter()
            .filter(|form| self.surface(*form).eq_ignore_ascii_case(raw))
            .collect()
    }
}

const VERB_LEXICON: &[VerbEntry] = &[
    VerbEntry {
        lemma: VerbLemma::Gain,
        forms: VerbForms {
            base: "gain",
            third_person_singular: "gains",
            past: "gained",
            past_participle: "gained",
            present_participle: "gaining",
        },
        frames: &[
            ArgumentFrame::LifeTotalChange,
            ArgumentFrame::KeywordList,
            ArgumentFrame::Control,
        ],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Lose,
        forms: VerbForms {
            base: "lose",
            third_person_singular: "loses",
            past: "lost",
            past_participle: "lost",
            present_participle: "losing",
        },
        frames: &[ArgumentFrame::LifeTotalChange, ArgumentFrame::Abilities],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Deal,
        forms: VerbForms {
            base: "deal",
            third_person_singular: "deals",
            past: "dealt",
            past_participle: "dealt",
            present_participle: "dealing",
        },
        frames: &[ArgumentFrame::DamageDealt],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Sacrifice,
        forms: VerbForms {
            base: "sacrifice",
            third_person_singular: "sacrifices",
            past: "sacrificed",
            past_participle: "sacrificed",
            present_participle: "sacrificing",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Discard,
        forms: VerbForms {
            base: "discard",
            third_person_singular: "discards",
            past: "discarded",
            past_participle: "discarded",
            present_participle: "discarding",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Tap,
        forms: VerbForms {
            base: "tap",
            third_person_singular: "taps",
            past: "tapped",
            past_participle: "tapped",
            present_participle: "tapping",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Untap,
        forms: VerbForms {
            base: "untap",
            third_person_singular: "untaps",
            past: "untapped",
            past_participle: "untapped",
            present_participle: "untapping",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Exile,
        forms: VerbForms {
            base: "exile",
            third_person_singular: "exiles",
            past: "exiled",
            past_participle: "exiled",
            present_participle: "exiling",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Return,
        forms: VerbForms {
            base: "return",
            third_person_singular: "returns",
            past: "returned",
            past_participle: "returned",
            present_participle: "returning",
        },
        frames: &[ArgumentFrame::ObjectToZone],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Put,
        forms: VerbForms {
            base: "put",
            third_person_singular: "puts",
            past: "put",
            past_participle: "put",
            present_participle: "putting",
        },
        frames: &[ArgumentFrame::CountersOn, ArgumentFrame::ObjectToZone],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Reveal,
        forms: VerbForms {
            base: "reveal",
            third_person_singular: "reveals",
            past: "revealed",
            past_participle: "revealed",
            present_participle: "revealing",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Mill,
        forms: VerbForms {
            base: "mill",
            third_person_singular: "mills",
            past: "milled",
            past_participle: "milled",
            present_participle: "milling",
        },
        frames: &[ArgumentFrame::CardCount],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Exert,
        forms: VerbForms {
            base: "exert",
            third_person_singular: "exerts",
            past: "exerted",
            past_participle: "exerted",
            present_participle: "exerting",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Remove,
        forms: VerbForms {
            base: "remove",
            third_person_singular: "removes",
            past: "removed",
            past_participle: "removed",
            present_participle: "removing",
        },
        frames: &[ArgumentFrame::CountersFrom],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Move,
        forms: VerbForms {
            base: "move",
            third_person_singular: "moves",
            past: "moved",
            past_participle: "moved",
            present_participle: "moving",
        },
        frames: &[ArgumentFrame::CountersFrom],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Pay,
        forms: VerbForms {
            base: "pay",
            third_person_singular: "pays",
            past: "paid",
            past_participle: "paid",
            present_participle: "paying",
        },
        frames: &[ArgumentFrame::ResourcePayment],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Blight,
        forms: VerbForms {
            base: "blight",
            third_person_singular: "blights",
            past: "blighted",
            past_participle: "blighted",
            present_participle: "blighting",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Create,
        forms: VerbForms {
            base: "create",
            third_person_singular: "creates",
            past: "created",
            past_participle: "created",
            present_participle: "creating",
        },
        frames: &[ArgumentFrame::TokenCreation],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Cycle,
        forms: VerbForms {
            base: "cycle",
            third_person_singular: "cycles",
            past: "cycled",
            past_participle: "cycled",
            present_participle: "cycling",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Attack,
        forms: VerbForms {
            base: "attack",
            third_person_singular: "attacks",
            past: "attacked",
            past_participle: "attacked",
            present_participle: "attacking",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Block,
        forms: VerbForms {
            base: "block",
            third_person_singular: "blocks",
            past: "blocked",
            past_participle: "blocked",
            present_participle: "blocking",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Become,
        forms: VerbForms {
            base: "become",
            third_person_singular: "becomes",
            past: "became",
            past_participle: "become",
            present_participle: "becoming",
        },
        frames: &[ArgumentFrame::StateChange],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Die,
        forms: VerbForms {
            base: "die",
            third_person_singular: "dies",
            past: "died",
            past_participle: "died",
            present_participle: "dying",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Enter,
        forms: VerbForms {
            base: "enter",
            third_person_singular: "enters",
            past: "entered",
            past_participle: "entered",
            present_participle: "entering",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Leave,
        forms: VerbForms {
            base: "leave",
            third_person_singular: "leaves",
            past: "left",
            past_participle: "left",
            present_participle: "leaving",
        },
        frames: &[ArgumentFrame::ZoneDeparture],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Cast,
        forms: VerbForms {
            base: "cast",
            third_person_singular: "casts",
            past: "cast",
            past_participle: "cast",
            present_participle: "casting",
        },
        frames: &[ArgumentFrame::SpellObject],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Draw,
        forms: VerbForms {
            base: "draw",
            third_person_singular: "draws",
            past: "drew",
            past_participle: "drawn",
            present_participle: "drawing",
        },
        frames: &[ArgumentFrame::CardCount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Exploit,
        forms: VerbForms {
            base: "exploit",
            third_person_singular: "exploits",
            past: "exploited",
            past_participle: "exploited",
            present_participle: "exploiting",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Get,
        forms: VerbForms {
            base: "get",
            third_person_singular: "gets",
            past: "got",
            past_participle: "gotten",
            present_participle: "getting",
        },
        frames: &[ArgumentFrame::PlayerCounters],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Choose,
        forms: VerbForms {
            base: "choose",
            third_person_singular: "chooses",
            past: "chose",
            past_participle: "chosen",
            present_participle: "choosing",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Shuffle,
        forms: VerbForms {
            base: "shuffle",
            third_person_singular: "shuffles",
            past: "shuffled",
            past_participle: "shuffled",
            present_participle: "shuffling",
        },
        frames: &[ArgumentFrame::ObjectToZone],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Search,
        forms: VerbForms {
            base: "search",
            third_person_singular: "searches",
            past: "searched",
            past_participle: "searched",
            present_participle: "searching",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
];

/// Every lemma of the lexicon, in entry order.
pub(in crate::oracle::canonical) static ALL_LEMMAS: std::sync::LazyLock<Vec<VerbLemma>> =
    std::sync::LazyLock::new(|| VERB_LEXICON.iter().map(|entry| entry.lemma).collect());

pub(in crate::oracle::canonical) fn verb_entry(lemma: VerbLemma) -> &'static VerbEntry {
    VERB_LEXICON
        .iter()
        .find(|entry| entry.lemma == lemma)
        .expect("every VerbLemma has one lexicon entry")
}

/// The lemma among `lemmas` that `raw` inflects, with every form it can be.
pub(in crate::oracle::canonical) fn lookup_verb(
    raw: &str,
    lemmas: &[VerbLemma],
) -> Option<(&'static VerbEntry, Vec<VerbForm>)> {
    lemmas.iter().find_map(|lemma| {
        let entry = verb_entry(*lemma);
        let forms = entry.forms_of(raw);
        (!forms.is_empty()).then_some((entry, forms))
    })
}

/// The subject a parent states for an action verb: its agreement and whether
/// a non-controller (third-person) actor makes the choices (`of their
/// choice`). `you`/imperative: `Base`, not third person; `that player`:
/// `ThirdPersonSingular`; `they`/`each player`... after `unless`: `Base`,
/// third person. One mechanism with `VerbAgreement` replaces per-verb forms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) struct ActingSubject {
    pub agreement: VerbAgreement,
    pub third_person: bool,
}

impl ActingSubject {
    /// The ability controller (imperative or `you`).
    pub(in crate::oracle::canonical) const CONTROLLER: Self = Self {
        agreement: VerbAgreement::Base,
        third_person: false,
    };
    /// A singular other player (`that player sacrifices`).
    pub(in crate::oracle::canonical) const THIRD_SINGULAR: Self = Self {
        agreement: VerbAgreement::ThirdPersonSingular,
        third_person: true,
    };
    /// A plural other actor with the bare verb (`unless they sacrifice`).
    pub(in crate::oracle::canonical) const THIRD_PLURAL: Self = Self {
        agreement: VerbAgreement::Base,
        third_person: true,
    };
}

/// True when `raw` is the given form of the lemma. Lookahead gates of other
/// families use this instead of a literal verb word.
pub(in crate::oracle::canonical) fn is_verb_form(
    raw: &str,
    lemma: VerbLemma,
    form: VerbForm,
) -> bool {
    verb_entry(lemma).surface(form).eq_ignore_ascii_case(raw)
}

#[cfg(test)]
#[path = "verb_lexicon_tests.rs"]
mod tests;
