//! docs/contracts/artifacts.md, ARTIFACTS-1 to 8: each scenario a test named after it, on the
//! store itself. The same scenarios run against a running obid in the Blok suite
//! (tools/obid-spec/workflows/routes/artifacts/).

use super::tests::{Temp, what};
use super::*;

/// An upload id shaped like a token: 40 letters and digits.
const LONG_ID: &str = "Kq7Zt2mW9xR4vB8nL3pF6hJ1sD5gA0cYeU7iO2wT";
const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR one pixel";

fn board(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><title>{title}</title></head><body>\n<x-dc>\n{body}\n</x-dc>\n<script type=\"text/x-dc\" data-dc-script>\nclass Component extends DCLogic {{}}\n</script>\n</body></html>\n"
    )
}

/// Claude Design's `canvas.json`, with its own spacing (never re-serialized by obid).
fn canvas(boards: &[&str]) -> String {
    let boards: serde_json::Map<String, serde_json::Value> = boards
        .iter()
        .map(|b| {
            (
                (*b).to_owned(),
                json!({"x": 0, "y": 0, "w": 800, "h": 600, "page": "design"}),
            )
        })
        .collect();
    let canvas = json!({
        "v": 3,
        "title": "Store design",
        "launch": {"view": "focused", "file": "Main.dc.html"},
        "pages": [{"id": "design", "name": "Design"}],
        "boards": boards,
        "order": boards.keys().collect::<Vec<_>>(),
        "designSystems": [{"title": "Store", "namespace": "store", "version": "1"}]
    });
    format!(
        "{}\n",
        serde_json::to_string_pretty(&canvas)
            .unwrap()
            .replace("  ", " ")
    )
}

/// A design at `folder` of the room: two boards, a design system, an upload with a plain id and
/// one with a token-like id.
fn design(t: &Temp, folder: &str) {
    t.write(
        &format!("{folder}/canvas.json"),
        canvas(&["Main.dc.html", "Card.dc.html"]),
    );
    t.write(
        &format!("{folder}/Main.dc.html"),
        board(
            "Main",
            &format!("<img src=\"/_blob/{LONG_ID}\">\n<img src=\"/_blob/logo\">\n<dc-import name=\"Card\"></dc-import>"),
        ),
    );
    t.write(
        &format!("{folder}/Card.dc.html"),
        board("Card", "<p>Card</p>"),
    );
    t.write(
        &format!("{folder}/ds/store/tokens.json"),
        r##"{"ink":"#ececec"}"##,
    );
    t.write(&format!("{folder}/_blob/logo"), PNG);
    t.write(&format!("{folder}/_blob/{LONG_ID}"), PNG);
}

fn hashed(id: &str) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(id.as_bytes()))[..16].to_owned()
}

/// ARTIFACTS-1: a Claude Design folder is a `design`, every board and `canvas.json` kept byte for
/// byte, its uploads under `assets/`.
#[tokio::test]
async fn artifacts_1_a_design_is_kept_byte_for_byte() {
    let t = Temp::new("a1");
    design(&t, "design");
    let (store, mut events) = t.store();
    let room = t.room();
    let (id, version) = store.publish(what(&room, "design")).await.unwrap();
    let artifact = store.get(&id).unwrap();
    assert_eq!((artifact.kind, version), (Kind::Design, 1));
    for name in [
        "canvas.json",
        "Main.dc.html",
        "Card.dc.html",
        "ds/store/tokens.json",
    ] {
        assert_eq!(
            store.file(&id, "1", name).unwrap().1,
            std::fs::read(room.join("design").join(name)).unwrap(),
            "{name}"
        );
    }
    let long = format!("assets/{}.png", hashed(LONG_ID));
    let mut files = vec![
        "Card.dc.html",
        "Main.dc.html",
        "assets/logo.png",
        "canvas.json",
        "ds/store/tokens.json",
        long.as_str(),
    ];
    files.sort();
    assert_eq!(artifact.versions[0].files, files);
    assert_eq!(events.try_recv().unwrap().data["kind"], "design");
}

