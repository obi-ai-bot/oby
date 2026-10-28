//! The frames the phone relay carries (spec `protocol.md` section 13): a stream, a kind and a
//! payload, sealed whole by crate::relay::envelope.

use serde_json::Value;

/// A frame's kind (protocol.md section 13).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A stream opens: its payload is JSON saying what it is.
    Open = 0,
    Data = 1,
    End = 2,
    /// The stream failed: one line saying why.
    Reset = 3,
}

/// One frame's plaintext: a stream, a kind and a payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub stream: u32,
    pub kind: Kind,
    pub payload: Vec<u8>,
}

impl Frame {
    pub fn new(stream: u32, kind: Kind, payload: impl Into<Vec<u8>>) -> Frame {
        Frame {
            stream,
            kind,
            payload: payload.into(),
        }
    }

    pub fn json(stream: u32, kind: Kind, value: &Value) -> Frame {
        Frame::new(stream, kind, value.to_string().into_bytes())
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(5 + self.payload.len());
        out.extend_from_slice(&self.stream.to_be_bytes());
        out.push(self.kind as u8);
        out.extend_from_slice(&self.payload);
        out
    }

    pub fn decode(bytes: &[u8]) -> Option<Frame> {
        let (head, payload) = (bytes.get(..5)?, bytes.get(5..)?);
        let kind = match head[4] {
            0 => Kind::Open,
            1 => Kind::Data,
            2 => Kind::End,
            3 => Kind::Reset,
            _ => return None,
        };
        Some(Frame {
            stream: u32::from_be_bytes([head[0], head[1], head[2], head[3]]),
            kind,
            payload: payload.to_vec(),
        })
    }

    /// The payload as JSON, for `open` frames and the control stream, read strictly (protocol.md
    /// section 2).
    pub fn value(&self) -> Option<Value> {
        obi_protocol::json::from_slice(&self.payload).ok()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{Frame, Kind};

    #[test]
    fn frames_round_trip() {
        let frame = Frame::json(
            7,
            Kind::Open,
            &json!({"type": "model", "method": "POST", "path": "/v1/messages"}),
        );
        assert_eq!(Frame::decode(&frame.encode()), Some(frame));
        assert_eq!(Frame::decode(&[0, 0, 0, 1, 9]), None);
        assert_eq!(Frame::decode(&[0, 0, 1]), None);
    }
}
