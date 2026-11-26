//! Generic event model (docs/oracle-event-model.md), combat family.
//!
//! Every combat action publishes one typed record: an attack declaration
//! (one per declared attacker, CR 508.1), a block declaration (one per
//! blocker/attacker pair, CR 509.1) and an attacking creature becoming
//! unblocked (CR 509.1h). A trigger's event is a pattern of the same shape:
//! a lemma, the role its subject occupies, a relation between that subject
//! and the trigger source, object predicates on the subject and on the
//! counterpart, and typed modifiers (`alone`, once per combat, a per-turn
//! ordinal). One matcher evaluates any pattern against any record.
//!
//! The historical phrase-shaped kinds (`declaredAttacker`,
//! `permanentBecameBlocked`, `controlledCreatureDeclaredBlocker`, ...) are
//! read as patterns; their emitted IR is unchanged. Each keeps the object
//! view its old check used (printed, state-aware or effective
//! characteristics) and its publication group, so the established stack
//! order and decision bindings are preserved.
use super::*;

const COUNTERPART_ID: &str = "combatCounterpartId";
const COUNTERPART_IDENTITY: &str = "combatCounterpartIdentity";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CombatLemma {
    /// A creature is declared as an attacker (CR 508.1, 508.3a).
    Attack,
    /// A creature is declared as a blocker of one attacking creature
    /// (CR 509.1). Seen from the attacker, the same record is `becomes
    /// blocked [by]` (CR 509.1h, 509.3c, 509.3d).
    Block,
    /// An attacking creature with no blocker becomes unblocked (CR 509.1h).
    BecomeUnblocked,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CombatRole {
    Attacker,
    Blocker,
}

/// One combat action, published once at the point the engine performs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CombatEventRecord {
    pub lemma: CombatLemma,
    /// The declaring player: the attacking player for an attack or an
    /// unblocked attacker, the player declaring the block otherwise.
    pub actor: String,
    pub attacker: String,
    pub blocker: Option<String>,
    /// The player the attacker attacks, or the controller of the attacked
    /// planeswalker or battle protector (CR 506.2).
    pub defending_player: String,
    /// The attacker was declared during this combat's declare-attackers step
    /// (CR 508.1), not put onto the battlefield attacking (CR 508.4).
    pub declared_attacker: bool,
    /// The attacker is the only creature declared as an attacker (CR 506.5).
    pub attacking_alone: bool,
    /// This is the first blocker declared for the attacker this combat.
    pub first_block_of_attacker: bool,
    /// This is the first creature the blocker blocks this combat.
    pub first_block_by_blocker: bool,
    /// Per-turn ordinal of this lemma for the attacker: its attacks (attack
    /// records), or the combats it became blocked in (block records).
    pub attacker_turn_ordinal: Option<usize>,
    /// Per-turn ordinal of this lemma for the blocker (block records).
    pub blocker_turn_ordinal: Option<usize>,
    /// The player whose turn it is when the action happens (CR 500.1): the
    /// turn owner a `during your turn` modifier compares with.
    pub turn_player: String,
    /// The planeswalker or battle the attacker attacks; `None` when it
    /// attacks a player (CR 506.2, 508.1b).
    pub attacked_permanent: Option<String>,
}

/// A turn-owner modifier of a pattern, relative to the source's controller
/// (CR 500.1), whoever performs the action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TurnOwnerRelation {
    /// `during your turn`.
    SourceController,
    /// `during an opponent's turn`.
    OpponentOfSourceController,
}

impl TurnOwnerRelation {
    /// The `duringControllerTurn` / `duringOpponentTurn` fields shared with
    /// the cast pattern; both at once is contradictory and unsupported.
    fn from_pattern(event: &Value) -> Option<Option<Self>> {
        match (
            optional_flag(event, "duringControllerTurn")?,
            optional_flag(event, "duringOpponentTurn")?,
        ) {
            (false, false) => Some(None),
            (true, false) => Some(Some(Self::SourceController)),
            (false, true) => Some(Some(Self::OpponentOfSourceController)),
            (true, true) => None,
        }
    }

    pub(super) fn matches(self, turn_player: &str, source_controller: &str) -> bool {
        match self {
            Self::SourceController => turn_player == source_controller,
            Self::OpponentOfSourceController => turn_player != source_controller,
        }
    }
}

impl CombatEventRecord {
    fn object(&self, role: CombatRole) -> Option<&str> {
        match role {
            CombatRole::Attacker => Some(self.attacker.as_str()),
            CombatRole::Blocker => self.blocker.as_deref(),
        }
    }

    fn turn_ordinal(&self, role: CombatRole) -> Option<usize> {
        match role {
            CombatRole::Attacker => self.attacker_turn_ordinal,
            CombatRole::Blocker => self.blocker_turn_ordinal,
        }
    }
}

/// How the pattern's subject relates to the trigger source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SubjectRelation {
    /// The source itself (`this creature`).
    Source,
    /// The permanent the source is attached to (`enchanted creature`).
    AttachedTo,
    /// A permanent controlled by the source's controller.
    ControlledBySourceController,
    /// A permanent with any controller, or an opponent's, other than the
    /// source when `exclude_source` (`a creature attacks`, `another creature
    /// an opponent controls attacks`).
    Permanents {
        scope: PermanentScope,
        exclude_source: bool,
    },
}

/// The controller relation of a `permanents` subject.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PermanentScope {
    Any,
    SourceController,
    OpponentOfSourceController,
}

/// Which characteristics an object predicate reads. New patterns read the
/// effective characteristics; historical kinds keep their established view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ObjectView {
    Effective,
    /// Printed definition plus tapped/attacking/blocking/counter state.
    PermanentState,
    Printed,
}

/// The established publication point of a pattern. Matching is one function;
/// the group only keeps the historical stack order and decision bindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PublicationGroup {
    /// The attacker's own abilities, per attacker.
    AttackSource,
    /// Battlefield observers of each attacker their controller controls.
    AttackControlled,
    /// Abilities of a permanent attached to an attacker, once per source.
    AttackAttached,
    /// Observers of each blocker.
    BlockerDeclared,
    /// Observers of the blocked attacker.
    AttackerBlocked,
    /// Attachments of the attacker, with both combatants bound.
    AttachmentOfAttacker,
    /// Attachments of the blocker, with both combatants bound.
    AttachmentOfBlocker,
    /// Observers of an unblocked attacker.
    Unblocked,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ObjectFilter<'a> {
    pub predicate: &'a Value,
    pub view: ObjectView,
}

/// A trigger event pattern for the combat family.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct CombatEventPattern<'a> {
    pub lemma: CombatLemma,
    pub subject_role: CombatRole,
    pub relation: SubjectRelation,
    pub subject: Option<ObjectFilter<'a>>,
    /// The other combatant of a block record (`blocks a creature with
    /// flying`, `becomes blocked by a Wall`). A pattern with a counterpart
    /// matches once per pair (CR 509.3b, 509.3d).
    pub counterpart: Option<ObjectFilter<'a>>,
    /// `one or more` counterparts: once each combat, at the first block pair
    /// whose counterpart matches.
    pub counterpart_one_or_more: bool,
    /// `blocks` / `becomes blocked` without counterpart: once each combat
    /// for that creature (CR 509.3a, 509.3c).
    pub once_per_combat: bool,
    pub attacking_alone: bool,
    pub declared_attacker: bool,
    pub saddled_only: bool,
    pub face_down_only: bool,
    /// `for the first time each turn`: the subject's per-turn ordinal.
    pub turn_ordinal: Option<usize>,
    /// `during your turn` / `during an opponent's turn` (CR 500.1).
    pub turn_owner: Option<TurnOwnerRelation>,
    /// `attacks you or a planeswalker you control`: the alternatives of the
    /// object the attacker attacks (`{"anyOf": [...]}`), CR 506.2.
    pub attacked: Option<&'a Value>,
    pub group: PublicationGroup,
    /// A pattern written in the generic `eventPattern` IR.
    pub generic: bool,
}

