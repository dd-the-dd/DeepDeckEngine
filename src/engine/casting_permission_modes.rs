//! Route selection precedes route-specific costs and source quota attribution.
use super::casting_cost::CastingMode;
use super::*;

/// The route that admits a card for casting or playing (see
/// [`GameEngine::casting_route`]).
#[derive(Clone, Copy, Debug)]
pub(super) enum CastingRoute<'a> {
    Hand,
    CommandZone,
    Mayhem,
    DiscardCast,
    Flashback,
    Escape,
    ArcadeGannon,
    StaticPermission,
    GraveyardLand,
    /// CR 702.127a: the aftermath half of a split card in its owner's
    /// graveyard; only that half is cast through this route.
    Aftermath,
    TopOfLibrary,
    CardPermission(&'a Value),
}

const ADVENTURE_PERMISSION: &str = "adventureCastPermission";

pub(super) struct PreparedCastingRoute {
    pub declarations: Vec<CastingDeclaration>,
    pub permission: Option<Value>,
    pub is_miracle: bool,
}
/// The life a card permission's alternative cost asks for, evaluated on the
/// definition actually being cast (CR 716.3a: an Adventure's own mana value).
pub(super) fn casting_permission_life_cost(permission: &Value, casting: &CardDefinition) -> i32 {
    let cost = &permission["alternativeLifeCost"];
    if value_kind(cost) == Some("manaValueOf") && value_kind(&cost["object"]) == Some("castCard") {
        return mana_value(casting).max(0);
    }
    0
}

fn ordinary_base(decisions: &BTreeMap<String, Value>) -> bool {
    CastingMode::of(decisions).uses_ordinary_base()
}
impl GameEngine {
    /// The one scope check for a permission that names a card: whether it
    /// currently lets `player_id` use that card. It resolves the permitted
    /// player (a prepared spell follows its source's controller), then checks
    /// expiry, its condition, controller-turn timing, source control, and, for
    /// its card, the zone it is confined to and the incarnation it was granted
    /// for (CR 400.7). Every admission of a card permission uses it.
    pub(super) fn card_permission_in_scope(
        &self,
        permission: &Value,
        player_id: &str,
        casting: Option<&CardDefinition>,
    ) -> bool {
        let source_controller = || {
            permission["sourceCardInstanceId"]
                .as_str()
                .and_then(|source_id| self.permanent_position(source_id))
                .map(|(source_player, source_index)| {
                    self.state.players[source_player].battlefield[source_index]
                        .controller
                        .as_str()
                })
        };
        let permitted_player = if value_kind(permission) == Some("preparedSpellPermission") {
            source_controller()
        } else {
            permission["playerId"].as_str()
        };
        permitted_player == Some(player_id)
            && permission["expiresAfterTurn"]
                .as_u64()
                .is_none_or(|turn| turn >= u64::from(self.state.turn_number))
            && self.permission_condition_met(permission, player_id)
            && (permission["duringControllerTurnOnly"].as_bool() != Some(true)
                || self.state.players[self.state.active_player].id == player_id)
            && (permission["whileSourceControlled"].as_bool() != Some(true)
                || source_controller() == Some(player_id))
            && permission["cardInstanceId"]
                .as_str()
                .is_none_or(|instance_id| {
                    permission["whileInZone"]
                        .as_str()
                        .is_none_or(|zone| self.card_zone_owner_id(instance_id, zone).is_some())
                        && self.permission_incarnation_current(permission, instance_id)
                })
            // What it permits, and for what: playing or casting, and only a
            // card matching its filter with the characteristics being used.
            && permission["action"]
                .as_str()
                .is_none_or(|action| matches!(action, "play" | "cast"))
            && casting.is_none_or(|definition| {
                permission
                    .get("where")
                    .is_none_or(|filter| matches_card_filter(definition, filter))
            })
    }

