//! What `GET /v1/events` sends: one Server-Sent Event per change (docs/runner-api.md,
//! "GET /v1/events"), to every app that follows, numbered so an app that dropped gets what it
//! missed (docs/contracts/events.md).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, PoisonError};

use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;

/// How many numbered events obid keeps for apps that resume, and how much of their data.
const RING: usize = 2000;
const RING_BYTES: usize = 8 * 1024 * 1024;

/// One event: its name (`job`, `inbox`, `accounts`, …), its data, and its number in this run of
/// obid (none for `transcript`, `delta` and what a follower is told alone: `transcriptStale`,
/// `resync`).
#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub name: &'static str,
    pub data: Value,
    pub id: Option<u64>,
}

/// Where events go out, on two queues of 1,024 events each ([`Events::follow`] says what an app
/// that falls behind on one gets).
#[derive(Clone)]
pub struct Events {
    sender: broadcast::Sender<Event>,
    /// `transcript` events, many to a job (each tool call's start and end), on a channel of
    /// their own: a busy job never pushes another event out of a slow listener's backlog.
    transcripts: broadcast::Sender<Event>,
    /// Numbered, kept and sent under this one lock, so a follower that copies the ring and
    /// subscribes under it misses nothing and gets nothing twice.
    numbered: Arc<Mutex<Numbered>>,
}

/// This run's epoch (random, never saved: a restart means `resync`), the last number given, and
/// the latest numbered events with the size of each one's data.
struct Numbered {
    epoch: String,
    last: u64,
    ring: VecDeque<(Event, usize)>,
    bytes: usize,
}

impl Numbered {
    /// The number `after` (`<epoch>-<seq>`) resumes from: this run's, and none of the events
    /// after it gone from the ring.
    fn resumes(&self, after: &str) -> Option<u64> {
        let (epoch, seq) = after.split_once('-')?;
        let seq = seq.parse::<u64>().ok()?;
        (epoch == self.epoch
            && self.last.saturating_sub(self.ring.len() as u64) <= seq
            && seq <= self.last)
            .then_some(seq)
    }
}

impl Events {
    pub fn new() -> Events {
        let (sender, _) = broadcast::channel(1024);
        let (transcripts, _) = broadcast::channel(1024);
        Events {
            sender,
            transcripts,
            numbered: Arc::new(Mutex::new(Numbered {
                epoch: uuid::Uuid::new_v4().simple().to_string(),
                last: 0,
                ring: VecDeque::new(),
                bytes: 0,
            })),
        }
    }

    pub fn send(&self, name: &'static str, data: Value) {
        // Nobody listening is fine.
        if name == "transcript" || name == "delta" {
            let channel = if name == "transcript" {
                &self.transcripts
            } else {
                &self.sender
            };
            let _ = channel.send(Event {
                name,
                data,
                id: None,
            });
            return;
        }
        let size = data.to_string().len();
        let mut numbered = self.numbered.lock().unwrap_or_else(PoisonError::into_inner);
        numbered.last += 1;
        let event = Event {
            name,
            data,
            id: Some(numbered.last),
        };
        numbered.ring.push_back((event.clone(), size));
        numbered.bytes += size;
        while numbered.ring.len() > RING || numbered.bytes > RING_BYTES {
            let Some((_, size)) = numbered.ring.pop_front() else {
                break;
            };
            numbered.bytes -= size;
        }
        let _ = self.sender.send(event);
    }

    /// This run's epoch, the part of an event's id before its number.
    pub fn epoch(&self) -> String {
        self.numbered
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .epoch
            .clone()
    }

    /// Every event but `transcript` ones.
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Both queues, live, as `GET /v1/events` sends them to an app that names no event.
    pub fn follow(&self) -> impl Stream<Item = Event> + use<> {
        self.follow_from(None)
    }

