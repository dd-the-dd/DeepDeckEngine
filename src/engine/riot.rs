use super::{
    ActionKind, CardInstance, DecisionKind, DecisionProvider, EngineDecisionRequest, EngineError,
    GameEngine, LegalAction, Value, json, value_kind,
};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RiotEntryChoice {
    Counter,
    Haste,
}

impl RiotEntryChoice {
    const OPTIONS: [(&'static str, Self); 2] =
        [("+1/+1 counter", Self::Counter), ("haste", Self::Haste)];

    fn from_value(value: Option<&Value>) -> Option<Self> {
        let value = value?.as_str()?;
        Self::OPTIONS
            .iter()
            .find_map(|(label, choice)| (*label == value).then_some(*choice))
    }
}

impl GameEngine {
    pub(super) fn apply_riot_entry_choice(
        &self,
        card: &mut CardInstance,
        value: Option<&Value>,
    ) -> Result<(), EngineError> {
        match RiotEntryChoice::from_value(value) {
            Some(RiotEntryChoice::Counter) => {
                let count = self.adjusted_counter_placement(card, "+1/+1", 1);
                if count > 0 {
                    *card.counters.entry("+1/+1".to_string()).or_default() += count;
                }
                Ok(())
            }
            Some(RiotEntryChoice::Haste) => {
                card.definition.rules.push(json!({
                    "kind": "keywordAbility",
                    "source": { "kind": "self" },
                    "ability": { "kind": "haste" },
                }));
                Ok(())
            }
            None => Err(EngineError::new("riot choice is missing or invalid")),
        }
    }

    /// Applies one independent, typed replacement choice for every Riot
    /// instance granted to the entering object by active continuous effects.
    pub(super) fn apply_granted_riot_entry_choices<P: DecisionProvider>(
        &mut self,
        card: &mut CardInstance,
        provider: &mut P,
    ) -> Result<(), EngineError> {
        let sources = self
            .battlefield_static_modifiers()
            .into_iter()
            .filter(|(source, modifier)| {
                let grants_riot = value_kind(modifier) == Some("grantKeyword")
                    && modifier["keyword"].as_str() == Some("riot")
                    // Temporary compatibility for the superseded composite IR.
                    || value_kind(modifier) == Some("grantRiot");
                grants_riot
                    && value_kind(&modifier["objects"]) == Some("permanents")
                    && Self::static_selector_matches(source, &modifier["objects"], card)
            })
            .map(|(source, _)| source.instance_id)
            .collect::<Vec<_>>();

        for (instance, source_id) in sources.into_iter().enumerate() {
            let decision_id = format!("grantedRiotChoice:{source_id}:{instance}");
            let options = RiotEntryChoice::OPTIONS
                .iter()
                .map(|(label, _)| LegalAction {
                    id: format!("choose:{decision_id}:{}", label.replace(' ', "-")),
                    kind: ActionKind::ChooseResolution,
                    player_id: card.controller.clone(),
                    label: format!("Choose {label}"),
                    card_instance_id: Some(card.instance_id.clone()),
                    payment_sources: Vec::new(),
                    decisions: BTreeMap::from([(
                        decision_id.clone(),
                        Value::String((*label).to_string()),
                    )]),
                    targets: BTreeMap::new(),
                    target_order: Vec::new(),
                    attacker_id: None,
                    blocker_id: None,
                })
                .collect::<Vec<_>>();
            let choice = self.choose_action(
                provider,
                EngineDecisionRequest {
                    id: format!("replacement:{}:{decision_id}", card.instance_id),
                    kind: DecisionKind::ReplacementChoice,
                    player_id: card.controller.clone(),
                    source_card: Some(card.clone()),
                    source_card_instance_id: Some(card.instance_id.clone()),
                    choice: None,
                    options,
                },
            )?;
            self.apply_riot_entry_choice(card, choice.decisions.get(&decision_id))?;
        }
        Ok(())
    }
}
