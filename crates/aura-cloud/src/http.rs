//! A small HTTP/1.1 client. The only code in AURA that opens a socket.
//!
//! Writing one rather than taking a dependency is a deliberate trade, and it is
//! worth being explicit about which way it cuts.
//!
//! What is bought: no transitive dependency tree behind the single most
//! security-sensitive operation in the product, a body that is bounded rather
//! than unbounded, deadlines that are actually enforced on every read, and a
//! transport that is a hundred lines a reviewer can hold in their head. A gateway
//! whose network layer is auditable in one sitting is worth more here than one
//! that supports HTTP/2.
//!
//! What is given up: TLS, connection reuse, redirects, compression and HTTP/2.
//! Redirects and compression are refused rather than followed - a provider that
//!302s us somewhere is a provider we should not silently follow, and every
//! vendor here returns identity-encoded JSON. TLS is the significant one, and
//! `docs/adr/ADR-0009-cloud-ai-policy.md` records the waiver: this transport
//! reaches `http://` endpoints, which in practice means a local or studio-network
//! OpenAI-compatible server, and the [`Connector`] port is where a `TlsStream`
//! lands when the supply-chain review of a TLS stack clears.
//!
//! Everything above this module is unaware of any of it. [`crate::provider::Transport`]
//! is the boundary, and the cassette transport satisfies it identically.

use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use aura_core::errors::cloud::{provider_error, unreachable};
use aura_core::AuraResult;

use crate::provider::{HttpRequest, HttpResponse, Transport};

/// The largest response body this client will read into memory.
///
/// A provider answering a schema-bound question returns kilobytes. Sixteen
/// megabytes is four orders of magnitude of headroom and still a bound, which is
/// what stops a misconfigured endpoint streaming until the machine swaps.
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;

/// The largest single header line accepted.
pub const MAX_HEADER_BYTES: usize = 16 * 1024;

/// The largest number of headers accepted.
pub const MAX_HEADERS: usize = 100;

/// A bidirectional byte stream. `TcpStream` today; a TLS stream later.
pub trait Stream: Read + Write + Send + fmt::Debug {}

impl<T: Read + Write + Send + fmt::Debug> Stream for T {}

/// Opening a stream to a host. The seam a TLS implementation slots into.
pub trait Connector: Send + Sync + fmt::Debug {
    /// Connect, honouring the deadline for the connect itself and setting it as
    /// the read and write timeout for the life of the stream.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6003` for a failure to resolve, connect, or set a deadline.
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> AuraResult<Box<dyn Stream>>;

    /// The URL scheme this connector serves.
    fn scheme(&self) -> &'static str;
}

/// Plain TCP.
#[derive(Debug, Default)]
pub struct TcpConnector;

impl Connector for TcpConnector {
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> AuraResult<Box<dyn Stream>> {
        // Resolved explicitly rather than through `TcpStream::connect(&str)` so
        // that the connect deadline applies. The string form has no timeout and
        // will sit on a black-holed address for the operating system's own
        // patience, which on Windows is 21 seconds per address.
        let mut addresses = (host, port)
            .to_socket_addrs()
            .map_err(|err| unreachable(host, format!("could not resolve {host}: {err}")))?;
        let address = addresses
            .next()
            .ok_or_else(|| unreachable(host, format!("{host} resolved to no addresses")))?;

        let stream = TcpStream::connect_timeout(&address, timeout)
            .map_err(|err| unreachable(host, format!("could not connect to {host}: {err}")))?;
        stream
            .set_read_timeout(Some(timeout))
            .and_then(|()| stream.set_write_timeout(Some(timeout)))
            .and_then(|()| stream.set_nodelay(true))
            .map_err(|err| unreachable(host, format!("could not set a deadline: {err}")))?;
        Ok(Box::new(stream))
    }

    fn scheme(&self) -> &'static str {
        "http"
    }
}

/// The parts of a URL this client needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// `http` or `https`.
    pub scheme: String,
    /// Host name or literal address.
    pub host: String,
    /// Port, defaulted from the scheme.
    pub port: u16,
    /// Path and query, starting with a slash.
    pub path: String,
}

