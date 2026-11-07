//! Deterministic Ollama client for `misaka ask`, independent of consensus PoW.

use std::io::{Read, Write};
use std::time::Duration;

pub const PALW_OLLAMA_MODEL_ENV: &str = "MISAKA_PALW_OLLAMA_MODEL";
pub const PALW_OLLAMA_URL_ENV: &str = "MISAKA_PALW_OLLAMA_URL";
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";

#[derive(Debug)]
pub enum OllamaError {
    Unavailable(String),
    RequestFailed(String),
}

impl std::fmt::Display for OllamaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable(message) | Self::RequestFailed(message) => f.write_str(message),
        }
    }
}

fn timeout() -> Duration {
    Duration::from_secs(std::env::var("MISAKA_ASK_TIMEOUT_SECS").ok().and_then(|s| s.parse().ok()).unwrap_or(300))
}

pub fn generate(
    url: &str,
    model: &str,
    prompt: &str,
    num_predict: u32,
    templated: bool,
    think: Option<bool>,
) -> Result<(String, u32, u32), OllamaError> {
    let mut body = serde_json::json!({
        "model": model,
        "prompt": prompt,
        "raw": !templated,
        "stream": false,
        "options": {
            "temperature": 0.0,
            "num_predict": num_predict,
            "num_ctx": 4096,
            "seed": 0,
            "num_gpu": 0,
        },
    });
    // Preserve the receipt request: omit think when the caller has not chosen a mode.
    if let Some(think) = think {
        body["think"] = serde_json::Value::Bool(think);
    }
    let body = body.to_string();
    let response = http_request("POST", url, "/api/generate", Some(&body), timeout())?;
    let doc: serde_json::Value =
        serde_json::from_slice(&response).map_err(|e| OllamaError::RequestFailed(format!("cannot parse the Ollama response: {e}")))?;
    if let Some(err) = doc.get("error").and_then(|v| v.as_str()) {
        return Err(OllamaError::RequestFailed(format!(
            "Ollama refused the generate request: {err} (is model {model} present? `ollama list`)"
        )));
    }
    let text = doc
        .get("response")
        .and_then(|v| v.as_str())
        .ok_or_else(|| OllamaError::RequestFailed("Ollama response lacks the `response` field".into()))?;
    Ok((
        text.to_owned(),
        doc.get("prompt_eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
        doc.get("eval_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
    ))
}

fn http_request(method: &str, base_url: &str, path: &str, body: Option<&str>, budget: Duration) -> Result<Vec<u8>, OllamaError> {
    use std::net::TcpStream;
    let hostport = base_url
        .strip_prefix("http://")
        .ok_or_else(|| OllamaError::Unavailable(format!("{PALW_OLLAMA_URL_ENV} must be http://host:port, got {base_url}")))?
        .trim_end_matches('/');
    let mut stream = TcpStream::connect(hostport).map_err(|e| {
        OllamaError::Unavailable(format!("cannot reach the Ollama server at {hostport}: {e} (is `ollama serve` running?)"))
    })?;
    stream.set_read_timeout(Some(budget)).ok();
    stream.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let request = match body {
        Some(body) => format!(
            "{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
        None => format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n"),
    };
    stream.write_all(request.as_bytes()).map_err(|e| OllamaError::RequestFailed(format!("cannot send the Ollama request: {e}")))?;
    let mut raw = Vec::new();
    stream
        .read_to_end(&mut raw)
        .map_err(|e| OllamaError::RequestFailed(format!("reading the Ollama response failed (budget {budget:?}): {e}")))?;
    let header_end = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| OllamaError::RequestFailed("malformed HTTP response from Ollama (no header end)".into()))?;
    let (head, rest) = raw.split_at(header_end + 4);
    let head_text = String::from_utf8_lossy(head);
    let status = head_text.lines().next().unwrap_or_default().to_string();
    if !status.contains(" 200 ") {
        let tail: String = String::from_utf8_lossy(rest).chars().take(300).collect();
        return Err(OllamaError::RequestFailed(format!("Ollama answered {status}: {tail}")));
    }
    let chunked = head_text.to_ascii_lowercase().contains("transfer-encoding: chunked");
    if !chunked {
        return Ok(rest.to_vec());
    }
    // De-chunk: size lines are hex; a 0-size chunk terminates.
    let mut out = Vec::with_capacity(rest.len());
    let mut i = 0;
    while i < rest.len() {
        let line_end = match rest[i..].windows(2).position(|w| w == b"\r\n") {
            Some(p) => i + p,
            None => break,
        };
        let size = usize::from_str_radix(String::from_utf8_lossy(&rest[i..line_end]).trim(), 16)
            .map_err(|_| OllamaError::RequestFailed("malformed chunk size from Ollama".into()))?;
        if size == 0 {
            break;
        }
        let start = line_end + 2;
        let end = (start + size).min(rest.len());
        out.extend_from_slice(&rest[start..end]);
        i = end + 2;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn ask_preserves_receipt_request_options_and_counts() {
        for (templated, think) in [(false, None), (true, Some(true))] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                let mut raw = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    let n = stream.read(&mut buf).unwrap();
                    assert!(n > 0);
                    raw.extend_from_slice(&buf[..n]);
                    if let Some(end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                        let header = String::from_utf8_lossy(&raw[..end]);
                        let size: usize =
                            header.lines().find_map(|line| line.strip_prefix("Content-Length: ")).unwrap().parse().unwrap();
                        if raw.len() >= end + 4 + size {
                            assert!(header.starts_with("POST /api/generate HTTP/1.1"));
                            let body: serde_json::Value = serde_json::from_slice(&raw[end + 4..end + 4 + size]).unwrap();
                            assert_eq!(body["model"], "local-model");
                            assert_eq!(body["prompt"], "a question");
                            assert_eq!(body["raw"], !templated);
                            assert_eq!(body["stream"], false);
                            assert_eq!(
                                body["options"],
                                serde_json::json!({"temperature":0.0,"num_predict":32,"num_ctx":4096,"seed":0,"num_gpu":0})
                            );
                            assert_eq!(body.get("think"), think.map(serde_json::Value::Bool).as_ref());
                            break;
                        }
                    }
                }
                let body = r#"{"response":"an answer","prompt_eval_count":5,"eval_count":2}"#;
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            });
            assert_eq!(generate(&url, "local-model", "a question", 32, templated, think).unwrap(), ("an answer".to_owned(), 5, 2));
            server.join().unwrap();
        }
    }
}
