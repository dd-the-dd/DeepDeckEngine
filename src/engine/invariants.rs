use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub(super) struct LoopResourceSnapshot {
    pub(super) context_fingerprint: u64,
    pub(super) resources: BTreeMap<String, i64>,
}

impl LoopResourceSnapshot {
    pub(super) fn dominates(&self, earlier: &Self) -> bool {
        self.context_fingerprint == earlier.context_fingerprint
            && earlier
                .resources
                .keys()
                .chain(self.resources.keys())
                .all(|resource| {
                    self.resources.get(resource).copied().unwrap_or(0)
                        >= earlier.resources.get(resource).copied().unwrap_or(0)
                })
    }
}
