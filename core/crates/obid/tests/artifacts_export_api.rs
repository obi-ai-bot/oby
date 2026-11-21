//! Export and import (docs/contracts/artifacts.md, ARTIFACTS-9 to 11; docs/contracts/signed-words.md,
//! WORDS-21), on the real obid binary: a Claude Design project zip imported with the owner's signed
//! words is a design of the owner's, titled from its `canvas.json`; its export is the import, entry
//! for entry and byte for byte, and again the same bytes; its standalone HTML holds every file and
//! reaches nothing outside itself; an unsigned import, or a zip with a link, a path out or a file
//! over its limit, keeps nothing.

mod support;

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde_json::Value;
use support::*;

const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR one pixel";
/// An upload id shaped like a token: kept under a hashed name, given back under its own.
const LONG_ID: &str = "Kq7Zt2mW9xR4vB8nL3pF6hJ1sD5gA0cYeU7iO2wT";

fn board(title: &str, body: &str) -> String {
    format!(
        "<!doctype html>\n<html><head><title>{title}</title></head><body>\n<x-dc>\n{body}\n</x-dc>\n<script type=\"text/x-dc\" data-dc-script>\nclass Component extends DCLogic {{}}\n</script>\n</body></html>\n"
    )
}

/// Claude Design's project zip: `project/` with `canvas.json` (its own spacing), two boards, a
/// design system and two uploads, one with a token-like id. Stored, not deflated, as Claude's own
/// zips may be: obid gives back the entries, not the zip's bytes.
fn project_zip() -> Vec<u8> {
    let canvas = "{\n \"v\": 3,\n \"title\": \"Store design\",\n \"launch\": {\"view\": \"focused\", \"file\": \"Main.dc.html\"},\n \"pages\": [{\"id\": \"design\", \"name\": \"Design\"}],\n \"boards\": {\"Main.dc.html\": {\"x\": 0, \"y\": 0, \"w\": 800, \"h\": 600, \"page\": \"design\"}, \"Card.dc.html\": {\"x\": 900, \"y\": 0, \"w\": 400, \"h\": 300, \"page\": \"design\"}},\n \"order\": [\"Main.dc.html\", \"Card.dc.html\"],\n \"designSystems\": [{\"title\": \"Store\", \"namespace\": \"store\", \"version\": \"1\"}]\n}\n";
    let main = board(
        "Main",
        &format!(
            "<link rel=\"stylesheet\" href=\"ds/store/site.css\">\n<img src=\"/_blob/{LONG_ID}\">\n<img src=\"/_blob/logo\">\n<dc-import name=\"Card\"></dc-import>"
        ),
    );
    zip_of(&[
        ("project/canvas.json", canvas.as_bytes()),
        ("project/Main.dc.html", main.as_bytes()),
        (
            "project/Card.dc.html",
            board("Card", "<p>Card</p>").as_bytes(),
        ),
        ("project/ds/store/site.css", b"body { color: #222 }"),
        ("project/ds/store/tokens.json", b"{\"ink\":\"#ececec\"}"),
        ("project/_blob/logo", PNG),
        (&format!("project/_blob/{LONG_ID}"), PNG),
    ])
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    zip_with(entries, &[])
}

/// A zip of `entries`, those named in `links` links to what their bytes say.
fn zip_with(entries: &[(&str, &[u8])], links: &[&str]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .unix_permissions(0o644);
        if links.contains(name) {
            let target = String::from_utf8_lossy(bytes).into_owned();
            w.add_symlink(*name, target, options).unwrap();
        } else {
            w.start_file(*name, options).unwrap();
            w.write_all(bytes).unwrap();
        }
    }
    w.finish().unwrap().into_inner()
}

/// A zip's entries and their bytes.
fn entries_of(zip: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(zip)).unwrap();
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).unwrap();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        out.push((entry.name().to_owned(), bytes));
    }
    out.sort();
    out
}

