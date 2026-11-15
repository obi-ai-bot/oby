//! HTTP/1.1 straight to obid's loopback address: no proxy is read, no redirect followed, and every
//! call goes through `Client::send`, which reads the token again once on a `401`.

use crate::endpoint::Endpoint;
use crate::exit::Exit;
use crate::token::{self, Token};
use crate::Fail;
use serde_json::Value;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::sync::Mutex;

const MAX_HEAD: usize = 64 * 1024;

/// What obid answered.
pub struct Answer {
    pub status: u16,
    pub body: Vec<u8>,
}

impl Answer {
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// The body as text, without the whitespace `/wait` sends while it waits.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).trim().to_string()
    }

    pub fn json(&self) -> Value {
        serde_json::from_str(&self.text()).unwrap_or(Value::Null)
    }

    /// obid's own `error` line, for an answer that isn't a 2xx: exit 1.
    pub fn refused(&self) -> Fail {
        let said = self
            .json()
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string);
        let said = said.unwrap_or_else(|| "no reason given".to_string());
        Fail::new(
            Exit::Failed,
            format!("obid answered {}: {said}", self.status),
        )
    }
}

/// obid at a loopback address, with the token read from `token_path`.
pub struct Client {
    endpoint: Endpoint,
    token_path: PathBuf,
    token: Mutex<Token>,
}

impl Client {
    pub fn new(endpoint: Endpoint, token_path: PathBuf) -> Result<Client, Fail> {
        let token = token::read(&token_path).map_err(|why| Fail::new(Exit::Unreachable, why))?;
        Ok(Client {
            endpoint,
            token_path,
            token: Mutex::new(token),
        })
    }

    /// One call. On a `401` the token file is read again under the same rules and the call sent once
    /// more (obid rotated it); a second `401` is exit 5.
    pub fn send(&self, method: &str, path: &str, body: Option<&[u8]>) -> Result<Answer, Fail> {
        let token = self.token.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let answer = self.exchange(method, path, body, &token)?;
        if answer.status != 401 {
            return Ok(answer);
        }
        let fresh =
            token::read(&self.token_path).map_err(|why| Fail::new(Exit::Unreachable, why))?;
        *self.token.lock().unwrap_or_else(|e| e.into_inner()) = fresh.clone();
        let answer = self.exchange(method, path, body, &fresh)?;
        if answer.status == 401 {
            let why = format!("obid refused the token in {}", self.token_path.display());
            return Err(Fail::new(Exit::Unreachable, why));
        }
        Ok(answer)
    }

    fn exchange(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        token: &Token,
    ) -> Result<Answer, Fail> {
        exchange(&self.endpoint, method, path, body, token).map_err(|e| {
            let why = format!(
                "obid isn't answering at http://{}: {e}",
                self.endpoint.authority()
            );
            Fail::new(Exit::Unreachable, why)
        })
    }
}

fn exchange(
    endpoint: &Endpoint,
    method: &str,
    path: &str,
    body: Option<&[u8]>,
    token: &Token,
) -> io::Result<Answer> {
    let mut stream = TcpStream::connect(endpoint.addr())?;
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nAuthorization: {}\r\nAccept: application/json\r\nConnection: close\r\n",
        endpoint.authority(),
        token.bearer()
    )
    .into_bytes();
    if let Some(body) = body {
        request.extend_from_slice(
            format!(
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )
            .as_bytes(),
        );
    } else if method == "POST" {
        request.extend_from_slice(b"Content-Length: 0\r\n");
    }
    request.extend_from_slice(b"\r\n");
    request.extend_from_slice(body.unwrap_or_default());
    stream.write_all(&request)?;
    read_answer(BufReader::new(stream))
}

fn broken(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, what.to_string())
}

fn line(reader: &mut impl BufRead, read: &mut usize) -> io::Result<String> {
    let mut line = String::new();
    let n = reader
        .by_ref()
        .take((MAX_HEAD - *read) as u64 + 1)
        .read_line(&mut line)?;
    *read += n;
    if n == 0 || *read > MAX_HEAD {
        return Err(broken("the connection ended mid-answer"));
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_string())
}

fn read_answer(mut reader: impl BufRead) -> io::Result<Answer> {
    let mut read = 0;
    let status_line = line(&mut reader, &mut read)?;
    let status = status_line
        .strip_prefix("HTTP/1.")
        .and_then(|rest| rest.get(2..5))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| broken("not an HTTP answer"))?;
    let (mut length, mut chunked) = (None, false);
    loop {
        let header = line(&mut reader, &mut read)?;
        if header.is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = Some(
                value
                    .parse::<u64>()
                    .map_err(|_| broken("a bad Content-Length"))?,
            );
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.to_ascii_lowercase().contains("chunked");
        }
    }
    let mut body = Vec::new();
    if chunked {
        loop {
            let size = line(&mut reader, &mut 0)?;
            let size = size.split(';').next().unwrap_or_default().trim();
            let size = u64::from_str_radix(size, 16).map_err(|_| broken("a bad chunk"))?;
            if size == 0 {
                while !line(&mut reader, &mut 0)?.is_empty() {}
                break;
            }
            let start = body.len();
            reader.by_ref().take(size).read_to_end(&mut body)?;
            if (body.len() - start) as u64 != size {
                return Err(broken("the connection ended mid-answer"));
            }
            line(&mut reader, &mut 0)?;
        }
    } else if let Some(length) = length {
        reader.take(length).read_to_end(&mut body)?;
        if body.len() as u64 != length {
            return Err(broken("the connection ended mid-answer"));
        }
    } else {
        reader.read_to_end(&mut body)?;
    }
    Ok(Answer { status, body })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chunked_answer_is_put_together() {
        let raw = b"HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\n\r\n1\r\n \r\n1;x=y\r\n \r\nb\r\n{\"id\":\"j1\"}\r\n0\r\n\r\n";
        let answer = read_answer(&raw[..]).unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(answer.text(), "{\"id\":\"j1\"}");
    }

    #[test]
    fn a_sized_answer_and_a_short_one() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 17\r\n\r\n{\"error\":\"gone\"}\n";
        let answer = read_answer(&raw[..]).unwrap();
        assert_eq!((answer.status, answer.ok()), (404, false));
        assert_eq!(answer.refused().message, "obid answered 404: gone");
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 50\r\n\r\n{}";
        assert!(read_answer(&short[..]).is_err());
        assert!(read_answer(&b"SSH-2.0\r\n\r\n"[..]).is_err());
    }
}
