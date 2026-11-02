use super::*;
use crate::{
    connection::{ChannelConnection, ChannelType, Unchanged},
    notification::test_helpers::{BlockAddedNotification, TestNotification},
    subscription::single::OverallSubscription,
};
use std::time::Duration;
use tokio::time::timeout;

#[derive(Clone, Debug)]
struct ObservedConnection {
    sender: async_channel::Sender<TestNotification>,
    entered: Option<async_channel::Sender<()>>,
}

#[tokio::test]
async fn pending_control_wake_and_closed_input_terminate_without_a_listener() {
    let (incoming, receiver) = async_channel::unbounded();
    let broadcaster = Arc::new(Broadcaster::new("control-close", 0, SubscriptionContext::new(), receiver, None));
    let (sender, _) = async_channel::unbounded();
    let connection = ObservedConnection { sender, entered: None };
    let scope: DynSubscription = Arc::new(OverallSubscription::new(EventType::BlockAdded, true));
    for _ in 0..100 {
        broadcaster.register(scope.clone(), 7, connection.clone()).unwrap();
        broadcaster.unregister(EventType::BlockAdded, 7).unwrap();
    }
    assert_eq!(broadcaster.ctl.receiver.len(), 1);
    incoming.close();
    broadcaster.start();
    broadcaster.start(); // idempotent start must not leave an extra worker/join signal
    timeout(Duration::from_secs(5), broadcaster.join()).await.unwrap().unwrap();
    assert!(broadcaster.registrations.lock().snapshot().is_empty());
    assert!(broadcaster.register(scope, 7, connection).is_err());
    assert!(broadcaster.unregister(EventType::BlockAdded, 7).is_err());
}

#[tokio::test]
async fn closed_input_waits_for_bounded_persistent_listener_to_drain() {
    let (incoming, receiver) = async_channel::unbounded();
    let (sync, synced) = async_channel::unbounded();
    let broadcaster = Arc::new(Broadcaster::new("persistent-close", 0, SubscriptionContext::new(), receiver, Some(sync)));
    let (sender, received) = async_channel::bounded(1);
    sender.send(notification(0)).await.unwrap();
    let connection = ChannelConnection::new("persistent", sender.clone(), ChannelType::Persistent);
    let scope: DynSubscription = Arc::new(OverallSubscription::new(EventType::BlockAdded, true));
    broadcaster.register(scope, 7, connection.clone()).unwrap();
    broadcaster.start();
    timeout(Duration::from_secs(5), synced.recv()).await.unwrap().unwrap();
    incoming.send(notification(10)).await.unwrap();
    timeout(Duration::from_secs(5), async {
        while !incoming.is_empty() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    incoming.close();
    assert!(!connection.close(), "Persistent does not close a shared consumer channel");
    assert!(!sender.is_closed());
    assert!(timeout(Duration::from_millis(25), broadcaster.join()).await.is_err(), "input close must not cancel an owed data send");
    // Resume the actual bounded consumer. Both queued data items survive and
    // closing the source then completes shutdown without forcibly closing it.
    assert_eq!(received.recv().await.unwrap(), notification(0));
    assert_eq!(timeout(Duration::from_secs(5), received.recv()).await.unwrap().unwrap(), notification(10));
    timeout(Duration::from_secs(5), broadcaster.join()).await.unwrap().unwrap();
    assert!(!sender.is_closed());
    assert!(broadcaster.registrations.lock().snapshot().is_empty());
}
impl Display for ObservedConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("observed")
    }
}
#[async_trait::async_trait]
impl Connection for ObservedConnection {
    type Notification = TestNotification;
    type Message = TestNotification;
    type Encoding = Unchanged;
    type Error = crate::error::Error;
    fn encoding(&self) -> Unchanged {
        Unchanged::Clone
    }
    fn into_message(notification: &TestNotification, _: &Unchanged) -> TestNotification {
        notification.clone()
    }
    async fn send(&self, notification: TestNotification) -> Result<()> {
        if let Some(entered) = &self.entered {
            let _ = entered.try_send(());
        }
        self.sender.send(notification).await?;
        Ok(())
    }
    fn close(&self) -> bool {
        self.sender.close()
    }
    fn is_closed(&self) -> bool {
        self.sender.is_closed()
    }
}
fn notification(data: u64) -> TestNotification {
    TestNotification::BlockAdded(BlockAddedNotification { data })
}

#[tokio::test]
async fn control_churn_while_send_waits_keeps_final_registration_and_generation() {
    let (incoming, receiver) = async_channel::unbounded();
    let (sync, synced) = async_channel::unbounded();
    let broadcaster = Arc::new(Broadcaster::new("control-pressure", 0, SubscriptionContext::new(), receiver, Some(sync)));
    let (old_sender, old_receiver) = async_channel::bounded(1);
    old_sender.send(notification(0)).await.unwrap(); // force the next send to wait
    let (entered, sending) = async_channel::bounded(1);
    let old = ObservedConnection { sender: old_sender.clone(), entered: Some(entered) };
    let scope: DynSubscription = Arc::new(OverallSubscription::new(EventType::BlockAdded, true));
    broadcaster.register(scope.clone(), 7, old.clone()).unwrap();
    broadcaster.start();
    timeout(Duration::from_secs(5), synced.recv()).await.unwrap().unwrap();
    incoming.send(notification(10)).await.unwrap();
    timeout(Duration::from_secs(5), sending.recv()).await.unwrap().unwrap();
    // The broadcaster is awaiting an actual listener send. Control admission
    // must stay synchronous, without retaining a history for churned IDs.
    for id in 100..10_100 {
        broadcaster.register(scope.clone(), id, old.clone()).unwrap();
        broadcaster.unregister(EventType::BlockAdded, id).unwrap();
    }
    assert_eq!(broadcaster.registrations.lock().snapshot().len(), 1);
    assert_eq!(broadcaster.ctl.receiver.len(), 1, "only one wake is retained");
    broadcaster.unregister(EventType::BlockAdded, 7).unwrap();
    let (new_sender, new_receiver) = async_channel::bounded(4);
    broadcaster.register(scope, 7, ObservedConnection { sender: new_sender, entered: None }).unwrap();
    incoming.send(notification(11)).await.unwrap();
    old_sender.close(); // old awaited send fails; its purge must not remove new ID 7
    timeout(Duration::from_secs(5), synced.recv()).await.unwrap().unwrap();
    timeout(Duration::from_secs(5), synced.recv()).await.unwrap().unwrap();
    assert_eq!(timeout(Duration::from_secs(5), new_receiver.recv()).await.unwrap().unwrap(), notification(11));
    assert_eq!(old_receiver.recv().await.unwrap(), notification(0));
    assert!(old_receiver.is_empty(), "no retry forwards the old in-flight event");
    assert_eq!(broadcaster.registrations.lock().snapshot().len(), 1);
    // Every data event for the current registration still travels in order.
    for n in [12, 13] {
        incoming.send(notification(n)).await.unwrap();
    }
    for n in [12, 13] {
        assert_eq!(timeout(Duration::from_secs(5), new_receiver.recv()).await.unwrap().unwrap(), notification(n));
    }
    incoming.close();
    timeout(Duration::from_secs(5), broadcaster.join()).await.unwrap().unwrap();
    assert!(broadcaster.registrations.lock().snapshot().is_empty());
}