/// What obid answered: its status, its head (lower case) and its body's bytes.
struct Raw {
    status: u16,
    head: String,
    body: Vec<u8>,
}

impl Raw {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or(Value::Null)
    }
    fn header(&self, name: &str) -> Option<&str> {
        self.head
            .lines()
            .find_map(|l| l.strip_prefix(&format!("{name}: ")))
    }
}

/// `X-Oby-Words` over `body`'s bytes, as the app signs an import (WORDS-21).
fn words_over(obid: &Obid, method: &str, path: &str, body: &[u8]) -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_millis();
    let whole = obi_protocol::time::format(obi_protocol::time::now());
    let signed_at = format!("{}.{millis:03}Z", &whole[..19]);
    let digest = hex::encode(obi_protocol::keys::sha256(body));
    let signed = owner_signs(
        obid,
        &format!("oby-words\n{method}\n{path}\n{digest}\n{signed_at}"),
        &signed_at,
    );
    format!(
        "{} {signed_at} {}",
        signed["device"].as_str().unwrap(),
        signed["signature"].as_str().unwrap()
    )
}

/// One request with obid's token, `body` as bytes, read whole (chunks joined).
fn send(obid: &Obid, method: &str, target: &str, headers: &[(&str, &str)], body: &[u8]) -> Raw {
    let mut stream = TcpStream::connect(("127.0.0.1", obid.port)).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(120)))
        .unwrap();
    let mut head = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: Bearer {TOKEN}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(body).unwrap();
    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    let end = raw.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(raw[..end].to_vec())
        .unwrap()
        .to_ascii_lowercase();
    let mut rest = raw[end + 4..].to_vec();
    if head.contains("transfer-encoding: chunked") {
        let mut out = Vec::new();
        loop {
            let line = rest.windows(2).position(|w| w == b"\r\n").unwrap();
            let size =
                usize::from_str_radix(std::str::from_utf8(&rest[..line]).unwrap().trim(), 16)
                    .unwrap();
            if size == 0 {
                break;
            }
            out.extend_from_slice(&rest[line + 2..line + 2 + size]);
            rest = rest[line + 4 + size..].to_vec();
        }
        rest = out;
    }
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Raw {
        status,
        head,
        body: rest,
    }
}

/// `bytes` imported as `content_type`, signed by the owner's device over them.
fn import(obid: &Obid, content_type: &str, bytes: &[u8]) -> Raw {
    let words = words_over(obid, "POST", "/v1/artifacts/import", bytes);
    send(
        obid,
        "POST",
        "/v1/artifacts/import",
        &[("Content-Type", content_type), ("X-Oby-Words", &words)],
        bytes,
    )
}

fn export(obid: &Obid, id: &str, version: u32, what: &str) -> Raw {
    send(
        obid,
        "GET",
        &format!("/v1/artifacts/{id}/export/{version}?as={what}"),
        &[],
        b"",
    )
}

fn a_running_obid(name: &str) -> (Folder, Obid) {
    let folder = Folder::new(name);
    let remote = fixture(&folder.0.join("fixture"));
    let obid = start(&folder.0, &config(&folder.0, &remote, &stub_agent()), None);
    (folder, obid)
}

fn how_many(obid: &Obid) -> usize {
    call(obid, "GET", "/v1/artifacts", None)
        .body
        .as_array()
        .map_or(0, Vec::len)
}

