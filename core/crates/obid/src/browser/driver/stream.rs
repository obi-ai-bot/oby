//! A job's live stream (BROWSER-7, 14): agent-browser's own stream WebSocket (on 127.0.0.1, its
//! port from `stream status` once `stream enable` ran), each frame handed to every reader's slot
//! (newest only), and the person's input sent back over it in agent-browser's shapes
//! (`input_mouse`, `input_keyboard`). Who may send input (the holder of the tab) is the API's to
//! check first. It owns the connection and its readers, and writes nothing.

use std::sync::{Arc, Mutex};

use futures_util::stream::SplitSink;
use futures_util::{SinkExt as _, StreamExt as _};
use serde_json::{Value, json};
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use super::{Call, Driver, Frame, StreamReader};
use crate::browser::contract::StreamInput;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// A job's live stream.
pub struct Stream {
    out: tokio::sync::Mutex<SplitSink<Socket, Message>>,
    readers: Arc<Mutex<Vec<Arc<StreamReader>>>>,
    task: tokio::task::JoinHandle<()>,
}

/// One message of agent-browser's stream as a frame, numbered `n` when it carries no number.
fn frame_of(m: &Value, n: u64) -> Option<Frame> {
    if m["type"] != "frame" {
        return None;
    }
    let meta = &m["metadata"];
    let size =
        |key: &str, plain: &str| meta[key].as_u64().or(m[plain].as_u64()).unwrap_or(0) as u32;
    Some(Frame {
        seq: m["seq"].as_u64().unwrap_or(n),
        data: m["data"].as_str().unwrap_or_default().to_string(),
        width: size("deviceWidth", "width"),
        height: size("deviceHeight", "height"),
    })
}

/// The person's input in agent-browser's own shape.
fn message_of(input: &StreamInput) -> Value {
    match input {
        StreamInput::Mouse {
            event_type,
            x,
            y,
            button,
            click_count,
            delta_x,
            delta_y,
        } => json!({
            "type": "input_mouse", "eventType": event_type, "x": x, "y": y,
            "button": button.clone().unwrap_or_else(|| "left".into()),
            "clickCount": click_count.unwrap_or(1),
            "deltaX": delta_x.unwrap_or(0.0), "deltaY": delta_y.unwrap_or(0.0),
        }),
        StreamInput::Key {
            event_type,
            key,
            text,
            code,
        } => {
            let mut m = json!({"type": "input_keyboard", "eventType": event_type, "key": key});
            if let Some(t) = text {
                m["text"] = json!(t);
            }
            if let Some(c) = code {
                m["code"] = json!(c);
            }
            m
        }
    }
}

impl Stream {
    /// Connects to agent-browser's stream on 127.0.0.1:`port`.
    pub async fn connect(port: u16) -> Result<Stream, String> {
        let mut request = format!("ws://127.0.0.1:{port}/")
            .into_client_request()
            .map_err(|e| e.to_string())?;
        request.headers_mut().insert(
            "Origin",
            "http://127.0.0.1"
                .parse()
                .map_err(|_| "a bad Origin header".to_string())?,
        );
        let (socket, _) = tokio_tungstenite::connect_async(request)
            .await
            .map_err(|e| format!("the browser's live stream didn't open: {e}"))?;
        let (out, mut incoming) = socket.split();
        let readers: Arc<Mutex<Vec<Arc<StreamReader>>>> = Arc::new(Mutex::new(vec![]));
        let fan = readers.clone();
        let task = tokio::spawn(async move {
            let mut n = 0u64;
            while let Some(Ok(message)) = incoming.next().await {
                let text = match message {
                    Message::Text(t) => t.to_string(),
                    Message::Close(_) => break,
                    _ => continue,
                };
                let Ok(m) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                n += 1;
                if let Some(frame) = frame_of(&m, n) {
                    let frame = Arc::new(frame);
                    for r in fan.lock().unwrap().iter() {
                        r.put(frame.clone());
                    }
                }
            }
            for r in fan.lock().unwrap().iter() {
                r.close();
            }
        });
        Ok(Stream {
            out: tokio::sync::Mutex::new(out),
            readers,
            task,
        })
    }

    /// A new reader: frames from now on.
    pub fn reader(&self) -> Arc<StreamReader> {
        let r = Arc::new(StreamReader::new());
        if self.task.is_finished() {
            r.close();
        }
        self.readers.lock().unwrap().push(r.clone());
        r
    }

    pub fn drop_reader(&self, reader: &Arc<StreamReader>) {
        self.readers
            .lock()
            .unwrap()
            .retain(|r| !Arc::ptr_eq(r, reader));
    }

    /// Closes the stream: every reader waiting is woken with nothing.
    pub fn close(&self) {
        self.task.abort();
        for r in self.readers.lock().unwrap().iter() {
            r.close();
        }
    }