impl Target {
    /// Split a URL into its parts.
    ///
    /// # Errors
    ///
    /// `AURA-CLOUD-6003` when the URL has no scheme, no host, or a port that is
    /// not a number.
    pub fn parse(url: &str) -> AuraResult<Self> {
        let (scheme, rest) = url
            .split_once("://")
            .ok_or_else(|| unreachable("url", format!("{url} has no scheme")))?;
        let (authority, path) = match rest.find('/') {
            Some(index) => {
                let (left, right) = rest.split_at(index);
                (left, right.to_string())
            }
            None => (rest, "/".to_string()),
        };
        // Userinfo in a URL is a credential in a place we will not carry one.
        if authority.contains('@') {
            return Err(unreachable(
                "url",
                "a user name or password in the endpoint URL is not accepted",
            ));
        }
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, port)) => (
                host.to_string(),
                port.parse::<u16>()
                    .map_err(|_| unreachable("url", format!("{port} is not a port number")))?,
            ),
            None => (
                authority.to_string(),
                if scheme == "https" { 443 } else { 80 },
            ),
        };
        if host.is_empty() {
            return Err(unreachable("url", format!("{url} has no host")));
        }
        Ok(Self {
            scheme: scheme.to_string(),
            host,
            port,
            path,
        })
    }

    /// The `Host` header value, which omits the port when it is the default.
    #[must_use]
    pub fn host_header(&self) -> String {
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default {
            self.host.clone()
        } else {
            format!("{}:{}", self.host, self.port)
        }
    }
}

/// HTTP/1.1 over whatever the connector provides.
#[derive(Debug)]
pub struct HttpTransport {
    connector: Arc<dyn Connector>,
}

impl HttpTransport {
    /// A transport over plain TCP.
    #[must_use]
    pub fn new() -> Self {
        Self {
            connector: Arc::new(TcpConnector),
        }
    }

    /// A transport over a supplied connector.
    #[must_use]
    pub fn with_connector(connector: Arc<dyn Connector>) -> Self {
        Self { connector }
    }
}

impl Default for HttpTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for HttpTransport {
    fn send(&self, request: &HttpRequest, timeout: Duration) -> AuraResult<HttpResponse> {
        let target = Target::parse(&request.url)?;
        if target.scheme != self.connector.scheme() {
            return Err(unreachable(
                &target.host,
                format!(
                    "this build can reach {} endpoints only; {} needs a TLS transport, which is \
                     waived in ADR-0009",
                    self.connector.scheme(),
                    target.scheme
                ),
            ));
        }

        let mut stream = self.connector.connect(&target.host, target.port, timeout)?;
        write_request(stream.as_mut(), request, &target)?;
        read_response(stream.as_mut(), &target.host)
    }

    fn name(&self) -> &'static str {
        "http"
    }
}

/// Serialise the request line, the headers and the body.
///
/// `Connection: close` on every request. Without connection reuse a keep-alive
/// is a socket left open for nothing, and with `close` the server frames the end
/// of the body for us even when it sends neither a length nor chunking.
fn write_request(
    stream: &mut dyn Stream,
    request: &HttpRequest,
    target: &Target,
) -> AuraResult<()> {
    use std::fmt::Write as _;

    // `write!` into a `String` is infallible; the result is discarded rather
    // than unwrapped, which R1 bans even where it cannot fire.
    let mut head = String::with_capacity(512);
    let _ = write!(head, "{} {} HTTP/1.1\r\n", request.method, target.path);
    let _ = write!(head, "host: {}\r\n", target.host_header());
    head.push_str("connection: close\r\n");
    head.push_str("accept-encoding: identity\r\n");
    let _ = write!(head, "user-agent: aura/{}\r\n", env!("CARGO_PKG_VERSION"));
    let _ = write!(head, "content-length: {}\r\n", request.body.len());
    for (name, value) in &request.headers {
        // A header value containing a newline would let a caller inject a second
        // request into this one. Nothing in this crate builds such a value, and
        // that is exactly why it is checked here rather than trusted.
        if value.contains('\r') || value.contains('\n') || name.contains(':') {
            return Err(unreachable(
                &target.host,
                format!("refusing to send a malformed `{name}` header"),
            ));
        }
        let _ = write!(head, "{}: {}\r\n", name.to_ascii_lowercase(), value);
    }
    head.push_str("\r\n");

    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(&request.body))
        .and_then(|()| stream.flush())
        .map_err(|err| unreachable(&target.host, format!("could not send the request: {err}")))
}

