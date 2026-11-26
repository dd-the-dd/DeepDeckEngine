//! Shared player populations independent of the action applied to them.
use super::super::{MachineResult, OracleTokenCursor};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerSetReferenceNode {
    EachPlayer,
    OpponentsOfAbilityController,
    /// `each other opponent` after an event naming one opponent (`deals
    /// combat damage to an opponent, it deals that much damage to each other
    /// opponent`): the ability controller's opponents but the event's player
    /// (CR 102.3). Read only by `recognize_other_opponents_of_event_player`.
    OpponentsOtherThanEventPlayer,
}

/// `each other opponent`. Lexical lookahead only.
pub(in crate::oracle::canonical) fn other_opponents_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "each") && word_at(cursor, 1, "other") && word_at(cursor, 2, "opponent")
}

/// `each other opponent`: `other` excludes the opponent the triggering
/// event names, so the frame must bind an event player; without one the
/// phrase has no antecedent and fails here.
pub(in crate::oracle::canonical) fn recognize_other_opponents_of_event_player(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    cursor.enter("player-set.other-opponent");
    cursor.consume_phrase(&["each", "other", "opponent"])?;
    cursor.enter("player-set.other-opponent.antecedent");
    cursor.require(
        super::installed_binding_environment(cursor)
            .frame
            .event_player,
        "an event player that `each other opponent` excludes",
    )?;
    Ok(PlayerSetReferenceNode::OpponentsOtherThanEventPlayer)
}

pub(in crate::oracle::canonical) fn recognize_player_set_reference(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<PlayerSetReferenceNode> {
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("each"))
    {
        cursor.enter("player-set.quantifier");
        cursor.consume("each")?;
        cursor.enter("player-set.role");
        let role = if cursor
            .peek()
            .is_some_and(|raw| raw.eq_ignore_ascii_case("opponent"))
        {
            cursor.consume("opponent")?;
            PlayerSetReferenceNode::OpponentsOfAbilityController
        } else {
            cursor.consume("player")?;
            PlayerSetReferenceNode::EachPlayer
        };
        cursor.enter("player-set.complete");
        return Ok(role);
    }
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("players"))
    {
        cursor.enter("player-set.role");
        cursor.consume("players")?;
        cursor.enter("player-set.complete");
        return Ok(PlayerSetReferenceNode::EachPlayer);
    }
    // Preserve the historical opponent reader's states and failure boundary.
    cursor.enter("player-set.possessor");
    cursor.consume("your")?;
    cursor.enter("player-set.role");
    cursor.consume("opponents")?;
    cursor.enter("player-set.complete");
    Ok(PlayerSetReferenceNode::OpponentsOfAbilityController)
}

#[cfg(test)]
#[path = "player_set_tests.rs"]
mod tests;

/// A zone holder is access scope, never an ownership predicate or target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum ZoneHolderQueryNode {
    Player(super::LocalPlayerReference),
    Players(PlayerSetReferenceNode),
    /// `that player's`, `their`, `its controller's`, `defending player's`,
    /// `target opponent's` (CR 108.3, 109.4, 115.1, 400.1): the one player
    /// another reference names. It is the same related-player node an
    /// object query's controller relation carries (`creatures that player
    /// controls`): a bound player is the binding environment's, a declared
    /// player is a target of the action that reads the query. The holder
    /// relation stays access scope; it never tests the cards' owner.
    Related(super::RelatedPlayer),
}

