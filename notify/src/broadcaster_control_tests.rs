use super::*;
use crate::{
    connection::Unchanged,
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