    /// The route that admits `card` (with the characteristics it is being
    /// cast with) from `zone` for `player`, or `None` (CR 601.2a, 601.3e). It
    /// is the one enumeration of casting sources: the creature and an
    /// Adventure (CR 716.3a) both go through it, each with its own
    /// characteristics. A card named by a permission is admitted from the
    /// zone it is actually in, never an assumed one.
    pub(super) fn casting_route<'a>(
        &'a self,
        player: &EnginePlayer,
        card: &CardInstance,
        zone: &str,
        adventure: bool,
    ) -> Option<CastingRoute<'a>> {
        let own = |cards: &[CardInstance]| {
            cards
                .iter()
                .any(|held| held.instance_id == card.instance_id)
        };
        let definition = &card.definition;
        let route = match zone {
            "hand" if own(&player.hand) => Some(CastingRoute::Hand),
            // Known gap, failing closed: no Adventure from the command zone.
            "commandZone" if own(&player.command_zone) && !adventure => {
                Some(CastingRoute::CommandZone)
            }
            "graveyard" if own(&player.graveyard) => {
                let discarded_this_turn = self.state.events.iter().any(|event| {
                    event.turn_number == self.state.turn_number
                        && event.kind == "cardDiscarded"
                        && event.player_id.as_deref() == Some(player.id.as_str())
                        && event.card_instance_id.as_deref() == Some(card.instance_id.as_str())
                });
                if mayhem_ability(definition).is_some() && discarded_this_turn {
                    Some(CastingRoute::Mayhem)
                } else if graveyard_discard_cast_ability(definition) {
                    Some(CastingRoute::DiscardCast)
                } else if self.effective_flashback_cost(card).is_some()
                    && (is_instant(definition) || is_sorcery(definition))
                {
                    Some(CastingRoute::Flashback)
                } else if escape_ability(definition).is_some() {
                    Some(CastingRoute::Escape)
                } else if self
                    .arcade_gannon_source_for_card(&player.id, card)
                    .is_some()
                {
                    Some(CastingRoute::ArcadeGannon)
                } else if self
                    .battlefield_casting_permission(&player.id, card, "graveyard")
                    .is_some()
                {
                    Some(CastingRoute::StaticPermission)
                } else if self.can_play_lands_from_graveyard(&player.id) && is_land(definition) {
                    Some(CastingRoute::GraveyardLand)
                } else if super::keyword_programs::split_card_has_aftermath(definition) {
                    Some(CastingRoute::Aftermath)
                } else {
                    None
                }
            }
            "exile"
                if own(&player.exile)
                    && card.owner == player.id
                    && self
                        .battlefield_casting_permission(&player.id, card, "exile")
                        .is_some() =>
            {
                Some(CastingRoute::StaticPermission)
            }
            "library"
                if player.library.last().map(|top| top.instance_id.as_str())
                    == Some(card.instance_id.as_str())
                    && self.top_library_cast_admitted(&player.id, definition) =>
            {
                Some(CastingRoute::TopOfLibrary)
            }
            _ => None,
        };
        route.or_else(|| {
            self.state
                .permissions
                .iter()
                .find(|permission| {
                    permission["cardInstanceId"].as_str() == Some(card.instance_id.as_str())
                        // CR 716.3d: the Adventure's own exile admits only the creature.
                        && !(adventure && value_kind(permission) == Some(ADVENTURE_PERMISSION))
                        && permission["sourceZone"]
                            .as_str()
                            .is_none_or(|source_zone| source_zone == zone)
                        && self.card_permission_in_scope(permission, &player.id, Some(definition))
                })
                .filter(|_| self.card_in_permission_zone(&card.instance_id, zone))
                .map(CastingRoute::CardPermission)
        })
    }

    /// Whether a card a permission names is really in `zone` and usable from
    /// there: any player's hand, graveyard, exile or command zone, or the top
    /// of a library (the zone it is actually in, not an assumed one).
    fn card_in_permission_zone(&self, instance_id: &str, zone: &str) -> bool {
        match zone {
            "library" => self.state.players.iter().any(|player| {
                player.library.last().map(|top| top.instance_id.as_str()) == Some(instance_id)
            }),
            "hand" | "graveyard" | "exile" | "commandZone" => {
                self.card_zone_owner_id(instance_id, zone).is_some()
            }
            _ => false,
        }
    }

    /// The zone a card outside the battlefield is actually in.
    pub(super) fn card_outside_battlefield_zone(&self, instance_id: &str) -> Option<&'static str> {
        ["hand", "graveyard", "exile", "library", "commandZone"]
            .into_iter()
            .find(|zone| self.card_zone_owner_id(instance_id, zone).is_some())
    }

    /// Whether a card with `definition` may be played from the top of
    /// `player_id`'s library: a top-of-library permission whose filter it
    /// matches, or this turn's unfiltered top-of-library casting.
    pub(super) fn top_library_cast_admitted(
        &self,
        player_id: &str,
        definition: &CardDefinition,
    ) -> bool {
        self.battlefield_static_modifiers()
            .iter()
            .any(|(source, modifier)| {
                source.controller == player_id
                    && value_kind(modifier) == Some("playCardsFromTopLibrary")
                    && matches_card_filter(definition, &modifier["where"])
            })
            || self.top_library_casting_window(player_id).is_some()
    }

    /// This turn's window to cast the top card of `player_id`'s library
    /// (`topLibraryCastingWindow`): typed attributes, not a card-named kind.
    /// `whileSourceControlled` ends it once its source changes control, and an
    /// `alternativeLifeCost` is paid instead of the mana cost.
    pub(super) fn top_library_casting_window(&self, player_id: &str) -> Option<&Value> {
        self.state.rule_modifiers.iter().find(|modifier| {
            value_kind(modifier) == Some("topLibraryCastingWindow")
                && modifier["playerId"].as_str() == Some(player_id)
                && modifier["expiresAfterTurn"].as_u64() == Some(u64::from(self.state.turn_number))
                && (modifier["whileSourceControlled"].as_bool() != Some(true)
                    || modifier["sourceCardInstanceId"]
                        .as_str()
                        .and_then(|source_id| self.permanent_position(source_id))
                        .is_some_and(|(source_player, source_index)| {
                            self.state.players[source_player].battlefield[source_index].controller
                                == player_id
                        }))
        })
    }

    /// A permission bound to one incarnation (`zoneIdentity`) ends when its
    /// card leaves that zone, even if it comes back (CR 400.7). An unbound
    /// permission is confined by `whileInZone` and ended by
    /// `end_card_permissions_confined_to` when its card leaves that zone.
    pub(super) fn permission_incarnation_current(
        &self,
        permission: &Value,
        instance_id: &str,
    ) -> bool {
        let Some(bound) = permission["zoneIdentity"].as_str() else {
            return true;
        };
        self.card_outside_battlefield(instance_id)
            .and_then(Self::tracked_zone_identity)
            == Some(bound)
    }

    /// Binds a single-card permission to its card's current incarnation
    /// (`zoneIdentity`), so the permission ends once that card leaves its zone
    /// (CR 400.7). A card on the battlefield or without one is left unbound.
    pub(super) fn bind_card_permission_to_incarnation(&self, permission: &mut Value) {
        let identity = permission["cardInstanceId"]
            .as_str()
            .and_then(|instance_id| self.card_outside_battlefield(instance_id))
            .and_then(Self::tracked_zone_identity)
            .map(ToOwned::to_owned);
        if let Some(identity) = identity {
            permission["zoneIdentity"] = Value::from(identity);
        }
    }

    /// CR 400.7: a card that becomes a new object leaves behind every
    /// permission confined to the zone it left.
    pub(super) fn end_card_permissions_confined_to(&mut self, instance_id: &str, zone: &str) {
        self.state.permissions.retain(|permission| {
            permission["cardInstanceId"].as_str() != Some(instance_id)
                || permission["whileInZone"].as_str() != Some(zone)
        });
    }

    pub(super) fn static_casting_permission_applies(
        &self,
        source: &CardInstance,
        modifier: &Value,
        player_id: &str,
        card: &CardInstance,
        source_zone: &str,
        require_without_mana_cost: bool,
    ) -> bool {
        value_kind(modifier) == Some("castingPermission")
            && modifier["sourceZone"].as_str() == Some(source_zone)
            && (!require_without_mana_cost
                || modifier["withoutPayingManaCost"].as_bool() == Some(true))
            && match value_kind(&modifier["players"]) {
                Some("eachPlayer") => true,
                Some("opponentsOf") => source.controller != player_id,
                Some("controllerOf") => source.controller == player_id,
                _ => false,
            }
            && matches_card_filter(&card.definition, &modifier["where"])
            && static_player_permissions::casting_permission_admits_card(modifier, source, card)
            && (!modifier["linkedToSource"].as_bool().unwrap_or(false)
                || self.state.rule_modifiers.iter().any(|link| {
                    value_kind(link) == Some("exiledWithSource")
                        && link["sourceCardInstanceId"].as_str()
                            == Some(source.instance_id.as_str())
                        && link["cardInstanceId"].as_str() == Some(card.instance_id.as_str())
                }))
            && (!modifier["duringControllerTurnOnly"]
                .as_bool()
                .unwrap_or(false)
                || self.state.players[self.state.active_player].id == player_id)
            && (!modifier["onceEachTurn"].as_bool().unwrap_or(false)
                || !self.state.events.iter().any(|event| {
                    event.turn_number == self.state.turn_number
                        && event.kind == "spellCast"
                        && event.player_id.as_deref() == Some(player_id)
                        && event.detail["decisions"]["staticCastingPermissionSource"].as_str()
                            == Some(source.instance_id.as_str())
                }))
    }

    pub(super) fn prepare_casting_routes(
        &self,
        card: &CardInstance,
        player_id: &str,
        source_zone: &str,
        declarations: Vec<CastingDeclaration>,
        permission: Option<&Value>,
        normal_timing: bool,
    ) -> Result<Vec<PreparedCastingRoute>, EngineError> {
        if source_zone != "hand" {
            let declarations = if let Some((source, modifier)) =
                self.battlefield_casting_permission(player_id, card, source_zone)
            {
                self.expand_bound_casting_permission_costs(
                    player_id,
                    &source,
                    &modifier,
                    declarations,
                )?
            } else {
                declarations
            };
            return Ok(vec![PreparedCastingRoute {
                declarations,
                permission: permission.cloned(),
                is_miracle: false,
            }]);
        }
        let miracle = permission.filter(|p| value_kind(p) == Some("miraclePermission"));
        let mut routes = Vec::new();
        if normal_timing {
            let mut normal = self.expand_impending_declarations(declarations.clone(), card);
            for declaration in &mut normal {
                if ordinary_base(&declaration.decisions) {
                    declaration
                        .decisions
                        .insert("withoutPayingManaCost".into(), json!(false));
                }
            }
            routes.push(PreparedCastingRoute {
                declarations: normal.clone(),
                permission: permission.filter(|_| miracle.is_none()).cloned(),
                is_miracle: false,
            });
            // Other runtime windows with their own alternative base remain
            // independent, as does Miracle below. Static waiver uses normal timing.
            if permission.is_none() || miracle.is_some() {
                let mut seen = Vec::<(String, Value)>::new();
                for (source, modifier) in
                    self.battlefield_static_modifiers()
                        .into_iter()
                        .filter(|(source, modifier)| {
                            self.static_casting_permission_applies(
                                source,
                                modifier,
                                player_id,
                                card,
                                source_zone,
                                true,
                            )
                        })
                {
                    if seen
                        .iter()
                        .any(|(id, policy)| id == &source.instance_id && policy == &modifier)
                    {
                        continue;
                    }
                    seen.push((source.instance_id.clone(), modifier.clone()));
                    if !modifier["additionalCost"].is_null()
                        && !matches!(
                            value_kind(&modifier["additionalCost"]),
                            Some("sacrificePermanent" | "removeCountersFromControlledCreatures")
                        )
                    {
                        return Err(EngineError::new(
                            "unsupported static casting permission route cost",
                        ));
                    }
                    let identity = object_identity::object_identity_of(&source)?;
                    let mut waived = Vec::new();
                    for mut declaration in normal
                        .iter()
                        .filter(|d| ordinary_base(&d.decisions))
                        .cloned()
                    {
                        declaration
                            .decisions
                            .insert("withoutPayingManaCost".into(), json!(true));
                        declaration
                            .decisions
                            .insert("usesStaticCastingPermissionBaseCost".into(), json!(true));
                        declaration.decisions.insert(
                            "staticCastingPermissionAuthorization".into(),
                            json!({"source":identity,"policy":modifier}),
                        );
                        if self.declared_x_is_legal_for_selected_casting_cost(
                            &card.definition,
                            player_id,
                            &declaration.decisions,
                        ) {
                            waived.push(declaration);
                        }
                    }
                    let waived = self.expand_bound_casting_permission_costs(
                        player_id, &source, &modifier, waived,
                    )?;
                    routes.push(PreparedCastingRoute {
                        declarations: waived,
                        permission: None,
                        is_miracle: false,
                    });
                }
            }
        }
        if let Some(miracle) = miracle {
            let cost = miracle["alternativeManaCost"]
                .as_str()
                .ok_or_else(|| EngineError::new("miracle permission lacks its base mana cost"))?;
            let mut selected = Vec::new();
            for mut declaration in declarations
                .into_iter()
                .filter(|d| ordinary_base(&d.decisions))
            {
                declaration
                    .decisions
                    .insert("miracleCast".into(), json!(true));
                declaration
                    .decisions
                    .insert("miracleManaCost".into(), json!(cost));
                declaration
                    .decisions
                    .insert("castDuringResolution".into(), json!(true));
                if self.declared_x_is_legal_for_selected_casting_cost(
                    &card.definition,
                    player_id,
                    &declaration.decisions,
                ) {
                    selected.push(declaration);
                }
            }
            routes.push(PreparedCastingRoute {
                declarations: selected,
                permission: Some(miracle.clone()),
                is_miracle: true,
            });
        }
        Ok(routes)
    }

    fn expand_bound_casting_permission_costs(
        &self,
        player_id: &str,
        permission_source: &CardInstance,
        permission_modifier: &Value,
        mut declarations: Vec<CastingDeclaration>,
    ) -> Result<Vec<CastingDeclaration>, EngineError> {
        let player = &self.state.players[self.player_index(player_id)?];
        if value_kind(&permission_modifier["additionalCost"]) == Some("sacrificePermanent") {
            let candidates = player
                .battlefield
                .iter()
                .filter(|permanent| {
                    permanent.controller == player_id
                        && !permanent.flags.get("phasedOut").copied().unwrap_or(false)
                        && self.permanent_can_be_sacrificed(permanent)
                        && self.permanent_matches_effective_filter(
                            permanent,
                            &permission_modifier["additionalCost"]["where"],
                        )
                })
                .map(|permanent| permanent.instance_id.clone())
                .collect::<Vec<_>>();
            declarations = declarations
                .into_iter()
                .flat_map(|declaration| {
                    candidates.iter().map({
                        let permission_source_id = permission_source.instance_id.clone();
                        move |instance_id| {
                            let mut declaration = declaration.clone();
                            declaration.decisions.insert(
                                "staticCastingPermissionSource".to_string(),
                                Value::String(permission_source_id.clone()),
                            );
                            declaration.decisions.insert(
                                "staticCastingSacrifice".to_string(),
                                Value::String(instance_id.clone()),
                            );
                            declaration.decisions.insert(
                                "staticCastingSacrificeFilter".to_string(),
                                permission_modifier["additionalCost"]["where"].clone(),
                            );
                            declaration
                        }
                    })
                })
                .collect();
        } else if value_kind(&permission_modifier["additionalCost"])
            == Some("removeCountersFromControlledCreatures")
        {
            let required = integer_value(&permission_modifier["additionalCost"]["count"])
                .and_then(|count| usize::try_from(count).ok())
                .unwrap_or(0);
            let mut counter_units = Vec::new();
            for permanent in player
                .battlefield
                .iter()
                .filter(|permanent| self.permanent_is_creature(permanent))
            {
                for (counter, count) in &permanent.counters {
                    for ordinal in 0..(*count).max(0) {
                        counter_units.push(format!(
                            "{}\u{1f}{}\u{1f}{ordinal}",
                            permanent.instance_id, counter
                        ));
                    }
                }
            }
            let selections =
                Self::resolution_selections(&counter_units, required, MAX_DECLARATION_OPTIONS);
            declarations = declarations
                .into_iter()
                .flat_map(|declaration| {
                    selections.iter().map({
                        let permission_source_id = permission_source.instance_id.clone();
                        move |selection| {
                            let mut counts = BTreeMap::<(String, String), i32>::new();
                            for unit in selection {
                                let mut parts = unit.split('\u{1f}');
                                let instance_id = parts.next().unwrap_or_default();
                                let counter = parts.next().unwrap_or_default();
                                *counts
                                    .entry((instance_id.to_string(), counter.to_string()))
                                    .or_default() += 1;
                            }
                            let payment = counts
                                .into_iter()
                                .map(|((instance_id, counter), count)| {
                                    json!({
                                        "cardInstanceId": instance_id,
                                        "counter": counter,
                                        "count": count,
                                    })
                                })
                                .collect::<Vec<_>>();
                            let mut next = declaration.clone();
                            next.decisions.insert(
                                "staticCastingPermissionSource".to_string(),
                                Value::String(permission_source_id.clone()),
                            );
                            next.decisions.insert(
                                "staticCastingCounterRemoval".to_string(),
                                Value::Array(payment),
                            );
                            next.decisions.insert(
                                "staticCastingCounterRemovalRequired".to_string(),
                                json!(i32::try_from(required).unwrap_or(i32::MAX)),
                            );
                            next
                        }
                    })
                })
                .take(MAX_DECLARATION_OPTIONS)
                .collect();
        } else {
            for declaration in &mut declarations {
                declaration.decisions.insert(
                    "staticCastingPermissionSource".to_string(),
                    Value::String(permission_source.instance_id.clone()),
                );
                if value_kind(&permission_modifier) == Some("eccMuldrothaPermission")
                    && let Some(permanent_type) = permission_modifier["permanentType"].as_str()
                {
                    declaration.decisions.insert(
                        "muldrothaPermanentType".to_string(),
                        Value::String(permanent_type.to_string()),
                    );
                }
            }
        }
        Ok(declarations)
    }

    /// A cost resource is a decision, not a Magic target/declaration. Standard
    /// sacrifice payment owns movement, replacements and deferred cost events.
    pub(super) fn apply_static_casting_sacrifice_choice(
        &mut self,
        action: &LegalAction,
    ) -> Result<(), EngineError> {
        let choice = action.decisions.get("staticCastingSacrifice");
        let filter = action.decisions.get("staticCastingSacrificeFilter");
        let policy_cost = action
            .decisions
            .get("staticCastingPermissionAuthorization")
            .and_then(|auth| auth.get("policy"))
            .and_then(|policy| policy.get("additionalCost"));
        if choice.is_none() && filter.is_none() {
            if policy_cost.is_some_and(|cost| value_kind(cost) == Some("sacrificePermanent")) {
                return Err(EngineError::new(
                    "static casting sacrifice cost choice is missing",
                ));
            }
            return Ok(());
        }
        let instance_id = choice
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::new("static casting sacrifice cost choice is missing"))?;
        let filter = filter
            .ok_or_else(|| EngineError::new("static casting sacrifice cost filter is missing"))?;
        if policy_cost.is_some_and(|cost| {
            value_kind(cost) != Some("sacrificePermanent") || &cost["where"] != filter
        }) {
            return Err(EngineError::new(
                "static casting sacrifice cost does not match selected permission",
            ));
        }
        let (owner, index) = self.permanent_position(instance_id).ok_or_else(|| {
            EngineError::new("static casting sacrifice cost permanent left battlefield")
        })?;
        let permanent = &self.state.players[owner].battlefield[index];
        if permanent.controller != action.player_id
            || permanent.flags.get("phasedOut").copied().unwrap_or(false)
            || !self.permanent_can_be_sacrificed(permanent)
            || !self.permanent_matches_effective_filter(permanent, filter)
        {
            return Err(EngineError::new(
                "static casting sacrifice cost is no longer payable",
            ));
        }
        let source = action
            .card_instance_id
            .as_deref()
            .ok_or_else(|| EngineError::new("static casting cost lacks spell source"))?;
        self.sacrifice_permanent_as_cost(&action.player_id, source, instance_id)
    }

    /// Authorization is checked before cost or stack mutation. Ordinary
    /// resolution-time waivers use their existing authorization route and are
    /// not tagged as this static-permission mode.
    pub(super) fn validate_static_casting_base_permission(
        &self,
        action: &LegalAction,
    ) -> Result<(), EngineError> {
        if action
            .decisions
            .get("usesStaticCastingPermissionBaseCost")
            .and_then(Value::as_bool)
            != Some(true)
        {
            // Raw priority actions are private; public adapters select from
            // generated actions rather than merge caller decisions. Still reject
            // an untagged hand waiver unless it is an exact current priority
            // action. Existing trusted resolution-time constructors have their
            // own castDuringResolution/paradigm context and remain unchanged.
            if action.kind == ActionKind::CastSpell
                && action
                    .decisions
                    .get("withoutPayingManaCost")
                    .and_then(Value::as_bool)
                    == Some(true)
                && action
                    .decisions
                    .get("castSourceZone")
                    .and_then(Value::as_str)
                    .unwrap_or("hand")
                    == "hand"
                && action
                    .decisions
                    .get("castDuringResolution")
                    .and_then(Value::as_bool)
                    != Some(true)
                && action
                    .decisions
                    .get("paradigmCopy")
                    .and_then(Value::as_bool)
                    != Some(true)
            {
                let player = self.player_index(&action.player_id)?;
                let authorized = self
                    .try_legal_priority_actions(player)?
                    .iter()
                    .any(|current| {
                        current.kind == action.kind
                            && current.id == action.id
                            && current.player_id == action.player_id
                            && current.card_instance_id == action.card_instance_id
                            && current.decisions == action.decisions
                            && current.targets == action.targets
                            && current.payment_sources == action.payment_sources
                            && current.target_order == action.target_order
                    });
                if !authorized {
                    return Err(EngineError::new(
                        "untagged hand casting waiver is not an authorized current action",
                    ));
                }
            }
            return Ok(());
        }
        if action.kind != ActionKind::CastSpell
            || action
                .decisions
                .get("withoutPayingManaCost")
                .and_then(Value::as_bool)
                != Some(true)
            || action
                .decisions
                .get("castSourceZone")
                .and_then(Value::as_str)
                != Some("hand")
        {
            return Err(EngineError::new("invalid static casting base-cost mode"));
        }
        let player = self.player_index(&action.player_id)?;
        let source_id = action
            .card_instance_id
            .as_deref()
            .ok_or_else(|| EngineError::new("casting mode lacks spell source"))?;
        let card = self.state.players[player]
            .hand
            .iter()
            .find(|card| card.instance_id == source_id)
            .ok_or_else(|| EngineError::new("static casting permission spell left hand"))?;
        let mut casting_card = card.clone();
        if let Some(face) = action
            .decisions
            .get("splitFaceIndex")
            .and_then(Value::as_u64)
        {
            let face = usize::try_from(face)
                .map_err(|_| EngineError::new("invalid casting permission face index"))?;
            if !apply_split_face(&mut casting_card, face) {
                return Err(EngineError::new(
                    "casting permission face is no longer available",
                ));
            }
        }
        if action.decisions.get("fused").and_then(Value::as_bool) == Some(true) {
            casting_card.definition =
                fused_split_definition(&casting_card.definition).ok_or_else(|| {
                    EngineError::new("casting permission fused face is no longer available")
                })?;
        }
        let authorization = action
            .decisions
            .get("staticCastingPermissionAuthorization")
            .ok_or_else(|| {
                EngineError::new("static casting permission authorization is missing")
            })?;
        let identity: object_identity::ObjectIdentity =
            serde_json::from_value(authorization["source"].clone()).map_err(|_| {
                EngineError::new("invalid static casting permission source identity")
            })?;
        let policy = &authorization["policy"];
        let current = self
            .battlefield_static_modifiers()
            .into_iter()
            .find(|(source, modifier)| {
                source.instance_id == identity.instance_id && modifier == policy
            });
        let Some((source, modifier)) = current else {
            return Err(EngineError::new(
                "selected static casting base-cost permission is no longer active",
            ));
        };
        if object_identity::object_identity_of(&source)? != identity
            || action
                .decisions
                .get("staticCastingPermissionSource")
                .and_then(Value::as_str)
                != Some(identity.instance_id.as_str())
            || !self.static_casting_permission_applies(
                &source,
                &modifier,
                &action.player_id,
                &casting_card,
                "hand",
                true,
            )
        {
            return Err(EngineError::new(
                "selected static casting base-cost permission is no longer active",
            ));
        }

        Ok(())
    }
}
#[cfg(all(test, feature = "ci-unit-tests-3"))]
#[path = "casting_permission_modes_tests.rs"]
mod tests;