pub(in crate::oracle::canonical) fn recognize_zone_holder_query(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<ZoneHolderQueryNode> {
    cursor.enter("query.zone.holder");
    if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("your"))
    {
        cursor.consume("your")?;
        Ok(ZoneHolderQueryNode::Player(
            super::LocalPlayerReference::Controller,
        ))
    } else if cursor
        .peek()
        .is_some_and(|raw| raw.eq_ignore_ascii_case("all"))
    {
        cursor.consume("all")?;
        // `all players' hands`: the same population named by its holders.
        if let Some(players) = cursor
            .peek()
            .filter(|raw| {
                raw.eq_ignore_ascii_case("players'") || raw.eq_ignore_ascii_case("players\u{2019}")
            })
            .map(str::to_owned)
        {
            cursor.consume(&players)?;
        }
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else if {
        let mut lexical = cursor.clone();
        super::recognize_card_zone_atom(&mut lexical)
            .ok()
            .is_some_and(|zone| {
                cursor
                    .peek()
                    .is_some_and(|raw| !raw.eq_ignore_ascii_case(zone.ir_name()))
            })
    } {
        // A plural zone noun denotes the holder population. The zone child
        // owns the actual noun and proves its lexical kind.
        Ok(ZoneHolderQueryNode::Players(
            PlayerSetReferenceNode::EachPlayer,
        ))
    } else if let Some(holder) = recognize_population_zone_holder(cursor) {
        holder
    } else if let Some(holder) = recognize_related_zone_holder(cursor) {
        holder.map(ZoneHolderQueryNode::Related)
    } else {
        Err(cursor.fail("your zone or all player zones; zone targeting/selection requires an explicit typed producer"))
    }
}

/// The lowercase words at the cursor, for the lexical lookahead of a zone
/// holder (missing positions are empty).
fn holder_words(cursor: &OracleTokenCursor<'_>, count: usize) -> Vec<String> {
    (0..count)
        .map(|offset| cursor.peek_ahead(offset).unwrap_or("").to_ascii_lowercase())
        .collect()
}

/// `NOUN's` written with either apostrophe.
fn is_possessive_of(word: &str, noun: &str) -> bool {
    word.strip_suffix("'s")
        .or_else(|| word.strip_suffix("\u{2019}s"))
        .is_some_and(|base| base == noun)
}

/// True when a registered card zone noun starts `offset` tokens ahead.
fn zone_noun_at(cursor: &OracleTokenCursor<'_>, offset: usize) -> bool {
    let mut lexical = cursor.clone();
    lexical.advance_to(lexical.position() + offset).is_ok()
        && super::recognize_card_zone_atom(&mut lexical).is_ok()
}

/// `each graveyard`, `each player's hand`, `each opponent's library` (CR
/// 400.1): the zone of every member of the population, the same holder as
/// `all graveyards` / `your opponents' graveyards`. `None` (nothing read)
/// for any other head.
fn recognize_population_zone_holder(
    cursor: &mut OracleTokenCursor<'_>,
) -> Option<MachineResult<ZoneHolderQueryNode>> {
    let words = holder_words(cursor, 2);
    if words[0] != "each" {
        return None;
    }
    let (width, players) = if is_possessive_of(&words[1], "player") {
        (2, PlayerSetReferenceNode::EachPlayer)
    } else if is_possessive_of(&words[1], "opponent") {
        (2, PlayerSetReferenceNode::OpponentsOfAbilityController)
    } else if zone_noun_at(cursor, 1) {
        (1, PlayerSetReferenceNode::EachPlayer)
    } else {
        return None;
    };
    cursor.enter("query.zone.holder.each");
    Some(
        cursor
            .advance_to(cursor.position() + width)
            .map(|()| ZoneHolderQueryNode::Players(players)),
    )
}

/// The token at `index` of `tokens` reclassified as `word` by the shared
/// tokenizer: a view of the same tokens (equal length, indices and spans)
/// in which a possessive holder reads as the player reference it restates.
fn reclassify_token(
    tokens: &mut [crate::oracle::model::RecognizedEntity],
    index: usize,
    word: &str,
) {
    if let Some(token) = tokens.get_mut(index) {
        if let Some(class) = crate::oracle::syntax::recognize_entities(word)
            .into_iter()
            .next()
        {
            token.category = class.category;
            token.token_type = class.token_type;
        }
        token.raw = word.to_string();
    }
}

/// The written form of a possessive player holder at the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelatedHolderHead {
    /// `their ZONE`: the distributive pronoun.
    Their,
    /// `that player's`, `that opponent's`.
    ThatPlayer,
    /// `defending player's`.
    DefendingPlayer,
    /// `its controller's`, `that creature's owner's`, `this creature's
    /// controller's`: the relation word is `relation` tokens ahead.
    ObjectRelation { relation: usize },
    /// `target player's`, `target opponent's`.
    DeclaredTarget { opponent: bool },
}

/// The possessive player holder starting at the cursor. Lexical lookahead
/// only; the shared resolvers prove the player.
fn related_holder_head(cursor: &OracleTokenCursor<'_>) -> Option<RelatedHolderHead> {
    let words = holder_words(cursor, 4);
    let relation_possessive =
        |word: &str| is_possessive_of(word, "controller") || is_possessive_of(word, "owner");
    if words[0] == "their" && zone_noun_at(cursor, 1) {
        return Some(RelatedHolderHead::Their);
    }
    if words[0] == "that"
        && (is_possessive_of(&words[1], "player") || is_possessive_of(&words[1], "opponent"))
    {
        return Some(RelatedHolderHead::ThatPlayer);
    }
    if words[0] == "defending" && is_possessive_of(&words[1], "player") {
        return Some(RelatedHolderHead::DefendingPlayer);
    }
    if words[0] == "target" {
        if is_possessive_of(&words[1], "player") {
            return Some(RelatedHolderHead::DeclaredTarget { opponent: false });
        }
        if is_possessive_of(&words[1], "opponent") {
            return Some(RelatedHolderHead::DeclaredTarget { opponent: true });
        }
    }
    if words[0] == "its" && relation_possessive(&words[1]) {
        return Some(RelatedHolderHead::ObjectRelation { relation: 1 });
    }
    if matches!(words[0].as_str(), "this" | "that" | "the")
        && (words[1].ends_with("'s") || words[1].ends_with("\u{2019}s"))
        && relation_possessive(&words[2])
    {
        return Some(RelatedHolderHead::ObjectRelation { relation: 2 });
    }
    None
}

