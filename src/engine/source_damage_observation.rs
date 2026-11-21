//! Dealt-damage records (CR 120.1, 120.2, 603.2c). Every damage path
//! (combat, a single recipient, a player batch) publishes each recipient's
//! damage as one record of the damage family of the event model
//! (`damage_events.rs`), matched by its one matcher.
use super::damage_events::{DamageEventRecord, DamageRecipientRecord};
use super::*;

/// The recipient of one dealt-damage event.
pub(super) enum DamageRecipientRef<'a> {
    Player(&'a str),
    Permanent(&'a CardInstance),
}

impl GameEngine {
    /// Publishes one dealt-damage record to every matching observer.
    pub(super) fn enqueue_source_damage_observers(
        &mut self,
        damage_source_id: &str,
        source_controller: Option<&str>,
        recipient: DamageRecipientRef<'_>,
        amount: i32,
        combat: bool,
    ) {
        let record = DamageEventRecord {
            source: damage_source_id.to_string(),
            source_controller: source_controller.map(ToOwned::to_owned),
            recipient: match recipient {
                DamageRecipientRef::Player(player_id) => {
                    DamageRecipientRecord::Player(player_id.to_string())
                }
                DamageRecipientRef::Permanent(permanent) => {
                    DamageRecipientRecord::Permanent(permanent.instance_id.clone())
                }
            },
            amount,
            combat,
            batch: self.current_damage_batch(),
        };
        self.publish_damage_source_observers(&record);
    }
}