/// ARTIFACTS-2: obid's list of frames and Swift's `{"frames": […]}` are both a `canvas`; a spoiled
/// design is refused saying what is wrong, and a link in one is refused as in any folder.
#[tokio::test]
async fn artifacts_2_both_canvas_shapes_and_a_spoiled_design_refused() {
    let t = Temp::new("a2");
    let frame = r#"{"title": "A", "file": "a.html", "x": 0, "y": 0, "w": 390, "h": 844}"#;
    t.write("obid/canvas.json", format!("[{frame}]"));
    t.write("obid/a.html", "<p>a</p>");
    t.write("swift/canvas.json", format!(r#"{{"frames": [{frame}]}}"#));
    t.write("swift/a.html", "<p>a</p>");
    for spoiled in [
        "v", "missing", "board", "two", "order", "page", "launch", "system", "script",
    ] {
        design(&t, spoiled);
    }
    let spoil = |folder: &str, from: &str, to: &str| {
        let path = t.room().join(folder).join("canvas.json");
        let text = std::fs::read_to_string(&path).unwrap().replace(from, to);
        std::fs::write(path, text).unwrap();
    };
    spoil("v", "\"v\": 3", "\"v\": 2");
    spoil("order", "\"order\": [", "\"order\": [\n  \"Gone.dc.html\",");
    spoil("page", "\"id\": \"design\"", "\"id\": \"no spaces\"");
    spoil(
        "launch",
        "\"file\": \"Main.dc.html\"",
        "\"file\": \"Gone.dc.html\"",
    );
    spoil(
        "system",
        "\"namespace\": \"store\"",
        "\"namespace\": \"other\"",
    );
    std::fs::remove_file(t.room().join("missing/Card.dc.html")).unwrap();
    t.write("board/Card.dc.html", "<p>no x-dc</p>");
    let card = board("Card", "<p>Card</p>");
    let logic = &card[card.find("<script").unwrap()..];
    t.write(
        "two/Card.dc.html",
        card.replace("</body>", &format!("{logic}</body>")),
    );
    t.write(
        "script/Card.dc.html",
        board("Card", "<p>Card</p>").replace("data-dc-script", ""),
    );
    design(&t, "linked");
    std::os::unix::fs::symlink(t.0.join("outside"), t.room().join("linked/out.css")).unwrap();
    let (store, _) = t.store();
    let room = t.room();
    for shape in ["obid", "swift"] {
        let (id, _) = store.publish(what(&room, shape)).await.unwrap();
        assert_eq!(store.get(&id).unwrap().kind, Kind::Canvas, "{shape}");
    }
    for (folder, why) in [
        (
            "v",
            "canvas.json's v is 3 in Claude Design's format, and here it is 2.",
        ),
        (
            "missing",
            "canvas.json's board \"Card.dc.html\" isn't a file of the design.",
        ),
        ("board", "Card.dc.html has no <x-dc>."),
        (
            "two",
            "Card.dc.html has 2 <script type=\"text/x-dc\" data-dc-script> blocks, and a board has one.",
        ),
        (
            "order",
            "canvas.json's order names \"Gone.dc.html\", which isn't one of its boards.",
        ),
        (
            "page",
            "canvas.json's page id \"no spaces\" isn't 1 to 40 letters, digits, _ or -.",
        ),
        (
            "launch",
            "canvas.json's launch names \"Gone.dc.html\", which isn't one of its pages or boards.",
        ),
        (
            "system",
            "canvas.json's design system \"other\" has no ds/other/ folder.",
        ),
        (
            "script",
            "Card.dc.html has 0 <script type=\"text/x-dc\" data-dc-script> blocks, and a board has one.",
        ),
        ("linked", "design/out.css is a link"),
    ] {
        let why_not = store.publish(what(&room, folder)).await.unwrap_err();
        assert!(
            why_not.starts_with("Not published: ")
                && why_not.contains(&why.replace("design/", &format!("{folder}/"))),
            "{folder}: {why_not}"
        );
    }
    assert_eq!(store.list(None, None).len(), 2);
}

/// ARTIFACTS-3: `{version}/_blob/<id>` serves the upload that version keeps, and nothing for an id
/// it doesn't keep, another version's included.
#[tokio::test]
async fn artifacts_3_an_upload_is_served_at_its_alias_inside_its_version() {
    let t = Temp::new("a3");
    design(&t, "design");
    let (store, _) = t.store();
    let room = t.room();
    let (id, _) = store.publish(what(&room, "design")).await.unwrap();
    let (path, bytes) = store.file(&id, "1", "_blob/logo").unwrap();
    assert_eq!((bytes.as_slice(), type_of(&path)), (PNG, "image/png"));
    assert_eq!(
        store.file(&id, "1", &format!("_blob/{LONG_ID}")).unwrap().1,
        PNG
    );
    assert!(store.file(&id, "1", "_blob/other").is_none());
    assert!(store.file(&id, "1", "_blob/../canvas.json").is_none());
    std::fs::remove_file(room.join("design/_blob/logo")).unwrap();
    let again = Publish {
        artifact: Some(&id),
        ..what(&room, "design")
    };
    assert_eq!(store.publish(again).await.unwrap().1, 2);
    assert!(store.file(&id, "2", "_blob/logo").is_none());
    assert!(store.file(&id, "1", "_blob/logo").is_some());
}

/// ARTIFACTS-4: the fonts, video and AVIF a design needs are served as what they are.
#[test]
fn artifacts_4_fonts_video_and_avif_have_their_types() {
    for (name, wanted) in [
        ("a.ttf", "font/ttf"),
        ("a.otf", "font/otf"),
        ("a.woff2", "font/woff2"),
        ("a.mp4", "video/mp4"),
        ("a.webm", "video/webm"),
        ("a.avif", "image/avif"),
        ("a.jsx", "application/octet-stream"),
    ] {
        assert_eq!(type_of(Path::new(name)), wanted, "{name}");
    }
}

/// ARTIFACTS-5: an upload whose id looks like a token is kept under the first 16 hex of its
/// SHA-256, so the design publishes; a file of the agent's own named so is still refused.
#[tokio::test]
async fn artifacts_5_a_token_like_upload_id_is_kept_hashed() {
    let t = Temp::new("a5");
    design(&t, "design");
    design(&t, "named");
    t.write(&format!("named/{LONG_ID}.png"), PNG);
    let (store, _) = t.store();
    let room = t.room();
    let (id, _) = store.publish(what(&room, "design")).await.unwrap();
    let kept = &store.get(&id).unwrap().versions[0];
    assert_eq!(
        kept.blobs[LONG_ID],
        format!("assets/{}.png", hashed(LONG_ID))
    );
    assert!(kept.files.iter().all(|f| !f.contains(LONG_ID)));
    assert_eq!(
        store.publish(what(&room, "named")).await.unwrap_err(),
        NAME_REFUSED
    );
}

/// ARTIFACTS-6: a rename sends `artifact` with `renamed`, its new title and what it belongs to.
#[tokio::test]
async fn artifacts_6_a_rename_says_so() {
    let t = Temp::new("a6");
    t.write("page/index.html", "<p>x</p>");
    let (store, mut events) = t.store();
    let room = t.room();
    let (id, _) = store.publish(what(&room, "page")).await.unwrap();
    events.try_recv().unwrap();
    store.rename(&id, "Pricing").unwrap().unwrap();
    assert_eq!(
        events.try_recv().unwrap().data,
        json!({"id": id, "renamed": true, "title": "Pricing", "kind": "html", "conversation": "c1", "job": null})
    );
}

/// ARTIFACTS-7: a follow-up job of the conversation gives its artifact a new version, the version
/// saying which job; another conversation can't, and a job's artifact with no conversation gets
/// versions only from that job.
#[tokio::test]
async fn artifacts_7_a_follow_up_job_of_the_conversation_versions_it() {
    let t = Temp::new("a7");
    t.write("page/index.html", "<p>one</p>");
    let (store, _) = t.store();
    let room = t.room();
    let (id, _) = store.publish(what(&room, "page")).await.unwrap();
    t.write("page/index.html", "<p>two</p>");
    let follow_up = Publish {
        job: Some("j2"),
        artifact: Some(&id),
        ..what(&room, "page")
    };
    assert_eq!(store.publish(follow_up).await.unwrap(), (id.clone(), 2));
    let artifact = store.get(&id).unwrap();
    assert_eq!(artifact.job, None);
    assert_eq!(artifact.versions[1].job.as_deref(), Some("j2"));
    // The conversation makes version 3; the job publishing its own files again, as its end may,
    // gets its own version back and nothing new over the conversation's.
    t.write("page/index.html", "<p>three</p>");
    let conversation = Publish {
        artifact: Some(&id),
        ..what(&room, "page")
    };
    assert_eq!(store.publish(conversation).await.unwrap(), (id.clone(), 3));
    t.write("page/index.html", "<p>two</p>");
    let again = Publish {
        job: Some("j2"),
        artifact: Some(&id),
        kept: Some(2),
        ..what(&room, "page")
    };
    assert_eq!(store.publish(again).await.unwrap(), (id.clone(), 2));
    assert_eq!(store.get(&id).unwrap().versions.len(), 3);
    let elsewhere = Publish {
        conversation: Some("c2"),
        artifact: Some(&id),
        ..what(&room, "page")
    };
    let why = store.publish(elsewhere).await.unwrap_err();
    assert!(why.contains("belongs to another conversation"), "{why}");
    let alone = Publish {
        conversation: None,
        job: Some("j1"),
        ..what(&room, "page")
    };
    let (lone, _) = store.publish(alone).await.unwrap();
    t.write("page/index.html", "<p>three</p>");
    let other_job = Publish {
        conversation: None,
        job: Some("j3"),
        artifact: Some(&lone),
        ..what(&room, "page")
    };
    assert!(store.publish(other_job).await.is_err());
}

/// ARTIFACTS-8: Claude's limits: a text file 16 MB, a binary 15 MB, an SVG 2 MB, a version 512
/// files and 256 MB, each refusal saying what is over and by how much.
#[tokio::test]
async fn artifacts_8_a_version_holds_claudes_limits() {
    const MB: u64 = 1024 * 1024;
    let t = Temp::new("a8");
    let sized = |path: &str, bytes: u64| {
        t.write(path, "");
        std::fs::File::create(t.room().join(path))
            .unwrap()
            .set_len(bytes)
            .unwrap();
    };
    t.write("text/index.html", "x");
    sized("text/big.js", 16 * MB + 1);
    t.write("fits/index.html", "x");
    sized("fits/big.js", 16 * MB);
    sized("fits/clip.mp4", 15 * MB);
    t.write("svg/index.html", "x");
    sized("svg/logo.svg", 2 * MB + 1);
    t.write("binary/index.html", "x");
    sized("binary/clip.mp4", 15 * MB + 1);
    t.write("version/index.html", "x");
    for i in 0..18 {
        sized(&format!("version/{i}.bin"), 15 * MB);
    }
    t.write("many/index.html", "x");
    for i in 0..MAX_FILES {
        t.write(&format!("many/{i}.txt"), "");
    }
    let (store, _) = t.store();
    let room = t.room();
    for (path, why) in [
        (
            "text",
            "big.js is 16 MB and 1 byte, and a text file may be 16 MB at most.",
        ),
        (
            "svg",
            "logo.svg is 2 MB and 1 byte, and an SVG may be 2 MB at most.",
        ),
        (
            "binary",
            "clip.mp4 is 15 MB and 1 byte, and a binary file (an image, a font, a video, a PDF) may be 15 MB at most.",
        ),
        (
            "version",
            "this version is more than 256 MB, the most a version may be.",
        ),
        (
            "many",
            "a version holds at most 512 files, and this holds 513.",
        ),
    ] {
        let why_not = store.publish(what(&room, path)).await.unwrap_err();
        assert!(why_not.contains(why), "{path}: {why_not}");
    }
    assert!(store.publish(what(&room, "fits")).await.is_ok());
    assert_eq!(store.list(None, None).len(), 1);
}
