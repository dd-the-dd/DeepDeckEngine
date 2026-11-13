use super::*;

fn action(id: &str, kind: ActionKind) -> LegalAction {
    LegalAction {
        id: id.to_string(),
        kind,
        player_id: "player-1".to_string(),
        label: id.to_string(),
        card_instance_id: None,
        payment_sources: Vec::new(),
        decisions: BTreeMap::new(),
        targets: BTreeMap::new(),
        target_order: Vec::new(),
        attacker_id: None,
        blocker_id: None,
    }
}

#[test]
fn agent_facing_decisions_exclude_ui_only_combat_revisions() {
    let request = EngineDecisionRequest {
        id: "attackers:1".to_string(),
        kind: DecisionKind::Attackers,
        player_id: "player-1".to_string(),
        source_card: None,
        source_card_instance_id: None,
        choice: None,
        options: vec![
            action("attack", ActionKind::DeclareAttacker),
            action("cancel", ActionKind::CancelAttacker),
            action("finish", ActionKind::FinishAttackers),
        ],
    };

    let agent_request = request.agent_facing();

    assert_eq!(request.options.len(), 3);
    assert_eq!(agent_request.options.len(), 2);
    assert!(
        agent_request
            .options
            .iter()
            .all(|action| action.kind != ActionKind::CancelAttacker)
    );
}
