use super::super::{MachineResult, OracleTokenCursor};
use super::{
    LocalPlayerReference, ObjectPredicate, recognize_player_reference,
    recognize_search_card_selector,
};
use crate::oracle::canonical::automata::{VerbForm, VerbLemma, is_verb_form};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardDeploymentSourceZone {
    Hand,
    Graveyard,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) enum CardDeploymentDestination {
    Battlefield,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::oracle::canonical) struct OptionalCardDeploymentEffectNode {
    pub player: LocalPlayerReference,
    pub filter: ObjectPredicate,
    pub source: CardDeploymentSourceZone,
    pub destination: CardDeploymentDestination,
    pub tapped: bool,
    /// `tapped and attacking [DEFENDER]` (CR 508.4).
    pub attacking: Option<super::EnteringAttackNode>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OptionalCardDeploymentState {
    Player,
    Modality,
    Action,
    Article,
    Filter,
    SourceRelation,
    SourcePossessive,
    SourceZone,
    DestinationRelation,
    DestinationArticle,
    DestinationZone,
    EntryState,
    Complete,
}

#[derive(Default)]
struct OptionalCardDeploymentFrame {
    player: Option<LocalPlayerReference>,
    filter: Option<ObjectPredicate>,
    source: Option<CardDeploymentSourceZone>,
    destination: Option<CardDeploymentDestination>,
    tapped: bool,
    attacking: Option<super::EnteringAttackNode>,
}

/// Recognizes an optional, selection-backed card deployment such as
/// `you may put a land card from your hand onto the battlefield tapped`.
///
/// The surrounding effect-sequence automaton owns sentence punctuation and
/// ordering. This child owns modality, the reusable card selector, source and
/// destination zones, and the optional entering state.
pub(in crate::oracle::canonical) fn recognize_optional_card_deployment_effect(
    cursor: &mut OracleTokenCursor<'_>,
) -> MachineResult<OptionalCardDeploymentEffectNode> {
    recognize_optional_card_deployment_effect_with_context(
        cursor,
        &super::EffectRecognitionContext::default(),
    )
}

pub(in crate::oracle::canonical) fn recognize_optional_card_deployment_effect_with_context(
    cursor: &mut OracleTokenCursor<'_>,
    context: &super::EffectRecognitionContext,
) -> MachineResult<OptionalCardDeploymentEffectNode> {
    let mut state = OptionalCardDeploymentState::Player;
    let mut frame = OptionalCardDeploymentFrame::default();
    loop {
        state = match state {
            OptionalCardDeploymentState::Player => {
                cursor.enter("effect.optional-card-deployment.player");
                frame.player = Some(recognize_player_reference(cursor, None)?);
                cursor.require(
                    frame.player == Some(LocalPlayerReference::Controller),
                    "the source controller as the deploying player",
                )?;
                OptionalCardDeploymentState::Modality
            }
            OptionalCardDeploymentState::Modality => {
                cursor.enter("effect.optional-card-deployment.modality");
                let modality = cursor.consume_type("modalityWord")?;
                cursor.require(
                    modality.raw.eq_ignore_ascii_case("may"),
                    "the optional `may` modality",
                )?;
                OptionalCardDeploymentState::Action
            }
            OptionalCardDeploymentState::Action => {
                cursor.enter("effect.optional-card-deployment.action");
                let action = cursor.consume_type("actionWord")?;
                cursor.require(
                    is_verb_form(&action.raw, VerbLemma::Put, VerbForm::Base),
                    "the `put` zone-change action",
                )?;
                OptionalCardDeploymentState::Article
            }
            OptionalCardDeploymentState::Article => {
                cursor.enter("effect.optional-card-deployment.article");
                let article = cursor
                    .peek()
                    .filter(|raw| raw.eq_ignore_ascii_case("a") || raw.eq_ignore_ascii_case("an"))
                    .map(ToOwned::to_owned)
                    .ok_or_else(|| cursor.fail("`a` or `an` before the card selector"))?;
                cursor.consume(&article)?;
                OptionalCardDeploymentState::Filter
            }
            OptionalCardDeploymentState::Filter => {
                cursor.enter("effect.optional-card-deployment.filter");
                frame.filter = Some(
                    match cursor.attempt(|trial| recognize_search_card_selector(trial, false)) {
                        Ok(predicate) => predicate,
                        Err(_) => {
                            let query = super::recognize_object_noun_query(
                            cursor,
                            super::ObjectQueryContext::resolution(
                                context.activation_source_availability
                                    == super::ExplicitActivationSourceAvailability::RetainedBattlefieldPermanent,
                            ),
                        )?;
                            // A resolution-owned card choice: the noun phrase is
                            // read once; this parent type-checks its domain and
                            // that no player relation scopes the source zone.
                            cursor.enter("selector.resolution-card.domain");
                            cursor.require(
                                query.predicate().is_in_domain(super::ObjectKind::Card),
                                "an explicit card-domain query",
                            )?;
                            cursor.require(
                                query.relations().is_empty(),
                                "an unscoped card predicate in the source zone",
                            )?;
                            query.predicate().clone()
                        }
                    },
                );
                OptionalCardDeploymentState::SourceRelation
            }
            OptionalCardDeploymentState::SourceRelation => {
                cursor.enter("effect.optional-card-deployment.source-relation");
                cursor.consume("from")?;
                OptionalCardDeploymentState::SourcePossessive
            }
            OptionalCardDeploymentState::SourcePossessive => {
                cursor.enter("effect.optional-card-deployment.source-possessive");
                cursor.consume("your")?;
                OptionalCardDeploymentState::SourceZone
            }
            OptionalCardDeploymentState::SourceZone => {
                cursor.enter("effect.optional-card-deployment.source-zone");
                let zone = cursor.consume_type("zoneWord")?;
                frame.source = Some(if zone.raw.eq_ignore_ascii_case("hand") {
                    CardDeploymentSourceZone::Hand
                } else if zone.raw.eq_ignore_ascii_case("graveyard") {
                    CardDeploymentSourceZone::Graveyard
                } else {
                    return Err(cursor.fail("the controller's `hand` or `graveyard`"));
                });
                OptionalCardDeploymentState::DestinationRelation
            }
            OptionalCardDeploymentState::DestinationRelation => {
                cursor.enter("effect.optional-card-deployment.destination-relation");
                cursor.consume("onto")?;
                OptionalCardDeploymentState::DestinationArticle
            }
            OptionalCardDeploymentState::DestinationArticle => {
                cursor.enter("effect.optional-card-deployment.destination-article");
                cursor.consume("the")?;
                OptionalCardDeploymentState::DestinationZone
            }
            OptionalCardDeploymentState::DestinationZone => {
                cursor.enter("effect.optional-card-deployment.destination-zone");
                cursor.consume("battlefield")?;
                frame.destination = Some(CardDeploymentDestination::Battlefield);
                OptionalCardDeploymentState::EntryState
            }
            OptionalCardDeploymentState::EntryState => {
                cursor.enter("effect.optional-card-deployment.entry-state");
                if cursor
                    .peek()
                    .is_some_and(|raw| raw.eq_ignore_ascii_case("tapped"))
                {
                    cursor.consume("tapped")?;
                    frame.tapped = true;
                    // `tapped and attacking`: the shared entering-attack reader.
                    if cursor
                        .peek()
                        .is_some_and(|raw| raw.eq_ignore_ascii_case("and"))
                        && cursor
                            .peek_ahead(1)
                            .is_some_and(|raw| raw.eq_ignore_ascii_case("attacking"))
                    {
                        cursor.consume("and")?;
                        frame.attacking = Some(super::recognize_entering_attack(cursor)?);
                    }
                }
                OptionalCardDeploymentState::Complete
            }
            OptionalCardDeploymentState::Complete => {
                cursor.enter("effect.optional-card-deployment.complete");
                return Ok(OptionalCardDeploymentEffectNode {
                    player: cursor.require_value(frame.player, "a deploying player")?,
                    filter: cursor.require_value(frame.filter, "a card filter")?,
                    source: cursor.require_value(frame.source, "a source card zone")?,
                    destination: cursor
                        .require_value(frame.destination, "a deployment destination")?,
                    tapped: frame.tapped,
                    attacking: frame.attacking,
                });
            }
        };
    }
}
