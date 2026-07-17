use anyhow::Result;
use tokio::sync::broadcast;

pub struct WsBroadcaster {
    tx: broadcast::Sender<String>,
}

impl WsBroadcaster {
    pub fn new() -> Self {
        let (tx, _) = broadcast::channel(100);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    pub fn broadcast(&self, event: &str) -> Result<()> {
        let _ = self.tx.send(event.to_string());
        Ok(())
    }
}

impl Default for WsBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ws_broadcaster() {
        let b = WsBroadcaster::new();
        let mut rx = b.subscribe();
        b.broadcast(r#"{"type":"test"}"#).unwrap();
        let received = rx.try_recv().unwrap();
        assert!(received.contains("test"));
    }

    #[test]
    fn test_multiple_subscribers() {
        let b = WsBroadcaster::new();
        let mut rx1 = b.subscribe();
        let mut rx2 = b.subscribe();
        b.broadcast("hello").unwrap();
        assert_eq!(rx1.try_recv().unwrap(), "hello");
        assert_eq!(rx2.try_recv().unwrap(), "hello");
    }
}
