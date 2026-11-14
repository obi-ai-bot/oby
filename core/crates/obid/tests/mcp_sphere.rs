//! obid's Obysphere members end to end: the real obid binary joined to a test Obysphere
//! (tests/support's fake Sphere, which checks what it's sent with the spec's checks), a teammate's
//! Oby (obi-ernesto) played through the Sphere's API, and `GET /v1/obid/sphere/members`.

mod support;

use serde_json::json;
use support::*;

/// obid (wellington) joined to a new test Obysphere, and obi-ernesto there too.
async fn joined(folder: &Folder) -> (FakeSphere, Obid, TestObi) {
    let sphere = FakeSphere::start().await;
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &stub_agent()), None);
    let before =
        tokio::task::block_in_place(|| call(&obid, "GET", "/v1/obid/sphere/members", None));
    assert_eq!(before.status, 409, "{}", before.text);
    let wellington = Owner::new("wellington");
    let joined = tokio::task::block_in_place(|| join_a(&obid, &sphere, &wellington));
    assert_eq!(joined["state"], "joined");
    let ernesto = TestObi::join_accepting(
        &sphere,
        "obi-ernesto",
        "ernesto",
        &["review_request", "question"],
    )
    .await;
    (sphere, obid, ernesto)
}

/// `GET /v1/obid/sphere/members`: the other members whose cards obid trusts, typed fields only,
/// read fresh, so a teammate who joined a moment ago shows.
#[tokio::test(flavor = "multi_thread")]
async fn members_are_the_teammates_obys() {
    let folder = Folder::new("mcp-members");
    let (_sphere, obid, _ernesto) = joined(&folder).await;
    let members =
        tokio::task::block_in_place(|| call(&obid, "GET", "/v1/obid/sphere/members", None));
    assert_eq!(members.status, 200, "{}", members.text);
    assert_eq!(
        members.body,
        json!([{"obi": "obi-ernesto", "owner": "ernesto", "accepts": ["question", "review_request"],
                "sealed": false, "verified": false}])
    );
}