impl<'a> CombatEventPattern<'a> {
    fn new(
        lemma: CombatLemma,
        subject_role: CombatRole,
        relation: SubjectRelation,
        group: PublicationGroup,
    ) -> Self {
        Self {
            lemma,
            subject_role,
            relation,
            subject: None,
            counterpart: None,
            counterpart_one_or_more: false,
            once_per_combat: false,
            attacking_alone: false,
            declared_attacker: false,
            saddled_only: false,
            face_down_only: false,
            turn_ordinal: None,
            turn_owner: None,
            attacked: None,
            group,
            generic: false,
        }
    }

    fn counterpart_role(&self) -> CombatRole {
        match self.subject_role {
            CombatRole::Attacker => CombatRole::Blocker,
            CombatRole::Blocker => CombatRole::Attacker,
        }
    }
}

fn optional_flag(event: &Value, field: &str) -> Option<bool> {
    match event.get(field) {
        None => Some(false),
        Some(value) => value.as_bool(),
    }
}

fn optional_filter<'a>(event: &'a Value, field: &str) -> Option<Option<&'a Value>> {
    match event.get(field) {
        None | Some(Value::Null) => Some(None),
        Some(filter) => card_filter_supported(filter).then_some(Some(filter)),
    }
}

/// The object predicate of an event subject read on the permanent itself
/// (the effective view, or the last-known information of a departed
/// permanent, CR 603.10a): the card predicate plus the permanent-state atoms
/// the battlefield matcher answers, counters (CR 122.1) and designations.
pub(super) fn event_subject_filter_supported(filter: &Value) -> bool {
    permanent_filter_supported(filter)
}

/// Whether a predicate reads permanent state (counters, designations) that
/// a printed-definition view cannot answer.
pub(super) fn filter_reads_permanent_state(filter: &Value) -> bool {
    match value_kind(filter) {
        Some("hasCounter" | "hasDesignation") => true,
        Some("and" | "or") => filter["operands"]
            .as_array()
            .is_some_and(|operands| operands.iter().any(filter_reads_permanent_state)),
        Some("not") => filter_reads_permanent_state(&filter["operand"]),
        _ => false,
    }
}

/// [`optional_filter`] for a subject matched in the effective view, which
/// also reads permanent state.
fn optional_state_filter<'a>(event: &'a Value, field: &str) -> Option<Option<&'a Value>> {
    match event.get(field) {
        None | Some(Value::Null) => Some(None),
        Some(filter) => event_subject_filter_supported(filter).then_some(Some(filter)),
    }
}

/// Support of the controlled-creature aggregate kinds whose subject
/// predicate is matched on each attacking or damage-dealing permanent with
/// the effective matcher, so it may read permanent state: `whenever you
/// attack with one or more creatures with counters on them`, `whenever one
/// or more creatures you control with +1/+1 counters on them deal combat
/// damage to a player`. `None` for every other kind and for a predicate
/// without state atoms, whose historical arm keeps validating it.
pub(super) fn controlled_creature_state_event_supported(event: &Value) -> Option<bool> {
    let filter = event.get("where").filter(|filter| !filter.is_null())?;
    if !filter_reads_permanent_state(filter) {
        return None;
    }
    match value_kind(event)? {
        "controlledCreaturesAttacked" => Some(
            (event["anyPlayer"].as_bool() == Some(true)
                || player_reference_supported(&event["player"])
                || event["opponentOfSourceController"].as_bool() == Some(true))
                && event.get("anyPlayer").is_none_or(Value::is_boolean)
                && event
                    .get("duringOpponentTurn")
                    .is_none_or(Value::is_boolean)
                && event.get("sourceMustAttack").is_none_or(Value::is_boolean)
                && event
                    .get("totalPowerAtLeast")
                    .is_none_or(|value| integer_value(value).is_some_and(|value| value > 0))
                && event_subject_filter_supported(filter)
                && event.get("drawOrdinal").is_none()
                && event.get("exceptFirstInDrawStep").is_none(),
        ),
        "controlledCreaturesCombatDamageToPlayer" => Some(
            player_reference_supported(
                event
                    .get("player")
                    .or_else(|| event.get("sourceController"))
                    .unwrap_or(&Value::Null),
            ) && event_subject_filter_supported(filter)
                && event
                    .get("greatestPowerAmongControllerCreatures")
                    .is_none_or(Value::is_boolean),
        ),
        _ => None,
    }
}

/// `{"kind":"controllerOf","object":{"kind":"self"}}`: the only player
/// relation the controlled-subject kinds are emitted with.
fn is_source_controller(player: &Value) -> bool {
    value_kind(player) == Some("controllerOf")
        && matches!(
            value_kind(&player["object"]),
            Some("self" | "abilitySource")
        )
}

fn has_only_fields(event: &Value, fields: &[&str]) -> bool {
    event
        .as_object()
        .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
}

/// Reads a historical combat kind or a generic `eventPattern` as a pattern.
/// `None` for any other kind; `Some(Err(()))` for a combat event whose
/// attributes the matcher cannot evaluate, which is unsupported, never widened.
pub(super) fn combat_event_pattern(event: &Value) -> Option<Result<CombatEventPattern<'_>, ()>> {
    use CombatLemma::*;
    use CombatRole::*;
    use PublicationGroup::*;
    use SubjectRelation::*;
    let self_object =
        |field: &str| matches!(value_kind(&event[field]), Some("self" | "abilitySource"));
    let pattern = match value_kind(event)? {
        "declaredAttacker" => self_object("object")
            .then(|| CombatEventPattern::new(Attack, Attacker, Source, AttackSource)),
        "declaredBlocker" => self_object("object").then(|| CombatEventPattern {
            once_per_combat: true,
            ..CombatEventPattern::new(Block, Blocker, Source, BlockerDeclared)
        }),
        "permanentBecameBlocked" => self_object("object").then(|| CombatEventPattern {
            once_per_combat: true,
            ..CombatEventPattern::new(Block, Attacker, Source, AttackerBlocked)
        }),
        "controlledCreatureBecameBlocked" => optional_filter(event, "where")
            .filter(|_| is_source_controller(&event["player"]))
            .map(|filter| CombatEventPattern {
                subject: filter.map(|predicate| ObjectFilter {
                    predicate,
                    view: ObjectView::PermanentState,
                }),
                once_per_combat: true,
                ..CombatEventPattern::new(
                    Block,
                    Attacker,
                    ControlledBySourceController,
                    AttackerBlocked,
                )
            }),
        "controlledCreatureDeclaredBlocker" => card_filter_supported(&event["where"])
            .then_some(())
            .filter(|()| is_source_controller(&event["player"]))
            .map(|()| CombatEventPattern {
                subject: Some(ObjectFilter {
                    predicate: &event["where"],
                    view: ObjectView::PermanentState,
                }),
                once_per_combat: true,
                ..CombatEventPattern::new(
                    Block,
                    Blocker,
                    ControlledBySourceController,
                    BlockerDeclared,
                )
            }),
        "controlledCreatureDeclaredAttacker" => {
            match (
                card_filter_supported(&event["where"]) && is_source_controller(&event["player"]),
                optional_flag(event, "saddledOnly"),
                optional_flag(event, "faceDownOnly"),
            ) {
                (true, Some(saddled_only), Some(face_down_only)) => Some(CombatEventPattern {
                    subject: Some(ObjectFilter {
                        predicate: &event["where"],
                        view: ObjectView::Effective,
                    }),
                    saddled_only,
                    face_down_only,
                    ..CombatEventPattern::new(
                        Attack,
                        Attacker,
                        ControlledBySourceController,
                        AttackControlled,
                    )
                }),
                _ => None,
            }
        }
        "attachedPermanentDeclaredAttacker" => {
            (has_only_fields(event, &["kind", "object", "attackingAlone"])
                && event.get("object").is_none_or(|_| self_object("object")))
            .then(|| optional_flag(event, "attackingAlone"))
            .flatten()
            .map(|attacking_alone| CombatEventPattern {
                attacking_alone,
                ..CombatEventPattern::new(Attack, Attacker, AttachedTo, AttackAttached)
            })
        }
        "attachedPermanentDeclaredBlocker" => (has_only_fields(event, &["kind", "object"])
            && event.get("object").is_none_or(|_| self_object("object")))
        .then(|| CombatEventPattern {
            once_per_combat: true,
            ..CombatEventPattern::new(Block, Blocker, AttachedTo, BlockerDeclared)
        }),
        // The historical attachment kinds match each block pair and read the
        // counterpart's printed definition; an absent filter is any creature.
        "attachedCreatureBecameBlocked" | "attachedCreatureBlocks" => {
            let subject_role = if value_kind(event) == Some("attachedCreatureBecameBlocked") {
                Attacker
            } else {
                Blocker
            };
            (self_object("attachment"))
                .then(|| optional_filter(event, "where"))
                .flatten()
                .map(|filter| CombatEventPattern {
                    counterpart: Some(ObjectFilter {
                        predicate: filter.unwrap_or(&Value::Null),
                        view: ObjectView::Printed,
                    }),
                    ..CombatEventPattern::new(
                        Block,
                        subject_role,
                        AttachedTo,
                        if subject_role == Attacker {
                            AttachmentOfAttacker
                        } else {
                            AttachmentOfBlocker
                        },
                    )
                })
        }
        "eventPattern"
            if matches!(
                event["lemma"].as_str(),
                Some("attack" | "block" | "becomeUnblocked")
            ) =>
        {
            generic_combat_pattern(event)
        }
        _ => return None,
    };
    Some(pattern.ok_or(()))
}