/// WORDS-21: an import is taken only with the owner's words signed over its exact bytes; with no
/// signature, or one over other bytes, it is refused and nothing is kept.
#[test]
fn words_21_an_import_is_taken_only_signed() {
    let (_folder, obid) = a_running_obid("artifacts-words-21");
    let zip = project_zip();
    let unsigned = send(
        &obid,
        "POST",
        "/v1/artifacts/import",
        &[("Content-Type", "application/zip")],
        &zip,
    );
    assert_eq!(
        unsigned.status,
        403,
        "{}",
        String::from_utf8_lossy(&unsigned.body)
    );
    let over_other_bytes = words_over(&obid, "POST", "/v1/artifacts/import", b"other bytes");
    let forged = send(
        &obid,
        "POST",
        "/v1/artifacts/import",
        &[
            ("Content-Type", "application/zip"),
            ("X-Oby-Words", &over_other_bytes),
        ],
        &zip,
    );
    assert_eq!(forged.status, 403);
    assert_eq!(how_many(&obid), 0);
    let signed = import(&obid, "application/zip", &zip);
    assert_eq!(
        signed.status,
        200,
        "{}",
        String::from_utf8_lossy(&signed.body)
    );
    assert_eq!(how_many(&obid), 1);
}

/// ARTIFACTS-9: a Claude Design project zip's export is the import, entry for entry and byte for
/// byte (each upload back at its own `_blob/<id>`), and a second import and export gives the same
/// zip bytes. A file of a version named `export` is still served as a file.
#[test]
fn artifacts_9_an_imports_export_is_the_import() {
    let (_folder, obid) = a_running_obid("artifacts-9");
    let zip = project_zip();
    let imported = import(&obid, "application/zip", &zip);
    assert_eq!(
        imported.status,
        200,
        "{}",
        String::from_utf8_lossy(&imported.body)
    );
    let id = imported.json()["id"].as_str().unwrap().to_owned();
    let out = export(&obid, &id, 1, "zip");
    assert_eq!(out.status, 200, "{}", String::from_utf8_lossy(&out.body));
    assert_eq!(out.header("content-type"), Some("application/zip"));
    assert_eq!(
        out.header("content-disposition"),
        Some("attachment; filename=\"store-design-v1.zip\"")
    );
    assert_eq!(entries_of(&out.body), entries_of(&zip));
    let again = import(&obid, "application/zip", &out.body);
    assert_eq!(again.status, 200);
    let second = export(&obid, again.json()["id"].as_str().unwrap(), 1, "zip");
    assert_eq!(second.body, out.body, "the export is deterministic");
    // No such version, no such kind of export.
    assert_eq!(export(&obid, &id, 2, "zip").status, 404);
    assert_eq!(export(&obid, &id, 1, "pdf").status, 400);
    // A page with a file named `export` keeps it served at its own path.
    let page = zip_of(&[("index.html", b"<p>page</p>"), ("export", b"plain")]);
    let page = import(&obid, "application/zip", &page);
    let page_id = page.json()["id"].as_str().unwrap().to_owned();
    let file = send(
        &obid,
        "GET",
        &format!("/v1/artifacts/{page_id}/1/export"),
        &[],
        b"",
    );
    assert_eq!((file.status, file.body.as_slice()), (200, &b"plain"[..]));
}

