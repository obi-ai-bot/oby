//! The person's secret going into a page (BROWSER-8): the tab they hold brought to the front, its
//! field found (`get box`) and clicked through the live stream, and the value typed there key by
//! key, the way their own keys would. The value goes only into the stream's key events: never into
//! an agent-browser argument, a log line, a file or an answer. Who may (the holder of the tab) is
//! the API's and the core's to check first. It owns no state and writes nothing.

use serde_json::Value;

use super::{Call, Driver};
use crate::browser::contract::StreamInput;

impl Driver {
    /// Types `value` into `field` (a CSS selector) of the tab whose target is `target`, in `job`'s
    /// browser, through its live stream.
    pub async fn person_types(
        &self,
        job: &str,
        target: &str,
        field: &str,
        value: &str,
    ) -> Result<(), String> {
        let session = self
            .session_of(job)
            .ok_or("This job's browser isn't running.")?;
        let limit = self.options.system_limit;
        let read = |call: Call| match call {
            Call::Ok(v) => Ok(v),
            Call::Failed(e) => Err(e),
            Call::TimedOut(ms) => Err(format!("agent-browser took over {ms} ms")),
        };
        read(
            session
                .call(&["tab".into(), target.to_string()], limit)
                .await,
        )?;
        let place: Value = read(
            session
                .call(&["get".into(), "box".into(), field.to_string()], limit)
                .await,
        )?;
        let at = |key: &str| place[key].as_f64().unwrap_or(0.0);
        let (x, y) = (at("x") + at("width") / 2.0, at("y") + at("height") / 2.0);
        let stream = self.stream(job).await?;
        for event_type in ["mousePressed", "mouseReleased"] {
            stream
                .send(&StreamInput::Mouse {
                    event_type: event_type.into(),
                    x,
                    y,
                    button: Some("left".into()),
                    click_count: Some(1),
                    delta_x: None,
                    delta_y: None,
                })
                .await?;
        }
        // The page takes the click's focus before the first key, as the lab found it needs.
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        for ch in value.chars() {
            let key = ch.to_string();
            stream
                .send(&StreamInput::Key {
                    event_type: "keyDown".into(),
                    key: key.clone(),
                    text: Some(key.clone()),
                    code: None,
                })
                .await?;
            stream
                .send(&StreamInput::Key {
                    event_type: "keyUp".into(),
                    key,
                    text: None,
                    code: None,
                })
                .await?;
        }
        Ok(())
    }
}
