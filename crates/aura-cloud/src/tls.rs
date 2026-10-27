//! TLS, so that `https://` endpoints are reachable.
//!
//! Phase 04 shipped without this and `docs/adr/ADR-0009-cloud-ai-policy.md`
//! recorded the waiver: the hand-written HTTP/1.1 client in [`crate::http`] could
//! reach a studio-network or `localhost` server and nothing else. That was a
//! defensible place to stop while the provider list was an engineering decision.
//! It stops being defensible the moment the product asks a photographer to paste
//! a key for Anthropic, OpenAI or any of the sixteen other vendors in
//! [`crate::catalog`], every one of which is HTTPS-only: a setup screen that
//! collects a key it cannot use is worse than no setup screen.
//!
//! `docs/adr/ADR-0066-tls-and-the-provider-catalogue.md` discharges the waiver
//! and records what was traded. Three things are worth having in front of you
//! before reading the code.
//!
//! **The crypto is `ring`, rustls's standard provider.** Until 2026-10-27 it was
//! `rustls-rustcrypto`, because `ring` and `aws-lc-rs` compile C and assembly and the
//! reference build machine had no C toolchain. That machine now has one (MinGW gcc
//! under the GNU Rust toolchain), so the swap ADR-0066 promised was made: the
//! pure-Rust provider was an alpha with no release since April 2024, and it pinned
//! `rustls-webpki` 0.102 and `crypto-bigint` 0.5, whose advisories (RUSTSEC-2026-0382,
//! RUSTSEC-2026-0387 and others) could only be argued around, never fixed.
//!
//! **Verification is on, and there is no switch that turns it off.** No
//! `dangerous_configuration`, no "accept invalid certificates" setting, no
//! environment variable. The roots are the Mozilla set that ships in
//! `webpki-roots` rather than the operating system's store, which means a studio
//! behind a TLS-inspecting proxy will see a certificate error rather than a
//! silent interception - the right way round for a product that sends
//! photographs of other people's weddings.
//!
//! **The key exchanges are `ring`'s defaults: X25519 first, then P-256 and P-384.**
//! All three run in constant time in `ring`, so the X25519-only restriction the pure-Rust
//! provider needed (its P-256 arithmetic was not constant-time) is gone, and a vendor that
//! prefers P-256 is no longer refused.
//!
//! **Plain HTTP still works, and is still what a local model uses.** The
//! transport picks a connector by scheme, so Ollama on `127.0.0.1` does not go
//! through any of this.

use std::fmt;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use aura_core::errors::cloud::unreachable;
use aura_core::AuraResult;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};

use crate::http::{Connector, Stream};

/// A TLS session over one TCP connection.
///
/// A named wrapper rather than `StreamOwned` directly, because [`Stream`] asks
/// for `Debug` and the debug of a live TLS session is the session's internal
/// buffers - which is to say the plaintext of whatever is in flight. This one
/// prints the peer and nothing else.
struct TlsStream {
    host: String,
    inner: StreamOwned<ClientConnection, TcpStream>,
}

impl fmt::Debug for TlsStream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TlsStream")
            .field("host", &self.host)
            .finish_non_exhaustive()
    }
}

impl Read for TlsStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.inner.read(buffer)
    }
}

impl Write for TlsStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        self.inner.write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// TLS 1.2 and 1.3 over TCP, with the Mozilla root set.
#[derive(Debug, Default)]
pub struct TlsConnector;

impl TlsConnector {
    /// A connector ready to use.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// The shared client configuration.
///
/// `ring`, rustls's standard provider, with its default key exchanges (see the module notes).
fn provider() -> rustls::crypto::CryptoProvider {
    rustls::crypto::ring::default_provider()
}

/// Built once. Assembling it parses several hundred root certificates and
/// negotiates the cipher suite list, and doing that per request on a run that
/// makes seventy calls is a measurable waste for no benefit - the configuration
/// has no per-connection state.
///
/// `None` means the crypto provider refused to produce a configuration at all,
/// which is a build-level fault rather than a network one. It is reported as a
/// reachability error so that a call degrades to the local fallback the way every
/// other transport failure does.
fn client_config() -> Option<Arc<ClientConfig>> {
    static CONFIG: OnceLock<Option<Arc<ClientConfig>>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let roots = RootCertStore {
                roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
            };
            let built = ClientConfig::builder_with_provider(Arc::new(provider()))
                .with_safe_default_protocol_versions()
                .ok()?
                .with_root_certificates(roots)
                .with_no_client_auth();
            Some(Arc::new(built))
        })
        .clone()
}