/// The relation word a possessive relation holder restates.
fn relation_word(raw: &str) -> &'static str {
    if is_possessive_of(&raw.to_ascii_lowercase(), "owner") {
        "owner"
    } else {
        "controller"
    }
}

/// A related zone holder (`that player's hand`, `their graveyard`, `its
/// controller's graveyard`, `defending player's hand`, `target opponent's
/// hand`) resolved by the one binding environment the enclosing program
/// installed. The possessive restates the player reference its readers
/// already own: `that player` (`recognize_that_player`), `defending player`,
/// `its controller` / `that NOUN's owner` (`recognize_bound_player_reference`)
/// and the distributive `they` of an action's own player
/// (`BindingEnvironment::acting_player`). `target PLAYER's` declares a player
/// target of the action reading the query, as `target opponent controls`
/// does. Only an action of an effect program resolves such a holder: a
/// static or cost count reads `your` or every player's zone, and a holder
/// read elsewhere fails here rather than counting another player's cards.
/// `None` (nothing read) when no related holder starts at the cursor.
fn recognize_related_zone_holder(
    cursor: &mut OracleTokenCursor<'_>,
) -> Option<MachineResult<super::RelatedPlayer>> {
    let head = related_holder_head(cursor)?;
    Some(read_related_zone_holder(cursor, head))
}

fn read_related_zone_holder(
    cursor: &mut OracleTokenCursor<'_>,
    head: RelatedHolderHead,
) -> MachineResult<super::RelatedPlayer> {
    cursor.enter("query.zone.holder.related");
    let environment = super::installed_binding_environment(cursor);
    let Some(effect_index) = environment.effect_index else {
        return Err(cursor.fail(
            "an action of an effect program that resolves the zone's holder; outside one, only `your` or every player's zone is read",
        ));
    };
    let start = cursor.position();
    let reference = match head {
        RelatedHolderHead::DeclaredTarget { opponent } => {
            cursor.enter("query.zone.holder.declared-player");
            cursor.consume("target")?;
            cursor.advance_to(cursor.position() + 1)?;
            return Ok(super::RelatedPlayer::DeclaredTarget {
                binding: super::PlayerBinding(effect_index),
                opponent,
                site: start,
            });
        }
        RelatedHolderHead::Their => {
            cursor.enter("query.zone.holder.their");
            // The distributive `they` of the action being read names its
            // own player (`each player draws a card for each creature card
            // in their graveyard`); otherwise the pronoun names the player
            // the triggering event records when no player of the program is
            // a nearer antecedent (the selector's `they control` policy).
            let reference = if environment.acting_player {
                super::BoundPlayerReference::ActingPlayer
            } else if environment.player.is_none()
                && environment.player_target.is_none()
                && environment.frame.event_player
            {
                super::BoundPlayerReference::EventPlayer
            } else {
                return Err(cursor.fail(
                    "a player `their` names: the player of the action being read or the event's player",
                ));
            };
            cursor.consume("their")?;
            reference
        }
        RelatedHolderHead::ThatPlayer
        | RelatedHolderHead::DefendingPlayer
        | RelatedHolderHead::ObjectRelation { .. } => {
            let mut tokens = cursor.token_slice(0, usize::MAX).to_vec();
            let (offset, word) = match head {
                RelatedHolderHead::ObjectRelation { relation } => {
                    let raw = cursor.peek_ahead(relation).unwrap_or("").to_string();
                    (relation, relation_word(&raw))
                }
                _ => (1, "player"),
            };
            reclassify_token(&mut tokens, start + offset, word);
            let mut view = cursor.with_tokens(&tokens);
            let reference = match head {
                RelatedHolderHead::ThatPlayer => {
                    view.enter("query.zone.holder.that-player");
                    super::recognize_that_player(&mut view, &environment)?
                }
                RelatedHolderHead::DefendingPlayer => {
                    view.enter("query.zone.holder.defending-player");
                    super::recognize_defending_player(
                        &mut view,
                        environment.frame.defending_player,
                    )?
                }
                _ => {
                    view.enter("query.zone.holder.object-relation");
                    view.require(
                        super::object_related_player_head_length(&view) == Some(offset + 1),
                        "`its controller's`, `that NOUN's owner's` or `this NOUN's controller's`",
                    )?;
                    super::recognize_bound_player_reference(&mut view, &environment)?
                }
            };
            cursor.advance_to(view.position())?;
            reference
        }
    };
    cursor.enter("query.zone.holder.related.lowering");
    cursor.require(
        related_holder_lowerable(reference),
        "a holder the program resolves: a declared or event player, the action's own player, or the player of a bound object",
    )?;
    Ok(super::RelatedPlayer::Bound(reference))
}