/// ARTIFACTS-10: the standalone HTML of a design holds every file of the version and reaches
/// nothing outside itself; a page's is the page with its own files inlined.
#[test]
fn artifacts_10_the_html_export_holds_every_file() {
    let (_folder, obid) = a_running_obid("artifacts-10");
    let imported = import(&obid, "application/zip", &project_zip());
    let artifact = imported.json();
    let id = artifact["id"].as_str().unwrap();
    let out = export(&obid, id, 1, "html");
    assert_eq!(out.status, 200);
    assert_eq!(out.header("content-type"), Some("text/html; charset=utf-8"));
    let html = String::from_utf8(out.body.clone()).unwrap();
    let manifest = html
        .split("<script type=\"application/json\" id=\"oby-artifact-files\">")
        .nth(1)
        .and_then(|rest| rest.split("</script>").next())
        .map(|m| serde_json::from_str::<Value>(&m.replace("<\\/", "</")).unwrap())
        .unwrap();
    let mut held: Vec<&str> = manifest["files"]
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    held.sort();
    let mut files: Vec<&str> = artifact["versions"][0]["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    files.sort();
    assert_eq!(held, files);
    assert_eq!(outside(&html), Vec::<String>::new());
    let page = zip_of(&[
        (
            "index.html",
            b"<link href=\"site.css\"><img src=\"logo.png\">",
        ),
        ("site.css", b"body { background: url(logo.png) }"),
        ("logo.png", PNG),
    ]);
    let page = import(&obid, "application/zip", &page);
    let page = export(&obid, page.json()["id"].as_str().unwrap(), 1, "html");
    let page = String::from_utf8(page.body).unwrap();
    assert_eq!(outside(&page), Vec::<String>::new(), "{page}");
    assert_eq!(page.matches("data:").count(), 2, "{page}");
}

/// Every `src`, `href` and `url(…)` that isn't a `data:` URL or a `#` in the page.
fn outside(html: &str) -> Vec<String> {
    let re =
        regex::Regex::new(r#"(?i)\b(?:src|href)\s*=\s*"([^"]*)"|url\(\s*['"]?([^'")]*)"#).unwrap();
    re.captures_iter(html)
        .filter_map(|c| c.get(1).or(c.get(2)).map(|m| m.as_str().to_string()))
        .filter(|r| !r.is_empty() && !r.starts_with("data:") && !r.starts_with('#'))
        .collect()
}

/// ARTIFACTS-11: an import is the owner's (no conversation, no job), `origin` the import, titled
/// from its `canvas.json`; a page and a document import too; a zip with a link, a path out or a
/// file over its limit is refused saying which, and nothing is kept.
#[test]
fn artifacts_11_an_import_is_the_owners_and_a_bad_zip_is_refused() {
    let (_folder, obid) = a_running_obid("artifacts-11");
    let design = import(&obid, "application/zip", &project_zip()).json();
    assert_eq!(design["kind"], "design");
    assert_eq!(design["title"], "Store design");
    assert_eq!(design["conversation"], Value::Null);
    assert_eq!(design["job"], Value::Null);
    assert_eq!(design["origin"]["kind"], "import");
    assert_eq!(design["origin"]["from"], "claude-design-zip");
    assert_eq!(design["versions"][0]["blobs"]["logo"], "assets/logo.png");
    let page = import(&obid, "text/html", b"<h1>Hi</h1>").json();
    assert_eq!(
        (&page["kind"], &page["title"], &page["origin"]["from"]),
        (&"html".into(), &"Imported".into(), &"page".into())
    );
    let document = import(&obid, "text/markdown", b"# Notes").json();
    assert_eq!(
        (&document["kind"], &document["origin"]["from"]),
        (&"markdown".into(), &"document".into())
    );
    let kept = how_many(&obid);
    let svg = vec![b' '; 2 * 1024 * 1024 + 1];
    for (zip, says) in [
        (
            zip_with(
                &[("index.html", b"<p>"), ("away", b"/etc/hosts")],
                &["away"],
            ),
            "\"away\" is a link",
        ),
        (
            zip_of(&[("index.html", b"<p>"), ("../escape.html", b"x")]),
            "\"../escape.html\" isn't a plain path inside the zip",
        ),
        (
            zip_of(&[("index.html", b"<p>"), ("/etc/escape.html", b"x")]),
            "\"/etc/escape.html\" isn't a plain path inside the zip",
        ),
        (
            zip_of(&[("index.html", b"<p>"), ("big.svg", &svg)]),
            "big.svg is 2 MB and 1 byte, and an SVG may be 2 MB at most.",
        ),
        (b"not a zip at all".to_vec(), "that isn't a zip"),
    ] {
        let refused = import(&obid, "application/zip", &zip);
        assert_eq!(refused.status, 400, "{says}");
        let body = refused.json();
        assert_eq!(body["reason"], "bad_import");
        assert!(
            body["error"]
                .as_str()
                .unwrap()
                .starts_with("Not published: ")
                && body["error"].as_str().unwrap().contains(says),
            "{body}"
        );
    }
    assert_eq!(how_many(&obid), kept, "nothing refused was kept");
}
