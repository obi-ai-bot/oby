//! A pinned download, checked before anything of it is used (BROWSER-23): the bytes are streamed
//! into a private file (0600, never executable) while their SHA-256 is computed, and a file whose
//! size or hash isn't its pin's is deleted, the answer naming both hashes. It writes only the file
//! it is given, and deletes it when it doesn't match.

use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use super::Pin;

/// Downloads `pin` into `to` and checks it against its pin. On any failure `to` is gone.
pub async fn fetch_verified(client: &reqwest::Client, pin: &Pin, to: &Path) -> Result<(), String> {
    let outcome = fetch(client, pin, to).await;
    if outcome.is_err() {
        let _ = std::fs::remove_file(to);
    }
    outcome
}

async fn fetch(client: &reqwest::Client, pin: &Pin, to: &Path) -> Result<(), String> {
    let failed = |why: String| format!("{}'s download from {} failed: {why}", pin.name, pin.url);
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(to)
            .map_err(|e| failed(format!("can't write {}: {e}", to.display())))?
    };
    let mut file = tokio::fs::File::from_std(file);
    let mut response = client
        .get(&pin.url)
        .send()
        .await
        .map_err(|e| failed(e.to_string()))?;
    if !response.status().is_success() {
        return Err(failed(format!("the server answered {}", response.status())));
    }
    let mut hash = Sha256::new();
    let mut got = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|e| failed(e.to_string()))? {
        got += chunk.len() as u64;
        if got > pin.bytes {
            return Err(format!(
                "{}'s download from {} is larger than its pinned {} bytes: it was deleted and nothing of it ran.",
                pin.name, pin.url, pin.bytes
            ));
        }
        hash.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| failed(format!("can't write {}: {e}", to.display())))?;
    }
    file.flush()
        .await
        .map_err(|e| failed(format!("can't write {}: {e}", to.display())))?;
    let sha256 = hex::encode(hash.finalize());
    if sha256 != pin.sha256 || got != pin.bytes {
        return Err(format!(
            "{}'s download from {} doesn't match its pin: its SHA-256 is {sha256} ({got} bytes), the pinned one is {} ({} bytes). It was deleted and nothing of it ran.",
            pin.name, pin.url, pin.sha256, pin.bytes
        ));
    }
    Ok(())
}

/// A file's SHA-256, lowercase hex, read in pieces.
pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hex::encode(hash.finalize()))
}