fn generic_combat_pattern(event: &Value) -> Option<CombatEventPattern<'_>> {
    use CombatRole::*;
    if !has_only_fields(
        event,
        &[
            "kind",
            "lemma",
            "subject",
            "counterpart",
            "attackingAlone",
            "declaredAttacker",
            "turnOrdinal",
            "duringControllerTurn",
            "duringOpponentTurn",
            "aggregation",
            "attacked",
        ],
    ) {
        return None;
    }
    let lemma = match event["lemma"].as_str()? {
        "attack" => CombatLemma::Attack,
        "block" => CombatLemma::Block,
        "becomeUnblocked" => CombatLemma::BecomeUnblocked,
        _ => return None,
    };
    let subject_role = match event["subject"]["role"].as_str()? {
        "attacker" => Attacker,
        "blocker" => Blocker,
        _ => return None,
    };
    // Only a block record has a blocker.
    if subject_role == Blocker && lemma != CombatLemma::Block {
        return None;
    }
    let object = &event["subject"]["object"];
    if !has_only_fields(&event["subject"], &["role", "object"]) {
        return None;
    }
    let (relation, subject) = match value_kind(object)? {
        "self" => (SubjectRelation::Source, None),
        "attachedPermanent" if value_kind(&object["attachment"]) == Some("self") => {
            (SubjectRelation::AttachedTo, None)
        }
        "controlledPermanent" if is_source_controller(&object["player"]) => (
            SubjectRelation::ControlledBySourceController,
            optional_state_filter(object, "where")?,
        ),
        "permanents"
            if has_only_fields(object, &["kind", "controller", "where", "excludeSource"]) =>
        {
            let scope = match object.get("controller") {
                None => PermanentScope::Any,
                Some(player) if is_source_controller(player) => PermanentScope::SourceController,
                Some(player)
                    if value_kind(player) == Some("opponentsOf")
                        && is_source_controller(&player["player"]) =>
                {
                    PermanentScope::OpponentOfSourceController
                }
                Some(_) => return None,
            };
            (
                SubjectRelation::Permanents {
                    scope,
                    exclude_source: optional_flag(object, "excludeSource")?,
                },
                optional_state_filter(object, "where")?,
            )
        }
        _ => return None,
    };
    let counterpart = match event.get("counterpart") {
        None => None,
        Some(counterpart) => {
            let role = match counterpart["role"].as_str()? {
                "attacker" => Attacker,
                "blocker" => Blocker,
                _ => return None,
            };
            if lemma != CombatLemma::Block
                || role == subject_role
                || !has_only_fields(counterpart, &["role", "where", "oneOrMore"])
            {
                return None;
            }
            Some((
                ObjectFilter {
                    predicate: optional_state_filter(counterpart, "where")?.unwrap_or(&Value::Null),
                    view: ObjectView::Effective,
                },
                optional_flag(counterpart, "oneOrMore")?,
            ))
        }
    };
    let counterpart_one_or_more = counterpart.is_some_and(|(_, one_or_more)| one_or_more);
    let counterpart = counterpart.map(|(filter, _)| filter);
    let turn_ordinal = match event.get("turnOrdinal") {
        None => None,
        // The ordinal is recorded for the attacker of an attack, for each
        // combatant of a block; other roles fail as unsupported.
        Some(value) => {
            let ordinal = integer_value(value).filter(|ordinal| *ordinal > 0)?;
            let recorded = matches!(
                (lemma, subject_role),
                (CombatLemma::Attack | CombatLemma::Block, Attacker)
                    | (CombatLemma::Block, Blocker)
            );
            if !recorded {
                return None;
            }
            Some(usize::try_from(ordinal).ok()?)
        }
    };
    // `one or more`: one trigger per declare-blockers event, claimed by the
    // publication site (CR 603.2c).
    if !event_batch::event_aggregation_supported(event)
        || (event_batch::event_is_aggregated(event) && lemma != CombatLemma::Block)
    {
        return None;
    }
    let attacking_alone = optional_flag(event, "attackingAlone")?;
    let declared_attacker = optional_flag(event, "declaredAttacker")?;
    if (attacking_alone || declared_attacker) && subject_role != Attacker {
        return None;
    }
    // The attacked object is an attribute of the attacker's attack record.
    let attacked = match event.get("attacked") {
        None => None,
        Some(attacked)
            if lemma == CombatLemma::Attack
                && subject_role == Attacker
                && attacked_recipients_supported(attacked) =>
        {
            Some(attacked)
        }
        Some(_) => return None,
    };
    let group = match (lemma, subject_role, relation) {
        (CombatLemma::Attack, _, SubjectRelation::Source) => PublicationGroup::AttackSource,
        (
            CombatLemma::Attack,
            _,
            SubjectRelation::ControlledBySourceController | SubjectRelation::Permanents { .. },
        ) => PublicationGroup::AttackControlled,
        (CombatLemma::Attack, _, SubjectRelation::AttachedTo) => PublicationGroup::AttackAttached,
        (CombatLemma::Block, Blocker, SubjectRelation::AttachedTo) => {
            PublicationGroup::AttachmentOfBlocker
        }
        (CombatLemma::Block, Attacker, SubjectRelation::AttachedTo) => {
            PublicationGroup::AttachmentOfAttacker
        }
        (CombatLemma::Block, Blocker, _) => PublicationGroup::BlockerDeclared,
        (CombatLemma::Block, Attacker, _) => PublicationGroup::AttackerBlocked,
        (CombatLemma::BecomeUnblocked, _, _) => PublicationGroup::Unblocked,
    };
    Some(CombatEventPattern {
        subject: subject.map(|predicate| ObjectFilter {
            predicate,
            view: ObjectView::Effective,
        }),
        once_per_combat: lemma == CombatLemma::Block && counterpart.is_none(),
        counterpart,
        counterpart_one_or_more,
        attacking_alone,
        declared_attacker,
        turn_ordinal,
        turn_owner: TurnOwnerRelation::from_pattern(event)?,
        attacked,
        generic: true,
        ..CombatEventPattern::new(lemma, subject_role, relation, group)
    })
}

