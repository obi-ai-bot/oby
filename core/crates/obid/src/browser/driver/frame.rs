//! One frame of a job's live stream as agent-browser sent it (BROWSER-14): its number, the image
//! (base64 JPEG) and its size. It owns no state and writes nothing.

/// One frame of a job's live stream.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// agent-browser's own frame number: a gap is a dropped frame.
    pub seq: u64,
    /// The image, base64.
    pub data: String,
    pub width: u32,
    pub height: u32,
}