/// The bound players a zone holder lowers (`effects::lower_zone_holder_player`):
/// an earlier player declaration of the program, the controller or owner of a
/// declared target, or any reference with a context-free value.
fn related_holder_lowerable(reference: super::BoundPlayerReference) -> bool {
    match reference {
        super::BoundPlayerReference::Bound(_)
        | super::BoundPlayerReference::ObjectRelation {
            object: super::PermanentEffectReference::Target(_),
            ..
        } => true,
        other => crate::oracle::canonical::effects::bound_player_reference_value(other, &[], &[])
            .is_some(),
    }
}

/// Which end of a measure a superlative player predicate selects (`the
/// most`, `the highest`; `the fewest`, `the lowest`, `the least`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerExtremum {
    Greatest,
    Least,
}

/// A relative predicate restricting a player population (CR 102.1-102.3):
/// the same player-state propositions the condition grammar reads
/// (`player_condition.rs`), here about each member in turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum PlayerRelativePredicate {
    /// `who controls N QUERY`, `who has N life`, `with exactly 13 life`,
    /// `who doesn't control QUERY` (`negated`).
    State {
        proposition: super::PlayerStateProposition,
        negated: bool,
    },
    /// `who controls the most|fewest QUERY`, `with the most|least life`,
    /// `with the highest|lowest life total`: every member whose measure
    /// is the extremum of the population, ties included.
    Superlative {
        measure: super::PlayerStateMeasure,
        extremum: PlayerExtremum,
    },
    /// `who lost life this turn`, `who was dealt combat damage this turn`:
    /// an event of this turn whose player is the member.
    History(MemberHistory),
    /// `who does`, `who doesn't`, `who didn't`, `who can't` (CR 608.2c):
    /// the member performed (`performed`) or did not perform the instruction
    /// of the immediately preceding population action of the program at
    /// `producer`, which every member of this population was given. The
    /// runtime records each member's measured performance of that action.
    Performance { producer: usize, performed: bool },
}

/// An event of this turn a population member took part in (CR 119.3 life
/// loss, CR 120.3 damage dealt to a player).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum MemberHistory {
    LostLife,
    DealtDamage { combat_only: bool },
}

/// A player population restricted by an exclusion and/or a relative
/// predicate (`each player other than its controller`, `each opponent who
/// doesn't control an Elf`, `each player who controls the most lands`).
/// The population, the excluded player and the predicate are independent
/// typed parts; the performer slot, a count or a recipient consume the same
/// node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct PlayerPopulationQueryNode {
    pub players: PlayerSetReferenceNode,
    /// `each other player`: the population minus the ability controller.
    pub excluding_controller: bool,
    /// `other than PLAYER`: one bound player removed from the population.
    pub except: Option<super::PlayerEffectSubject>,
    pub predicate: Option<PlayerRelativePredicate>,
    /// `the player who controls the most creatures`, `the player with the
    /// most life`: a definite description names one player. Only a sole
    /// member holding the extremum is that player; on a tie no player is
    /// (an `or tied` restatement would include the tied players and is not
    /// this description).
    pub unique: bool,
}

/// True when a relative restriction of a player population starts at the
/// cursor (`who`, `whose`, `with`, `other than`). Lexical lookahead only.
pub(in crate::oracle::canonical) fn player_relative_clause_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "who")
        || word_at(cursor, 0, "whose")
        || word_at(cursor, 0, "with")
        || (word_at(cursor, 0, "other") && word_at(cursor, 1, "than"))
}

fn word_at(cursor: &OracleTokenCursor<'_>, offset: usize, expected: &str) -> bool {
    cursor
        .peek_ahead(offset)
        .is_some_and(|raw| raw.eq_ignore_ascii_case(expected))
}

/// The bindings a population exclusion may name (`other than its
/// controller`, `other than that player`).
#[derive(Clone, Debug, Default)]
pub(in crate::oracle::canonical) struct PlayerRelativeContext {
    pub player_antecedent: Option<super::PlayerBinding>,
    pub event_player: bool,
    pub object_antecedent: Option<super::ObjectSubjectAntecedent>,
    /// The immediately preceding population action of the program, whose
    /// per-member performance `who does` / `who didn't` observes.
    pub performance_producer: Option<super::PopulationPerformanceProducer>,
}

