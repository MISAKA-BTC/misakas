//! Latest authoritative registrations, rather than a history of control commands.
//! The wake channel carries no payload; coalescing a wake never loses this state.
use std::{
    collections::{HashMap, hash_map::Entry},
    hash::Hash,
    sync::Arc,
};

#[derive(Clone, Debug)]
pub(super) struct RegistrationToken(Arc<()>);
impl RegistrationToken {
    fn new() -> Self {
        Self(Arc::new(()))
    }
    fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[derive(Debug)]
pub(super) struct RegistrationState<K, V> {
    entries: HashMap<K, (RegistrationToken, V)>,
}
impl<K, V> Default for RegistrationState<K, V> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}
impl<K: Eq + Hash, V> RegistrationState<K, V> {
    pub(super) fn insert(&mut self, key: K, value: V) {
        match self.entries.entry(key) {
            Entry::Occupied(mut entry) => entry.get_mut().1 = value,
            Entry::Vacant(entry) => {
                entry.insert((RegistrationToken::new(), value));
            }
        }
    }
    pub(super) fn remove(&mut self, key: &K) {
        self.entries.remove(key);
    }
    pub(super) fn matches(&self, key: &K, token: &RegistrationToken) -> bool {
        self.entries.get(key).is_some_and(|(current, _)| current.same(token))
    }
    pub(super) fn remove_if_current(&mut self, key: &K, token: &RegistrationToken) {
        if self.matches(key, token) {
            self.entries.remove(key);
        }
    }
    pub(super) fn clear(&mut self) {
        self.entries.clear();
    }
}
impl<K: Clone, V: Clone> RegistrationState<K, V> {
    pub(super) fn snapshot(&self) -> Vec<(K, RegistrationToken, V)> {
        self.entries.iter().map(|(key, (token, value))| (key.clone(), token.clone(), value.clone())).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn churn_retains_current_registrations_without_command_history_or_tombstones() {
        let mut state = RegistrationState::default();
        for id in 0..100_000u64 {
            state.insert((1, id), vec![id]);
            state.remove(&(1, id));
        }
        assert!(state.snapshot().is_empty());
        assert!(state.entries.capacity() <= 3, "churn does not accumulate keys/tombstones");
        for version in 0..100_000u64 {
            state.insert((1, 7), vec![version]);
        }
        assert_eq!(state.snapshot().into_iter().map(|(key, _, value)| (key, value)).collect::<Vec<_>>(), vec![((1, 7), vec![99_999])]);
    }
    #[test]
    fn last_full_state_replaces_scope_and_unregister_cannot_resurrect_it() {
        let mut state = RegistrationState::default();
        state.insert((1, 7), vec![1, 2]);
        let in_flight = state.snapshot();
        state.insert((1, 7), vec![3]);
        state.insert((2, 7), vec![4]);
        state.remove(&(1, 7));
        assert_eq!(in_flight[0].0, (1, 7));
        assert_eq!(in_flight[0].2, vec![1, 2]);
        assert_eq!(state.snapshot()[0].0, (2, 7));
        assert_eq!(state.snapshot()[0].2, vec![4]);
        state.clear();
        assert!(state.snapshot().is_empty());
    }
    #[test]
    fn stopped_generation_cannot_send_to_or_purge_a_restarted_listener() {
        let mut state = RegistrationState::default();
        state.insert((1, 7), "old connection");
        let old = state.snapshot()[0].1.clone();
        state.insert((1, 7), "updated scope, same connection");
        assert!(state.matches(&(1, 7), &old), "scope updates preserve an in-flight lifetime");
        state.remove(&(1, 7));
        assert!(!state.matches(&(1, 7), &old));
        state.insert((1, 7), "new connection");
        let new = state.snapshot()[0].1.clone();
        assert!(!state.matches(&(1, 7), &old), "stop/start creates a distinct generation");
        state.remove_if_current(&(1, 7), &old); // old awaited send returned closed
        assert!(state.matches(&(1, 7), &new), "stale send completion cannot remove the new generation");
        state.remove_if_current(&(1, 7), &new);
        assert!(state.snapshot().is_empty());
    }
}