impl Connector for TlsConnector {
    fn connect(&self, host: &str, port: u16, timeout: Duration) -> AuraResult<Box<dyn Stream>> {
        let config = client_config().ok_or_else(|| {
            unreachable(
                host,
                "this build could not assemble a TLS configuration; see ADR-0066",
            )
        })?;

        // The name is validated before the socket is opened. A host that is not a
        // valid DNS name or IP address cannot be verified against a certificate,
        // and connecting first would mean sending bytes to something we have
        // already decided we cannot authenticate.
        let server_name = ServerName::try_from(host.to_string()).map_err(|_| {
            unreachable(
                host,
                format!("{host} is not a name a certificate can be checked against"),
            )
        })?;

        // Resolved explicitly for the same reason [`crate::http::TcpConnector`]
        // does it: the string form of `connect` has no deadline.
        let mut addresses = (host, port)
            .to_socket_addrs()
            .map_err(|err| unreachable(host, format!("could not resolve {host}: {err}")))?;
        let address = addresses
            .next()
            .ok_or_else(|| unreachable(host, format!("{host} resolved to no addresses")))?;

        let socket = TcpStream::connect_timeout(&address, timeout)
            .map_err(|err| unreachable(host, format!("could not connect to {host}: {err}")))?;
        socket
            .set_read_timeout(Some(timeout))
            .and_then(|()| socket.set_write_timeout(Some(timeout)))
            .and_then(|()| socket.set_nodelay(true))
            .map_err(|err| unreachable(host, format!("could not set a deadline: {err}")))?;

        let session = ClientConnection::new(config, server_name).map_err(|err| {
            unreachable(
                host,
                format!("could not start a TLS session with {host}: {err}"),
            )
        })?;

        Ok(Box::new(TlsStream {
            host: host.to_string(),
            inner: StreamOwned::new(session, socket),
        }))
    }

    fn scheme(&self) -> &'static str {
        "https"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_can_be_built() {
        assert!(
            client_config().is_some(),
            "the crypto provider refused to produce a client configuration"
        );
    }

    #[test]
    fn x25519_is_preferred_and_every_key_exchange_is_constant_time() {
        let groups: Vec<_> = provider().kx_groups.iter().map(|g| g.name()).collect();
        // `ring` implements all three in constant time; X25519 is offered first.
        assert_eq!(
            groups,
            vec![
                rustls::NamedGroup::X25519,
                rustls::NamedGroup::secp256r1,
                rustls::NamedGroup::secp384r1
            ]
        );
    }

    #[test]
    fn the_configuration_is_shared_rather_than_rebuilt() {
        let first = client_config().expect("a configuration");
        let second = client_config().expect("a configuration");
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn a_host_that_cannot_be_verified_is_refused_before_a_socket_is_opened() {
        // No DNS lookup, no connect: the name is rejected on its own terms.
        let refused = TlsConnector::new().connect("not a host name", 443, Duration::from_secs(1));
        let error = refused.expect_err("an unverifiable name must be refused");
        assert_eq!(error.code.0, "AURA-CLOUD-6003");
    }

    #[test]
    fn the_connector_serves_https() {
        assert_eq!(TlsConnector::new().scheme(), "https");
    }

    /// A real handshake with a public AI provider: certificate chain verified against the
    /// bundled roots, a key exchange and cipher suite agreed, and an HTTP status line read back.
    /// No key and no photograph is sent.
    #[test]
    #[ignore = "live network"]
    fn a_public_ai_provider_completes_a_verified_handshake() {
        for host in ["api.anthropic.com", "api.openai.com"] {
            let mut stream = TlsConnector::new()
                .connect(host, 443, Duration::from_secs(20))
                .unwrap_or_else(|e| panic!("{host}: {}", e.code.0));
            let request = format!("HEAD / HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
            stream.write_all(request.as_bytes()).expect("write");
            let mut reply = Vec::new();
            let _ = stream.read_to_end(&mut reply);
            let text = String::from_utf8_lossy(&reply);
            assert!(
                text.starts_with("HTTP/1.1 "),
                "{host}: {}",
                &text[..text.len().min(80)]
            );
        }
    }
}