/// Reads the restriction of `population` at the cursor: an optional
/// `other than PLAYER`, then an optional relative predicate (`who ...`,
/// `with ...`). A restriction this grammar does not type fails with its
/// own state (`player-set.relative.*`), never as an unrestricted population.
pub(in crate::oracle::canonical) fn recognize_player_population_restriction(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    context: &PlayerRelativeContext,
) -> MachineResult<PlayerPopulationQueryNode> {
    recognize_player_population_restriction_in_number(
        cursor,
        players,
        excluding_controller,
        context,
        false,
    )
}

/// The same restriction after a plural population noun (`opponents who
/// control an artifact`): the relative verb agrees in the plural.
pub(in crate::oracle::canonical) fn recognize_player_population_restriction_in_number(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    context: &PlayerRelativeContext,
    plural: bool,
) -> MachineResult<PlayerPopulationQueryNode> {
    let mut except = None;
    if word_at(cursor, 0, "other") && word_at(cursor, 1, "than") {
        cursor.enter("player-set.relative.exclusion");
        cursor.require(
            !excluding_controller,
            "one exclusion of a player population (`each other player` already excludes you)",
        )?;
        cursor.consume_phrase(&["other", "than"])?;
        except = Some(recognize_excluded_player(cursor, context)?);
    }
    let predicate =
        if word_at(cursor, 0, "who") || word_at(cursor, 0, "whose") || word_at(cursor, 0, "with") {
            Some(recognize_player_relative_predicate(
                cursor,
                PopulationRestriction {
                    players,
                    excluding_controller,
                    restricted: excluding_controller || except.is_some(),
                    plural,
                },
                context,
            )?)
        } else {
            None
        };
    cursor.enter("player-set.relative.complete");
    cursor.require(
        except.is_some() || predicate.is_some(),
        "an exclusion or a relative predicate of the player population",
    )?;
    Ok(PlayerPopulationQueryNode {
        players,
        excluding_controller,
        except,
        predicate,
        unique: false,
    })
}

/// True when `the player who|with ...` starts at the cursor: a definite
/// description of one player by a relative predicate. Lexical lookahead.
pub(in crate::oracle::canonical) fn definite_player_description_starts(
    cursor: &OracleTokenCursor<'_>,
) -> bool {
    word_at(cursor, 0, "the")
        && word_at(cursor, 1, "player")
        && (word_at(cursor, 2, "who") || word_at(cursor, 2, "with"))
}

/// `the player who controls the most QUERY`, `the player with the most
/// life`, `the player who has the highest life total`: the shared
/// superlative over every player (`recognize_player_population_restriction`)
/// read as one player (`unique`). A description by any other predicate
/// (`the player who lost life this turn`) does not single out one player
/// and fails explicitly; so does `or tied`, which names several players.
pub(in crate::oracle::canonical) fn recognize_definite_player_description(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PlayerRelativeContext,
) -> MachineResult<PlayerPopulationQueryNode> {
    cursor.enter("player-set.definite-description");
    cursor.consume_phrase(&["the", "player"])?;
    let mut query = recognize_player_population_restriction(
        cursor,
        PlayerSetReferenceNode::EachPlayer,
        false,
        context,
    )?;
    cursor.enter("player-set.definite-description.superlative");
    cursor.require(
        matches!(
            query.predicate,
            Some(PlayerRelativePredicate::Superlative { .. })
        ) && query.except.is_none(),
        "a superlative that singles out one player (`the player with the most life`)",
    )?;
    cursor.require(
        !(word_at(cursor, 0, "or") && word_at(cursor, 1, "tied")),
        "one player; `or tied` names every tied player and is not a definite description",
    )?;
    query.unique = true;
    Ok(query)
}

/// `other than its controller|owner`, `other than that player`: one player
/// the binding environment already names (CR 109.4, 108.3). A declared
/// target (`other than target player`) would open a declaration inside the
/// population and is refused explicitly.
fn recognize_excluded_player(
    cursor: &mut OracleTokenCursor<'_>,
    context: &PlayerRelativeContext,
) -> MachineResult<super::PlayerEffectSubject> {
    use super::source_sacrifice::atomic_action::player_subject::{
        player_anaphor_at, recognize_object_related_player_subject, recognize_player_effect_subject,
    };
    cursor.enter("player-set.relative.exclusion.player");
    if super::object_related_player_head_length(cursor).is_some() {
        return recognize_object_related_player_subject(cursor, context.object_antecedent.clone());
    }
    if player_anaphor_at(cursor).is_some() {
        return recognize_player_effect_subject(
            cursor,
            super::PlayerBinding(usize::MAX),
            context.player_antecedent,
            context.event_player,
        );
    }
    Err(cursor.fail(
        "a bound player after `other than` (`its controller`, `that player`); a declared target is not read here",
    ))
}

