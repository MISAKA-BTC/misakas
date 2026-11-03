extern crate derive_more;
use crate::{
    connection::Connection,
    error::Result,
    events::{EventArray, EventType},
    listener::ListenerId,
    notification::Notification,
    subscription::{BroadcastingSingle, DynSubscription, context::SubscriptionContext},
};
use async_channel::{Receiver, Sender};
use core::fmt::Debug;
use derive_more::Deref;
use futures::{future::FutureExt, select_biased};
use indexmap::IndexMap;
use kaspa_core::{debug, trace};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use workflow_core::channel::Channel;

#[path = "registration_state.rs"]
mod registration_state;
use registration_state::{RegistrationState, RegistrationToken};

type ConnectionSet<T> = HashMap<ListenerId, T>;

/// Broadcasting plan structured by subscription, encoding and connection
#[derive(Deref)]
struct Plan<C: Connection>(IndexMap<DynSubscription, HashMap<C::Encoding, ConnectionSet<(RegistrationToken, C)>>>);

impl<C> Plan<C>
where
    C: Connection,
{
    fn insert_snapshot_entry(&mut self, subscription: DynSubscription, id: ListenerId, token: RegistrationToken, connection: C) {
        // The authoritative snapshot has one entry per (event, listener), so
        // rebuilding a plan does not need a linear scan to remove duplicates.
        let encoding = connection.encoding();
        self.0.entry(subscription.clone()).or_default().entry(encoding).or_default().entry(id).or_insert_with(|| {
            #[cfg(test)]
            trace!("Broadcasting plan: insert listener {} with {:?}", id, subscription);
            (token, connection)
        });
    }

    fn remove(&mut self, id: &ListenerId) -> Option<(RegistrationToken, C)> {
        let mut result = None;
        let mut found_subscription: Option<DynSubscription> = None;
        let mut found_encoding: Option<C::Encoding> = None;
        'outer: for (subscription, encoding_set) in self.0.iter_mut() {
            for (encoding, connection_set) in encoding_set.iter_mut() {
                if let Some(connection) = connection_set.remove(id) {
                    #[cfg(test)]
                    trace!("Broadcasting plan: removed listener {}", id);
                    result = Some(connection);
                    if connection_set.is_empty() {
                        found_encoding = Some(encoding.clone());
                        found_subscription = Some((*subscription).clone());
                        //  The plan is guaranteed to contain no duplicate occurrence of every id
                        break 'outer;
                    }
                }
            }
        }
        // Cleaning empty entries
        if let Some(ref subscription) = found_subscription
            && let Some(ref encoding) = found_encoding
        {
            self.0.get_mut(subscription).unwrap().remove(encoding);
            if self.0.get(subscription).unwrap().is_empty() {
                self.0.swap_remove(subscription);
            }
        }
        result
    }

    // fn len(&self) -> usize {
    //     self.0.values().map(|encodings| encodings.values().map(|connections| connections.len()).count()).count()
    // }
}

impl<C: Connection> Default for Plan<C> {
    fn default() -> Self {
        Self(Default::default())
    }
}

#[derive(Debug)]
pub(crate) struct Broadcaster<N, C>
where
    N: Notification,
    C: Connection,
{
    name: &'static str,
    index: usize,
    context: SubscriptionContext,
    started: Arc<AtomicBool>,
    // Bounded wakeups carry no command history. The complete desired state is
    // authoritative even while a slow downstream send prevents this worker
    // from polling control. Memory follows active registrations, not churn.
    ctl: Channel<()>,
    registrations: Mutex<RegistrationState<(EventType, ListenerId), (DynSubscription, C)>>,
    incoming: Receiver<N>,
    shutdown: Channel<()>,
    /// Sync channel, for handling of messages in predictable sequence; exclusively intended for tests.
    _sync: Option<Sender<()>>,
}