/// Read the status line, the headers and the body.
fn read_response(stream: &mut dyn Stream, host: &str) -> AuraResult<HttpResponse> {
    let mut reader = BufReader::new(stream);

    let status_line = read_line(&mut reader, host)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            provider_error(
                host,
                0,
                format!("the first line was not a status line: {status_line:?}"),
            )
        })?;

    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        let line = read_line(&mut reader, host)?;
        if line.is_empty() {
            break;
        }
        if headers.len() >= MAX_HEADERS {
            return Err(provider_error(host, status, "too many response headers"));
        }
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
        }
    }

    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
    let length = headers
        .iter()
        .find(|(name, _)| name == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok());

    let body = if chunked {
        read_chunked(&mut reader, host)?
    } else if let Some(length) = length {
        if length > MAX_BODY_BYTES {
            return Err(provider_error(
                host,
                status,
                format!("the response declared {length} bytes, over the {MAX_BODY_BYTES} ceiling"),
            ));
        }
        let mut body = vec![0u8; length];
        reader
            .read_exact(&mut body)
            .map_err(|err| unreachable(host, format!("the response body stopped short: {err}")))?;
        body
    } else {
        // No length and no chunking: the body ends when the connection does,
        // which is why `Connection: close` is sent on every request.
        let mut body = Vec::new();
        reader
            .take(MAX_BODY_BYTES as u64)
            .read_to_end(&mut body)
            .map_err(|err| unreachable(host, format!("could not read the response: {err}")))?;
        body
    };

    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

/// One CRLF-terminated line, bounded.
fn read_line(reader: &mut BufReader<&mut dyn Stream>, host: &str) -> AuraResult<String> {
    let mut line = Vec::new();
    let read = reader
        .by_ref()
        .take(MAX_HEADER_BYTES as u64)
        .read_until(b'\n', &mut line)
        .map_err(|err| unreachable(host, format!("could not read the response: {err}")))?;
    if read == 0 {
        return Err(unreachable(host, "the connection closed before a reply"));
    }
    while line
        .last()
        .is_some_and(|byte| *byte == b'\n' || *byte == b'\r')
    {
        line.pop();
    }
    Ok(String::from_utf8_lossy(&line).to_string())
}

/// Reassemble a `Transfer-Encoding: chunked` body.
fn read_chunked(reader: &mut BufReader<&mut dyn Stream>, host: &str) -> AuraResult<Vec<u8>> {
    let mut body = Vec::new();
    loop {
        let header = read_line(reader, host)?;
        // A chunk header may carry extensions after a semicolon; the size is the
        // part before it, in hexadecimal.
        let size_text = header.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_text, 16)
            .map_err(|_| provider_error(host, 0, format!("{size_text:?} is not a chunk size")))?;
        if size == 0 {
            // Trailers, then the final empty line.
            while !read_line(reader, host)?.is_empty() {}
            break;
        }
        if body.len().saturating_add(size) > MAX_BODY_BYTES {
            return Err(provider_error(
                host,
                0,
                format!("the chunked body passed the {MAX_BODY_BYTES} byte ceiling"),
            ));
        }
        let mut chunk = vec![0u8; size];
        reader
            .read_exact(&mut chunk)
            .map_err(|err| unreachable(host, format!("a chunk stopped short: {err}")))?;
        body.extend_from_slice(&chunk);
        // The CRLF that terminates the chunk.
        drop(read_line(reader, host)?);
    }
    Ok(body)
}
