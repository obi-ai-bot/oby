//! What `GET /v1/events` sends: one Server-Sent Event per change (docs/runner-api.md,
//! "GET /v1/events"), to every app that follows.

use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// One event: its name (`job`, `inbox`, `accounts`, …) and its data.
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub name: &'static str,
    pub data: Value,
}

/// Where events go out, on two queues of 1,024 events each ([`Events::follow`] says what an app
/// that falls behind on one gets).
#[derive(Clone)]
pub struct Events {
    sender: broadcast::Sender<Event>,
    /// `transcript` events, many to a job (each tool call's start and end), on a channel of
    /// their own: a busy job never pushes another event out of a slow listener's backlog.
    transcripts: broadcast::Sender<Event>,
}

impl Events {
    pub fn new() -> Events {
        let (sender, _) = broadcast::channel(1024);
        let (transcripts, _) = broadcast::channel(1024);
        Events {
            sender,
            transcripts,
        }
    }

    pub fn send(&self, name: &'static str, data: Value) {
        let channel = if name == "transcript" {
            &self.transcripts
        } else {
            &self.sender
        };
        // Nobody listening is fine.
        let _ = channel.send(Event { name, data });
    }

    /// Every event but `transcript` ones.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Both queues, as `GET /v1/events` sends them. An app more than 1,024 `transcript` events
    /// behind gets one `transcriptStale` event in place of those it missed, and the stream goes on:
    /// a busy job's lines never cut a phone off, so it never reconnects only to fall behind again.
    /// One more than 1,024 of any other event behind has its stream ended there, and fetches again
    /// when it reconnects, as every app does.
    pub fn follow(&self) -> impl Stream<Item = Event> + use<> {
        let others = BroadcastStream::new(self.sender.subscribe()).map(Result::ok);
        let transcripts = BroadcastStream::new(self.transcripts.subscribe()).map(|event| {
            Some(
                event.unwrap_or_else(|BroadcastStreamRecvError::Lagged(missed)| Event {
                    name: "transcriptStale",
                    data: json!({"missed": missed}),
                }),
            )
        });
        stream::select(others, transcripts)
            .take_while(|event| std::future::ready(event.is_some()))
            .filter_map(std::future::ready)
    }
}

impl Default for Events {
    fn default() -> Events {
        Events::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// WIRE-4: behind on `transcript` events, the stream says so once and goes on.
    #[tokio::test]
    async fn a_stream_behind_on_transcripts_says_so_and_goes_on() {
        let events = Events::new();
        let mut followed = Box::pin(events.follow());
        for n in 0..2000 {
            events.send("transcript", json!({"n": n}));
        }
        let stale = followed.next().await.unwrap();
        assert_eq!(stale.name, "transcriptStale");
        assert_eq!(stale.data, json!({"missed": 2000 - 1024}));
        assert_eq!(
            followed.next().await.unwrap().data,
            json!({"n": 2000 - 1024})
        );
        events.send("job", json!({"id": "j"}));
        let mut job = None;
        while let Some(event) = followed.next().await {
            if event.name == "job" {
                job = Some(event);
                break;
            }
        }
        assert_eq!(job.unwrap().data, json!({"id": "j"}));
    }

    /// WIRE-4: behind on any other event, the stream ends.
    #[tokio::test]
    async fn a_stream_behind_on_other_events_ends() {
        let events = Events::new();
        let mut followed = Box::pin(events.follow());
        for n in 0..2000 {
            events.send("job", json!({"n": n}));
        }
        assert!(followed.next().await.is_none());
    }

    #[test]
    fn transcripts_never_crowd_the_other_events() {
        let events = Events::new();
        let mut others = events.subscribe();
        let mut transcripts = events.transcripts.subscribe();
        for n in 0..2000 {
            events.send("transcript", json!({"n": n}));
        }
        events.send("job", json!({"id": "j"}));
        assert_eq!(others.try_recv().unwrap().name, "job");
        assert!(matches!(
            transcripts.try_recv(),
            Err(broadcast::error::TryRecvError::Lagged(_))
        ));
    }
}
