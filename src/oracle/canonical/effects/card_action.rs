//! Lexical card-collection bindings shared by ordinary action sequences.
use super::super::*;

#[derive(Clone, Default)]
pub(in crate::oracle::canonical) struct CardCollectionMemory {
    view: Option<Value>,
    view_maximum: Option<i64>,
    /// The verb that produced `view` when a later `VERBed this way` restates it.
    view_verb: Option<CardCollectionVerb>,
    /// The zone holding the observed cards when the producing action proves
    /// it (drawn cards are in hand). Readers needing a zone require it.
    view_zone: Option<CardZoneName>,
    /// The view is a whole zone named by a singular noun (`their hand`), so
    /// `from it` may name the collection, not one card.
    view_zone_noun: bool,
    /// The view is library cards whose identities this program bound when
    /// its look or reveal began (CR 608.2c): a later reference to them names
    /// exactly those cards, wherever an action moves them (CR 400.7).
    view_from_library_top: bool,
    /// Those library cards are in the controller's own library, from which
    /// a resolution-time cast of this program may cast them (CR 608.2g).
    view_library_of_controller: bool,
    /// The cards the most recent reveal of this scope revealed (`the
    /// revealed cards`).
    last_revealed: Option<(Value, Option<i64>)>,
    /// The cards the most recent exile of this scope moved (`the exiled
    /// card`), when that exile is an action of this program.
    last_exiled: Option<(Value, Option<i64>)>,
    /// The cards a resolution-time cast of this scope cast (`that weren't
    /// cast`), bound by the cast primitive.
    last_cast: Option<Value>,
    /// The card copies the latest copy action created (CR 707.12), until
    /// the cast frame that follows it consumes them.
    last_copies: Option<(Value, Option<i64>)>,
    selected: Vec<Value>,
    complement_basis: Option<(Value, Option<i64>)>,
    complement_exclusions: Vec<Value>,
    singular_focus: Option<(Value, Option<i64>)>,
    plural_focus: Option<(Value, Option<i64>)>,
    allocated_ids: std::collections::BTreeSet<String>,
    // A lower bound on how many members were removed, bounded by view size.
    removed_up_to_size: i64,
    next_view: usize,
    next_selection: usize,
    next_order: usize,
    next_export: usize,
    next_zone_target: usize,
    next_card_target: usize,
    next_cast: usize,
    next_permission: usize,
    next_copy: usize,
    /// The declared target slots of the enclosing program, by binding: a
    /// card pronoun whose antecedent is a declared target reads its slot.
    object_bindings: Vec<(TargetBinding, String)>,
}

/// The card collection this program put onto the battlefield; `it` after
/// that move reads this binding. One battlefield move per program.
pub(in crate::oracle::canonical) const MOVED_ONTO_BATTLEFIELD_BINDING: &str =
    "movedOntoBattlefield";

fn fresh(base: &str, next: &mut usize) -> String {
    *next += 1;
    if *next == 1 {
        base.to_owned()
    } else {
        format!("{base}{next}")
    }
}

/// The largest literal number of card copies one copy action creates.
const MAXIMUM_CARD_COPIES: i64 = 10;

fn value_kind_name(value: &Value) -> Option<&str> {
    value.get("kind").and_then(Value::as_str)
}
fn bound(name: &str) -> Value {
    json!({"kind":"boundObjects", "binding":name})
}
fn count(objects: Value) -> Value {
    json!({"kind":"countObjects", "objects":objects})
}
fn difference(left: Value, right: Value) -> Value {
    json!({"kind":"setDifference", "left":left, "right":right})
}

impl CardCollectionMemory {
    /// Claims the one moved-onto-battlefield binding of this program (the
    /// same single allocation a battlefield-entry card move takes). `false`
    /// when another producer already holds it.
    pub(in crate::oracle::canonical) fn allocate_moved_onto_battlefield(&mut self) -> bool {
        self.allocated_ids
            .insert(MOVED_ONTO_BATTLEFIELD_BINDING.to_string())
    }

    /// Records the decision IDs a library search lowered from its parse-time
    /// collection binding (`searchedCards`, `searchedCards:N`, and their
    /// `/slotN` choices). The parse-time allocator already made them unique in
    /// the program, so a nested optional scope may carry them.
    pub(in crate::oracle::canonical) fn register_collection_binding_ids(
        &mut self,
        effects: &[Value],
        binding: &str,
    ) {
        fn visit(value: &Value, binding: &str, allocated: &mut std::collections::BTreeSet<String>) {
            match value {
                Value::Array(items) => {
                    for item in items {
                        visit(item, binding, allocated);
                    }
                }
                Value::Object(object) => {
                    if let Some(id) = object.get("id").and_then(Value::as_str)
                        && (id == binding
                            || id
                                .strip_prefix(binding)
                                .is_some_and(|rest| rest.starts_with('/')))
                    {
                        allocated.insert(id.to_string());
                    }
                    for item in object.values() {
                        visit(item, binding, allocated);
                    }
                }
                _ => {}
            }
        }
        for effect in effects {
            visit(effect, binding, &mut self.allocated_ids);
        }
    }

    /// Installs the declared target slots the enclosing program lowered so
    /// far; a card pronoun naming a declared target reads its slot.
    pub(in crate::oracle::canonical) fn bind_object_references(
        &mut self,
        bindings: &[(TargetBinding, String)],
    ) {
        self.object_bindings = bindings.to_vec();
    }

    /// A card pronoun with a typed object antecedent names the program's
    /// observed card collection when the program has one: that collection is
    /// the nearer antecedent of a card pronoun. Every other reading keeps
    /// the typed object. Returns the action with each such operand restored
    /// to its collection reference, or `None` when nothing changes.
    fn observed_collection_reading(&self, action: &CardActionNode) -> Option<CardActionNode> {
        self.view.as_ref()?;
        let restore = |operand: &mut CardOperandNode| {
            if let CardOperandNode::ObjectAntecedent { reference, .. } = operand {
                *operand = CardOperandNode::Reference(*reference);
                true
            } else {
                false
            }
        };
        let mut action = action.clone();
        let changed = match &mut action {
            CardActionNode::LookAt(operand)
            | CardActionNode::Reveal(operand)
            | CardActionNode::Move { cards: operand, .. }
            | CardActionNode::Copy { cards: operand, .. }
            | CardActionNode::Permit(CardPermissionNode { cards: operand, .. }) => restore(operand),
            CardActionNode::GrantKeyword(grant) => restore(&mut grant.cards),
            CardActionNode::Look { .. }
            | CardActionNode::ChooseFromZone(_)
            | CardActionNode::ChooseTargetCards(_)
            | CardActionNode::RevealUntil { .. }
            | CardActionNode::Choose(_)
            | CardActionNode::Shuffle { .. } => false,
        };
        changed.then_some(action)
    }

    /// The cards a typed object antecedent names for a card action (CR
    /// 400.7): the object itself while it is one, the card it became once a
    /// zone change the ability can follow moved it. One object is read
    /// through the shared single-object adapter, which finds nothing once
    /// the object became a new object; a declared target set or the created
    /// tokens are read as their collection.
    fn object_antecedent_cards(
        &self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
        object: PermanentEffectReference,
    ) -> MachineResult<(Value, Option<i64>)> {
        let value = super::permanent_subject::bound_permanent_reference_value(
            object,
            &self.object_bindings,
        )
        .ok_or_else(|| cursor.fail("the declared object the card pronoun names"))?;
        match reference {
            CardCollectionReferenceNode::Selected => {
                let single =
                    super::permanent_subject::single_object_reference(value).ok_or_else(|| {
                        cursor.fail("one object as the antecedent of a singular card pronoun")
                    })?;
                Ok((
                    super::permanent_subject::referenced_object_collection(single),
                    Some(1),
                ))
            }
            CardCollectionReferenceNode::Antecedent => {
                cursor.require(
                    matches!(value_kind_name(&value), Some("chosenTargets" | "boundObjects")),
                    "a declared target set or created tokens as the antecedent of a plural card pronoun",
                )?;
                Ok((value, None))
            }
            _ => Err(cursor.fail("`it`, `that card`, `them` or `those cards` naming an object")),
        }
    }

    /// Selection bindings share one allocator across nested program scopes.
    pub(in crate::oracle::canonical) fn allocate_selection_id(&mut self, prefix: &str) -> String {
        let id = fresh(prefix, &mut self.next_selection);
        self.allocated_ids.insert(id.clone());
        id
    }
    /// The singular card result of the current scope, if one is proven: the
    /// single looked-at card or the result of a one-card choice.
    pub(in crate::oracle::canonical) fn singular_focus_object(&self) -> Option<Value> {
        self.view.as_ref()?;
        self.singular_focus
            .as_ref()
            .filter(|(_, maximum)| maximum.is_some_and(|maximum| maximum <= 1))
            .map(|(value, _)| value.clone())
    }

    /// Restores the antecedent scope of `snapshot` (a conditional's other
    /// branch reads the antecedents before the first branch) while keeping
    /// this allocator's identifiers, so the branches never share an ID.
    pub(in crate::oracle::canonical) fn restore_scope(&mut self, snapshot: &Self) {
        self.view = snapshot.view.clone();
        self.view_maximum = snapshot.view_maximum;
        self.view_verb = snapshot.view_verb;
        self.view_zone = snapshot.view_zone;
        self.view_zone_noun = snapshot.view_zone_noun;
        self.view_from_library_top = snapshot.view_from_library_top;
        self.view_library_of_controller = snapshot.view_library_of_controller;
        self.last_revealed = snapshot.last_revealed.clone();
        self.last_exiled = snapshot.last_exiled.clone();
        self.last_cast = snapshot.last_cast.clone();
        self.last_copies = snapshot.last_copies.clone();
        self.selected = snapshot.selected.clone();
        self.complement_basis = snapshot.complement_basis.clone();
        self.complement_exclusions = snapshot.complement_exclusions.clone();
        self.singular_focus = snapshot.singular_focus.clone();
        self.plural_focus = snapshot.plural_focus.clone();
        self.removed_up_to_size = snapshot.removed_up_to_size;
    }

    /// Whether two memories expose the same antecedent scope.
    pub(in crate::oracle::canonical) fn same_scope(&self, other: &Self) -> bool {
        self.view == other.view
            && self.view_maximum == other.view_maximum
            && self.view_verb == other.view_verb
            && self.view_zone == other.view_zone
            && self.view_zone_noun == other.view_zone_noun
            && self.view_from_library_top == other.view_from_library_top
            && self.view_library_of_controller == other.view_library_of_controller
            && self.last_revealed == other.last_revealed
            && self.last_exiled == other.last_exiled
            && self.last_cast == other.last_cast
            && self.last_copies == other.last_copies
            && self.selected == other.selected
            && self.complement_basis == other.complement_basis
            && self.complement_exclusions == other.complement_exclusions
            && self.singular_focus == other.singular_focus
            && self.plural_focus == other.plural_focus
            && self.removed_up_to_size == other.removed_up_to_size
    }

    pub(in crate::oracle::canonical) fn clear_antecedent(&mut self) {
        self.view = None;
        self.selected.clear();
        self.complement_basis = None;
        self.complement_exclusions.clear();
        self.view_maximum = None;
        self.view_verb = None;
        self.view_zone = None;
        self.view_zone_noun = false;
        self.view_from_library_top = false;
        self.view_library_of_controller = false;
        self.last_revealed = None;
        self.last_exiled = None;
        self.last_cast = None;
        self.singular_focus = None;
        self.plural_focus = None;
        self.removed_up_to_size = 0;
    }