/// The population a relative predicate restricts, as read so far.
#[derive(Clone, Copy)]
struct PopulationRestriction {
    players: PlayerSetReferenceNode,
    excluding_controller: bool,
    /// An exclusion was read before the predicate.
    restricted: bool,
    /// The member noun is plural (`opponents who control`).
    plural: bool,
}

/// `who controls|has ...`, `who doesn't control ...`, `with ...`. The
/// proposition is the shared player-state grammar about the member; a
/// superlative compares the member with the whole population, so it is
/// read only for the unrestricted `each player` population.
fn recognize_player_relative_predicate(
    cursor: &mut OracleTokenCursor<'_>,
    population: PopulationRestriction,
    context: &PlayerRelativeContext,
) -> MachineResult<PlayerRelativePredicate> {
    let PopulationRestriction {
        players,
        restricted,
        plural,
        ..
    } = population;
    // The member is a third-person player: its zones are `their` zones.
    let member = super::ConditionPlayerSubject::Quantified {
        quantifier: super::PlayerQuantifier::Every,
        population: super::PlayerPopulation::Players,
    };
    if word_at(cursor, 0, "whose") {
        cursor.consume("whose")?;
        cursor.enter("player-set.relative.whose");
        return Err(cursor.fail(
            "a player predicate this grammar types; a possessive `whose` predicate is not read",
        ));
    }
    if word_at(cursor, 0, "with") {
        cursor.enter("player-set.relative.with");
        cursor.consume("with")?;
        if superlative_starts(cursor) {
            return recognize_superlative(
                cursor,
                players,
                restricted,
                SuperlativeFrame::Possession,
            );
        }
        cursor.enter("player-set.relative.with.proposition");
        return super::recognize_player_possession_proposition(cursor, member).map(|proposition| {
            PlayerRelativePredicate::State {
                proposition,
                negated: false,
            }
        });
    }
    cursor.enter("player-set.relative.who");
    cursor.consume("who")?;
    if let Some((elided, width)) = elliptical_performance_at(cursor, plural) {
        return recognize_performance_relative(cursor, population, context, elided, width);
    }
    let negated = if word_at(cursor, 0, "doesn't") || word_at(cursor, 0, "don't") {
        let raw = cursor.peek().expect("checked negation").to_string();
        cursor.consume(&raw)?;
        true
    } else if word_at(cursor, 0, "does") && word_at(cursor, 1, "not") {
        cursor.consume_phrase(&["does", "not"])?;
        true
    } else {
        false
    };
    if !negated && let Some(history) = recognize_member_history(cursor, plural)? {
        return Ok(PlayerRelativePredicate::History(history));
    }
    cursor.enter("player-set.relative.who.verb");
    // `who doesn't control`, `opponents who control`: the base form after
    // a negation or a plural member; the third person otherwise.
    let (controls, has) = if negated || plural {
        ("control", "have")
    } else {
        ("controls", "has")
    };
    let possession = word_at(cursor, 0, has);
    if !possession && !word_at(cursor, 0, controls) {
        let found = cursor.peek().unwrap_or("").to_string();
        return Err(cursor.fail(&format!(
            "a player predicate this grammar types (`who controls`, `who has`, `who doesn't control`); `who {found}` is not read"
        )));
    }
    let raw = cursor.peek().expect("checked verb").to_string();
    cursor.consume(&raw)?;
    if possession {
        if !negated && superlative_starts(cursor) {
            return recognize_superlative(
                cursor,
                players,
                restricted,
                SuperlativeFrame::Possession,
            );
        }
        cursor.enter("player-set.relative.who.has");
        return super::recognize_player_possession_proposition(cursor, member).map(|proposition| {
            PlayerRelativePredicate::State {
                proposition,
                negated,
            }
        });
    }
    if !negated && superlative_starts(cursor) {
        return recognize_superlative(cursor, players, restricted, SuperlativeFrame::Controls);
    }
    cursor.enter("player-set.relative.who.controls");
    super::recognize_controlled_permanents_proposition(cursor).map(|proposition| {
        PlayerRelativePredicate::State {
            proposition,
            negated,
        }
    })
}

/// The elided outcome of a relative whose verb phrase restates the
/// preceding instruction by ellipsis (CR 608.2c).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ElidedPerformance {
    /// `who does`, `who did`.
    Did,
    /// `who doesn't`, `who didn't`, `who does not`, `who did not`.
    DidNot,
    /// `who can't`, `who cannot`: the member could not perform a mandatory
    /// instruction.
    CouldNot,
}

