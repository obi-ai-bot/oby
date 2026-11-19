//! One pattern for every connector, starting with code hosts: a host's definition (names, URLs,
//! remote recognition, sign-in, capabilities, what its states mean), the calls Oby makes of it
//! ([`CodeHost`]), and the conformance suite every adapter passes ([`conformance`]).
//!
//! No I/O and no async runtime: plain data and pure functions. obid's adapters do the I/O behind
//! these signatures (docs/contracts/connector-pattern.md, proposed). See README.md for how an
//! adapter runs the suite, and where this differs from T3 Code's shape.

pub mod conformance;
pub mod definition;
pub mod error;
pub mod host;
pub mod remote;
pub mod words;

pub use definition::{
    Capabilities, Capability, ChangeRequestLink, CodeHostDefinition, CommitLink, IssueLink, Nouns,
    SignIn,
};
pub use error::{ConnectorError, Detail};
pub use host::{
    ChangeRequest, Checks, CodeHost, Cursor, ListQuery, MergeMethod, Page, Review, Viewer,
    list_all, require,
};
pub use remote::{HostNames, Repo};
pub use words::{
    ChangeRequestState, ChecksState, Family, MergeState, Mergeable, ReviewDecision, ReviewState,
};