    /// Both queues, as `GET /v1/events` sends them. After `after` (`<epoch>-<seq>`, the last
    /// event an app had), the numbered events since it come first, from the ring; an `after` the
    /// ring can't resume from (another run's, older than the ring, ahead of it, or not an id)
    /// gets one `resync` first instead. An app more than 1,024 `transcript` events behind gets
    /// one `transcriptStale` event in place of those it missed, and the stream goes on: a busy
    /// job's lines never cut a phone off, so it never reconnects only to fall behind again. One
    /// more than 1,024 of any other event behind has its stream ended there, and resumes with
    /// `after` when it reconnects.
    pub fn follow_from(&self, after: Option<&str>) -> impl Stream<Item = Event> + use<> {
        let (others, first, seen) = {
            let numbered = self.numbered.lock().unwrap_or_else(PoisonError::into_inner);
            let others = self.sender.subscribe();
            match after.map(|after| numbered.resumes(after)) {
                None => (others, vec![], 0),
                Some(Some(seq)) => {
                    let missed = numbered
                        .ring
                        .iter()
                        .filter(|(event, _)| event.id > Some(seq))
                        .map(|(event, _)| Some(event.clone()))
                        .collect();
                    (others, missed, numbered.last)
                }
                Some(None) => {
                    let resync = Event {
                        name: "resync",
                        data: json!({"epoch": numbered.epoch}),
                        id: None,
                    };
                    (others, vec![Some(resync)], 0)
                }
            }
        };
        let others = BroadcastStream::new(others)
            .map(Result::ok)
            .filter(move |event| {
                let again = matches!(event, Some(Event { id: Some(id), .. }) if *id <= seen);
                std::future::ready(!again)
            });
        let transcripts = BroadcastStream::new(self.transcripts.subscribe()).map(|event| {
            Some(
                event.unwrap_or_else(|BroadcastStreamRecvError::Lagged(missed)| Event {
                    name: "transcriptStale",
                    data: json!({"missed": missed}),
                    id: None,
                }),
            )
        });
        stream::iter(first)
            .chain(stream::select(others, transcripts))
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

    /// EVENTS-1: every event on the main queue but `delta` is numbered, one after another, with
    /// its data as sent; `transcript` and `delta` events are neither numbered nor kept.
    #[test]
    fn events_1_numbered_events_carry_an_id() {
        let events = Events::new();
        let epoch = events.epoch();
        assert!(!epoch.is_empty() && !epoch.contains('-'));
        let mut others = events.subscribe();
        let mut transcripts = events.transcripts.subscribe();
        events.send("job", json!({"id": "a"}));
        events.send("delta", json!({"text": "to"}));
        events.send("transcript", json!({"line": 1}));
        events.send("job", json!({"id": "b"}));
        let first = others.try_recv().unwrap();
        let delta = others.try_recv().unwrap();
        let second = others.try_recv().unwrap();
        assert_eq!((first.name, &first.data), ("job", &json!({"id": "a"})));
        assert_eq!((second.name, &second.data), ("job", &json!({"id": "b"})));
        assert_eq!(second.id, first.id.map(|id| id + 1));
        assert_eq!((delta.name, delta.id), ("delta", None));
        assert_eq!(transcripts.try_recv().unwrap().id, None);
        let id = format!("{epoch}-{}", first.id.unwrap());
        let (got_epoch, seq) = id.split_once('-').unwrap();
        assert_eq!(
            (got_epoch, seq.parse::<u64>().ok()),
            (epoch.as_str(), first.id)
        );
        let ring = &events.numbered.lock().unwrap().ring;
        let kept: Vec<_> = ring.iter().map(|(event, _)| event.name).collect();
        assert_eq!(kept, ["job", "job"]);
    }

    /// The events `follow_from(after)` yields at once, before any live one.
    fn replayed(events: &Events, after: &str) -> Vec<Event> {
        let mut followed = Box::pin(events.follow_from(Some(after)));
        let mut got = vec![];
        while let Some(Some(event)) = futures_util::FutureExt::now_or_never(followed.next()) {
            got.push(event);
        }
        got
    }

    /// EVENTS-3: the ring keeps the last 2,000 numbered events and no more than 8 MB of their
    /// data; an id older than that gets one `resync`, one inside it the rest.
    #[tokio::test]
    async fn events_3_older_than_the_ring_gets_resync() {
        let events = Events::new();
        let epoch = events.epoch();
        let resync = vec![Event {
            name: "resync",
            data: json!({"epoch": epoch}),
            id: None,
        }];
        for n in 0..2005 {
            events.send("job", json!({"n": n}));
        }
        assert_eq!(events.numbered.lock().unwrap().ring.len(), 2000);
        assert_eq!(replayed(&events, &format!("{epoch}-4")), resync);
        let rest = replayed(&events, &format!("{epoch}-5"));
        assert_eq!(rest.len(), 2000);
        assert_eq!((rest[0].id, rest[1999].id), (Some(6), Some(2005)));
        assert!(replayed(&events, &format!("{epoch}-2005")).is_empty());
        assert_eq!(replayed(&events, &format!("{epoch}-2006")), resync);

        let big = "x".repeat(1024 * 1024);
        for _ in 0..9 {
            events.send("job", json!({"big": big}));
        }
        let numbered = events.numbered.lock().unwrap();
        assert!(numbered.bytes <= RING_BYTES && numbered.ring.len() < 9);
        let oldest = numbered.ring.front().unwrap().0.id.unwrap();
        drop(numbered);
        assert_eq!(
            replayed(&events, &format!("{epoch}-{}", oldest - 2)),
            resync
        );
        assert_eq!(
            replayed(&events, &format!("{epoch}-{}", oldest - 1)).len(),
            2014 - oldest as usize + 1
        );
    }
}