/// `{"anyOf": [ALTERNATIVE, ...]}`: each alternative a player relation
/// (`{"kind":"player","player":REF}`) or a permanent query
/// (`{"kind":"permanents","controller":REF,"where":FILTER}`).
fn attacked_recipients_supported(attacked: &Value) -> bool {
    has_only_fields(attacked, &["anyOf"])
        && attacked["anyOf"].as_array().is_some_and(|alternatives| {
            !alternatives.is_empty()
                && alternatives
                    .iter()
                    .all(|alternative| match value_kind(alternative) {
                        Some("player") => {
                            has_only_fields(alternative, &["kind", "player", "where"])
                                && player_action_events::event_player_reference_supported(
                                    &alternative["player"],
                                )
                                && alternative
                                    .get("where")
                                    .is_none_or(attacked_player_condition_supported)
                        }
                        Some("permanents") => {
                            has_only_fields(alternative, &["kind", "controller", "where"])
                                && alternative.get("where").is_none_or(card_filter_supported)
                                && alternative.get("controller").is_none_or(
                                    player_action_events::event_player_reference_supported,
                                )
                        }
                        _ => false,
                    })
        })
}

/// The member condition of an attacked player (`a player who controls
/// eight or more lands`): the shared `filterPlayers` member condition,
/// whose `quantifiedPlayer` is the attacked player.
fn attacked_player_condition_supported(condition: &Value) -> bool {
    super::effect_domains::player_designations::filtered_players_supported(&json!({
        "kind": "filterPlayers",
        "players": {"kind": "eachPlayer"},
        "where": condition,
    }))
}

/// Event kinds whose matcher evaluates a compound member's own
/// `eventCondition` when it observes the occurrence (CR 603.2).
const MEMBER_EVENT_CONDITION_KINDS: [&str; 2] = ["controlledCreaturesAttacked", "turnedFaceUp"];

/// A compound member's `eventCondition` is supported only on a kind whose
/// matcher evaluates it; any other placement is unsupported, never ignored.
pub(super) fn member_event_condition_supported(event: &Value) -> bool {
    event.get("eventCondition").is_none_or(|condition| {
        value_kind(event).is_some_and(|kind| MEMBER_EVENT_CONDITION_KINDS.contains(&kind))
            && condition_supported(condition)
    })
}

/// `turnedFaceUp` observing another permanent (CR 708.8): `None` for the
/// self form (the established arm) and for other kinds.
pub(super) fn turned_face_up_event_supported(event: &Value) -> Option<bool> {
    if value_kind(event) != Some("turnedFaceUp")
        || value_kind(&event["object"]) != Some("permanents")
    {
        return None;
    }
    let object = &event["object"];
    Some(
        has_only_fields(event, &["kind", "object", "eventCondition"])
            && has_only_fields(object, &["kind", "controller", "where", "excludeSource"])
            && object.get("where").is_none_or(card_filter_supported)
            && object
                .get("controller")
                .is_none_or(player_action_events::event_player_reference_supported)
            && object.get("excludeSource").is_none_or(Value::is_boolean),
    )
}

/// `permanentUntapped` of the source (CR 701.26b: a tapped permanent
/// becomes untapped): `None` for other kinds.
pub(super) fn became_untapped_event_supported(event: &Value) -> Option<bool> {
    (value_kind(event) == Some("permanentUntapped")).then(|| {
        has_only_fields(event, &["kind", "object"])
            && matches!(value_kind(&event["object"]), Some("self" | "abilitySource"))
    })
}

/// Support of a combat-family trigger event; `None` for other families.
pub(super) fn combat_event_supported(event: &Value) -> Option<bool> {
    combat_event_pattern(event).map(|pattern| pattern.is_ok())
}

/// The combat patterns of a rule's event (its `oneOf` variants included).
pub(super) fn rule_combat_patterns(rule: &Value) -> Vec<CombatEventPattern<'_>> {
    triggered_event_variants(&rule["event"])
        .into_iter()
        .filter_map(|event| combat_event_pattern(event).and_then(Result::ok))
        .collect()
}

impl GameEngine {
    fn combat_object_matches(&self, object: &CardInstance, filter: ObjectFilter<'_>) -> bool {
        match filter.view {
            ObjectView::Effective => {
                filter.predicate.is_null()
                    || self.permanent_matches_effective_filter(object, filter.predicate)
            }
            ObjectView::PermanentState => self.permanent_state_matches(object, filter.predicate),
            ObjectView::Printed => matches_card_filter(
                object_predicate::DefinitionView::for_permanent(object),
                filter.predicate,
            ),
        }
    }

    fn combat_permanent(&self, instance_id: &str) -> Option<&CardInstance> {
        self.permanent_position(instance_id)
            .map(|(player_index, card_index)| {
                &self.state.players[player_index].battlefield[card_index]
            })
    }

    pub(super) fn combat_attacker_instance(&self, instance_id: &str) -> Option<CardInstance> {
        self.combat_permanent(instance_id).cloned()
    }

    fn permanent_was_saddled_this_turn(&self, permanent: &CardInstance) -> bool {
        self.state.rule_modifiers.iter().any(|modifier| {
            value_kind(modifier) == Some("saddled")
                && modifier["cardInstanceId"].as_str() == Some(permanent.instance_id.as_str())
                && modifier["expiresAfterTurn"].as_u64() == Some(u64::from(self.state.turn_number))
        })
    }

    /// The one matcher: does `record` satisfy `pattern` observed by `source`?
    pub(super) fn combat_event_matches(
        &self,
        pattern: &CombatEventPattern<'_>,
        record: &CombatEventRecord,
        source: &CardInstance,
    ) -> bool {
        if pattern.lemma != record.lemma {
            return false;
        }
        let Some(subject_id) = record.object(pattern.subject_role) else {
            return false;
        };
        let related = match pattern.relation {
            SubjectRelation::Source => source.instance_id == subject_id,
            SubjectRelation::AttachedTo => source.attached_to.as_deref() == Some(subject_id),
            SubjectRelation::ControlledBySourceController => self
                .combat_permanent(subject_id)
                .is_some_and(|subject| subject.controller == source.controller),
            SubjectRelation::Permanents {
                scope,
                exclude_source,
            } => {
                !(exclude_source && source.instance_id == subject_id)
                    && self
                        .combat_permanent(subject_id)
                        .is_some_and(|subject| match scope {
                            PermanentScope::Any => true,
                            PermanentScope::SourceController => {
                                subject.controller == source.controller
                            }
                            PermanentScope::OpponentOfSourceController => {
                                subject.controller != source.controller
                            }
                        })
            }
        };
        if !related {
            return false;
        }
        if pattern.attacking_alone && !record.attacking_alone {
            return false;
        }
        if pattern.declared_attacker && !record.declared_attacker {
            return false;
        }
        if pattern.once_per_combat {
            let first = match pattern.subject_role {
                CombatRole::Attacker => record.first_block_of_attacker,
                CombatRole::Blocker => record.first_block_by_blocker,
            };
            if !first {
                return false;
            }
        }
        if let Some(ordinal) = pattern.turn_ordinal
            && record.turn_ordinal(pattern.subject_role) != Some(ordinal)
        {
            return false;
        }
        if let Some(owner) = pattern.turn_owner
            && !owner.matches(&record.turn_player, &source.controller)
        {
            return false;
        }
        if let Some(attacked) = pattern.attacked
            && !self.attacked_recipient_matches(attacked, record, source)
        {
            return false;
        }
        if pattern.subject.is_some() || pattern.saddled_only || pattern.face_down_only {
            let Some(subject) = self.combat_permanent(subject_id) else {
                return false;
            };
            if pattern
                .subject
                .is_some_and(|filter| !self.combat_object_matches(subject, filter))
                || (pattern.saddled_only && !self.permanent_was_saddled_this_turn(subject))
                || (pattern.face_down_only
                    && !subject.flags.get("faceDown").copied().unwrap_or(false))
            {
                return false;
            }
        }
        if let Some(filter) = pattern.counterpart {
            let Some(counterpart) = record
                .object(pattern.counterpart_role())
                .and_then(|id| self.combat_permanent(id))
            else {
                return false;
            };
            if !self.combat_object_matches(counterpart, filter) {
                return false;
            }
            if pattern.counterpart_one_or_more
                && self.earlier_matching_counterpart(pattern, record, subject_id, filter)
            {
                return false;
            }
        }
        true
    }

