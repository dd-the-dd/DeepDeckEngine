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
    /// Per-turn ordinal of this lemma for the attacker (attack records).
    pub attacker_turn_ordinal: Option<usize>,
    /// Per-turn ordinal of this lemma for the blocker (block records).
    pub blocker_turn_ordinal: Option<usize>,
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
        "eventPattern" => generic_combat_pattern(event),
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
            optional_filter(object, "where")?,
        ),
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
                    predicate: optional_filter(counterpart, "where")?.unwrap_or(&Value::Null),
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
        // The ordinal is recorded for the attacker of an attack and for the
        // blocker of a block; other roles fail as unsupported.
        Some(value) => {
            let ordinal = integer_value(value).filter(|ordinal| *ordinal > 0)?;
            let recorded = matches!(
                (lemma, subject_role),
                (CombatLemma::Attack, Attacker) | (CombatLemma::Block, Blocker)
            );
            if !recorded {
                return None;
            }
            Some(usize::try_from(ordinal).ok()?)
        }
    };
    let attacking_alone = optional_flag(event, "attackingAlone")?;
    let declared_attacker = optional_flag(event, "declaredAttacker")?;
    if (attacking_alone || declared_attacker) && subject_role != Attacker {
        return None;
    }
    let group = match (lemma, subject_role, relation) {
        (CombatLemma::Attack, _, SubjectRelation::Source) => PublicationGroup::AttackSource,
        (CombatLemma::Attack, _, SubjectRelation::ControlledBySourceController) => {
            PublicationGroup::AttackControlled
        }
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
        generic: true,
        ..CombatEventPattern::new(lemma, subject_role, relation, group)
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
            ObjectView::PermanentState => matches_permanent_filter(object, filter.predicate),
            ObjectView::Printed => matches_card_filter(&object.definition, filter.predicate),
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
                source
                    .definition
                    .rules
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
    /// current event is logged before its triggers are collected.
    fn combat_turn_ordinal(&self, logged_kind: &str, instance_id: &str) -> usize {
        self.state
            .events
            .iter()
            .filter(|event| {
                event.turn_number == self.state.turn_number
                    && event.kind == logged_kind
                    && event.card_instance_id.as_deref() == Some(instance_id)
            })
            .count()
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
            attacker_turn_ordinal: None,
            blocker_turn_ordinal: Some(self.combat_turn_ordinal("blockerDeclared", blocker_id)),
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

    /// Bindings of a generic block pattern: both combatants.
    fn bind_generic_block_counterparts(
        rule: &Value,
        record: &CombatEventRecord,
        decisions: &mut BTreeMap<String, Value>,
    ) {
        if rule_combat_patterns(rule)
            .iter()
            .any(|pattern| pattern.generic)
        {
            decisions.insert(
                "blockingPermanentId".to_string(),
                Value::String(record.blocker.clone().unwrap_or_default()),
            );
            decisions.insert(
                "blockedPermanentId".to_string(),
                Value::String(record.attacker.clone()),
            );
        }
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
            let mut decisions = BTreeMap::from([(
                "triggeringPermanentId".to_string(),
                Value::String(blocker_id.clone()),
            )]);
            Self::bind_generic_block_counterparts(&rule, record, &mut decisions);
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            self.check_quantity_failure()?;
        }
        for (source, rule) in self.combat_record_triggers(record, PublicationGroup::AttackerBlocked)
        {
            let mut decisions = BTreeMap::from([(
                "triggeringPermanentId".to_string(),
                Value::String(record.attacker.clone()),
            )]);
            if rule_combat_patterns(&rule).iter().any(|pattern| {
                pattern.group == PublicationGroup::AttackerBlocked
                    && pattern.relation == SubjectRelation::Source
            }) {
                // The defending player of the blocked attacker.
                decisions.insert(
                    "triggeringPlayerId".to_string(),
                    Value::String(record.actor.clone()),
                );
            }
            Self::bind_generic_block_counterparts(&rule, record, &mut decisions);
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
        let mut attachment_triggers =
            self.combat_record_triggers(record, PublicationGroup::AttachmentOfAttacker);
        attachment_triggers
            .extend(self.combat_record_triggers(record, PublicationGroup::AttachmentOfBlocker));
        for (source, rule) in attachment_triggers {
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
            Self::bind_generic_block_counterparts(&rule, record, &mut decisions);
            self.put_triggered_ability_on_stack_with_decisions(&source, rule, decisions);
            self.check_quantity_failure()?;
        }
        Ok(())
    }
}