    /// Cards chosen on resolution from a named zone or a linked exile (CR
    /// 608.2d) become the observed collection: `that card`, `it` and `them`
    /// name exactly the chosen cards, which are in `zone` when one zone is
    /// proved. Nothing else of the choice (no complement) is observed.
    fn observe_chosen_cards(
        &mut self,
        chosen: Value,
        maximum: Option<i64>,
        zone: Option<CardZoneName>,
    ) {
        self.clear_antecedent();
        self.view = Some(chosen.clone());
        self.view_maximum = maximum;
        self.view_zone = zone;
        self.mention((chosen.clone(), maximum));
        self.plural_focus = Some((chosen, maximum));
    }

    /// A player's revealed hand (CR 701.20a) is the observed collection: its
    /// identities are fixed when revealed (CR 608.2c), and `from it` names
    /// the whole hand. Returns the binding that records those cards.
    pub(in crate::oracle::canonical) fn observe_revealed_hand(&mut self) -> String {
        let id = fresh("revealedHand", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, None);
        self.view_verb = Some(CardCollectionVerb::Revealed);
        self.view_zone = Some(CardZoneName::Hand);
        self.view_zone_noun = true;
        id
    }

    /// The cards a lexical reference (`that card`, `them`) names for an action
    /// performed by another lowering family, such as a player's discard.
    pub(in crate::oracle::canonical) fn referenced_cards(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<Value> {
        let value = self.reference(cursor, reference)?;
        self.mention(value.clone());
        Ok(value.0)
    }

    /// The cards a mill just put into a graveyard become the observed
    /// collection (`mill four cards, then put a creature card from among them
    /// into your hand`). Returns the binding the mill primitive records.
    pub(in crate::oracle::canonical) fn observe_milled_cards(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("milledCards", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_zone = Some(CardZoneName::Graveyard);
        id
    }

    /// The cards a controller draw just put into hand become the observed
    /// collection (`draw three cards and reveal them`). Returns the binding
    /// the draw primitive must record.
    pub(in crate::oracle::canonical) fn observe_drawn_cards(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("drawnCards", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_zone = Some(CardZoneName::Hand);
        id
    }

    /// The cards a library search found (`library_search_move.rs`): its
    /// choice result, fixed when chosen (CR 608.2c), is the observed
    /// collection the search's later ordinary actions read.
    pub(in crate::oracle::canonical) fn observe_searched_cards(
        &mut self,
        found: Value,
        maximum: Option<i64>,
    ) {
        self.clear_antecedent();
        self.view = Some(found.clone());
        self.view_maximum = maximum;
        self.complement_basis = Some((found.clone(), maximum));
        // The grammatical number of the selection decides whether `it` or
        // `them` names the found cards: one found card is never `them`.
        self.mention((found, maximum));
    }

    /// The permanent a scalar exile instruction just moved is the observed
    /// collection of its program: a later `that card` or `it` reads the
    /// object it became in exile (CR 400.7, 610.3). Returns the binding the
    /// exile primitive must record.
    pub(in crate::oracle::canonical) fn observe_exiled_object(&mut self) -> String {
        self.observe_exiled_objects(Some(1))
    }

    /// The cards an exile of declared targets produced (`exile any number
    /// of target ..., then return those cards`): the same observed exile
    /// view as one object, with the declaration's maximum when it is a
    /// literal and no fixed size otherwise.
    pub(in crate::oracle::canonical) fn observe_exiled_objects(
        &mut self,
        maximum: Option<i64>,
    ) -> String {
        let id = fresh("exiledObject", &mut self.next_view);
        self.allocated_ids.insert(id.clone());
        self.observe(&id, maximum);
        self.view_verb = Some(CardCollectionVerb::Exiled);
        self.view_zone = Some(CardZoneName::Exile);
        id
    }

    fn mention(&mut self, value: (Value, Option<i64>)) {
        if value.1.is_some_and(|maximum| maximum <= 1) {
            self.singular_focus = Some(value);
        } else {
            self.plural_focus = Some(value);
            self.singular_focus = None;
        }
    }

    fn reference(
        &self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<(Value, Option<i64>)> {
        // `the exiled card(s)` restates this program's exile; without one it
        // names the cards a linked ability exiled (CR 607.2a), proved by the
        // card-composition pass. `cards exiled with this` is always linked.
        // `exiled with this` reads every exile of the object (`face`); `the
        // exiled card(s)` names the one exile it is paired with (`pair`).
        match reference {
            // `the exiled card(s)` after an exile action of this program
            // names exactly the cards that action moved.
            CardCollectionReferenceNode::Exiled if self.last_exiled.is_some() => {
                return Ok(self.last_exiled.clone().expect("checked exile result"));
            }
            CardCollectionReferenceNode::Revealed => {
                return self
                    .last_revealed
                    .clone()
                    .ok_or_else(|| cursor.fail("a preceding reveal action of this program"));
            }
            CardCollectionReferenceNode::Linked => {
                return Ok((
                    json!({"kind":"cardsExiledWithSource", "link": "face"}),
                    None,
                ));
            }
            CardCollectionReferenceNode::Exiled
                if self.view.is_none() || self.view_verb != Some(CardCollectionVerb::Exiled) =>
            {
                return Ok((
                    json!({"kind":"cardsExiledWithSource", "link": "pair"}),
                    None,
                ));
            }
            CardCollectionReferenceNode::Exiled => {
                return self
                    .singular_focus
                    .clone()
                    .or_else(|| self.plural_focus.clone())
                    .ok_or_else(|| cursor.fail("the cards this program exiled"));
            }
            // `cards revealed|exiled this way`: the whole observed collection
            // of this scope, produced by the restated verb (CR 608.2c).
            CardCollectionReferenceNode::Restated(verb) => {
                let view = self.view.clone().ok_or_else(|| {
                    cursor.fail("an observed card collection in the current lexical scope")
                })?;
                cursor.require(
                    self.view_verb == Some(verb),
                    "a collection produced by the verb restated with `this way`",
                )?;
                return Ok((view, self.view_maximum));
            }
            _ => {}
        }
        // Without an observed collection, `it` / `that card` is the card the
        // triggering zone change put into a public zone (CR 400.7); `them`
        // after `one or more cards are put into a graveyard` is every card of
        // that event (CR 603.2c), whose number is not static.
        if self.view.is_none()
            && matches!(
                reference,
                CardCollectionReferenceNode::Selected | CardCollectionReferenceNode::Antecedent
            )
            && let Some(environment) = cursor
                .binding_environment()
                .filter(|environment| environment.frame.event_card.is_some())
        {
            let maximum = if reference == CardCollectionReferenceNode::Selected
                || environment.frame.event_object.is_some()
            {
                Some(1)
            } else {
                None
            };
            return Ok((json!({"kind":"triggeringCard"}), maximum));
        }
        self.view.as_ref().ok_or_else(|| {
            cursor.fail("an observed card collection in the current lexical scope")
        })?;
        match reference {
            CardCollectionReferenceNode::Antecedent => self.plural_focus.clone().ok_or_else(|| {
                cursor.fail("a plural card-collection antecedent in the current scope")
            }),
            CardCollectionReferenceNode::Selected => self
                .singular_focus
                .clone()
                .ok_or_else(|| cursor.fail("a singular card antecedent in the current scope")),
            CardCollectionReferenceNode::Exiled
            | CardCollectionReferenceNode::Linked
            | CardCollectionReferenceNode::Revealed
            | CardCollectionReferenceNode::Restated(_) => {
                unreachable!("linked, exiled, revealed and restated references are resolved above")
            }
            CardCollectionReferenceNode::Zone => {
                cursor.require(
                    self.view_zone_noun,
                    "an observed zone named by a singular noun (a revealed hand) for `from it`",
                )?;
                Ok((
                    self.view.clone().expect("checked observed collection"),
                    self.view_maximum,
                ))
            }
            CardCollectionReferenceNode::Remainder | CardCollectionReferenceNode::Other => {
                cursor.require(
                    !self.complement_exclusions.is_empty(),
                    "a preceding selection for the remainder reference",
                )?;
                let (basis, basis_maximum) = self
                    .complement_basis
                    .as_ref()
                    .ok_or_else(|| cursor.fail("the source scope of the preceding selection"))?;
                let maximum = basis_maximum
                    .map(|maximum| maximum.saturating_sub(self.removed_up_to_size).max(0));
                if reference == CardCollectionReferenceNode::Other {
                    cursor.require(
                        maximum.is_some_and(|maximum| maximum <= 1),
                        "a statically bounded singular complement for the other card",
                    )?;
                }
                Ok((
                    self.complement_exclusions
                        .iter()
                        .cloned()
                        .fold(basis.clone(), difference),
                    maximum,
                ))
            }
        }
    }

    /// Optional outputs are real typed empty collections on the declined path,
    /// never an absent decision interpreted as empty. The producing body stays
    /// inside permission. An optional new observation cannot escape this scope.
    pub(in crate::oracle::canonical) fn export_optional_outputs(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        before: &Self,
        body: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.require(
            self.view == before.view,
            "an optional block whose observation scope does not escape conditionally",
        )?;
        // Local IDs from old children are not globally allocated yet. Refuse
        // such nested output rather than aliasing an outer choice accidentally.
        fn ids_are_allocated(
            value: &Value,
            allocated: &std::collections::BTreeSet<String>,
        ) -> bool {
            match value {
                Value::Array(items) => items.iter().all(|item| ids_are_allocated(item, allocated)),
                Value::Object(object) => {
                    object
                        .get("id")
                        .and_then(Value::as_str)
                        .is_none_or(|id| allocated.contains(id))
                        && object
                            .values()
                            .all(|item| ids_are_allocated(item, allocated))
                }
                _ => true,
            }
        }
        cursor.require(
            body.iter()
                .all(|effect| ids_are_allocated(effect, &self.allocated_ids)),
            "shared lexical ID allocation for every nested optional decision",
        )?;
        let Some(view) = self.view.clone() else {
            return Ok(Vec::new());
        };
        let mut initialize = Vec::new();
        let mut exports = Vec::<(Value, Value)>::new();
        for index in before.selected.len()..self.selected.len() {
            let original = self.selected[index].clone();
            let id = fresh("optionalCards", &mut self.next_export);
            self.allocated_ids.insert(id.clone());
            initialize.push(
                json!({"kind":"bind", "id":id, "value":difference(view.clone(),view.clone())}),
            );
            body.push(json!({"kind":"bind", "id":id, "value":original}));
            self.selected[index] = bound(&id);
            exports.push((original, self.selected[index].clone()));
        }
        for (current, prior) in [
            (&mut self.singular_focus, &before.singular_focus),
            (&mut self.plural_focus, &before.plural_focus),
        ] {
            if current == prior {
                continue;
            }
            let Some((value, maximum)) = current.as_mut() else {
                continue;
            };
            if let Some((_, replacement)) = exports.iter().find(|(original, _)| original == value) {
                *value = replacement.clone();
            } else {
                let id = fresh("optionalCards", &mut self.next_export);
                self.allocated_ids.insert(id.clone());
                initialize.push(
                    json!({"kind":"bind", "id":id, "value":difference(view.clone(),view.clone())}),
                );
                body.push(json!({"kind":"bind", "id":id, "value":value}));
                *value = bound(&id);
            }
            let _ = maximum;
        }
        fn replace_export_references(value: &mut Value, exports: &[(Value, Value)]) {
            if let Some((_, replacement)) = exports.iter().find(|(original, _)| original == value) {
                *value = replacement.clone();
                return;
            }
            match value {
                Value::Array(items) => {
                    for item in items {
                        replace_export_references(item, exports);
                    }
                }
                Value::Object(object) => {
                    for item in object.values_mut() {
                        replace_export_references(item, exports);
                    }
                }
                _ => {}
            }
        }
        let same_basis = self.complement_basis == before.complement_basis;
        if let Some((basis, _)) = &mut self.complement_basis {
            replace_export_references(basis, &exports);
        }
        for exclusion in &mut self.complement_exclusions {
            replace_export_references(exclusion, &exports);
        }
        for (result, _) in [&mut self.last_revealed, &mut self.last_exiled]
            .into_iter()
            .flatten()
        {
            replace_export_references(result, &exports);
        }
        self.removed_up_to_size = if same_basis {
            before.removed_up_to_size
        } else {
            0
        };
        Ok(initialize)
    }

    /// A collection a choice produced (`a card at random from your hand`, a
    /// card chosen from outside the game) is the sole antecedent scope of a
    /// later `it` / `them`, like an observation (CR 608.2c).
    fn observe_value(&mut self, value: Value, maximum: Option<i64>, zone: Option<CardZoneName>) {
        self.clear_antecedent();
        self.view = Some(value.clone());
        self.view_maximum = maximum;
        self.view_zone = zone;
        self.complement_basis = Some((value.clone(), maximum));
        self.mention((value.clone(), maximum));
        self.plural_focus = Some((value, maximum));
    }

    /// A freshly produced collection becomes the sole antecedent scope: its
    /// remainder, singular and plural references all read this one binding.
    fn observe(&mut self, id: &str, maximum: Option<i64>) {
        self.clear_antecedent();
        self.view = Some(bound(id));
        self.view_maximum = maximum;
        self.complement_basis = Some((bound(id), maximum));
        self.mention((bound(id), maximum));
        // A one-card observation can still be referenced as a collection.
        self.plural_focus = Some((bound(id), maximum));
    }

    fn operand(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        operand: &CardOperandNode,
        declarations: TargetlessDeclarationSupport,
        effects: &mut Vec<Value>,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<(Value, Option<i64>)> {
        if let CardOperandNode::CardCopies { .. } = operand {
            // Card copies exist only to be cast (CR 707.10a, 707.12); the
            // cast frame reads them directly, every other verb refuses them.
            return Err(
                cursor.fail("card copies only as the object of the cast that follows the copy")
            );
        }
        if let CardOperandNode::HiddenChoice(choice) = operand {
            return self.hidden_choice(cursor, choice, declarations, effects, target_decisions);
        }
        if let CardOperandNode::CommandZone { .. } = operand {
            // Only a cast reads the command zone (CR 903.8); no other verb
            // has a command-zone writer.
            return Err(cursor.fail("the command zone only as the origin of a cast"));
        }
        if let CardOperandNode::ObjectAntecedent { reference, object } = operand {
            let value = self.object_antecedent_cards(cursor, *reference, *object)?;
            self.mention(value.clone());
            return Ok(value);
        }
        if let CardOperandNode::Targets(target) = operand {
            validate_object_predicate_numeric_expressions(&target.predicate, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            cursor.require(
                target.zone == CardZoneName::Graveyard,
                "an admitted target-card source zone",
            )?;
            let zone = match target.owner {
                CardTargetZoneOwner::Player(LocalPlayerReference::Controller) => {
                    graveyard(controller())
                }
                // `from that player's graveyard`: the event's player owns
                // the zone the targets are declared in.
                CardTargetZoneOwner::Player(LocalPlayerReference::Bound(player)) => graveyard(
                    lower_event_bound_player(&player)
                        .ok_or_else(|| cursor.fail("a bound target-card zone owner"))?,
                ),
                CardTargetZoneOwner::AnyPlayer
                | CardTargetZoneOwner::OnePlayer
                | CardTargetZoneOwner::Opponent => json!({"kind":"anyGraveyard"}),
                _ => return Err(cursor.fail("a bound target-card zone owner")),
            };
            let id = fresh("targetCardOperand", &mut self.next_card_target);
            self.allocated_ids.insert(id.clone());
            let mut decision = json!({"id":id,"kind":"chooseTargets",
                "minimum":lower_target_cardinality_bound(target.cardinality.minimum),
                "maximum":lower_target_cardinality_bound(target.cardinality.maximum),
                "candidates":{"kind":"cards","zone":zone,"where":lower_card_object_predicate(&target.predicate)}});
            // `another target ... card`: the source card is not a candidate.
            if target.exclude_source {
                decision["candidates"]["excludeSource"] = json!(true);
            }
            // `from an opponent's graveyard`: cards an opponent owns.
            if target.owner == CardTargetZoneOwner::Opponent {
                decision["candidates"]["owner"] =
                    json!({"kind":"opponentsOf", "player":controller()});
            }
            // `any number of target ... cards` (CR 601.2c): the shared count.
            if target.unbounded {
                let applied = apply_target_count_to_decision(&mut decision, &target.count());
                cursor
                    .require_value(applied, "an unbounded card-target count the IR represents")?;
            }
            if matches!(
                target.owner,
                CardTargetZoneOwner::OnePlayer | CardTargetZoneOwner::Opponent
            ) && (target.unbounded
                || target.cardinality != ExpressedTargetCardinality::EXACTLY_ONE)
            {
                decision["selectionConstraint"] =
                    json!({"kind":"sameZoneOwner","zone":target.zone.ir_name()});
            }
            target_decisions.push(decision);
            self.clear_antecedent();
            let maximum = match target.cardinality.maximum {
                NumericAtom::Integer(value) if !target.unbounded => Some(value),
                _ => None,
            };
            return Ok((json!({"kind":"chosenTargets","id":id}), maximum));
        }
        if let CardOperandNode::LibraryTop(top) = operand {
            // `the top card of each player's library`: every library of the
            // population, read on resolution; the number of libraries is
            // not static, so the collection has no fixed size.
            if let CardLibraryOwnerNode::Players(players) = &top.owner {
                let count_ir = lower_targetless_numeric_expression(&top.count, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let id = fresh("topCards", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                self.observe(&id, None);
                self.view_from_library_top = true;
                effects.push(json!({"kind":"bind", "id":id,
                    "value":{"kind":"libraryTopCards",
                        "zone":{"kind":"library",
                            "players":lower_player_set_reference(*players)},
                        "count":count_ir}}));
                return Ok((bound(&id), None));
            }
            let player = match &top.owner {
                CardLibraryOwnerNode::Player(LocalPlayerReference::Controller) => controller(),
                // `that player's library`: the player the event records.
                CardLibraryOwnerNode::Player(LocalPlayerReference::Bound(
                    BoundPlayerReference::EventPlayer,
                )) => json!({"kind":"triggeringPlayer"}),
                CardLibraryOwnerNode::Player(_) => {
                    return Err(cursor.fail("a supported explicit library owner"));
                }
                CardLibraryOwnerNode::Players(_) => {
                    unreachable!("a library population is lowered above")
                }
                // `the top N cards of target opponent's library`: the
                // declared player owns the library the cards are read from.
                CardLibraryOwnerNode::TargetPlayer(targets) => {
                    let [slot] = targets.slots.as_slice() else {
                        return Err(cursor.fail("one library-owner target slot"));
                    };
                    let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                        .ok_or_else(|| {
                            cursor.fail("a player target selector for the library owner")
                        })?;
                    cursor.require(
                        matches!(slot.selector, TargetSelector::Player(_))
                            && slot.cardinality == TargetCardinality::EXACTLY_ONE,
                        "a singular targeted player as library owner",
                    )?;
                    let id = fresh("targetZonePlayer", &mut self.next_zone_target);
                    self.allocated_ids.insert(id.clone());
                    target_decisions.push(target_decision(&id, candidates, 1, 1));
                    chosen_target(&id)
                }
            };
            let count_ir = lower_targetless_numeric_expression(&top.count, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            let maximum = match &top.count {
                NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                _ => None,
            };
            let id = fresh("topCards", &mut self.next_view);
            self.allocated_ids.insert(id.clone());
            let controller_library = player == controller();
            self.observe(&id, maximum);
            self.view_from_library_top = true;
            self.view_library_of_controller = controller_library;
            // The set is fixed when the action starts (CR 608.2c): later verbs
            // read these identities, not whatever is on top afterwards.
            effects.push(json!({"kind":"bind", "id":id,
                "value":{"kind":"libraryTopCards",
                    "zone":{"kind":"library", "player":player}, "count":count_ir}}));
            return Ok((bound(&id), maximum));
        }
        if let CardOperandNode::ZoneCollection(query) = operand {
            let objects =
                self.zone_collection_objects(cursor, query, declarations, target_decisions)?;
            if let Some(quantity) = &query.quantity {
                // The acting player chooses on resolution (no target): as many
                // matching cards as stated, or all of them if fewer exist; up
                // to a count, or any number of them.
                let available = count(objects.clone());
                let id = fresh("chosenZoneCards", &mut self.next_selection);
                self.allocated_ids.insert(id.clone());
                let mut choose = json!({"kind":"chooseCards", "id":id, "player":controller(),
                    "from":objects.clone()});
                let fixed = match quantity {
                    CardSelectionQuantityNode::Exactly(amount) => {
                        let NumericExpression::Atom(NumericAtom::Integer(fixed)) = amount else {
                            return Err(
                                cursor.fail("a literal number of cards chosen from the zone")
                            );
                        };
                        cursor.require(*fixed > 0, "a positive number of chosen cards")?;
                        choose["count"] =
                            json!({"kind":"minimum", "operands":[integer(*fixed), available]});
                        Some(*fixed)
                    }
                    CardSelectionQuantityNode::UpTo(amount) => {
                        let value = lower_targetless_numeric_expression(amount, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        choose["minimum"] = integer(0);
                        choose["maximum"] =
                            json!({"kind":"minimum", "operands":[value, available]});
                        match amount {
                            NumericExpression::Atom(NumericAtom::Integer(fixed)) => Some(*fixed),
                            _ => None,
                        }
                    }
                    CardSelectionQuantityNode::AnyNumber => {
                        choose["minimum"] = integer(0);
                        choose["maximum"] = available;
                        None
                    }
                    CardSelectionQuantityNode::All { .. } => {
                        return Err(cursor.fail("a chosen quantity of zone cards"));
                    }
                };
                effects.push(choose);
                self.clear_antecedent();
                let result = decision_result(&id);
                self.mention((result.clone(), fixed));
                return Ok((result, fixed));
            }
            // A named zone is not an observation or selection result. Never
            // inherit an unrelated earlier look as its later complement.
            self.clear_antecedent();
            return Ok((objects, None));
        }
        let CardOperandNode::Selection(selection) = operand else {
            let CardOperandNode::Reference(reference) = operand else {
                unreachable!()
            };
            let value = self.reference(cursor, *reference)?;
            if *reference == CardCollectionReferenceNode::Remainder
                && !value.1.is_some_and(|maximum| maximum <= 1)
            {
                // `exile one of them, then put the rest into their graveyard.
                // You may cast that card`: the plural remainder is a
                // complement, never the antecedent of a singular pronoun, so
                // the chosen card keeps its singular focus (CR 608.2c).
                self.plural_focus = Some(value.clone());
            } else {
                self.mention(value.clone());
            }
            return Ok(value);
        };
        let (from, source_maximum) = self.reference(cursor, selection.from)?;
        if self.complement_basis.as_ref().map(|(basis, _)| basis) != Some(&from) {
            self.complement_basis = Some((from.clone(), source_maximum));
            self.complement_exclusions.clear();
            self.removed_up_to_size = 0;
        }
        // A bare `card` noun is the domain itself and filters nothing; any
        // other predicate keeps its lowering, never silently dropped.
        let predicate = selection
            .predicate
            .as_ref()
            .filter(|predicate| **predicate != ObjectPredicate::ObjectKind(ObjectKind::Card))
            .map(lower_card_object_predicate);
        if let CardSelectionQuantityNode::All { via, except_cast } = &selection.quantity {
            // No player choice: the selected set is the filtered view itself,
            // and `the rest` is whatever this set leaves in the view.
            if let Some(via) = via {
                cursor.require(
                    self.view_verb == Some(*via),
                    "a collection produced by the verb restated with `this way`",
                )?;
            }
            let result = predicate.as_ref().map_or_else(
                || from.clone(),
                |predicate| json!({"kind":"filterObjects", "objects":from, "where":predicate}),
            );
            // `that weren't cast`: the members the preceding resolution cast
            // of this scope did not cast (CR 608.2c).
            let result = if *except_cast {
                let cast = self.last_cast.clone().ok_or_else(|| {
                    cursor.fail("a preceding resolution-time cast of the restated cards")
                })?;
                difference(result, cast)
            } else {
                result
            };
            self.selected.push(result.clone());
            self.complement_exclusions.push(result.clone());
            // A filter or a subtraction leaves no static size.
            let maximum = if predicate.is_none() && !*except_cast {
                source_maximum
            } else {
                None
            };
            self.mention((result.clone(), maximum));
            return Ok((result, maximum));
        }
        let filtered = predicate.as_ref().map_or_else(
            || from.clone(),
            |predicate| json!({"kind":"filterObjects", "objects":from, "where":predicate}),
        );
        let available = count(filtered);
        let (minimum, maximum, exact, fixed) = match &selection.quantity {
            CardSelectionQuantityNode::Exactly(amount)
            | CardSelectionQuantityNode::UpTo(amount) => {
                let value = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let fixed = match amount {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => {
                        cursor.require(*value >= 0, "a nonnegative selection quantity")?;
                        Some(*value)
                    }
                    _ => None,
                };
                let bounded = json!({"kind":"minimum", "operands":[value,available]});
                let exact = matches!(selection.quantity, CardSelectionQuantityNode::Exactly(_));
                (
                    if exact { bounded.clone() } else { integer(0) },
                    bounded,
                    exact,
                    fixed,
                )
            }
            CardSelectionQuantityNode::AnyNumber => (integer(0), available, false, None),
            CardSelectionQuantityNode::All { .. } => {
                unreachable!("an unconditional selection returns before any player choice")
            }
        };
        let id = fresh("cardsForHand", &mut self.next_selection);
        self.allocated_ids.insert(id.clone());
        let mut choose = json!({"kind":"chooseCards", "id":id, "player":controller(), "from":from});
        if exact {
            choose["count"] = maximum;
        } else {
            choose["minimum"] = minimum;
            choose["maximum"] = maximum;
        }
        if let Some(predicate) = predicate {
            choose["where"] = predicate;
        }
        effects.push(choose);
        let result = decision_result(&id);
        self.selected.push(result.clone());
        self.complement_exclusions.push(result.clone());
        self.mention((result.clone(), fixed));
        if exact
            && selection.predicate.is_none()
            && self.complement_basis.as_ref().map(|(basis, _)| basis) == Some(&from)
        {
            // Only the first unfiltered choice proves a lower bound. Later
            // independent choices may overlap; do not sum them as disjoint.
            self.removed_up_to_size = self.removed_up_to_size.max(fixed.unwrap_or(0));
        }
        Ok((result, fixed))
    }

    /// The player-owned zone a zone collection names, with its filter. A
    /// declared zone owner becomes this scope's player target decision.
    fn zone_collection_objects(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        query: &CardZoneCollectionNode,
        declarations: TargetlessDeclarationSupport,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Value> {
        // `all cards from your library` (CR 401.1): every card of the zone,
        // read on resolution; no member is chosen, so its hidden order and
        // contents need no player view. A chosen or coordinated library
        // collection keeps no runtime reader.
        let whole_library = query.zone == CardZoneName::Library
            && query.quantity.is_none()
            && query.additional_zones.is_empty();
        cursor.require(
            whole_library
                || matches!(
                    query.zone,
                    CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
                ),
            "an executable hand, graveyard or exile collection, or a whole library",
        )?;
        // `all graveyards`, `an opponent's graveyard`: the zone of every
        // player of the population, read on resolution.
        let zone = match &query.owner {
            CardZoneOwnerNode::Players(players) => json!({"kind":query.zone.ir_name(),
                "players":lower_player_set_reference(*players)}),
            owner => json!({"kind":query.zone.ir_name(),
                "player":self.zone_owner_player(cursor, owner, target_decisions)?}),
        };
        let mut objects = json!({"kind":"cardsInZone","zone":zone});
        if let Some(predicate) = &query.predicate {
            validate_object_predicate_numeric_expressions(predicate, declarations)
                .map_err(|error| cursor.fail(error.expectation()))?;
            objects["where"] = lower_card_object_predicate(predicate)
        }
        // `from your hand or graveyard`: one union of the same owner's
        // zones, each with the same filter.
        if !query.additional_zones.is_empty() {
            cursor.require(
                query.additional_zones.iter().all(|zone| {
                    matches!(
                        zone,
                        CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Exile
                    )
                }) && !matches!(query.owner, CardZoneOwnerNode::Players(_)),
                "a coordinated hand, graveyard or exile zone of one owner",
            )?;
            let mut sets = vec![objects.clone()];
            for other in &query.additional_zones {
                let mut set = objects.clone();
                set["zone"]["kind"] = json!(other.ir_name());
                sets.push(set);
            }
            objects = json!({"kind":"union", "sets":sets});
        }
        Ok(objects)
    }

    /// A choice among cards of a hidden source: the shared card-selection
    /// primitive over a zone or the controller's cards outside the game
    /// (CR 400.11a), by the controller or at random with the engine's
    /// replayable random source. The chosen cards become the observed
    /// collection a later `it` names.
    fn hidden_choice(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        choice: &HiddenCardChoiceNode,
        declarations: TargetlessDeclarationSupport,
        effects: &mut Vec<Value>,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<(Value, Option<i64>)> {
        let mut cursor = cursor.clone();
        cursor.enter("lower.card-action.hidden-choice");
        validate_object_predicate_numeric_expressions(&choice.predicate, declarations)
            .map_err(|error| cursor.fail(error.expectation()))?;
        let (zone, view_zone) = match &choice.source {
            HiddenCardSourceNode::OutsideGame => {
                (json!({"kind":"outsideGame", "player":controller()}), None)
            }
            HiddenCardSourceNode::Zone { zone, owner } => {
                cursor.require(
                    !matches!(owner, CardZoneOwnerNode::Players(_)),
                    "one player's zone to choose from",
                )?;
                let player = self.zone_owner_player(&cursor, owner, target_decisions)?;
                (json!({"kind":zone.ir_name(), "player":player}), Some(*zone))
            }
        };
        let literal = |amount: &NumericExpression| match amount {
            NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
            _ => None,
        };
        let (minimum, maximum, fixed) = match &choice.quantity {
            CardSelectionQuantityNode::Exactly(amount) => {
                let value = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                (value.clone(), value, literal(amount))
            }
            CardSelectionQuantityNode::UpTo(amount) => {
                let value = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                (integer(0), value, literal(amount))
            }
            CardSelectionQuantityNode::AnyNumber | CardSelectionQuantityNode::All { .. } => {
                return Err(cursor.fail("a bounded number of cards chosen from a hidden source"));
            }
        };
        if let Some(fixed) = fixed {
            cursor.require(fixed > 0, "a positive number of chosen cards")?;
        }
        let id = fresh("chosenHiddenCards", &mut self.next_selection);
        self.allocated_ids.insert(id.clone());
        let mut choose = json!({"kind":"chooseCards", "id":id, "player":controller(),
            "candidates":{"kind":"cards", "zone":zone,
                "where":lower_card_object_predicate(&choice.predicate)},
            "minimum":minimum, "maximum":maximum});
        if choice.random {
            choose["selection"] = json!({"kind":"random"});
        }
        effects.push(choose);
        let result = decision_result(&id);
        self.observe_value(result.clone(), fixed, view_zone);
        Ok((result, fixed))
    }

    /// `copy CARD [N times]` (CR 707.12): each copy is created in exile, the
    /// zone the copied card is in, and bound for the cast frame that must
    /// follow. The copied card is one card this program exiled.
    fn lower_card_copy(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        cards: &CardOperandNode,
        count: &NumericExpression,
        effects: &mut Vec<Value>,
    ) -> MachineResult<()> {
        cursor.enter("lower.card-action.copy");
        let CardOperandNode::Reference(reference) = cards else {
            return Err(cursor.fail("a bound card to copy"));
        };
        let (copied, maximum) = self.reference(cursor, *reference)?;
        cursor.require(
            maximum == Some(1) && self.view_zone == Some(CardZoneName::Exile),
            "one card this program exiled as the copied card",
        )?;
        cursor.require(
            value_kind_name(&copied) == Some("boundObjects"),
            "an exiled card bound by its exile action",
        )?;
        let copied = json!({"kind":"boundObject", "binding":copied["binding"]});
        let NumericExpression::Atom(NumericAtom::Integer(count)) = count else {
            return Err(cursor.fail("a literal number of card copies"));
        };
        cursor.require(
            (1..=MAXIMUM_CARD_COPIES).contains(count),
            "a positive literal number of card copies within the copy bound",
        )?;
        let mut copies = Vec::new();
        for _ in 0..*count {
            let id = fresh("cardCopy", &mut self.next_copy);
            self.allocated_ids.insert(id.clone());
            effects.push(json!({"kind":"createCardCopy", "card":copied.clone(),
                "fromZone":"exile", "player":controller(), "to":"exile", "bind":id}));
            copies.push(bound(&id));
        }
        let copies = if copies.len() == 1 {
            copies.remove(0)
        } else {
            json!({"kind":"union", "sets":copies})
        };
        self.last_copies = Some((copies, Some(*count)));
        Ok(())
    }

    /// The player a zone owner names: the controller, or a declared player
    /// target this scope adds as a decision.
    fn zone_owner_player(
        &mut self,
        cursor: &OracleTokenCursor<'_>,
        owner: &CardZoneOwnerNode,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Value> {
        match owner {
            CardZoneOwnerNode::Player(LocalPlayerReference::Controller) => Ok(controller()),
            // The player the triggering event records (`that player's hand`
            // after `deals combat damage to a player`, `defending player's`).
            CardZoneOwnerNode::Player(LocalPlayerReference::Bound(player)) => {
                lower_event_bound_player(player)
                    .ok_or_else(|| cursor.fail("a resolved collection owner reference"))
            }
            CardZoneOwnerNode::Player(_) => {
                Err(cursor.fail("a resolved collection owner reference"))
            }
            CardZoneOwnerNode::Players(_) => {
                Err(cursor.fail("one zone owner, not a population of players"))
            }
            CardZoneOwnerNode::TargetPlayer(targets) => {
                let [slot] = targets.slots.as_slice() else {
                    return Err(cursor.fail("one zone-owner target slot"));
                };
                let (_, candidates) = lower_target_selector_candidates(&slot.selector)
                    .ok_or_else(|| cursor.fail("a player target selector for the zone owner"))?;
                cursor.require(
                    matches!(slot.selector, TargetSelector::Player(_))
                        && slot.cardinality == TargetCardinality::EXACTLY_ONE,
                    "a singular targeted player as zone owner",
                )?;
                let id = fresh("targetZonePlayer", &mut self.next_zone_target);
                self.allocated_ids.insert(id.clone());
                target_decisions.push(target_decision(&id, candidates, 1, 1));
                Ok(chosen_target(&id))
            }
        }
    }

    /// The zone the observed cards a reference names are in, for a cast
    /// from among them. Linked exiles are in exile (CR 607.2a). A card in
    /// another player's hand is cast by its caster (CR 601.2a).
    fn observed_cast_zone(
        &self,
        cursor: &OracleTokenCursor<'_>,
        reference: CardCollectionReferenceNode,
    ) -> MachineResult<CardZoneName> {
        let linked_exile = reference == CardCollectionReferenceNode::Linked
            || (reference == CardCollectionReferenceNode::Exiled
                && (self.last_exiled.is_some()
                    || self.view.is_none()
                    || self.view_verb != Some(CardCollectionVerb::Exiled)));
        if linked_exile {
            return Ok(CardZoneName::Exile);
        }
        // Library cards this program looked at or revealed in the
        // controller's own library, none of which an earlier action of the
        // scope moved elsewhere: the cast takes them from where they are,
        // whatever their position (CR 608.2g, 601.2a).
        if self.view_zone.is_none() && self.view_from_library_top && self.view_library_of_controller
        {
            return Ok(CardZoneName::Library);
        }
        let zone = self.view_zone.ok_or_else(|| {
            cursor.fail(
                "observed cards whose zone (hand, graveyard, exile or your library) is proved",
            )
        })?;
        Ok(zone)
    }

    /// Whether a cast frame casts while the effect resolves (CR 608.2g): a
    /// frame without a window does, except the optional printed-cost cast of
    /// cards this program exiled, which keeps the earlier permission
    /// contract (CR 611.2a, 400.7: while they remain exiled).
    fn casts_during_resolution(
        &self,
        cursor: &OracleTokenCursor<'_>,
        permit: &CardPermissionNode,
    ) -> bool {
        if permit.duration.is_some() || permit.window.is_some() {
            return false;
        }
        if !permit.optional
            || permit.cost != CardCastCostNode::Printed
            || permit.graveyard_exile.is_some()
        {
            return true;
        }
        match &permit.cards {
            CardOperandNode::Reference(reference) => {
                !self.reference(cursor, *reference).is_ok_and(|(cards, _)| {
                    (self.view_verb == Some(CardCollectionVerb::Exiled)
                        && Some(&cards) == self.view.as_ref())
                        || value_kind_name(&cards) == Some("cardsExiledWithSource")
                })
            }
            _ => true,
        }
    }

    /// CR 608.2g: the cards are cast while this effect resolves, from the
    /// zone they are proved to be in, at their printed cost or without their
    /// mana cost, optionally (`you may cast`) or if able (an imperative
    /// `cast`). The cast cards are bound for a later `that weren't cast`;
    /// the spells carry the stated graveyard exile replacement (CR 614.1a).
    /// Card copies that are not cast cease to exist (CR 707.10a).
    fn lower_resolution_cast(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        permit: &CardPermissionNode,
        declarations: TargetlessDeclarationSupport,
        effects: &mut Vec<Value>,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.enter("lower.card-action.resolution-cast");
        cursor.require(
            !permit.mana_of_any_type,
            "a spending policy only on a later casting permission",
        )?;
        // `you may play the exiled card` (CR 305.2, 305.3): a land among the
        // cards is played while the effect resolves, as a land play of the
        // player's own turn; the other cards are cast.
        let play_lands = permit.verb == CardPermissionVerb::Play;
        cursor.require(
            !play_lands || permit.optional,
            "`you may play`: an imperative play has no writer",
        )?;
        let (without_paying, alternative_cost) = match &permit.cost {
            CardCastCostNode::WithoutManaCost => (true, None),
            CardCastCostNode::Printed => (false, None),
            CardCastCostNode::Alternative(_) => {
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::ResolutionCostAction,
                    "a resolution-time cast paying a substituted mana cost",
                ));
            }
            // CR 118.9: life or the player's counters equal to the mana value
            // of the spell actually cast, instead of its mana cost.
            CardCastCostNode::AlternativeNonMana(NonManaCastCostNode::LifeEqualToManaValue) => (
                false,
                Some(json!({"kind":"payLife", "amount":{"kind":"manaValueOfCastCard"}})),
            ),
            CardCastCostNode::AlternativeNonMana(
                NonManaCastCostNode::PlayerCountersEqualToManaValue(counter),
            ) => (
                false,
                Some(
                    json!({"kind":"payPlayerCounters", "counter":counter.as_str(),
                    "count":{"kind":"manaValueOfCastCard"}}),
                ),
            ),
            CardCastCostNode::AlternativeNonMana(NonManaCastCostNode::Discard(_)) => {
                return Err(cursor.engine_gap(
                    crate::oracle::canonical::engine_gap::EngineGap::ResolutionCostAction,
                    "a resolution-time cast discarding cards instead of its mana cost",
                ));
            }
        };
        cursor.require(
            alternative_cost.is_none() || !play_lands,
            "an alternative cost only for a cast, not for a land play",
        )?;
        fn cast_maximum(
            cursor: &OracleTokenCursor<'_>,
            quantity: &CardSelectionQuantityNode,
        ) -> MachineResult<Option<i64>> {
            match quantity {
                CardSelectionQuantityNode::Exactly(NumericExpression::Atom(
                    NumericAtom::Integer(value),
                ))
                | CardSelectionQuantityNode::UpTo(NumericExpression::Atom(NumericAtom::Integer(
                    value,
                ))) if *value > 0 => Ok(Some(*value)),
                CardSelectionQuantityNode::AnyNumber
                | CardSelectionQuantityNode::All {
                    via: None,
                    except_cast: false,
                } => Ok(None),
                _ => Err(cursor
                    .fail("a positive literal, `any number` or every one of the cards to cast")),
            }
        }
        // An imperative cast of a stated number of cards is performed if
        // able; `any number`, `up to N` and `up to N` declared targets keep
        // their own zero lower bound.
        let mut mandatory = !permit.optional;
        let mut uncast_copies = None;
        // The cards to cast and every zone they may be cast from.
        let (cards, maximum, zones) = match &permit.cards {
            CardOperandNode::CardCopies { plural } => {
                cursor.enter("lower.card-action.resolution-cast.copies");
                let (copies, count) = self
                    .last_copies
                    .take()
                    .ok_or_else(|| cursor.fail("a preceding copy action of this program"))?;
                cursor.require(
                    *plural || count == Some(1),
                    "`the copy` for one copy, `the copies` for several",
                )?;
                uncast_copies = Some(copies.clone());
                (Some(copies), count, vec![CardZoneName::Exile.ir_name()])
            }
            CardOperandNode::Reference(reference) => {
                let (cards, maximum) = self.reference(cursor, *reference)?;
                let zone = self.observed_cast_zone(cursor, *reference)?;
                (Some(cards), maximum, vec![zone.ir_name()])
            }
            CardOperandNode::Selection(selection) => {
                let (from, _) = self.reference(cursor, selection.from)?;
                let maximum = cast_maximum(cursor, &selection.quantity)?;
                mandatory &= matches!(selection.quantity, CardSelectionQuantityNode::Exactly(_));
                let cards = match &selection.predicate {
                    Some(predicate) => {
                        validate_object_predicate_numeric_expressions(predicate, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        require_lowerable_card_predicate(cursor, predicate)?;
                        json!({"kind":"filterObjects", "objects":from,
                            "where":lower_card_object_predicate(predicate)})
                    }
                    None => from,
                };
                let zone = self.observed_cast_zone(cursor, selection.from)?;
                (Some(cards), maximum, vec![zone.ir_name()])
            }
            CardOperandNode::ZoneCollection(query) => {
                // A library is cast from only through cards this program
                // observed in it; a zone query never names library cards.
                cursor.require(
                    query.zone != CardZoneName::Library,
                    "a hand, graveyard or exile zone to cast from",
                )?;
                if let Some(predicate) = &query.predicate {
                    require_lowerable_card_predicate(cursor, predicate)?;
                }
                let cards =
                    self.zone_collection_objects(cursor, query, declarations, target_decisions)?;
                let maximum = match &query.quantity {
                    Some(quantity) => {
                        mandatory &= matches!(quantity, CardSelectionQuantityNode::Exactly(_));
                        cast_maximum(cursor, quantity)?
                    }
                    None => None,
                };
                // `from your hand or graveyard`: one cast among the cards
                // of every named zone (CR 601.2a: from the zone it is in).
                let zones = std::iter::once(query.zone)
                    .chain(query.additional_zones.iter().copied())
                    .map(CardZoneName::ir_name)
                    .collect::<Vec<_>>();
                (Some(cards), maximum, zones)
            }
            // `you may cast target instant card from your graveyard without
            // paying`: the declared cards, cast from their declared zone.
            CardOperandNode::Targets(target) => {
                mandatory &=
                    target.cardinality.minimum == target.cardinality.maximum && !target.unbounded;
                let (cards, maximum) = self.operand(
                    cursor,
                    &permit.cards,
                    declarations,
                    effects,
                    target_decisions,
                )?;
                (Some(cards), maximum, vec![target.zone.ir_name()])
            }
            // `cast your commander from the command zone without paying its
            // mana cost` (CR 903.8: the commander tax is still paid). The
            // runtime reads the caster's own command zone; the designation
            // and the caster's ownership are the cards' filter.
            CardOperandNode::CommandZone { predicate } => {
                cursor.enter("lower.card-action.resolution-cast.command-zone");
                cursor.require(
                    *predicate
                        == ObjectPredicate::Intersection(vec![
                            ObjectPredicate::IsCommander,
                            ObjectPredicate::OwnedByAbilityController { negated: false },
                        ]),
                    "your commander as the card cast from the command zone",
                )?;
                mandatory = false;
                (None, Some(1), vec!["commandZone"])
            }
            _ => return Err(cursor.fail("an observed card collection to cast from")),
        };
        cursor.require(
            zones.iter().all(|zone| {
                matches!(
                    *zone,
                    "hand" | "graveyard" | "exile" | "commandZone" | "library"
                )
            }),
            "a hand, graveyard, exile, command-zone or observed-library source for the cast",
        )?;
        let id = fresh("castCards", &mut self.next_cast);
        self.allocated_ids.insert(id.clone());
        let mut cast = json!({
            "kind": "castAnyNumber",
            "player": controller(),
            "where": {"kind": "canBeCastAsSpell"},
            "timing": {"kind": "duringResolution"},
            "withoutPayingManaCost": without_paying,
            "alternativeCostsAllowed": !without_paying && alternative_cost.is_none(),
            "additionalCostsApply": true,
            "variableManaValue": integer(0),
            "bind": id,
        });
        match cards {
            Some(cards) => cast["cards"] = cards,
            // The caster's commanders in their own command zone.
            None => cast["cardsWhere"] = json!({"kind":"isCommander"}),
        }
        match zones.as_slice() {
            [zone] => cast["sourceZone"] = json!(zone),
            zones => cast["sourceZones"] = json!(zones),
        }
        if let Some(cost) = alternative_cost {
            cast["alternativeCost"] = cost;
        }
        if play_lands {
            cast["playLands"] = json!(true);
        }
        if let Some(maximum) = maximum {
            cast["maximum"] = integer(maximum);
        }
        if mandatory {
            cast["mandatory"] = json!(true);
        }
        if let Some(replacement) = &permit.graveyard_exile {
            cursor.enter("lower.card-action.resolution-cast.graveyard-exile");
            cast["exileInsteadOfGraveyard"] =
                lower_cast_graveyard_exile(cursor, replacement, declarations)?;
        }
        effects.push(cast);
        if matches!(permit.cards, CardOperandNode::Selection(_)) {
            // `Then put the rest ...`: the observed cards this cast did not
            // cast (CR 608.2c), like the remainder of any other selection.
            self.complement_exclusions.push(bound(&id));
        }
        if let Some(copies) = uncast_copies {
            // CR 707.10a: a copy of a card outside the stack and the
            // battlefield ceases to exist.
            effects.push(json!({"kind":"ceaseToExist", "objects":copies, "fromZone":"exile"}));
        }
        self.last_cast = Some(bound(&id));
        Ok(std::mem::take(effects))
    }

    /// The card reference a casting permission names: a bound collection or
    /// decision result directly, any other collection value through a new
    /// binding fixed while the effect resolves (CR 608.2c).
    fn permission_card_reference(
        &mut self,
        cards: Value,
        maximum: Option<i64>,
        effects: &mut Vec<Value>,
    ) -> Value {
        match value_kind_name(&cards) {
            Some("boundObjects") if maximum.is_some_and(|maximum| maximum <= 1) => {
                json!({"kind":"boundObject", "binding":cards["binding"]})
            }
            Some("boundObjects" | "decisionResult") => cards,
            _ => {
                let id = fresh("permittedCards", &mut self.next_permission);
                self.allocated_ids.insert(id.clone());
                effects.push(json!({"kind":"bind", "id":id, "value":cards}));
                bound(&id)
            }
        }
    }

    pub(in crate::oracle::canonical) fn lower_action(
        &mut self,
        cursor: &mut OracleTokenCursor<'_>,
        action: &CardActionNode,
        declarations: TargetlessDeclarationSupport,
        target_decisions: &mut Vec<Value>,
    ) -> MachineResult<Vec<Value>> {
        cursor.enter("lower.card-action.bindings");
        let observed_reading = self.observed_collection_reading(action);
        let action = observed_reading.as_ref().unwrap_or(action);
        let mut effects = Vec::new();
        match action {
            CardActionNode::Look {
                count: amount,
                owner,
            } => {
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "a supported explicit library owner",
                )?;
                let amount_ir = lower_targetless_numeric_expression(amount, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let id = fresh("lookedCards", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                let maximum = match amount {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                    _ => None,
                };
                self.observe(&id, maximum);
                self.view_from_library_top = true;
                self.view_library_of_controller = true;
                effects.push(json!({"kind":"lookAtTopCards", "zone":{"kind":"library", "player":controller()}, "count":amount_ir, "bind":id}));
            }
            CardActionNode::LookAt(operand) => {
                cursor.enter("lower.card-action.look-at");
                match operand {
                    // `look at the top N cards of target player's library`:
                    // a private look by the controller (CR 401.2).
                    CardOperandNode::LibraryTop(top) => {
                        // A declared player's library, or the library of the
                        // player the binding environment names (`that
                        // player's library`), through one owner resolver.
                        let owner = match &top.owner {
                            CardLibraryOwnerNode::TargetPlayer(targets) => {
                                CardZoneOwnerNode::TargetPlayer(targets.clone())
                            }
                            CardLibraryOwnerNode::Player(player) => {
                                CardZoneOwnerNode::Player(player.clone())
                            }
                            // A private look at several libraries has no one
                            // looked-at collection of this frame.
                            CardLibraryOwnerNode::Players(_) => {
                                return Err(cursor.fail(
                                    "one library to look at; a look at each library of a population is not read",
                                ));
                            }
                        };
                        let player = self.zone_owner_player(cursor, &owner, target_decisions)?;
                        let amount_ir =
                            lower_targetless_numeric_expression(&top.count, declarations)
                                .map_err(|error| cursor.fail(error.expectation()))?;
                        let maximum = match &top.count {
                            NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                            _ => None,
                        };
                        let id = fresh("lookedCards", &mut self.next_view);
                        self.allocated_ids.insert(id.clone());
                        self.observe(&id, maximum);
                        self.view_from_library_top = true;
                        effects.push(json!({"kind":"lookAtTopCards",
                            "zone":{"kind":"library", "player":player},
                            "count":amount_ir, "bind":id}));
                    }
                    // `look at target player's hand`: the controller sees the
                    // whole hand; `from it` names that hand afterwards.
                    CardOperandNode::ZoneCollection(query)
                        if query.zone == CardZoneName::Hand
                            && query.additional_zones.is_empty()
                            && query.predicate.is_none()
                            && query.quantity.is_none() =>
                    {
                        let player =
                            self.zone_owner_player(cursor, &query.owner, target_decisions)?;
                        let id = fresh("lookedHand", &mut self.next_view);
                        self.allocated_ids.insert(id.clone());
                        self.observe(&id, None);
                        self.view_zone = Some(CardZoneName::Hand);
                        self.view_zone_noun = true;
                        effects.push(json!({"kind":"lookAtHand",
                            "player":player.clone(), "viewer":controller()}));
                        effects.push(json!({"kind":"bind", "id":id,
                            "value":{"kind":"cardsInZone",
                                "zone":{"kind":"hand", "player":player}}}));
                    }
                    _ => return Err(cursor.fail("a library top or a whole hand to look at")),
                }
            }
            CardActionNode::Reveal(operand) => {
                let (cards, maximum) = self.operand(
                    cursor,
                    operand,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                effects.push(json!({"kind":"revealCards", "cards":cards.clone()}));
                if matches!(operand, CardOperandNode::LibraryTop(_)) {
                    self.view_verb = Some(CardCollectionVerb::Revealed);
                }
                self.last_revealed = Some((cards, maximum));
            }
            // `exile cards from the top of your library until you exile a
            // PREDICATE card`: the shared exile-until writer. Every exiled
            // card is observed in exile; the matching card is `that card`,
            // the others `the rest`, all of them `the exiled cards`.
            CardActionNode::RevealUntil {
                predicate,
                owner,
                verb: CardCollectionVerb::Exiled,
                ..
            } => {
                cursor.enter("lower.card-action.exile-until");
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "a supported explicit library owner",
                )?;
                let id = fresh("exiledUntil", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                let hit = fresh("exiledUntilMatch", &mut self.next_view);
                self.allocated_ids.insert(hit.clone());
                effects.push(json!({"kind":"exileFromTopUntil",
                    "zone":{"kind":"library", "player":controller()},
                    "bind":id, "stopCardBind":hit, "faceDown":false,
                    "stopWhere":lower_card_object_predicate(predicate),
                    "alsoStopsWhen":{"kind":"sourceZoneEmpty"}}));
                self.observe(&id, None);
                self.view_verb = Some(CardCollectionVerb::Exiled);
                self.view_zone = Some(CardZoneName::Exile);
                self.last_exiled = Some((bound(&id), None));
                self.selected.push(bound(&hit));
                self.complement_exclusions.push(bound(&hit));
                self.mention((bound(&hit), Some(1)));
            }
            CardActionNode::RevealUntil {
                quantity,
                predicate,
                owner,
                verb: CardCollectionVerb::Revealed,
            } => {
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "a supported explicit library owner",
                )?;
                let count_ir = lower_targetless_numeric_expression(quantity, declarations)
                    .map_err(|error| cursor.fail(error.expectation()))?;
                let maximum = match quantity {
                    NumericExpression::Atom(NumericAtom::Integer(value)) => Some(*value),
                    _ => None,
                };
                let id = fresh("revealedCards", &mut self.next_view);
                self.allocated_ids.insert(id.clone());
                let matching = json!({"kind":"filterObjects", "objects":bound(&id),
                    "where":lower_card_object_predicate(predicate)});
                effects.push(json!({"kind":"revealFromTopUntilCount",
                    "zone":{"kind":"library", "player":controller()},
                    "count":count_ir, "where":lower_card_object_predicate(predicate), "bind":id}));
                // Every revealed card is observed; the matching cards are what a
                // later `that card` names and what `the rest` excludes. The
                // binding fixed the revealed library cards' identities when
                // the reveal happened (CR 608.2c, 701.20a).
                self.observe(&id, None);
                self.view_from_library_top = true;
                self.view_library_of_controller = true;
                self.view_verb = Some(CardCollectionVerb::Revealed);
                self.selected.push(matching.clone());
                self.complement_exclusions.push(matching.clone());
                self.mention((matching, maximum));
            }
            CardActionNode::Choose(selection) => {
                cursor.enter("lower.card-action.choose");
                // The choice itself is the action: the operand child emits
                // `chooseCards` and records its result as the antecedent.
                let (chosen, maximum) = self.operand(
                    cursor,
                    &CardOperandNode::Selection(selection.clone()),
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                // A choice among the source's linked exile observes nothing
                // before it: the chosen cards, in exile, are the collection
                // a later `that card` names (CR 607.2a).
                if selection.from == CardCollectionReferenceNode::Linked && self.view.is_none() {
                    self.observe_chosen_cards(chosen, maximum, Some(CardZoneName::Exile));
                }
            }
            CardActionNode::ChooseFromZone(zone) => {
                cursor.enter("lower.card-action.choose-zone");
                // The zone collection child emits `chooseCards` among the
                // zone's matching cards; the chosen cards become the observed
                // collection (the chosen-card binding), in that zone.
                let (chosen, maximum) = self.operand(
                    cursor,
                    &CardOperandNode::ZoneCollection(zone.clone()),
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                cursor.require(
                    value_kind_name(&chosen) == Some("decisionResult"),
                    "a resolution choice among the zone's cards",
                )?;
                let single_zone = zone.additional_zones.is_empty().then_some(zone.zone);
                self.observe_chosen_cards(chosen, maximum, single_zone);
            }
            CardActionNode::ChooseTargetCards(targets) => {
                cursor.enter("lower.card-action.choose-target-cards");
                // The declaration is the action (CR 115.1): the card target
                // child registers the decision; the declared cards become
                // the observed collection, in their zone.
                let (chosen, maximum) = self.operand(
                    cursor,
                    &CardOperandNode::Targets(targets.clone()),
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                self.observe_chosen_cards(chosen, maximum, Some(targets.zone));
            }
            CardActionNode::GrantKeyword(grant) => {
                cursor.enter("lower.card-action.grant-keyword");
                let (cards, maximum) = self.operand(
                    cursor,
                    &grant.cards,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                // One declared card is the target reference the grant reads;
                // any other collection is resolved into its cards, each of
                // which gains the keyword (CR 611.2c).
                let object =
                    if value_kind_name(&cards) == Some("chosenTargets") && maximum == Some(1) {
                        json!({"kind":"chosenTarget", "id":cards["id"].clone()})
                    } else {
                        cards
                    };
                let GrantedCardKeywordNode::Flashback(GrantedFlashbackCostNode::ManaCostOfCard) =
                    grant.keyword;
                // CR 702.34a: within the granted ability, `its mana cost` is
                // the mana cost of the card that has it (`abilitySource`).
                effects.push(json!({"kind":"grantAbility", "object":object,
                    "duration":{"kind":"untilEndOfCurrentTurn"},
                    "ability":{"kind":"flashback",
                        "cost":{"kind":"manaCostOf", "card":{"kind":"abilitySource"}}}}));
            }
            CardActionNode::Shuffle { owner } => {
                cursor.enter("lower.card-action.shuffle");
                cursor.require(
                    *owner == LocalPlayerReference::Controller,
                    "the controller's library",
                )?;
                // Shuffled cards are new unordered identities for later text.
                self.clear_antecedent();
                effects.push(json!({"kind":"shuffleZone",
                    "zone":{"kind":"library", "player":controller()}}));
            }
            CardActionNode::Copy { cards, count } => {
                self.lower_card_copy(cursor, cards, count, &mut effects)?;
            }
            CardActionNode::Permit(permit) if self.casts_during_resolution(cursor, permit) => {
                return self.lower_resolution_cast(
                    cursor,
                    permit,
                    declarations,
                    &mut effects,
                    target_decisions,
                );
            }
            CardActionNode::Permit(permit) => {
                cursor.enter("lower.card-action.permit.window");
                // A window permission is a player's later choice to cast;
                // a substituted cost has no runtime on it yet.
                if matches!(
                    permit.cost,
                    CardCastCostNode::Alternative(_) | CardCastCostNode::AlternativeNonMana(_)
                ) {
                    return Err(cursor.engine_gap(
                        crate::oracle::canonical::engine_gap::EngineGap::PermissionWindow,
                        "a later casting permission carrying a substituted cost",
                    ));
                }
                // `If that spell would be put into your graveyard, exile it
                // instead.` (CR 614.1a): the replacement each spell cast
                // through this permission carries, as a resolution-time cast
                // carries it.
                let graveyard_exile = match &permit.graveyard_exile {
                    Some(replacement) => {
                        cursor.enter("lower.card-action.permit.graveyard-exile");
                        cursor.require(
                            permit.verb == CardPermissionVerb::Cast,
                            "a cast permission for the graveyard exile of its spells",
                        )?;
                        Some(lower_cast_graveyard_exile(
                            cursor,
                            replacement,
                            declarations,
                        )?)
                    }
                    None => None,
                };
                cursor.require(
                    matches!(
                        permit.cards,
                        CardOperandNode::Reference(_)
                            | CardOperandNode::ObjectAntecedent { .. }
                            | CardOperandNode::Selection(_)
                            | CardOperandNode::ZoneCollection(_)
                            | CardOperandNode::Targets(_)
                    ),
                    "a bound card collection, declared card targets or a card query as the permitted cards",
                )?;
                // `you may cast creature spells from your graveyard this turn`:
                // a permission over a zone's matching cards, current and
                // future, not over cards fixed when the effect resolved.
                if let CardOperandNode::ZoneCollection(query) = &permit.cards {
                    cursor.enter("lower.card-action.permit.zone");
                    cursor.require(
                        permit.verb == CardPermissionVerb::Cast
                            && query.quantity.is_none()
                            && query.owner
                                == CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
                            && matches!(query.zone, CardZoneName::Graveyard | CardZoneName::Exile)
                            && query.additional_zones.is_empty()
                            && permit.duration == Some(EffectDurationNode::UntilEndOfCurrentTurn)
                            && permit.window.is_none()
                            && !permit.without_paying_mana_cost()
                            && !permit.mana_of_any_type
                            && permit.graveyard_exile.is_none(),
                        "every matching spell of your graveyard or exile, cast this turn at its usual cost, without a graveyard replacement",
                    )?;
                    let mut filter = json!({"kind":"canBeCastAsSpell"});
                    if let Some(predicate) = &query.predicate {
                        validate_object_predicate_numeric_expressions(predicate, declarations)
                            .map_err(|error| cursor.fail(error.expectation()))?;
                        require_lowerable_card_predicate(cursor, predicate)?;
                        filter = lower_card_object_predicate(predicate);
                    }
                    effects.push(json!({"kind":"grantZoneCastingUntilEndOfTurn",
                        "player":controller(), "sourceZone":query.zone.ir_name(),
                        "where":filter}));
                    return Ok(effects);
                }
                let (cards, maximum) = self.operand(
                    cursor,
                    &permit.cards,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                cursor.enter("lower.card-action.permit");
                // Cards this program exiled, or cards exiled with the source
                // (CR 607.2a): both are in exile when the permission begins.
                let permits_exiled_view = (self.view_verb == Some(CardCollectionVerb::Exiled)
                    && Some(&cards) == self.view.as_ref())
                    || value_kind_name(&cards) == Some("cardsExiledWithSource");
                cursor.require(
                    matches!(
                        value_kind_name(&cards),
                        Some(
                            "boundObjects"
                                | "decisionResult"
                                | "chosenTargets"
                                | "filterObjects"
                                | "setDifference"
                                | "cardsExiledWithSource"
                        )
                    ),
                    "a bound card collection as the permission object",
                )?;
                let card = self.permission_card_reference(cards, maximum, &mut effects);
                let mut replace_from_source = false;
                let duration = match (&permit.duration, permit.window) {
                    // CR 611.2a + 400.7: an unbounded permission over cards
                    // this program exiled lasts while they remain in exile.
                    (None, None) => {
                        cursor.enter("lower.card-action.permit.unbounded");
                        cursor.require(
                            permits_exiled_view,
                            "an unbounded permission over the cards this program exiled",
                        )?;
                        json!({"kind":"whileInExile"})
                    }
                    (None, Some(CardPermissionWindowNode::WhileInExile)) => {
                        json!({"kind":"whileInExile"})
                    }
                    (None, Some(CardPermissionWindowNode::UntilYourNextEndStep)) => {
                        json!({"kind":"untilNextEndStep"})
                    }
                    // The next card this source exiles replaces the window;
                    // until then the permission lasts while its card is in
                    // exile (CR 400.7).
                    (None, Some(CardPermissionWindowNode::UntilSourceExilesAnother)) => {
                        replace_from_source = true;
                        json!({"kind":"whileInExile"})
                    }
                    (Some(EffectDurationNode::UntilEndOfCurrentTurn), None) => {
                        json!({"kind":"untilEndOfCurrentTurn"})
                    }
                    (
                        Some(EffectDurationNode::UntilEndOfNextTurn {
                            player: LocalPlayerReference::Controller,
                        }),
                        None,
                    ) => json!({"kind":"untilEndOfNextTurn", "player":controller()}),
                    (Some(EffectDurationNode::WhileSourceControlled), None) => {
                        json!({"kind":"whileSourceControlled"})
                    }
                    // `until your next turn` (CR 500.1): it ends as the
                    // controller's next turn begins.
                    (
                        Some(EffectDurationNode::UntilNextTurn {
                            player: LocalPlayerReference::Controller,
                        }),
                        None,
                    ) => json!({"kind":"untilNextTurn", "player":controller()}),
                    // `until [the beginning of] your next upkeep`: no player
                    // has priority before that upkeep (CR 502.4, 503.1).
                    (None, Some(CardPermissionWindowNode::UntilYourNextUpkeep)) => {
                        json!({"kind":"untilNextUpkeep", "player":controller()})
                    }
                    _ => {
                        return Err(cursor.engine_gap(
                            crate::oracle::canonical::engine_gap::EngineGap::PermissionWindow,
                            "a permission window the runtime can end: this turn, your next turn or upkeep, the end of your next turn, your next end step, exile, or source control",
                        ));
                    }
                };
                // `without paying its mana cost` (CR 118.9): the permitted
                // cast has no mana cost to pay.
                let normal_costs_apply = !permit.without_paying_mana_cost();
                let mut action = json!({"kind":match permit.verb {
                        CardPermissionVerb::Play => "play",
                        CardPermissionVerb::Cast => "cast",
                    },
                    "card":card, "normalTimingApplies":true,
                    "normalCostsApply":normal_costs_apply});
                if permit.mana_of_any_type {
                    action["spendManaAsAnyType"] = json!(true);
                }
                if let Some(replacement) = graveyard_exile {
                    action["exileInsteadOfGraveyard"] = replacement;
                }
                let mut grant = json!({"kind":"grantPermission", "player":controller(),
                    "action":action, "duration":duration});
                if replace_from_source {
                    grant["replacePermissionsFromSource"] = json!(true);
                }
                effects.push(grant);
            }
            CardActionNode::Move {
                cards: operand,
                destination,
            } => {
                let (mut cards, maximum) = self.operand(
                    cursor,
                    operand,
                    declarations,
                    &mut effects,
                    target_decisions,
                )?;
                // Cards of the observed library collection moved out of the
                // library: a later cast can no longer prove where they are.
                if destination.zone != CardZoneName::Library
                    && matches!(
                        operand,
                        CardOperandNode::Reference(_) | CardOperandNode::Selection(_)
                    )
                {
                    self.view_library_of_controller = false;
                }
                if destination.origin_library {
                    // `put them back`, `on the bottom of that library`: the
                    // cards this program observed in a library return to it
                    // (CR 400.3: their owner's library).
                    cursor.enter("lower.card-action.move.origin-library");
                    cursor.require(
                        self.view_from_library_top
                            && matches!(
                                operand,
                                CardOperandNode::Reference(_) | CardOperandNode::Selection(_)
                            ),
                        "cards this program observed in a library, moved back to that library",
                    )?;
                }
                let player = match &destination.owner {
                    CardMoveOwnerNode::Player(LocalPlayerReference::Controller) => controller(),
                    // Exile in each owner's exile zone, or the battlefield
                    // under each owner's control (CR 110.2a).
                    // A hand, library or graveyard is always its owner's
                    // (CR 400.3); exile and the battlefield name it.
                    CardMoveOwnerNode::EachCardOwner => {
                        json!({"kind":"ownerOf","object":{"kind":"candidate"}})
                    }
                    _ => return Err(cursor.fail("a supported explicit destination owner")),
                };
                let mut to = json!({"kind":destination.zone.ir_name(), "player":player});
                if destination.zone == CardZoneName::Exile {
                    // `exile ... face down` (CR 406.3): hidden in exile.
                    to["faceDown"] = json!(destination.face_down)
                }
                if let Some(position) = destination.position {
                    let (word, above) = position.engine_position();
                    to["position"] = json!(word);
                    // `ORDINAL from the top`: the cards that stay above the
                    // placed card (CR 401.2).
                    if let Some(above) = above {
                        cursor.enter("lower.card-action.move.beneath-top");
                        cursor.require(
                            maximum.is_some_and(|maximum| maximum <= 1),
                            "one card placed beneath the top cards of a library",
                        )?;
                        to["cardsAbove"] = lower_numeric_atom(above);
                    }
                }
                // The card-movement runtime places a card into a library only
                // at a stated position (CR 401.4); `into your library` with no
                // position fails here instead of emitting a refused move.
                cursor.require(
                    destination.zone != CardZoneName::Library
                        || destination.position.is_some()
                        || destination.shuffle,
                    "a library placement on the top or bottom (no runtime places a card at an unstated library position)",
                )?;
                if matches!(operand, CardOperandNode::Targets(_))
                    && destination.zone == CardZoneName::Library
                    && destination.position == Some(LibraryPosition::Top)
                    && destination.order == Some(CollectionOrder::Random)
                    && !maximum.is_some_and(|maximum| maximum <= 1)
                {
                    return Err(cursor.fail(
                        "a proved random-order library-top writer for multiple targeted cards",
                    ));
                }
                let mut order = None;
                // CR 401.4: cards put into one library position at the same
                // time are ordered by their owner. When every moved card
                // comes from the controller's own zone, that owner is the
                // controller, who orders them as a written `in any order`.
                let owner_orders = destination.order.is_none()
                    && destination.zone == CardZoneName::Library
                    && !maximum.is_some_and(|maximum| maximum <= 1)
                    && matches!(
                        destination.owner,
                        CardMoveOwnerNode::Player(LocalPlayerReference::Controller)
                    )
                    && moved_cards_owned_by_controller(operand);
                let order_policy = if owner_orders {
                    Some(CollectionOrder::Chosen)
                } else {
                    destination.order
                };
                match order_policy {
                    Some(CollectionOrder::Chosen) => {
                        let id = fresh("remainderOrder", &mut self.next_order);
                        self.allocated_ids.insert(id.clone());
                        effects.push(json!({"kind":"chooseOrder", "id":id, "player":controller(), "objects":cards}));
                        cards = decision_result(&id);
                        order = Some(json!({"kind":"decisionOrder", "decisionId":id}));
                    }
                    Some(CollectionOrder::Random) => order = Some(json!({"kind":"random"})),
                    None => cursor.require(
                        destination.zone != CardZoneName::Library
                            || maximum.is_some_and(|maximum| maximum <= 1),
                        "an explicit ordering policy for a multi-card library placement",
                    )?,
                }
                // Battlefield movement is admitted for cards whose identities
                // are proved: a choice from an observed collection, declared
                // targets, the object an exile just produced, or a zone query
                // evaluated on resolution. Other observed references (cards
                // revealed until a match) still lack a proved runtime writer.
                // The engine applies entry replacements (CR 614.12).
                if destination.zone == CardZoneName::Battlefield {
                    cursor.enter("lower.card-action.battlefield-entry");
                    let proved_operand = match operand {
                        CardOperandNode::Selection(_)
                        | CardOperandNode::Targets(_)
                        | CardOperandNode::ZoneCollection(_) => true,
                        // The card a zone change put into a public zone.
                        CardOperandNode::Reference(_)
                            if matches!(
                                value_kind_name(&cards),
                                Some("triggeringCard" | "cardsExiledWithSource")
                            ) =>
                        {
                            true
                        }
                        // The cards a resolution choice chose (a search's
                        // found cards): identities fixed by the choice.
                        CardOperandNode::Reference(_) if is_choice_result(&cards) => true,
                        CardOperandNode::Reference(_) => {
                            (self.view_verb == Some(CardCollectionVerb::Exiled)
                                && self.view_zone == Some(CardZoneName::Exile))
                                // The library cards this program looked at or
                                // revealed: the binding fixed their identities
                                // when the action began, so the writer moves
                                // exactly them (`Reveal the top card of your
                                // library. If it's a land card, put it onto
                                // the battlefield.`).
                                || (self.view_from_library_top
                                    && self.view.as_ref() == Some(&cards))
                                // The matching cards of that observation
                                // (`reveal cards until you reveal a creature
                                // card. Put that card onto the battlefield`):
                                // a filter over the same fixed identities.
                                || (self.view_from_library_top
                                    && value_kind_name(&cards) == Some("filterObjects")
                                    && self.view.as_ref() == Some(&cards["objects"]))
                        }
                        // A card chosen (or chosen at random) from a hidden
                        // zone or from outside the game: the choice fixed
                        // its identity (CR 400.11, 608.2c).
                        CardOperandNode::HiddenChoice(_) => is_choice_result(&cards),
                        // The typed object a pronoun names: its reference
                        // fixes the identity it follows (CR 400.7).
                        CardOperandNode::ObjectAntecedent { .. } => true,
                        // Card copies are refused by `operand` before this
                        // point; they exist only to be cast (CR 707.12).
                        CardOperandNode::LibraryTop(_)
                        | CardOperandNode::CardCopies { .. }
                        | CardOperandNode::CommandZone { .. } => false,
                    };
                    cursor.require(
                        proved_operand
                            && !self.allocated_ids.contains(MOVED_ONTO_BATTLEFIELD_BINDING),
                        "one battlefield entry of chosen, targeted, exiled or zone-queried cards",
                    )?;
                    to["tapped"] = json!(destination.tapped);
                    // `tapped and attacking [DEFENDER]` (CR 508.4): the
                    // entered permanents join combat without being declared.
                    if let Some(attack) = &destination.attacking {
                        cursor.enter("lower.card-action.battlefield-entry.attacking");
                        cursor.require(
                            destination.tapped,
                            "an attacking entry that is also tapped",
                        )?;
                        to["attacking"] = cursor.require_value(
                            lower_entering_attack_policy(attack),
                            "an attacked player the triggering event records",
                        )?;
                    }
                    // The shared entry modifiers (CR 712.14a, 708.2).
                    if destination.transformed {
                        to["faceChange"] = json!({"kind":"transformFace"});
                    }
                    if destination.face_down {
                        to["faceDown"] = json!(true);
                    }
                    if let Some(counters) = &destination.entering_counters {
                        let count =
                            lower_targetless_numeric_expression(&counters.count, declarations)
                                .map_err(|error| cursor.fail(error.expectation()))?;
                        to["enterWithCounters"] =
                            json!([{"counter": counters.counter.as_str(), "count": count}]);
                    }
                    // `attached to OBJECT` (CR 303.4f): the recipient is
                    // declared or chosen before the entry; the entered object
                    // is attached to it afterwards.
                    let attachment = match &destination.attached_to {
                        Some(attachment) => {
                            let id = self.allocate_selection_id("attachRecipient");
                            Some(lower_entry_attachment_recipient(
                                cursor,
                                attachment,
                                &id,
                                declarations,
                                &mut effects,
                                target_decisions,
                            )?)
                        }
                        None => None,
                    };
                    self.allocated_ids
                        .insert(MOVED_ONTO_BATTLEFIELD_BINDING.to_string());
                    effects.push(json!({"kind":"moveCards", "cards":cards, "to":to,
                        "bind":MOVED_ONTO_BATTLEFIELD_BINDING}));
                    if let Some(recipient) = attachment {
                        effects.push(json!({"kind":"attachPermanent",
                            "attachment":bound(MOVED_ONTO_BATTLEFIELD_BINDING),
                            "to":recipient}));
                    }
                    return Ok(effects);
                }
                cursor.require(
                    !destination.tapped
                        && destination.attacking.is_none()
                        && (destination.entering_counters.is_none()
                            || (destination.zone == CardZoneName::Exile
                                && !destination.face_down))
                        && !destination.transformed
                        && destination.attached_to.is_none()
                        && (!destination.face_down || destination.zone == CardZoneName::Exile),
                    "an untapped, untransformed nonbattlefield destination, face down or with counters only in exile",
                )?;
                // `exile it with N KIND counters on it` (CR 702.62a).
                if let Some(counters) = &destination.entering_counters {
                    let count = lower_targetless_numeric_expression(&counters.count, declarations)
                        .map_err(|error| cursor.fail(error.expectation()))?;
                    to["enterWithCounters"] =
                        json!([{"counter": counters.counter.as_str(), "count": count}]);
                }
                if matches!(operand, CardOperandNode::LibraryTop(_))
                    && destination.zone == CardZoneName::Exile
                {
                    // The exiled library cards are the observed collection,
                    // now in exile (CR 400.7: their new objects).
                    self.view_verb = Some(CardCollectionVerb::Exiled);
                    self.view_zone = Some(CardZoneName::Exile);
                }
                // `the exiled card(s)` later names these cards, when their
                // identities are fixed (never a zone query re-read after
                // the move empties it).
                if destination.zone == CardZoneName::Exile
                    && matches!(
                        value_kind_name(&cards),
                        Some(
                            "boundObjects"
                                | "decisionResult"
                                | "chosenTargets"
                                | "filterObjects"
                                | "setDifference"
                        )
                    )
                {
                    self.last_exiled = Some((cards.clone(), maximum));
                }
                let mut movement = json!({"kind":"moveCards", "cards":cards, "to":to});
                if matches!(operand, CardOperandNode::Targets(_))
                    && destination.zone == CardZoneName::Exile
                {
                    // `exile target card ... Copy it.`: the exiled targets
                    // are the observed collection a later `it` / `that card`
                    // names, bound to the objects they became (CR 400.7).
                    movement["bind"] = json!(match maximum {
                        Some(1) => self.observe_exiled_object(),
                        maximum => self.observe_exiled_objects(maximum),
                    });
                }
                if let Some(order) = order {
                    movement["order"] = order;
                }
                if destination.shuffle {
                    // `shuffle X into a library` (CR 701.24b): the moved
                    // cards name each library to shuffle. Shuffled cards
                    // are no later antecedent.
                    cursor.require(
                        destination.zone == CardZoneName::Library,
                        "a shuffle only after a library placement",
                    )?;
                    let id = fresh("shuffledCards", &mut self.next_selection);
                    self.allocated_ids.insert(id.clone());
                    movement["bind"] = json!(id);
                    effects.push(movement);
                    effects.push(lower_shuffle_libraries_holding(bound(&id)));
                    self.clear_antecedent();
                } else {
                    effects.push(movement);
                }
            }
        }
        Ok(effects)
    }
}

/// The player a bound reference names when the triggering event records it
/// (`that player`, `defending player`, `that spell's controller`).
fn lower_event_bound_player(player: &BoundPlayerReference) -> Option<Value> {
    match player {
        BoundPlayerReference::EventPlayer => Some(json!({"kind":"triggeringPlayer"})),
        BoundPlayerReference::EventSpellController => {
            Some(json!({"kind":"controllerOf", "object":{"kind":"triggeringStackObject"}}))
        }
        _ => None,
    }
}

/// The defenders an attacking entry admits (CR 508.4): any defender the
/// controller chooses, or the one player the triggering event records,
/// possibly with the planeswalkers that player controls. Shared by every
/// battlefield entry that reads `tapped and attacking`.
pub(in crate::oracle::canonical) fn lower_entering_attack_policy(
    attack: &EnteringAttackNode,
) -> Option<Value> {
    let (kind, player) = match attack {
        EnteringAttackNode::AnyDefender => return Some(json!({"kind":"anyDefender"})),
        EnteringAttackNode::Player(player) => ("player", player),
        EnteringAttackNode::PlayerOrPlaneswalkerTheyControl(player) => {
            ("playerOrPlaneswalkers", player)
        }
    };
    Some(json!({"kind":kind, "player":lower_event_bound_player(player)?}))
}

/// The graveyard exile replacement of the spells a cast frame casts (CR
/// 614.1a): whose graveyard it watches and, for `a PREDICATE spell cast this
/// way`, the cards it applies to. One lowering for a cast during resolution
/// and for a later casting permission.
fn lower_cast_graveyard_exile(
    cursor: &OracleTokenCursor<'_>,
    replacement: &CastGraveyardExileNode,
    declarations: TargetlessDeclarationSupport,
) -> MachineResult<Value> {
    let mut lowered = json!({"graveyard": match replacement.graveyard {
        CastGraveyardOwnerNode::Controller => "controller",
        CastGraveyardOwnerNode::Any => "any",
    }});
    if let Some(predicate) = &replacement.predicate {
        validate_object_predicate_numeric_expressions(predicate, declarations)
            .map_err(|error| cursor.fail(error.expectation()))?;
        lowered["where"] = lower_card_object_predicate(predicate);
    }
    Ok(lowered)
}

/// Every mana-value comparison of a card predicate has a runtime operand.
/// The shared predicate lowering omits a comparison it cannot represent,
/// which would widen the query; this proof makes that an explicit failure.
fn require_lowerable_card_predicate(
    cursor: &OracleTokenCursor<'_>,
    predicate: &ObjectPredicate,
) -> MachineResult<()> {
    match predicate {
        ObjectPredicate::NumericComparison {
            left,
            operator,
            right,
        } => cursor.require(
            lower_selector_numeric_comparison(*left, *operator, right).is_some(),
            "a card comparison whose numeric operand has a runtime value",
        ),
        ObjectPredicate::Not(operand) => require_lowerable_card_predicate(cursor, operand),
        ObjectPredicate::Intersection(operands) | ObjectPredicate::Union(operands) => operands
            .iter()
            .try_for_each(|operand| require_lowerable_card_predicate(cursor, operand)),
        _ => Ok(()),
    }
}

/// A decision result, or a union of decision results: cards a resolution
/// choice fixed (CR 608.2c).
fn is_choice_result(cards: &Value) -> bool {
    match value_kind_name(cards) {
        Some("decisionResult") => true,
        Some("union") => cards["sets"]
            .as_array()
            .is_some_and(|sets| !sets.is_empty() && sets.iter().all(is_choice_result)),
        _ => false,
    }
}

/// True when every card an operand names is owned by the ability's
/// controller: declared targets or a chosen collection in the controller's
/// own zone (CR 400.3: a card is only ever in its owner's hand, library or
/// graveyard).
fn moved_cards_owned_by_controller(operand: &CardOperandNode) -> bool {
    match operand {
        CardOperandNode::Targets(target) => {
            target.owner == CardTargetZoneOwner::Player(LocalPlayerReference::Controller)
                && matches!(
                    target.zone,
                    CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Library
                )
        }
        CardOperandNode::ZoneCollection(query) => {
            query.owner == CardZoneOwnerNode::Player(LocalPlayerReference::Controller)
                && matches!(
                    query.zone,
                    CardZoneName::Hand | CardZoneName::Graveyard | CardZoneName::Library
                )
        }
        CardOperandNode::Reference(_)
        | CardOperandNode::ObjectAntecedent { .. }
        | CardOperandNode::Selection(_)
        | CardOperandNode::LibraryTop(_)
        | CardOperandNode::CardCopies { .. }
        | CardOperandNode::HiddenChoice(_)
        | CardOperandNode::CommandZone { .. } => false,
    }
}

#[cfg(test)]
#[path = "library_top_operand_tests.rs"]
mod library_top_operand_tests;
