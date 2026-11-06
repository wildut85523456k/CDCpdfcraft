//! HTTP transport for OCSP, CRL, and RFC 3161 timestamp fetches
//! (ADR-0009: the `pdfcraft-sign` crate stays transport-free; this module
//! provides the concrete `ureq` implementation of its [`Fetcher`] and
//! [`TimestampAuthority`] traits).
//!
//! [`HttpFetcher`] is what the CLI, MCP server and automation tools use to do
//! PAdES B-T / B-LT / B-LTA signing against live responders: a TSA for
//! signature and document timestamps, and the OCSP responders and CRL
//! distribution points named by each certificate's AIA and CRLDP extensions.

use std::time::Duration;

use pdfcraft_sign::ltv::Fetcher;
use pdfcraft_sign::{SignError, TimestampAuthority, Time};

/// HTTP fetcher and timestamp authority built on `ureq`. Pass one to
/// [`pdfcraft_sign::sign_with_ltv`] (or to the engine's `Session::sign_with_ltv`)
/// to produce a PAdES B-LTA signature against live TSA, OCSP and CRL servers.
///
/// The TSA URL is supplied at construction; OCSP and CRL URLs come from each
/// certificate's AIA / CRL distribution-point extensions at fetch time.
pub struct HttpFetcher {
    tsa_url: String,
    user_agent: String,
    agent: ureq::Agent,
}

impl HttpFetcher {
    /// A fetcher that talks to `tsa_url` for RFC 3161 timestamps and uses the
    /// certificate-supplied URLs for OCSP and CRL. The default timeout is 30 s.
    pub fn new(tsa_url: impl Into<String>) -> Self {
        Self::with_timeout(tsa_url, Duration::from_secs(30))
    }

    /// Like [`Self::new`] but with a custom timeout for every HTTP request.
    pub fn with_timeout(tsa_url: impl Into<String>, timeout: Duration) -> Self {
        let ua = format!("pdfcraft/{}", env!("CARGO_PKG_VERSION"));
        let agent = ureq::Agent::config_builder()
            .timeout_per_read(Some(timeout))
            .timeout_per_write(Some(timeout))
            .user_agent(ua.as_str())
            .build()
            .into();
        Self {
            tsa_url: tsa_url.into(),
            user_agent: ua,
            agent,
        }
    }

    /// Change the user-agent header (default: `pdfcraft/<version>`).
    pub fn user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = ua.into();
        self
    }

    /// The TSA URL this fetcher posts timestamp requests to.
    pub fn tsa_url(&self) -> &str {
        &self.tsa_url
    }

    fn fetch_get(&self, url: &str) -> Result<Vec<u8>, SignError> {
        let resp = self
            .agent
            .get(url)
            .call()
            .map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(SignError::Pdf(format!("HTTP GET {} returned status {}", url, status.as_u16())));
        }
        resp.into_body().read_to_vec().map_err(body_err)
    }

    fn fetch_post(&self, url: &str, body: &[u8], content_type: &str) -> Result<Vec<u8>, SignError> {
        let resp = self
            .agent
            .post(url)
            .content_type(content_type)
            .send(body)
            .map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(SignError::Pdf(format!("HTTP POST {} returned status {}", url, status.as_u16())));
        }
        resp.into_body().read_to_vec().map_err(body_err)
    }
}

impl Fetcher for HttpFetcher {
    fn get(&self, url: &str) -> Result<Vec<u8>, SignError> {
        self.fetch_get(url)
    }

    fn post(&self, url: &str, body: &[u8], content_type: &str) -> Result<Vec<u8>, SignError> {
        self.fetch_post(url, body, content_type)
    }

    fn now(&self) -> Time {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Time::from_unix(secs)
    }
}

impl TimestampAuthority for HttpFetcher {
    fn timestamp(&self, request: &[u8]) -> Result<Vec<u8>, SignError> {
        let resp = self
            .agent
            .post(&self.tsa_url)
            .content_type("application/timestamp-query")
            .send(request)
            .map_err(http_err)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(SignError::Pdf(format!("TSA {} returned status {}", self.tsa_url, status.as_u16())));
        }
        resp.into_body().read_to_vec().map_err(body_err)
    }
}

/// Convert a `ureq::Error` into a `SignError` with context.
fn http_err(e: ureq::Error) -> SignError {
    SignError::Pdf(format!("HTTP/fetch error: {e}"))
}

/// Convert an error reading the response body into a `SignError`.
fn body_err(e: ureq::Error) -> SignError {
    SignError::Pdf(format!("body read error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fetcher_exposes_tsa_url() {
        let f = HttpFetcher::new("http://example.com/tsa");
        assert_eq!(f.tsa_url(), "http://example.com/tsa");
    }

    #[test]
    fn now_returns_reasonable_time() {
        let f = HttpFetcher::new("http://example.com/tsa");
        let t = f.now();
        assert!(t.year >= 2020, "year {} is not >= 2020", t.year);
    }

    #[test]
    fn user_agent_can_be_set() {
        let f = HttpFetcher::new("http://example.com/tsa")
            .user_agent("test-agent/1.0");
        assert_eq!(f.user_agent, "test-agent/1.0");
    }

    #[test]
    fn fetcher_implements_all_traits() {
        let f = HttpFetcher::new("http://example.com/tsa");
        let _: &dyn Fetcher = &f;
        let _: &dyn TimestampAuthority = &f;
    }
}