    /// Whether an earlier block pair of this combat already gave the subject
    /// a matching counterpart (`becomes blocked by one or more ...`).
    fn earlier_matching_counterpart(
        &self,
        pattern: &CombatEventPattern<'_>,
        record: &CombatEventRecord,
        subject_id: &str,
        filter: ObjectFilter<'_>,
    ) -> bool {
        let blocker = record.blocker.as_deref().unwrap_or_default();
        let current = self
            .state
            .combat
            .blockers
            .iter()
            .position(|block| block.attacker_id == record.attacker && block.blocker_id == blocker)
            .unwrap_or(self.state.combat.blockers.len());
        self.state.combat.blockers[..current].iter().any(|block| {
            let (subject, counterpart) = match pattern.subject_role {
                CombatRole::Attacker => (&block.attacker_id, &block.blocker_id),
                CombatRole::Blocker => (&block.blocker_id, &block.attacker_id),
            };
            subject == subject_id
                && self
                    .combat_permanent(counterpart)
                    .is_some_and(|counterpart| self.combat_object_matches(counterpart, filter))
        })
    }

    /// Whether some combat pattern of `rule` in `group` matches `record`.
    pub(super) fn rule_matches_combat_record(
        &self,
        rule: &Value,
        group: PublicationGroup,
        record: &CombatEventRecord,
        source: &CardInstance,
    ) -> bool {
        rule_combat_patterns(rule).iter().any(|pattern| {
            pattern.group == group && self.combat_event_matches(pattern, record, source)
        })
    }

    /// Battlefield triggered rules of `group` matching `record`, in
    /// battlefield order (the established publication order).
    pub(super) fn combat_record_triggers(
        &self,
        record: &CombatEventRecord,
        group: PublicationGroup,
    ) -> Vec<(CardInstance, Value)> {
        self.state
            .players
            .iter()
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(&source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && self.rule_matches_combat_record(rule, group, record, source)
                    })
                    .map(|rule| (source.clone(), rule.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Per-turn ordinal of a logged combat event kind for one object: the
    /// current event is logged before its triggers are collected. Only the
    /// object's current incarnation counts: after a zone change it is a new
    /// object with no history (CR 400.7).
    fn combat_turn_ordinal(&self, logged_kind: &str, instance_id: &str) -> usize {
        let identity = self.object_identity_value(instance_id);
        self.state
            .events
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == logged_kind
                    && event.card_instance_id.as_deref() == Some(instance_id)
                    && event.detail.get("objectIdentity").unwrap_or(&Value::Null) == &identity
            })
            .count()
    }

    /// The number of combats this turn in which the attacker's current
    /// incarnation became blocked, this one included (`becomes blocked for
    /// the first time each turn`). A creature becomes blocked once per combat
    /// (CR 509.1h): each attack declaration opens a combat, and only its
    /// first logged block of the attacker counts. The current block is logged
    /// before its triggers are collected. A block logged without the
    /// attacker's identity cannot be attributed to this object and is not
    /// counted (CR 400.7).
    fn attacker_blocked_turn_ordinal(&self, attacker_id: &str) -> usize {
        let identity = self.object_identity_value(attacker_id);
        let mut combats = 0;
        let mut blocked_this_combat = false;
        for event in self
            .state
            .events
            .iter()
            .filter(|event| event.turn_number == self.state.turn_number)
        {
            match event.kind.as_str() {
                "attackDeclared" => blocked_this_combat = false,
                "blockerDeclared"
                    if !blocked_this_combat
                        && event.detail["attackerId"].as_str() == Some(attacker_id)
                        && event.detail.get("attackerIdentity").unwrap_or(&Value::Null)
                            == &identity =>
                {
                    blocked_this_combat = true;
                    combats += 1;
                }
                _ => {}
            }
        }
        combats
    }