impl<N, C> Broadcaster<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    pub fn new(
        name: &'static str,
        index: usize,
        context: SubscriptionContext,
        incoming: Receiver<N>,
        _sync: Option<Sender<()>>,
    ) -> Self {
        Self {
            name,
            index,
            context,
            started: Arc::new(AtomicBool::default()),
            ctl: Channel::bounded(1),
            registrations: Mutex::new(RegistrationState::default()),
            incoming,
            _sync,
            shutdown: Channel::oneshot(),
        }
    }

    pub fn start(self: &Arc<Self>) {
        self.clone().spawn_notification_broadcasting_task();
    }

    fn spawn_notification_broadcasting_task(self: Arc<Self>) {
        // The task can only be spawned once
        if self.started.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            return;
        }
        trace!("[{}] Starting notification broadcasting task", self);
        let context = self.context.clone();
        workflow_core::task::spawn(async move {
            // Broadcasting plan by event type
            let mut plan = EventArray::<Plan<C>>::default();
            // Create a store for closed connections to be removed from the plan
            let mut purge: Vec<(ListenerId, RegistrationToken)> = Vec::new();
            loop {
                select_biased! {
                    ctl = self.ctl.recv().fuse() => {
                        if ctl.is_ok() {
                            // Read a finite snapshot, then release the local control
                            // mutex before building plans or awaiting any connection.
                            let registrations = self.registrations.lock().snapshot();
                            plan = EventArray::<Plan<C>>::default();
                            for ((event_type, id), token, (subscription, connection)) in registrations {
                                plan[event_type].insert_snapshot_entry(subscription.broadcasting(&context), id, token, connection);
                            }
                        } else {
                            break;
                        }
                    },

                    notification = self.incoming.recv().fuse() => {
                        if let Ok(notification) = notification {
                            // Broadcast the notification...
                            let event = notification.event_type();
                            for (subscription, encoding_set) in plan[event].iter() {
                                // ... by subscription scope
                                if let Some(applied_notification) = notification.apply_subscription(&**subscription, &context) {
                                    for (encoding, connection_set) in encoding_set.iter() {
                                        // ... by message encoding
                                        let message = C::into_message(&applied_notification, encoding);
                                        for (id, (token, connection)) in connection_set.iter() {
                                            // A stop/start can happen while an earlier listener's
                                            // send awaits. Never route this old plan to a replacement.
                                            // A send already in flight may finish on its old connection.
                                            if !self.registrations.lock().matches(&(event, *id), token) {
                                                continue;
                                            }
                                            // ... to listeners connections
                                            match connection.send(message.clone()).await {
                                                Ok(_) => {
                                                    trace!("[{}] sent notification {notification} to listener {id}", self);
                                                },
                                                Err(_) => {
                                                    if connection.is_closed() {
                                                        trace!("[{}] could not send a notification to listener {id} because its connection is closed - removing it", self);
                                                        purge.push((*id, token.clone()));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            // Remove closed connections
                            for (id, token) in purge.drain(..) {
                                plan[event].remove(&id);
                                self.registrations.lock().remove_if_current(&(event, id), &token);
                            }

                        } else {
                            break;
                        }
                    }
                }

                // In case we have a sync channel, report that the command was processed.
                // This is for test only.
                if let Some(ref sync) = self._sync {
                    let _ = sync.try_send(());
                }
            }
            debug!("[{}] notification stream ended", self);
            self.ctl.sender.close();
            self.registrations.lock().clear();
            let _ = self.shutdown.drain();
            let _ = self.shutdown.try_send(());
        });
    }

    pub fn register(&self, subscription: DynSubscription, id: ListenerId, connection: C) -> Result<()> {
        assert!(subscription.active());
        let mut registrations = self.registrations.lock();
        if self.ctl.sender.is_closed() {
            return Err(crate::error::Error::AlreadyStoppedError);
        }
        registrations.insert((subscription.event_type(), id), (subscription, connection));
        self.wake_control()
    }

    pub fn unregister(&self, event_type: EventType, id: ListenerId) -> Result<()> {
        let mut registrations = self.registrations.lock();
        if self.ctl.sender.is_closed() {
            return Err(crate::error::Error::AlreadyStoppedError);
        }
        registrations.remove(&(event_type, id));
        self.wake_control()
    }

    fn wake_control(&self) -> Result<()> {
        match self.ctl.try_send(()) {
            Ok(()) | Err(async_channel::TrySendError::Full(())) => Ok(()),
            Err(async_channel::TrySendError::Closed(())) => Err(crate::error::Error::AlreadyStoppedError),
        }
    }

    async fn join_notification_broadcasting_task(&self) -> Result<()> {
        trace!("[{}] joining", self);
        self.shutdown.recv().await?;
        debug!("[{}] terminated", self);
        Ok(())
    }

    pub async fn join(&self) -> Result<()> {
        self.join_notification_broadcasting_task().await
    }
}

impl<N, C> Display for Broadcaster<N, C>
where
    N: Notification,
    C: Connection<Notification = N>,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Broadcaster-{}-{}", self.name, self.index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        connection::{ChannelConnection, ChannelType},
        listener::Listener,
        notification::test_helpers::*,
        notifier::test_helpers::{
            SYNC_MAX_DELAY, Step, TestConnection, overall_test_steps, utxos_changed_test_steps, virtual_chain_changed_test_steps,
        },
        subscription::context::SubscriptionContext,
    };
    use async_channel::{Sender, unbounded};
    use tokio::time::timeout;

    type TestBroadcaster = Broadcaster<TestNotification, ChannelConnection<TestNotification>>;

    struct Test {
        name: &'static str,
        broadcaster: Arc<TestBroadcaster>,
        /// Listeners, vector index = ListenerId
        listeners: Vec<Listener<TestConnection>>,
        subscription_context: SubscriptionContext,
        sync_receiver: Receiver<()>,
        notification_sender: Sender<TestNotification>,
        notification_receivers: Vec<Receiver<TestNotification>>,
        steps: Vec<Step>,
    }

    impl Test {
        fn new(name: &'static str, listener_count: usize, steps: Vec<Step>) -> Self {
            const IDENT: &str = "test";
            let subscription_context = SubscriptionContext::new();
            let (sync_sender, sync_receiver) = unbounded();
            let (notification_sender, notification_receiver) = unbounded();
            let broadcaster =
                Arc::new(TestBroadcaster::new(IDENT, 0, subscription_context.clone(), notification_receiver, Some(sync_sender)));
            let mut listeners = Vec::with_capacity(listener_count);
            let mut notification_receivers = Vec::with_capacity(listener_count);
            for i in 0..listener_count {
                let (sender, receiver) = unbounded();
                let connection = TestConnection::new(IDENT, sender, ChannelType::Closable);
                let listener = Listener::new(i as ListenerId, connection);
                listeners.push(listener);
                notification_receivers.push(receiver);
            }
            Self {
                name,
                broadcaster: broadcaster.clone(),
                listeners,
                subscription_context,
                sync_receiver,
                notification_sender,
                notification_receivers,
                steps,
            }
        }

        async fn run(&mut self) {
            self.broadcaster.start();

            // Execute the test steps
            for (step_idx, step) in self.steps.iter().enumerate() {
                // Apply the subscription mutations and register the changes into the broadcaster
                trace!("{} #{} - Initial Subscription Context {}", self.name, step_idx, self.subscription_context.address_tracker);
                for (idx, mutation) in step.mutations.iter().enumerate() {
                    if let Some(mutation) = mutation {
                        trace!("{} #{} - {}: L{} {:?}", self.name, step_idx, step.name, idx, mutation);
                        let event = mutation.event_type();
                        let outcome =
                            self.listeners[idx].mutate(mutation.clone(), Default::default(), &self.subscription_context).unwrap();
                        if outcome.has_new_state() {
                            trace!(
                                "{} #{} - {}: - L{} has the new state {:?}",
                                self.name, step_idx, step.name, idx, self.listeners[idx].subscriptions[event]
                            );
                            let result = match mutation.active() {
                                true => self.broadcaster.register(
                                    self.listeners[idx].subscriptions[event].clone(),
                                    idx as u64,
                                    self.listeners[idx].connection(),
                                ),
                                false => self.broadcaster.unregister(event, idx as u64),
                            };
                            assert!(result.is_ok(), "{} #{} - {}: registration failed", self.name, step_idx, step.name);
                            assert!(
                                timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
                                "{} #{} - {}: receiving a sync message failed",
                                self.name,
                                step_idx,
                                step.name
                            );
                        } else if outcome.has_changes() {
                            trace!(
                                "{} #{} - {}: - L{} is inner changed into {:?}",
                                self.name, step_idx, step.name, idx, self.listeners[idx].subscriptions[event]
                            );
                        } else {
                            trace!(
                                "{} #{} - {}: - L{} is unchanged {:?}",
                                self.name, step_idx, step.name, idx, self.listeners[idx].subscriptions[event]
                            );
                        }
                    }
                }

                // Send the notification
                if step_idx == 8 {
                    trace!("#8");
                }
                trace!("{} #{} - {}: sending a notification...", self.name, step_idx, step.name);
                assert!(
                    self.notification_sender.send_blocking(step.notification.clone()).is_ok(),
                    "{} #{} - {}: sending the notification failed",
                    self.name,
                    step_idx,
                    step.name
                );
                trace!("{} #{} - {}: receiving sync signal...", self.name, step_idx, step.name);
                assert!(
                    timeout(SYNC_MAX_DELAY, self.sync_receiver.recv()).await.unwrap().is_ok(),
                    "{} #{} - {}: receiving a sync message failed",
                    self.name,
                    step_idx,
                    step.name
                );

                // Check what the listeners do receive
                for (idx, expected) in step.expected_notifications.iter().enumerate() {
                    if let Some(expected) = expected {
                        assert!(
                            !self.notification_receivers[idx].is_empty(),
                            "{} #{} - {}: listener[{}] has no notification in its channel though some is expected",
                            self.name,
                            step_idx,
                            step.name,
                            idx
                        );
                        let notification = self.notification_receivers[idx].recv().await.unwrap();
                        assert_eq!(
                            *expected, notification,
                            "{} #{} - {}: listener[{}] got wrong notification",
                            self.name, step_idx, step.name, idx
                        );
                    } else {
                        assert!(
                            self.notification_receivers[idx].is_empty(),
                            "{} #{} - {}: listener[{}] has a notification in its channel but should not",
                            self.name,
                            step_idx,
                            step.name,
                            idx
                        );
                    }
                }
            }
            self.notification_sender.close();
            assert!(self.broadcaster.join().await.is_ok(), "broadcaster failed to stop");
        }
    }

    #[tokio::test]
    async fn test_overall() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("BlockAdded broadcast (OverallSubscription type)", 2, overall_test_steps(0));
        test.run().await;
    }

    #[tokio::test]
    async fn test_virtual_chain_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("VirtualChainChanged broadcast", 2, virtual_chain_changed_test_steps(0));
        test.run().await;
    }

    #[tokio::test]
    async fn test_utxos_changed() {
        kaspa_core::log::try_init_logger("trace,kaspa_notify=trace");
        let mut test = Test::new("UtxosChanged broadcast", 3, utxos_changed_test_steps(0));
        test.run().await;
    }
}

#[cfg(test)]
#[path = "broadcaster_control_tests.rs"]
mod control_tests;