/// `does|did|doesn't|didn't|does not|did not|can't|cannot` after `who`,
/// followed directly by the finite verb (or modal) of the main clause: the
/// relative's verb phrase is elided (`each player who does draws seven
/// cards`, `each opponent who can't loses 3 life`). A verb phrase written
/// out (`who doesn't control an Elf`, `who didn't discard a card this way`)
/// is not an ellipsis and stays with the proposition grammar. Only a
/// singular member is read: after a plural noun the main verb's base form
/// cannot be told from a written-out relative verb. Lexical lookahead.
fn elliptical_performance_at(
    cursor: &OracleTokenCursor<'_>,
    plural: bool,
) -> Option<(ElidedPerformance, usize)> {
    if plural {
        return None;
    }
    let words = holder_words(cursor, 2);
    let (performance, width) = match (
        words[0].replace('\u{2019}', "'").as_str(),
        words[1].as_str(),
    ) {
        ("does" | "did", "not") => (ElidedPerformance::DidNot, 2),
        ("does" | "did", _) => (ElidedPerformance::Did, 1),
        ("doesn't" | "didn't", _) => (ElidedPerformance::DidNot, 1),
        ("can't" | "cannot", _) => (ElidedPerformance::CouldNot, 1),
        _ => return None,
    };
    let main = cursor.peek_ahead(width)?;
    let finite = super::lookup_auxiliary(main).is_some()
        || super::lookup_verb(main, &super::ALL_LEMMAS).is_some_and(|(_, forms)| {
            forms.contains(&super::VerbForm::ThirdPersonSingular)
                && !forms.contains(&super::VerbForm::Base)
        });
    finite.then_some((performance, width))
}

/// The elided relative of a population whose members were all given the
/// immediately preceding population action of the program (the context's
/// performance producer, CR 608.2c): `who does` keeps the members who
/// performed its instruction, `who doesn't` / `who didn't` those who did
/// not, and `who can't` those who could not perform a mandatory one. The
/// performance is the engine's measured one (an instruction partly done is
/// not performed, CR 609.3). Without such a producer the ellipsis restates
/// nothing and fails here; a member outside the producer's population was
/// never given the instruction and is refused too.
fn recognize_performance_relative(
    cursor: &mut OracleTokenCursor<'_>,
    population: PopulationRestriction,
    context: &PlayerRelativeContext,
    elided: ElidedPerformance,
    width: usize,
) -> MachineResult<PlayerRelativePredicate> {
    cursor.enter("player-set.relative.who.performance");
    cursor.advance_to(cursor.position() + width)?;
    cursor.enter("player-set.relative.who.performance.producer");
    let producer = cursor.require_value(
        context.performance_producer,
        "an immediately preceding instruction of each player of a population that `who does` / `who didn't` restates",
    )?;
    cursor.require(
        super::population_within_producer(
            &producer,
            population.players,
            population.excluding_controller,
        ),
        "a population whose every member was given the preceding instruction",
    )?;
    cursor.enter("player-set.relative.who.performance.permission");
    cursor.require(
        !(elided == ElidedPerformance::CouldNot && producer.optional),
        "`who can't` after a mandatory instruction; a permission's outcome is `who doesn't`",
    )?;
    Ok(PlayerRelativePredicate::Performance {
        producer: producer.effect_index,
        performed: elided == ElidedPerformance::Did,
    })
}

/// `lost life this turn`, `was|were dealt [combat] damage this turn` after
/// `who`: the member's events of this turn. `None` (nothing consumed) for
/// another verb, which the proposition grammar reads.
fn recognize_member_history(
    cursor: &mut OracleTokenCursor<'_>,
    plural: bool,
) -> MachineResult<Option<MemberHistory>> {
    let history = if word_at(cursor, 0, "lost") && word_at(cursor, 1, "life") {
        cursor.enter("player-set.relative.who.lost-life");
        cursor.consume_phrase(&["lost", "life"])?;
        MemberHistory::LostLife
    } else if (word_at(cursor, 0, "was") || word_at(cursor, 0, "were"))
        && word_at(cursor, 1, "dealt")
    {
        cursor.enter("player-set.relative.who.dealt-damage");
        cursor.require(
            word_at(cursor, 0, if plural { "were" } else { "was" }),
            "`was` after a singular member, `were` after a plural one",
        )?;
        let copula = cursor.peek().expect("checked copula").to_string();
        cursor.consume(&copula)?;
        cursor.consume("dealt")?;
        let combat_only = word_at(cursor, 0, "combat");
        if combat_only {
            cursor.consume("combat")?;
        }
        cursor.consume("damage")?;
        MemberHistory::DealtDamage { combat_only }
    } else {
        return Ok(None);
    };
    // CR 500: the events of this turn; another window is not read here.
    cursor.enter("player-set.relative.who.history-window");
    cursor.consume_phrase(&["this", "turn"])?;
    Ok(Some(history))
}