    /// The attack declaration of the current combat (`attackDeclared`).
    fn current_attack_declaration(&self) -> Vec<String> {
        self.state
            .events
            .iter()
            .rev()
            .find(|event| {
                event.turn_number == self.state.turn_number && event.kind == "attackDeclared"
            })
            .and_then(|event| event.detail["attackerIds"].as_array())
            .map(|ids| {
                ids.iter()
                    .filter_map(Value::as_str)
                    .map(ToOwned::to_owned)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Whether the attacked object of `record` is one of the alternatives:
    /// a player in the given relation to the source, or a planeswalker or
    /// battle matching the permanent query (CR 506.2).
    fn attacked_recipient_matches(
        &self,
        attacked: &Value,
        record: &CombatEventRecord,
        source: &CardInstance,
    ) -> bool {
        attacked["anyOf"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|alternative| match value_kind(alternative) {
                Some("player") => {
                    record.attacked_permanent.is_none()
                        && self.event_player_reference_matches(
                            &alternative["player"],
                            source,
                            &record.defending_player,
                        )
                        && alternative.get("where").is_none_or(|condition| {
                            self.attacked_player_condition_holds(
                                condition,
                                &record.defending_player,
                            )
                        })
                }
                Some("permanents") => record
                    .attacked_permanent
                    .as_deref()
                    .and_then(|id| self.combat_permanent(id))
                    .is_some_and(|permanent| {
                        alternative.get("controller").is_none_or(|controller| {
                            self.event_player_reference_matches(
                                controller,
                                source,
                                &permanent.controller,
                            )
                        }) && alternative.get("where").is_none_or(|filter| {
                            self.permanent_matches_effective_filter(permanent, filter)
                        })
                    }),
                _ => false,
            })
    }

    /// The member condition of an attacked player, read with that player as
    /// the condition's reference player (as `filterPlayers` reads each
    /// member), when the attack is declared (CR 506.2, 603.2).
    fn attacked_player_condition_holds(&self, condition: &Value, attacked_player_id: &str) -> bool {
        let condition = crate::engine::player_quantifier::bind_quantified_player(condition);
        self.condition_value_with_source(&condition, attacked_player_id, &BTreeMap::new(), None)
    }

    /// The planeswalker or battle an attacker attacks, if not a player.
    fn attacked_permanent_of(&self, attacker_id: &str) -> Option<String> {
        self.state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == attacker_id)
            .and_then(|assignment| match &assignment.defender {
                TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                _ => None,
            })
    }

    fn defending_player_of(&self, attacker_id: &str) -> String {
        self.state
            .combat
            .attackers
            .iter()
            .find(|assignment| assignment.attacker_id == attacker_id)
            .map(|assignment| assignment.defending_player_id.clone())
            .unwrap_or_default()
    }

    /// One record per declared attacker of one declaration (CR 508.1).
    pub(super) fn attack_records(&self, attacker_ids: &[String]) -> Vec<CombatEventRecord> {
        let actor = self.state.players[self.state.active_player].id.clone();
        attacker_ids
            .iter()
            .map(|attacker_id| CombatEventRecord {
                lemma: CombatLemma::Attack,
                actor: actor.clone(),
                attacker: attacker_id.clone(),
                blocker: None,
                defending_player: self.defending_player_of(attacker_id),
                declared_attacker: true,
                attacking_alone: attacker_ids.len() == 1,
                first_block_of_attacker: false,
                first_block_by_blocker: false,
                attacker_turn_ordinal: Some(
                    self.combat_turn_ordinal("attackerDeclared", attacker_id),
                ),
                blocker_turn_ordinal: None,
                turn_player: actor.clone(),
                attacked_permanent: self.attacked_permanent_of(attacker_id),
            })
            .collect()
    }

    /// The record of one declared block pair, after the pair was added to
    /// the combat state and logged (CR 509.1).
    pub(super) fn block_record(
        &self,
        attacker_id: &str,
        blocker_id: &str,
        declaring_player: &str,
    ) -> CombatEventRecord {
        let blockers = &self.state.combat.blockers;
        let declaration = self.current_attack_declaration();
        CombatEventRecord {
            lemma: CombatLemma::Block,
            actor: declaring_player.to_string(),
            attacker: attacker_id.to_string(),
            blocker: Some(blocker_id.to_string()),
            defending_player: self.defending_player_of(attacker_id),
            declared_attacker: declaration.iter().any(|id| id == attacker_id),
            attacking_alone: declaration.len() == 1 && declaration[0] == attacker_id,
            first_block_of_attacker: blockers
                .iter()
                .filter(|block| block.attacker_id == attacker_id)
                .count()
                == 1,
            first_block_by_blocker: blockers
                .iter()
                .filter(|block| block.blocker_id == blocker_id)
                .count()
                == 1,
            attacker_turn_ordinal: Some(self.attacker_blocked_turn_ordinal(attacker_id)),
            blocker_turn_ordinal: Some(self.combat_turn_ordinal("blockerDeclared", blocker_id)),
            turn_player: self.state.players[self.state.active_player].id.clone(),
            attacked_permanent: self.attacked_permanent_of(attacker_id),
        }
    }

    /// One record per attacking creature without blocker once blockers are
    /// declared (CR 509.1h), in attacker order.
    pub(super) fn unblocked_records(
        &self,
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> Vec<CombatEventRecord> {
        let actor = self.state.players[self.state.active_player].id.clone();
        let declaration = self.current_attack_declaration();
        self.state
            .combat
            .attackers
            .iter()
            .filter(|assignment| {
                blocks
                    .get(&assignment.attacker_id)
                    .is_none_or(|blockers| blockers.is_empty())
                    && self.combat_permanent(&assignment.attacker_id).is_some()
            })
            .map(|assignment| CombatEventRecord {
                lemma: CombatLemma::BecomeUnblocked,
                actor: actor.clone(),
                attacker: assignment.attacker_id.clone(),
                blocker: None,
                defending_player: assignment.defending_player_id.clone(),
                declared_attacker: declaration.contains(&assignment.attacker_id),
                attacking_alone: declaration.len() == 1 && declaration[0] == assignment.attacker_id,
                first_block_of_attacker: false,
                first_block_by_blocker: false,
                attacker_turn_ordinal: None,
                blocker_turn_ordinal: None,
                turn_player: actor.clone(),
                attacked_permanent: match &assignment.defender {
                    TargetRef::Permanent { instance_id } => Some(instance_id.clone()),
                    _ => None,
                },
            })
            .collect()
    }

    /// Publishes the observers of unblocked attackers (generic patterns
    /// only: no historical kind observes this record).
    pub(super) fn publish_unblocked_triggers(
        &mut self,
        blocks: &BTreeMap<String, Vec<String>>,
    ) -> Result<(), EngineError> {
        for record in self.unblocked_records(blocks) {
            for (source, rule) in self.combat_record_triggers(&record, PublicationGroup::Unblocked)
            {
                self.put_triggered_ability_on_stack_with_decisions(
                    &source,
                    rule,
                    BTreeMap::from([
                        (
                            "triggeringPermanentId".to_string(),
                            Value::String(record.attacker.clone()),
                        ),
                        (
                            "triggeringPlayerId".to_string(),
                            Value::String(record.defending_player.clone()),
                        ),
                    ]),
                );
                self.check_quantity_failure()?;
            }
        }
        Ok(())
    }

    /// `Whenever one or more CREATURES you control attack, ... that many`
    /// (CR 508.1, 603.2c): the number of the source controller's attackers
    /// of this declaration that the batch pattern matched, bound as the
    /// event's object count. Rules without that pattern bind nothing.
    pub(super) fn bind_attack_batch_count(
        &self,
        rule: &Value,
        source: &CardInstance,
        attacker_ids: &[String],
        decisions: &mut BTreeMap<String, Value>,
    ) {
        let Some(event) = triggered_event_variants(&rule["event"])
            .into_iter()
            .find(|event| value_kind(event) == Some("controlledCreaturesAttacked"))
        else {
            return;
        };
        let count = attacker_ids
            .iter()
            .filter_map(|attacker_id| self.permanent_position(attacker_id))
            .filter(|(player_index, card_index)| {
                let permanent = &self.state.players[*player_index].battlefield[*card_index];
                permanent.controller == source.controller
                    && event.get("where").is_none_or(|filter| {
                        self.permanent_matches_effective_filter(permanent, filter)
                    })
            })
            .count();
        decisions.insert("eventObjectCount".to_string(), Value::from(count as u64));
    }

    /// Bindings of a generic block pattern: both combatants, and the other
    /// combatant of the subject with its object identity (CR 400.7).
    pub(super) fn bind_generic_block_counterparts(
        &self,
        rule: &Value,
        group: PublicationGroup,
        record: &CombatEventRecord,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        if !rule_combat_patterns(rule)
            .iter()
            .any(|pattern| pattern.generic && pattern.group == group)
        {
            return;
        }
        let blocker = record.blocker.clone().unwrap_or_default();
        decisions.insert(
            "blockingPermanentId".to_string(),
            Value::String(blocker.clone()),
        );
        decisions.insert(
            "blockedPermanentId".to_string(),
            Value::String(record.attacker.clone()),
        );
        let counterpart = match group {
            PublicationGroup::BlockerDeclared | PublicationGroup::AttachmentOfBlocker => {
                record.attacker.clone()
            }
            _ => blocker,
        };
        decisions.insert(
            COUNTERPART_IDENTITY.to_string(),
            self.object_identity_value(&counterpart),
        );
        decisions.insert(COUNTERPART_ID.to_string(), Value::String(counterpart));
    }

    /// The current incarnation of a permanent, or null when it has none.
    pub(super) fn object_identity_value(&self, instance_id: &str) -> Value {
        self.combat_permanent(instance_id)
            .and_then(Self::tracked_zone_identity)
            .map_or(Value::Null, |identity| Value::String(identity.to_string()))
    }

    /// The decisions of event references read by delayed effects, copied
    /// when the delayed trigger is created (the other combatant and its
    /// identity), so the later trigger denotes the same object.
    pub(super) fn transported_event_references(
        &self,
        effects: &Value,
        stack_object: &StackObject,
    ) -> Vec<(String, Value)> {
        fn reads_counterpart(value: &Value) -> bool {
            match value {
                Value::Object(map) => {
                    value_kind(value) == Some("combatCounterpart")
                        || map.values().any(reads_counterpart)
                }
                Value::Array(items) => items.iter().any(reads_counterpart),
                _ => false,
            }
        }
        if !reads_counterpart(effects) {
            return Vec::new();
        }
        [COUNTERPART_ID, COUNTERPART_IDENTITY]
            .into_iter()
            .filter_map(|key| {
                stack_object
                    .decisions
                    .get(key)
                    .map(|value| (key.to_string(), value.clone()))
            })
            .collect()
    }

    /// The bound counterpart, only while it is the same object it was when
    /// the ability triggered (CR 400.7).
    pub(super) fn combat_counterpart_target(
        &self,
        stack_object: &StackObject,
    ) -> Option<TargetRef> {
        let instance_id = stack_object.decisions.get(COUNTERPART_ID)?.as_str()?;
        let captured = stack_object
            .decisions
            .get(COUNTERPART_IDENTITY)
            .cloned()
            .unwrap_or(Value::Null);
        (self.combat_permanent(instance_id).is_some()
            && self.object_identity_value(instance_id) == captured)
            .then(|| TargetRef::Permanent {
                instance_id: instance_id.to_string(),
            })
    }

    /// Publishes the observers of one declared block pair, in the
    /// established order: blocker observers, then observers of the blocked
    /// attacker. Attachment observers follow in a separate step.
    pub(super) fn publish_block_observer_triggers(
        &mut self,
        record: &CombatEventRecord,
    ) -> Result<(), EngineError> {
        let blocker_id = record.blocker.clone().unwrap_or_default();
        for (source, rule) in self.combat_record_triggers(record, PublicationGroup::BlockerDeclared)
        {
            if self.aggregated_trigger_already_claimed(&source.instance_id, &rule) {
                continue;
            }
            let mut decisions = BTreeMap::from([(
                "triggeringPermanentId".to_string(),
                Value::String(blocker_id.clone()),
            )]);
            self.bind_generic_block_counterparts(
                &rule,
                PublicationGroup::BlockerDeclared,
                record,
                &mut decisions,
            );
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            self.check_quantity_failure()?;
        }
        self.publish_created_blocker_triggers(record)?;
        for (source, rule) in self.combat_record_triggers(record, PublicationGroup::AttackerBlocked)
        {
            if self.aggregated_trigger_already_claimed(&source.instance_id, &rule) {
                continue;
            }
            let mut decisions = BTreeMap::from([(
                "triggeringPermanentId".to_string(),
                Value::String(record.attacker.clone()),
            )]);
            if rule_combat_patterns(&rule)
                .iter()
                .any(|pattern| pattern.group == PublicationGroup::AttackerBlocked)
            {
                // The defending player of the blocked attacker (CR 506.2),
                // whichever attacker the pattern observes.
                decisions.insert(
                    "triggeringPlayerId".to_string(),
                    Value::String(record.defending_player.clone()),
                );
            }
            self.bind_generic_block_counterparts(
                &rule,
                PublicationGroup::AttackerBlocked,
                record,
                &mut decisions,
            );
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            self.check_quantity_failure()?;
        }
        Ok(())
    }

    /// Publishes the attachments of the attacker, then of the blocker, with
    /// both combatants bound.
    pub(super) fn publish_block_attachment_triggers(
        &mut self,
        record: &CombatEventRecord,
    ) -> Result<(), EngineError> {
        let blocker_id = record.blocker.clone().unwrap_or_default();
        let mut attachment_triggers = self
            .combat_record_triggers(record, PublicationGroup::AttachmentOfAttacker)
            .into_iter()
            .map(|trigger| (PublicationGroup::AttachmentOfAttacker, trigger))
            .collect::<Vec<_>>();
        attachment_triggers.extend(
            self.combat_record_triggers(record, PublicationGroup::AttachmentOfBlocker)
                .into_iter()
                .map(|trigger| (PublicationGroup::AttachmentOfBlocker, trigger)),
        );
        for (group, (source, rule)) in attachment_triggers {
            let mut decisions = BTreeMap::from([
                (
                    "triggeringPermanentId".to_string(),
                    Value::String(record.attacker.clone()),
                ),
                (
                    "blockingPermanentId".to_string(),
                    Value::String(blocker_id.clone()),
                ),
            ]);
            self.bind_generic_block_counterparts(&rule, group, record, &mut decisions);
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            self.check_quantity_failure()?;
        }
        Ok(())
    }

    /// The rule's `eventCondition` (CR 603.2): a state the occurrence
    /// requires, evaluated once as the event is observed and never on
    /// resolution (that is the intervening `if`, CR 603.4). `true` when the
    /// condition suppresses this publication.
    pub(super) fn event_condition_suppresses(
        &self,
        rule: &Value,
        source: &CardInstance,
        decisions: &BTreeMap<String, Value>,
    ) -> bool {
        rule.get("eventCondition").is_some_and(|condition| {
            !self.trigger_publication_condition_holds(condition, source, decisions)
        })
    }

    /// A compound member's own `eventCondition`, evaluated by the matcher
    /// of that member's kind as it observes the occurrence.
    pub(super) fn member_event_condition_holds(
        &self,
        event: &Value,
        source: &CardInstance,
    ) -> bool {
        event.get("eventCondition").is_none_or(|condition| {
            self.trigger_publication_condition_holds(condition, source, &BTreeMap::new())
        })
    }

    /// Whether the `turnedFaceUp` pattern observed by `source` matches the
    /// permanent `turned` (CR 708.8): the source itself, or a permanent
    /// matching the shared query (controller relation, effective filter,
    /// source exclusion).
    fn turned_face_up_matches(
        &self,
        event: &Value,
        source: &CardInstance,
        turned: &CardInstance,
    ) -> bool {
        let object = &event["object"];
        let object_matches = match value_kind(object) {
            Some("self" | "abilitySource") => source.instance_id == turned.instance_id,
            Some("permanents") => {
                !(object["excludeSource"].as_bool() == Some(true)
                    && source.instance_id == turned.instance_id)
                    && object.get("controller").is_none_or(|controller| {
                        self.event_player_reference_matches(controller, source, &turned.controller)
                    })
                    && object.get("where").is_none_or(|filter| {
                        self.permanent_matches_effective_filter(turned, filter)
                    })
            }
            _ => false,
        };
        object_matches && self.member_event_condition_holds(event, source)
    }

    /// Publishes every battlefield observer of a permanent turned face up
    /// (CR 708.8), in battlefield order. The permanent's own `turned face
    /// up` abilities keep their established binding-free publication; an
    /// observer of another permanent binds it as the triggering permanent.
    pub(super) fn publish_turned_face_up_observers(&mut self, turned: &CardInstance) {
        let triggers = self
            .state
            .players
            .iter()
            .filter(|player| !player.has_lost)
            .flat_map(|player| player.battlefield.iter())
            .flat_map(|source| {
                self.effective_rules(source)
                    .iter()
                    .filter(|rule| {
                        triggered_rule_supported(rule)
                            && triggered_event_variants(&rule["event"])
                                .iter()
                                .any(|event| {
                                    value_kind(event) == Some("turnedFaceUp")
                                        && self.turned_face_up_matches(event, source, turned)
                                })
                    })
                    .map(|rule| (source.clone(), rule.clone()))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        for (source, rule) in triggers {
            if source.instance_id == turned.instance_id {
                self.put_triggered_ability_on_stack(&source, rule);
            } else {
                self.put_triggered_ability_on_stack_with_decisions(
                    &source,
                    rule,
                    BTreeMap::from([(
                        "triggeringPermanentId".to_string(),
                        Value::String(turned.instance_id.clone()),
                    )]),
                );
            }
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// Publishes the abilities of a permanent that became untapped (CR
    /// 701.26b, `whenever this creature becomes untapped`), whatever untapped
    /// it: the untap step, an effect or another rule. A stun counter that
    /// replaced the untap is no untap.
    pub(super) fn publish_became_untapped_observers(&mut self, instance_id: &str) {
        let Some(source) = self.combat_permanent(instance_id).cloned() else {
            return;
        };
        let rules = self
            .effective_rules(&source)
            .iter()
            .filter(|rule| {
                triggered_rule_supported(rule)
                    && triggered_event_variants(&rule["event"])
                        .iter()
                        .any(|event| {
                            value_kind(event) == Some("permanentUntapped")
                                && matches!(
                                    value_kind(&event["object"]),
                                    Some("self" | "abilitySource")
                                )
                        })
            })
            .cloned()
            .collect::<Vec<_>>();
        for rule in rules {
            self.put_triggered_ability_on_stack(&source, rule);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }

    /// Publishes the abilities of a permanent that gained a designation it
    /// did not have (CR 701.37b `becomes monstrous`, 702.112b `becomes
    /// renowned`): the permanent's own `designationGained` observers.
    pub(super) fn publish_designation_gained_observers(
        &mut self,
        instance_id: &str,
        designation: &str,
    ) {
        let Some(source) = self.combat_permanent(instance_id).cloned() else {
            return;
        };
        let rules = self
            .effective_rules(&source)
            .iter()
            .filter(|rule| {
                triggered_rule_supported(rule)
                    && self.rule_active_for_source(&source, rule)
                    && triggered_event_variants(&rule["event"])
                        .iter()
                        .any(|event| {
                            value_kind(event) == Some("designationGained")
                                && matches!(
                                    value_kind(&event["object"]),
                                    Some("self" | "abilitySource")
                                )
                                && event["designation"].as_str() == Some(designation)
                        })
            })
            .cloned()
            .collect::<Vec<_>>();
        for rule in rules {
            self.put_triggered_ability_on_stack(&source, rule);
            if self.check_quantity_failure().is_err() {
                return;
            }
        }
    }
}

/// `designationGained` of the source (CR 701.37b, 702.112b): `None` for
/// other kinds. Only designations given by an action are observed.
pub(super) fn designation_gained_event_supported(event: &Value) -> Option<bool> {
    (value_kind(event) == Some("designationGained")).then(|| {
        has_only_fields(event, &["kind", "object", "designation"])
            && matches!(value_kind(&event["object"]), Some("self" | "abilitySource"))
            && matches!(
                event["designation"].as_str(),
                Some("monstrous" | "renowned")
            )
    })
}

/// `attackersAttackedPlayer` (CR 508.1, 603.2c): one declaration of
/// attackers is one event for each player it attacks. The pattern counts the
/// attackers that attack that player directly (not a planeswalker or
/// battle, CR 506.2) and match the attackers' controller relation and
/// predicate; the attacked player matches the written relation (`you`, `an
/// opponent`, `a player`). `None` for other kinds.
pub(super) fn attackers_attacked_player_event_supported(event: &Value) -> Option<bool> {
    (value_kind(event) == Some("attackersAttackedPlayer")).then(|| {
        let attackers = &event["attackers"];
        has_only_fields(event, &["kind", "attackers", "attacked"])
            && attackers.is_object()
            && has_only_fields(attackers, &["controller", "where", "minimum"])
            && attackers
                .get("controller")
                .is_none_or(player_action_events::event_player_reference_supported)
            && attackers
                .get("where")
                .is_none_or(event_subject_filter_supported)
            && attackers
                .get("minimum")
                .is_none_or(|minimum| integer_value(minimum).is_some_and(|minimum| minimum >= 1))
            && player_action_events::event_player_reference_supported(&event["attacked"])
    })
}

impl GameEngine {
    /// The attackers of one declaration that attack `defending_player_id`
    /// directly, grouped per attacked player in declaration order.
    fn attackers_by_attacked_player(
        &self,
        attacker_ids: &[String],
    ) -> Vec<(String, Vec<CardInstance>)> {
        let mut groups: Vec<(String, Vec<CardInstance>)> = Vec::new();
        for attacker_id in attacker_ids {
            let Some(assignment) = self
                .state
                .combat
                .attackers
                .iter()
                .find(|assignment| assignment.attacker_id == *attacker_id)
            else {
                continue;
            };
            let TargetRef::Player { player_id } = &assignment.defender else {
                continue;
            };
            let Some(attacker) = self.combat_permanent(attacker_id).cloned() else {
                continue;
            };
            match groups.iter_mut().find(|(player, _)| player == player_id) {
                Some((_, attackers)) => attackers.push(attacker),
                None => groups.push((player_id.clone(), vec![attacker])),
            }
        }
        groups
    }

    /// The attackers among `attackers` that the pattern's controller
    /// relation and predicate select, seen from `source`.
    fn attackers_attacked_player_count(
        &self,
        event: &Value,
        source: &CardInstance,
        attackers: &[CardInstance],
    ) -> usize {
        let pattern = &event["attackers"];
        attackers
            .iter()
            .filter(|attacker| {
                pattern.get("controller").is_none_or(|controller| {
                    self.event_player_reference_matches(controller, source, &attacker.controller)
                }) && pattern
                    .get("where")
                    .is_none_or(|filter| self.permanent_matches_effective_filter(attacker, filter))
            })
            .count()
    }

    /// Whether `event` of `source` observes the attackers of one declaration
    /// that attack `defending_player_id`.
    fn attackers_attacked_player_matches(
        &self,
        event: &Value,
        source: &CardInstance,
        defending_player_id: &str,
        attackers: &[CardInstance],
    ) -> bool {
        let minimum = event["attackers"]
            .get("minimum")
            .and_then(integer_value)
            .unwrap_or(1)
            .max(1);
        let matching = self.attackers_attacked_player_count(event, source, attackers);
        value_kind(event) == Some("attackersAttackedPlayer")
            && self.event_player_reference_matches(&event["attacked"], source, defending_player_id)
            && i32::try_from(matching).is_ok_and(|matching| matching >= minimum)
    }

    /// Publishes `attackersAttackedPlayer` once per attacked player of one
    /// declaration (CR 603.2c), with that player as the triggering player
    /// (`that player`).
    pub(super) fn publish_attackers_attacked_player(&mut self, attacker_ids: &[String]) {
        for (defending_player_id, attackers) in self.attackers_by_attacked_player(attacker_ids) {
            let triggers = self
                .state
                .players
                .iter()
                .filter(|player| !player.has_lost)
                .flat_map(|player| player.battlefield.iter())
                .flat_map(|source| {
                    self.effective_rules(source)
                        .iter()
                        .filter(|rule| {
                            triggered_rule_supported(rule)
                                && self.rule_active_for_source(source, rule)
                                && triggered_event_variants(&rule["event"])
                                    .iter()
                                    .any(|event| {
                                        self.attackers_attacked_player_matches(
                                            event,
                                            source,
                                            &defending_player_id,
                                            &attackers,
                                        )
                                    })
                        })
                        .cloned()
                        .map(|rule| (source.clone(), rule))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            for (source, rule) in triggers {
                // The attackers the occurrence matched (`that many`).
                let count = triggered_event_variants(&rule["event"])
                    .into_iter()
                    .find(|event| {
                        self.attackers_attacked_player_matches(
                            event,
                            &source,
                            &defending_player_id,
                            &attackers,
                        )
                    })
                    .map_or(0, |event| {
                        self.attackers_attacked_player_count(event, &source, &attackers)
                    });
                let decisions = BTreeMap::from([
                    (
                        "triggeringPlayerId".to_string(),
                        Value::String(defending_player_id.clone()),
                    ),
                    ("eventObjectCount".to_string(), Value::from(count as u64)),
                ]);
                self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
                if self.check_quantity_failure().is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "trigger_condition_runtime_tests.rs"]
mod trigger_condition_runtime_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "event_subject_state_tests.rs"]
mod event_subject_state_tests;

#[cfg(all(
    test,
    any(not(feature = "ci-test-partition"), feature = "ci-unit-tests-3")
))]
#[path = "antecedent_producer_runtime_tests.rs"]
mod antecedent_producer_runtime_tests;
