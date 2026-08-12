//! The HTTP/1.1 client, driven over an in-memory stream.
//!
//! This is the only code in the product that writes bytes to a socket, so it is
//! tested at the byte level: what exactly goes out, and what exactly is
//! understood coming back. The [`Connector`] port makes that possible without a
//! listening socket, which also means these tests run on a machine with no
//! network at all.

use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use std::time::Duration;

use aura_cloud::http::{Connector, HttpTransport, Stream, Target};
use aura_cloud::provider::{HttpRequest, Transport};
use aura_core::AuraResult;
use parking_lot::Mutex;

/// A stream that replays a canned response and records what was written.
#[derive(Debug)]
struct Loopback {
    written: Arc<Mutex<Vec<u8>>>,
    to_read: Cursor<Vec<u8>>,
}

impl Read for Loopback {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.to_read.read(buffer)
    }
}

impl Write for Loopback {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.written.lock().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// A connector that hands out [`Loopback`] streams.
#[derive(Debug)]
struct Canned {
    response: Vec<u8>,
    written: Arc<Mutex<Vec<u8>>>,
}

impl Canned {
    fn new(response: &str) -> Self {
        Self {
            response: response.as_bytes().to_vec(),
            written: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn sent(&self) -> String {
        String::from_utf8_lossy(&self.written.lock()).to_string()
    }
}

impl Connector for Canned {
    fn connect(&self, _host: &str, _port: u16, _timeout: Duration) -> AuraResult<Box<dyn Stream>> {
        Ok(Box::new(Loopback {
            written: Arc::clone(&self.written),
            to_read: Cursor::new(self.response.clone()),
        }))
    }

    fn scheme(&self) -> &'static str {
        "http"
    }
}

fn request(url: &str) -> HttpRequest {
    HttpRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        headers: vec![
            ("content-type".to_string(), "application/json".to_string()),
            ("x-api-key".to_string(), "secret".to_string()),
        ],
        body: b"{\"ask\":1}".to_vec(),
    }
}

#[test]
fn a_url_is_split_into_its_parts() {
    let target = Target::parse("http://127.0.0.1:11434/v1/chat/completions").expect("parses");
    assert_eq!(target.scheme, "http");
    assert_eq!(target.host, "127.0.0.1");
    assert_eq!(target.port, 11434);
    assert_eq!(target.path, "/v1/chat/completions");
    assert_eq!(target.host_header(), "127.0.0.1:11434");

    let bare = Target::parse("https://api.anthropic.com").expect("parses");
    assert_eq!(bare.port, 443);
    assert_eq!(bare.path, "/");
    assert_eq!(
        bare.host_header(),
        "api.anthropic.com",
        "the default port is omitted"
    );
}

#[test]
fn a_url_with_a_password_in_it_is_refused() {
    let err = Target::parse("http://user:hunter2@example.test/v1").expect_err("refused");
    assert_eq!(err.code.0, "AURA-CLOUD-6003");
}

#[test]
fn a_url_with_no_scheme_is_refused() {
    assert!(Target::parse("api.anthropic.com/v1/messages").is_err());
}

#[test]
fn the_request_is_framed_correctly() {
    let canned = Arc::new(Canned::new(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 12\r\n\r\n{\"ok\":true}\n",
    ));
    let transport = HttpTransport::with_connector(Arc::clone(&canned) as Arc<dyn Connector>);
    let response = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect("a well-formed exchange");

    assert_eq!(response.status, 200);
    assert_eq!(response.header("content-type"), Some("application/json"));

    let sent = canned.sent();
    assert!(sent.starts_with("POST /v1/messages HTTP/1.1\r\n"), "{sent}");
    assert!(sent.contains("host: localhost:8080\r\n"), "{sent}");
    assert!(sent.contains("content-length: 9\r\n"), "{sent}");
    assert!(sent.contains("connection: close\r\n"), "{sent}");
    assert!(sent.contains("accept-encoding: identity\r\n"), "{sent}");
    assert!(sent.contains("x-api-key: secret\r\n"), "{sent}");
    assert!(sent.ends_with("{\"ask\":1}"), "{sent}");
}

#[test]
fn a_chunked_body_is_reassembled() {
    // Two chunks - `{"scene":` is 9 bytes, `"ritual"}` is 9 - then the zero
    // chunk. The second chunk header carries an extension, which is legal and
    // which a naive parser trips over.
    let canned = Arc::new(Canned::new(concat!(
        "HTTP/1.1 200 OK\r\n",
        "content-type: application/json\r\n",
        "transfer-encoding: chunked\r\n",
        "\r\n",
        "9\r\n",
        "{\"scene\":\r\n",
        "9;note=second\r\n",
        "\"ritual\"}\r\n",
        "0\r\n",
        "\r\n",
    )));
    let transport = HttpTransport::with_connector(Arc::clone(&canned) as Arc<dyn Connector>);
    let response = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect("a chunked exchange");

    assert_eq!(response.status, 200);
    assert_eq!(response.text(), "{\"scene\":\"ritual\"}");
}

#[test]
fn a_body_with_no_length_and_no_chunking_ends_with_the_connection() {
    let canned = Arc::new(Canned::new(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\r\n{\"scene\":\"venue\"}",
    ));
    let transport = HttpTransport::with_connector(Arc::clone(&canned) as Arc<dyn Connector>);
    let response = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect("read to end of stream");
    assert_eq!(response.text(), "{\"scene\":\"venue\"}");
}

#[test]
fn a_non_2xx_status_is_a_response_not_a_transport_error() {
    // Mapping a status is the provider's job. The transport's job is to deliver
    // whatever came back, so a 429 must arrive rather than raise.
    let canned = Arc::new(Canned::new(
        "HTTP/1.1 429 Too Many Requests\r\nretry-after: 7\r\ncontent-length: 2\r\n\r\n{}",
    ));
    let transport = HttpTransport::with_connector(Arc::clone(&canned) as Arc<dyn Connector>);
    let response = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect("a 429 is delivered");
    assert_eq!(response.status, 429);
    assert_eq!(response.retry_after_ms(), Some(7_000));
}

#[test]
fn an_https_url_is_refused_with_an_answerable_message() {
    let canned = Arc::new(Canned::new("HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n"));
    let transport = HttpTransport::with_connector(canned as Arc<dyn Connector>);
    let err = transport
        .send(
            &request("https://api.anthropic.com/v1/messages"),
            Duration::from_secs(1),
        )
        .expect_err("this build has no TLS");
    assert_eq!(err.code.0, "AURA-CLOUD-6003");
    assert!(err.detail.contains("TLS"), "{}", err.detail);
    assert!(err.detail.contains("ADR-0009"), "{}", err.detail);
}

#[test]
fn a_header_value_containing_a_newline_cannot_smuggle_a_second_request() {
    let canned = Arc::new(Canned::new("HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n"));
    let transport = HttpTransport::with_connector(Arc::clone(&canned) as Arc<dyn Connector>);

    let mut hostile = request("http://localhost:8080/v1/messages");
    hostile.headers.push((
        "x-note".to_string(),
        "ok\r\nGET /admin HTTP/1.1\r\nhost: localhost".to_string(),
    ));

    let err = transport
        .send(&hostile, Duration::from_secs(1))
        .expect_err("a split header is refused");
    assert_eq!(err.code.0, "AURA-CLOUD-6003");
    assert!(
        !canned.sent().contains("/admin"),
        "the smuggled request was written to the socket"
    );
}

#[test]
fn a_connection_that_closes_before_replying_is_a_coded_error() {
    let canned = Arc::new(Canned::new(""));
    let transport = HttpTransport::with_connector(canned as Arc<dyn Connector>);
    let err = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect_err("an empty reply is an error");
    assert_eq!(err.code.0, "AURA-CLOUD-6003");
}

#[test]
fn a_first_line_that_is_not_a_status_line_is_a_provider_error() {
    let canned = Arc::new(Canned::new("this is not http\r\n\r\n"));
    let transport = HttpTransport::with_connector(canned as Arc<dyn Connector>);
    let err = transport
        .send(
            &request("http://localhost:8080/v1/messages"),
            Duration::from_secs(1),
        )
        .expect_err("garbage is an error");
    assert_eq!(err.code.0, "AURA-CLOUD-6010");
}

#[test]
fn a_request_digest_ignores_header_values_so_it_never_carries_the_key() {
    let mut with_key = request("http://localhost:8080/v1/messages");
    let mut with_other_key = request("http://localhost:8080/v1/messages");
    with_key.headers = vec![("x-api-key".to_string(), "aaaa".to_string())];
    with_other_key.headers = vec![("x-api-key".to_string(), "bbbb".to_string())];

    assert_eq!(
        with_key.digest(),
        with_other_key.digest(),
        "two keys must produce the same cassette key"
    );
    assert_eq!(with_key.digest().len(), 64);
}

#[test]
fn a_request_debug_rendering_shows_header_names_and_no_values() {
    let rendered = format!("{:?}", request("http://localhost:8080/v1/messages"));
    assert!(rendered.contains("x-api-key"));
    assert!(!rendered.contains("secret"));
}
