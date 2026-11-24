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
    /// `support N` (CR 701.32).
    Support,
    /// `attach OBJECT to OBJECT` (CR 701.3).
    Attach,
    /// `prevent DAMAGE` (CR 615).
    Prevent,
    /// `regenerate OBJECT` (CR 701.15).
    Regenerate,
    /// `goad OBJECT` (CR 701.38).
    Goad,
    /// `add MANA` (CR 106.4).
    Add,
    /// `amass [TYPE] N` (CR 701.47).
    Amass,
    /// `empower` (a named keyword-action row of the effect lexicon).
    Empower,
    /// `investigate` (CR 701.36).
    Investigate,
    /// `scry N` (CR 701.18).
    Scry,
    /// `surveil N` (CR 701.25).
    Surveil,
    /// `earthbend N` (the keyword action of the same name).
    Earthbend,
    /// `transform OBJECT` (CR 701.28).
    Transform,
    /// `double QUANTITY` (CR 701.10).
    Double,
    /// `counter SPELL` (CR 701.5).
    Counter,
    /// `destroy OBJECT` (CR 701.7).
    Destroy,
    /// `fight OBJECT` (CR 701.14).
    Fight,
    /// `have`/`has` (possession of abilities, CR 113.1; also the causative).
    Have,
    /// `assign` (combat damage assignment, CR 510.1).
    Assign,
    /// `activate` (CR 602.2: activating an activated ability), an event verb.
    Activate,
    /// `turn` in `is turned face up` (CR 708.8), an event verb.
    Turn,
    /// `commit` in `commit a crime` (CR 700.13), an event verb.
    Commit,
    /// `take an extra turn after this one` (CR 500.7).
    Take,
    /// `play a land` (CR 305.1), an event verb read by turn histories.
    Play,
    /// `descend` (a permanent card put into its owner's graveyard from
    /// anywhere), an event verb read by turn histories.
    Descend,
    /// `skip your next turn|untap step|draw step|combat phase` (CR 500.11,
    /// 614.10): a player's own future turn part does not happen.
    Skip,
    /// `the Ring tempts PLAYER` (CR 701.54a): the subject is the Ring, the
    /// object the tempted player.
    Tempt,
    /// `OBJECT phases out` (CR 702.26b): the permanent's phasing status.
    Phase,
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
    /// `MANA` produced for the acting player (`add {G}`, CR 106.4).
    ManaProduction,
    /// `the next N damage ...` / `all combat damage ...` (CR 615).
    DamagePrevention,
    /// `the number of COUNTERS on OBJECT` | `OBJECT's power` (CR 701.10).
    Doubling,
    /// `an extra turn after this one` (CR 500.7): the acting player takes
    /// the turn directly after the current one.
    ExtraTurn,
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
            | Self::PlayerCounters
            | Self::ManaProduction
            | Self::DamagePrevention
            | Self::Doubling
            | Self::ExtraTurn
            | Self::SkippedTurnPart
            | Self::PlayerDesignation
            | Self::PlayerObject
            | Self::PhasingDirection => None,
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
        frames: &[ArgumentFrame::StateChange, ArgumentFrame::PlayerDesignation],
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
    VerbEntry {
        lemma: VerbLemma::Amass,
        forms: VerbForms {
            base: "amass",
            third_person_singular: "amasses",
            past: "amassed",
            past_participle: "amassed",
            present_participle: "amassing",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Investigate,
        forms: VerbForms {
            base: "investigate",
            third_person_singular: "investigates",
            past: "investigated",
            past_participle: "investigated",
            present_participle: "investigating",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Scry,
        forms: VerbForms {
            base: "scry",
            third_person_singular: "scries",
            past: "scried",
            past_participle: "scried",
            present_participle: "scrying",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Surveil,
        forms: VerbForms {
            base: "surveil",
            third_person_singular: "surveils",
            past: "surveilled",
            past_participle: "surveilled",
            present_participle: "surveilling",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Take,
        forms: VerbForms {
            base: "take",
            third_person_singular: "takes",
            past: "took",
            past_participle: "taken",
            present_participle: "taking",
        },
        frames: &[ArgumentFrame::ExtraTurn, ArgumentFrame::PlayerDesignation],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Skip,
        forms: VerbForms {
            base: "skip",
            third_person_singular: "skips",
            past: "skipped",
            past_participle: "skipped",
            present_participle: "skipping",
        },
        frames: &[ArgumentFrame::SkippedTurnPart],
        voices: &[Voice::Active],
    },
];

/// Lemmas the effect clause dispatches on (`effect_head.rs`) whose actions
/// do not yet carry a performer through lowering. They are full lexicon
/// rows (forms, frames, voices) for every clause reader, but the
/// performer's person shift (`ALL_LEMMAS`) does not admit them: `each player
/// adds {B}` keeps failing explicitly until a performed mana ability is
/// classified and executed as that player's (CR 605.1a, 106.4). A lemma whose
/// primitives name only `you` (`scry`, `investigate`, `surveil`, `amass`) is
/// a performed row: the engine runs the ordinary primitive as the
/// performer's instruction (CR 608.2c).
const NON_PERFORMED_LEXICON: &[VerbEntry] = &[
    VerbEntry {
        lemma: VerbLemma::Tempt,
        forms: VerbForms {
            base: "tempt",
            third_person_singular: "tempts",
            past: "tempted",
            past_participle: "tempted",
            present_participle: "tempting",
        },
        frames: &[ArgumentFrame::PlayerObject],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Phase,
        forms: VerbForms {
            base: "phase",
            third_person_singular: "phases",
            past: "phased",
            past_participle: "phased",
            present_participle: "phasing",
        },
        frames: &[ArgumentFrame::PhasingDirection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Support,
        forms: VerbForms {
            base: "support",
            third_person_singular: "supports",
            past: "supported",
            past_participle: "supported",
            present_participle: "supporting",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Attach,
        forms: VerbForms {
            base: "attach",
            third_person_singular: "attaches",
            past: "attached",
            past_participle: "attached",
            present_participle: "attaching",
        },
        frames: &[ArgumentFrame::ObjectToZone],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Prevent,
        forms: VerbForms {
            base: "prevent",
            third_person_singular: "prevents",
            past: "prevented",
            past_participle: "prevented",
            present_participle: "preventing",
        },
        frames: &[ArgumentFrame::DamagePrevention],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Regenerate,
        forms: VerbForms {
            base: "regenerate",
            third_person_singular: "regenerates",
            past: "regenerated",
            past_participle: "regenerated",
            present_participle: "regenerating",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Goad,
        forms: VerbForms {
            base: "goad",
            third_person_singular: "goads",
            past: "goaded",
            past_participle: "goaded",
            present_participle: "goading",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Add,
        forms: VerbForms {
            base: "add",
            third_person_singular: "adds",
            past: "added",
            past_participle: "added",
            present_participle: "adding",
        },
        frames: &[ArgumentFrame::ManaProduction],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Empower,
        forms: VerbForms {
            base: "empower",
            third_person_singular: "empowers",
            past: "empowered",
            past_participle: "empowered",
            present_participle: "empowering",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Earthbend,
        forms: VerbForms {
            base: "earthbend",
            third_person_singular: "earthbends",
            past: "earthbent",
            past_participle: "earthbent",
            present_participle: "earthbending",
        },
        frames: &[ArgumentFrame::KeywordAmount],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Transform,
        forms: VerbForms {
            base: "transform",
            third_person_singular: "transforms",
            past: "transformed",
            past_participle: "transformed",
            present_participle: "transforming",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Double,
        forms: VerbForms {
            base: "double",
            third_person_singular: "doubles",
            past: "doubled",
            past_participle: "doubled",
            present_participle: "doubling",
        },
        frames: &[ArgumentFrame::Doubling],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Counter,
        forms: VerbForms {
            base: "counter",
            third_person_singular: "counters",
            past: "countered",
            past_participle: "countered",
            present_participle: "countering",
        },
        frames: &[ArgumentFrame::SpellObject],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Destroy,
        forms: VerbForms {
            base: "destroy",
            third_person_singular: "destroys",
            past: "destroyed",
            past_participle: "destroyed",
            present_participle: "destroying",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Fight,
        forms: VerbForms {
            base: "fight",
            third_person_singular: "fights",
            past: "fought",
            past_participle: "fought",
            present_participle: "fighting",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Have,
        forms: VerbForms {
            base: "have",
            third_person_singular: "has",
            past: "had",
            past_participle: "had",
            present_participle: "having",
        },
        frames: &[ArgumentFrame::KeywordList],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Assign,
        forms: VerbForms {
            base: "assign",
            third_person_singular: "assigns",
            past: "assigned",
            past_participle: "assigned",
            present_participle: "assigning",
        },
        frames: &[ArgumentFrame::DamageDealt],
        voices: &[Voice::Active, Voice::Passive],
    },
];

/// Lemmas read only by event clauses (player actions and object state
/// changes a trigger observes). They are full lexicon rows, so their forms
/// are lexicon data, but the effect clause does not dispatch on them: no
/// instruction of the effect language performs them yet.
const EVENT_VERB_LEXICON: &[VerbEntry] = &[
    VerbEntry {
        lemma: VerbLemma::Activate,
        forms: VerbForms {
            base: "activate",
            third_person_singular: "activates",
            past: "activated",
            past_participle: "activated",
            present_participle: "activating",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Turn,
        forms: VerbForms {
            base: "turn",
            third_person_singular: "turns",
            past: "turned",
            past_participle: "turned",
            present_participle: "turning",
        },
        frames: &[ArgumentFrame::StateChange],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Commit,
        forms: VerbForms {
            base: "commit",
            third_person_singular: "commits",
            past: "committed",
            past_participle: "committed",
            present_participle: "committing",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active],
    },
    VerbEntry {
        lemma: VerbLemma::Play,
        forms: VerbForms {
            base: "play",
            third_person_singular: "plays",
            past: "played",
            past_participle: "played",
            present_participle: "playing",
        },
        frames: &[ArgumentFrame::ObjectSelection],
        voices: &[Voice::Active, Voice::Passive],
    },
    VerbEntry {
        lemma: VerbLemma::Descend,
        forms: VerbForms {
            base: "descend",
            third_person_singular: "descends",
            past: "descended",
            past_participle: "descended",
            present_participle: "descending",
        },
        frames: &[ArgumentFrame::Intransitive],
        voices: &[Voice::Active],
    },
];

/// The event-only lemmas, in entry order.
pub(in crate::oracle::canonical) static EVENT_LEMMAS: std::sync::LazyLock<Vec<VerbLemma>> =
    std::sync::LazyLock::new(|| EVENT_VERB_LEXICON.iter().map(|entry| entry.lemma).collect());

/// Every lemma a performer may take, in entry order.
pub(in crate::oracle::canonical) static ALL_LEMMAS: std::sync::LazyLock<Vec<VerbLemma>> =
    std::sync::LazyLock::new(|| VERB_LEXICON.iter().map(|entry| entry.lemma).collect());

/// Every lemma of the lexicon, performed or not: the vocabulary the effect
/// clause dispatches on.
pub(in crate::oracle::canonical) static EFFECT_LEMMAS: std::sync::LazyLock<Vec<VerbLemma>> =
    std::sync::LazyLock::new(|| {
        VERB_LEXICON
            .iter()
            .chain(NON_PERFORMED_LEXICON)
            .map(|entry| entry.lemma)
            .collect()
    });

pub(in crate::oracle::canonical) fn verb_entry(lemma: VerbLemma) -> &'static VerbEntry {
    VERB_LEXICON
        .iter()
        .chain(NON_PERFORMED_LEXICON)
        .chain(EVENT_VERB_LEXICON)
        .find(|entry| entry.lemma == lemma)
        .expect("every VerbLemma has one lexicon entry")
}

/// A finite auxiliary before a bare verb: the clause modality (CR 608.2d
/// `may`, CR 614.1a `would`, prohibitions CR 101.2 `can't`) or the negated
/// `do` support (`doesn't untap`). The agreement is the subject's when the
/// auxiliary inflects (`doesn't` / `don't`), else `None`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::oracle::canonical) enum Auxiliary {
    May,
    Can,
    Cannot,
    Would,
    Must,
    DoNot,
}

/// The auxiliary `raw` writes and the agreement it shows, if any. The
/// typographic apostrophe reads as the ASCII one.
pub(in crate::oracle::canonical) fn lookup_auxiliary(
    raw: &str,
) -> Option<(Auxiliary, Option<VerbAgreement>)> {
    const AUXILIARIES: &[(&str, Auxiliary, Option<VerbAgreement>)] = &[
        ("may", Auxiliary::May, None),
        ("can", Auxiliary::Can, None),
        ("can't", Auxiliary::Cannot, None),
        ("cannot", Auxiliary::Cannot, None),
        ("would", Auxiliary::Would, None),
        ("must", Auxiliary::Must, None),
        (
            "doesn't",
            Auxiliary::DoNot,
            Some(VerbAgreement::ThirdPersonSingular),
        ),
        ("don't", Auxiliary::DoNot, Some(VerbAgreement::Base)),
    ];
    let raw = raw.replace('\u{2019}', "'");
    AUXILIARIES
        .iter()
        .find(|(surface, _, _)| surface.eq_ignore_ascii_case(&raw))
        .map(|(_, auxiliary, agreement)| (*auxiliary, *agreement))
}

/// The present copula (`is` / `are`) and the agreement it shows. `be` has
/// no regular paradigm in `VerbForms` (its plural present is not its base),
/// so it is a closed table of its own.
pub(in crate::oracle::canonical) fn copula_agreement(raw: &str) -> Option<VerbAgreement> {
    const COPULA: &[(&str, VerbAgreement)] = &[
        ("is", VerbAgreement::ThirdPersonSingular),
        ("are", VerbAgreement::Base),
    ];
    COPULA
        .iter()
        .find(|(surface, _)| surface.eq_ignore_ascii_case(raw))
        .map(|(_, agreement)| *agreement)
}

/// The lemma `raw` inflects among every lexicon lemma, with its forms.
pub(in crate::oracle::canonical) fn lookup_effect_verb(
    raw: &str,
) -> Option<(&'static VerbEntry, Vec<VerbForm>)> {
    lookup_verb(raw, &EFFECT_LEMMAS)
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