    pub fn is_open(&self) -> bool {
        !self.task.is_finished()
    }

    /// Sends the person's input to the page.
    pub async fn send(&self, input: &StreamInput) -> Result<(), String> {
        self.out
            .lock()
            .await
            .send(Message::text(message_of(input).to_string()))
            .await
            .map_err(|e| format!("the person's input didn't reach the browser: {e}"))
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        self.close();
    }
}

impl Driver {
    /// A job's live stream: agent-browser's turned on and connected once, shared by every reader.
    pub async fn stream(&self, job: &str) -> Result<Arc<Stream>, String> {
        let mut streams = self.streams.lock().await;
        if let Some(s) = streams.get(job).filter(|s| s.is_open()) {
            return Ok(s.clone());
        }
        let session = self
            .session_of(job)
            .ok_or("This job's browser isn't running.")?;
        let limit = self.options.system_limit;
        // Already on is fine: the status says its port either way.
        let _ = session
            .call(&["stream".into(), "enable".into()], limit)
            .await;
        let port = match session
            .call(&["stream".into(), "status".into()], limit)
            .await
        {
            Call::Ok(v) => v["port"]
                .as_u64()
                .filter(|p| (1..=u16::MAX as u64).contains(p))
                .ok_or("agent-browser's stream status gave no port")?
                as u16,
            Call::Failed(e) => return Err(e),
            Call::TimedOut(ms) => {
                return Err(format!("agent-browser stream status took over {ms} ms"));
            }
        };
        let stream = Arc::new(Stream::connect(port).await?);
        streams.insert(job.into(), stream.clone());
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::super::{Binaries, Options, stand_in};
    use super::*;
    use crate::browser::engine::Browsers;

    /// A stand-in for agent-browser's stream server: sends `frames` frames as soon as a client
    /// connects, then hands back every text message it gets.
    async fn stream_server(frames: u64) -> (u16, tokio::sync::mpsc::UnboundedReceiver<Value>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            let (conn, _) = listener.accept().await.unwrap();
            let socket = tokio_tungstenite::accept_async(conn).await.unwrap();
            let (mut out, mut incoming) = socket.split();
            for seq in 1..=frames {
                let m = json!({"type": "frame", "seq": seq, "data": format!("img{seq}"), "metadata": {"deviceWidth": 1280, "deviceHeight": 800}});
                out.send(Message::text(m.to_string())).await.unwrap();
            }
            out.send(Message::text(
                json!({"type": "status", "connected": true}).to_string(),
            ))
            .await
            .unwrap();
            while let Some(Ok(Message::Text(t))) = incoming.next().await {
                let _ = tx.send(serde_json::from_str::<Value>(&t).unwrap());
            }
        });
        (port, rx)
    }

    #[tokio::test]
    async fn browser_14_a_slow_reader_of_the_live_stream_gets_the_newest_frame_and_input_goes_back()
    {
        let home = stand_in::scratch("stream");
        let d = Driver::new(
            Options::new(home.clone(), Binaries::InPlace(stand_in::install(&home))),
            Arc::new(std::sync::Mutex::new(Browsers::new())),
        );
        d.start("job-a").await.unwrap();
        let (port, mut got) = stream_server(10).await;
        let session = d.session_of("job-a").unwrap();
        std::fs::write(session.home.join("stream-port"), port.to_string()).unwrap();
        let stream = d.stream("job-a").await.unwrap();
        let reader = stream.reader();
        // The reader reads nothing until all ten arrived: the newest only, nine dropped.
        assert!(
            stand_in::within(Duration::from_secs(10), || async { reader.dropped() == 9 }).await
        );
        let (frame, dropped) = reader.next().await.unwrap();
        assert_eq!(
            (frame.seq, frame.data.as_str(), frame.width, dropped),
            (10, "img10", 1280, 9)
        );
        // The same stream for the next reader.
        assert!(Arc::ptr_eq(&stream, &d.stream("job-a").await.unwrap()));
        stream
            .send(&StreamInput::Mouse {
                event_type: "mousePressed".into(),
                x: 10.0,
                y: 20.0,
                button: None,
                click_count: None,
                delta_x: None,
                delta_y: None,
            })
            .await
            .unwrap();
        let m = got.recv().await.unwrap();
        assert_eq!(m["type"], "input_mouse");
        assert_eq!(
            (m["eventType"].as_str(), m["button"].as_str()),
            (Some("mousePressed"), Some("left"))
        );
        d.end("job-a").await;
        // Its job's end closes it.
        let after = tokio::time::timeout(Duration::from_secs(10), reader.next()).await;
        assert!(matches!(after, Ok(None)), "{after:?}");
        let _ = std::fs::remove_dir_all(home);
    }
}