fn superlative_starts(cursor: &OracleTokenCursor<'_>) -> bool {
    word_at(cursor, 0, "the")
        && ["most", "fewest", "least", "highest", "lowest", "greatest"]
            .iter()
            .any(|extremum| word_at(cursor, 1, extremum))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SuperlativeFrame {
    /// `controls the most QUERY`.
    Controls,
    /// `has|with the most life`, `the highest life total`.
    Possession,
}

/// `the most|fewest QUERY` after `controls`; `the most|least life`, `the
/// highest|lowest life total` after `has`/`with` (compared with every
/// player of the game, ties included).
fn recognize_superlative(
    cursor: &mut OracleTokenCursor<'_>,
    players: PlayerSetReferenceNode,
    restricted: bool,
    frame: SuperlativeFrame,
) -> MachineResult<PlayerRelativePredicate> {
    cursor.enter("player-set.relative.superlative");
    cursor.require(
        players == PlayerSetReferenceNode::EachPlayer && !restricted,
        "a superlative of every player (`each player who controls the most ...`)",
    )?;
    cursor.consume("the")?;
    let written = cursor
        .peek()
        .map(str::to_ascii_lowercase)
        .ok_or_else(|| cursor.fail("`most`, `fewest`, `highest` or `lowest`"))?;
    let extremum = match written.as_str() {
        "most" | "highest" | "greatest" => PlayerExtremum::Greatest,
        "fewest" | "least" | "lowest" => PlayerExtremum::Least,
        _ => return Err(cursor.fail("`most`, `fewest`, `highest` or `lowest`")),
    };
    cursor.advance_to(cursor.position() + 1)?;
    let measure = match frame {
        SuperlativeFrame::Controls => {
            cursor.enter("player-set.relative.superlative.controls");
            cursor.require(
                written == "most" || written == "fewest",
                "`the most` or `the fewest` before a controlled-permanent query",
            )?;
            let query = super::recognize_object_query(cursor)?;
            cursor.require(
                query.controller.is_none(),
                "a permanent query without its own controller relation",
            )?;
            cursor.require(
                query
                    .predicate
                    .supports_domain(super::ObjectKind::Permanent),
                "a predicate valid for a battlefield permanent",
            )?;
            super::PlayerStateMeasure::ControlledPermanents {
                predicate: query.predicate,
                exclude_source: false,
            }
        }
        // `the most cards in hand`: the comparative possession measure the
        // `more cards in hand than each other player` condition reads.
        SuperlativeFrame::Possession if word_at(cursor, 0, "cards") => {
            cursor.enter("player-set.relative.superlative.cards");
            cursor.require(
                matches!(written.as_str(), "most" | "fewest"),
                "`the most|fewest cards in ZONE`",
            )?;
            super::recognize_comparative_possession(cursor)?
        }
        SuperlativeFrame::Possession => {
            cursor.enter("player-set.relative.superlative.life");
            cursor.consume("life")?;
            if matches!(written.as_str(), "highest" | "lowest" | "greatest") {
                cursor.consume("total")?;
            } else {
                cursor.require(
                    !word_at(cursor, 0, "total"),
                    "`the most life` or `the highest life total`",
                )?;
            }
            super::PlayerStateMeasure::Life
        }
    };
    // `or tied for the most ...` restates the ties the extremum includes;
    // it is not read here and fails at the parent's verb.
    cursor.enter("player-set.relative.superlative.complete");
    Ok(PlayerRelativePredicate::Superlative { measure, extremum })
}

/// True when the action's player slot names a player other than the
/// ability's controller, so a distributive `they` in its trailing operands
/// (`... for each creature they control`) names that player: a performed
/// action (its performer), each member of a player set, a declared or bound
/// player, or another player reference.
pub(in crate::oracle::canonical) fn names_acting_player(effect: &super::LocalEffectNode) -> bool {
    match effect {
        super::LocalEffectNode::Performed(_)
        | super::LocalEffectNode::QuantifiedPlayerAction(_)
        | super::LocalEffectNode::TargetPlayerLife(_) => true,
        super::LocalEffectNode::PlayerAction(action) => {
            action.player != super::PlayerEffectSubject::Controller
        }
        super::LocalEffectNode::GainLife(life) => {
            life.player != super::LocalPlayerReference::Controller
        }
        super::LocalEffectNode::LoseLife(life) => {
            life.player != super::LocalPlayerReference::Controller
        }
        _ => false,
    }
}

/// Reads an action's operands with its acting player installed (the shared
/// binding-environment slot, `player_subject::with_acting_player`).
pub(in crate::oracle::canonical) fn read_with_acting_player<T>(
    cursor: &mut OracleTokenCursor<'_>,
    read: impl FnOnce(&mut OracleTokenCursor<'_>) -> T,
) -> T {
    super::source_sacrifice::atomic_action::player_subject::with_acting_player(cursor, read)
}
