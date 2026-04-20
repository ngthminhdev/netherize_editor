use std::collections::HashMap;

use crate::async_runtime::message::RequestTopic;

/// RevisionState giữ generation/revision hiện tại theo từng task domain.
/// Dùng để reconcile result trả về từ worker với state mới nhất ở UI thread.
#[derive(Debug, Default, Clone)]
pub struct RevisionState {
    latest_by_topic: HashMap<RequestTopic, u64>,
}

impl RevisionState {
    pub fn current(&self, topic: RequestTopic) -> u64 {
        self.latest_by_topic.get(&topic).copied().unwrap_or(0)
    }

    pub fn bump(&mut self, topic: RequestTopic) -> u64 {
        let entry = self.latest_by_topic.entry(topic).or_insert(0);
        *entry += 1;
        *entry
    }

    pub fn observe(&mut self, topic: RequestTopic, revision: u64) {
        let entry = self.latest_by_topic.entry(topic).or_insert(0);
        if revision > *entry {
            *entry = revision;
        }
    }

    pub fn snapshot(&self) -> HashMap<RequestTopic, u64> {
        self.latest_by_topic.clone()
    }
}
