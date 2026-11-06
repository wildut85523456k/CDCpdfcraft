//! Online LTV evidence (PAdES B-LT): fetching OCSP responses and CRLs so an offline
//! signature can be validated without a network. Transport-free by design — like
//! [`crate::timestamp::TimestampAuthority`], the caller supplies the transport.
//!
//! [`fetch_evidence`] walks a certificate chain, queries each certificate's OCSP
//! responder (from its AIA extension) or CRL distribution point (from its CRL
//! distribution-points extension), checks the response against the certificate
//! and its issuer, and collects the verified evidence plus the issuer certificates
//! into an [`crate::dss::Evidence`] for [`crate::dss::embed`]. Only evidence that
//! verifies is included; unverifiable evidence is omitted.

use crate::dss::Evidence;
use crate::keys::DigestAlg;
use crate::revocation::{CertId, CertificateList, OcspResponse, RevocationStatus};
use crate::x509::Certificate;
use crate::{SignError, Time};

/// A caller-supplied transport for fetching revocation and timestamp evidence.
/// The sign crate never creates sockets; this trait is implemented at the
/// application boundary by [`pdfcraft_platform::http::HttpFetcher`] or a test
/// double.
pub trait Fetcher {
    /// HTTP GET `url`, returning the response body.
    fn get(&self, url: &str) -> Result<Vec<u8>, SignError>;
    /// HTTP POST `url` with `body` and `content_type` (e.g. `"application/ocsp-request"`).
    fn post(&self, url: &str, body: &[u8], content_type: &str) -> Result<Vec<u8>, SignError>;
    /// The current UTC time, used to check evidence validity windows.
    fn now(&self) -> Time;
}

/// Fetch revocation evidence for `chain` (leaf first, root last) and package it
/// into an [`Evidence`] for DSS embedding. OCSP is tried first for each
/// certificate (from its Authority Information Access / OCSP URLs); a certificate
/// with no usable OCSP responder falls back to its CRL distribution-point URLs.
/// Self-signed certificates are skipped — they are trust anchors, not
/// revocation-checked. Only evidence that parses and verifies is included.
/// Certificates that have no revocation URLs at all are simply skipped.
pub fn fetch_evidence(chain: &[Certificate], fetcher: &dyn Fetcher) -> Evidence {
    let at = fetcher.now();
    let mut evidence = Evidence::default();
    // Collect issuer certificates (everything except the leaf) into the DSS certs so
    // offline validation can build the chain.
    for cert in chain.iter().rev().skip(1) {
        if !evidence.certs.iter().any(|c| c == &cert.raw) {
            evidence.certs.push(cert.raw.clone());
        }
    }
    // For each non-root certificate, fetch OCSP then CRL from its issuer.
    for i in 0..chain.len() {
        let cert = &chain[i];
        if cert.is_self_signed() {
            continue;
        }
        let issuer = match chain.get(i + 1) {
            Some(iss) => iss,
            None => continue,
        };
        // OCSP: POST a request built from the CertID.
        for url in &cert.ocsp_urls {
            if let Some(ocsp) = try_ocsp(cert, issuer, &at, fetcher, url) {
                if !evidence.ocsps.iter().any(|o| o == &ocsp) {
                    evidence.ocsps.push(ocsp);
                }
                break; // one OCSP response per certificate is enough
            }
        }
        // CRL: GET the DER-encoded list.
        for url in &cert.crl_urls {
            if let Some(crl) = try_crl(cert, issuer, &at, fetcher, url) {
                if !evidence.crls.iter().any(|c| c == &crl) {
                    evidence.crls.push(crl);
                }
                break;
            }
        }
    }
    evidence
}

/// Build an OCSP request for `cert`/`issuer`, POST it to `url`, parse and verify the
/// response. Returns the raw DER bytes only when the response is `Good` or `Revoked`
/// (never `Unknown`, which means it couldn't be checked).
fn try_ocsp(
    cert: &Certificate,
    issuer: &Certificate,
    at: &Time,
    fetcher: &dyn Fetcher,
    url: &str,
) -> Option<Vec<u8>> {
    let id = CertId::build(cert, issuer, DigestAlg::Sha1);
    let request = id.encode_request();
    let raw = fetcher.post(url, &request, "application/ocsp-request").ok()?;
    let resp = OcspResponse::parse(&raw).ok()?;
    let status = resp.check(cert, issuer, *at);
    if status == RevocationStatus::Unknown {
        return None;
    }
    Some(raw)
}

/// GET a CRL from `url`, parse and verify it. Returns the raw DER bytes only when
/// the CRL is `Good` or `Revoked` for `cert`.
fn try_crl(
    cert: &Certificate,
    issuer: &Certificate,
    at: &Time,
    fetcher: &dyn Fetcher,
    url: &str,
) -> Option<Vec<u8>> {
    let raw = fetcher.get(url).ok()?;
    let crl = CertificateList::parse(&raw).ok()?;
    let status = crl.check(cert, issuer, *at);
    if status == RevocationStatus::Unknown {
        return None;
    }
    Some(raw)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `Fetcher` that returns canned bytes — no network. Lets the tests exercise
    /// the OCSP/CRL selection and verification logic deterministically.
    struct StubFetcher {
        now: Time,
        ocsp: Vec<u8>,
        crl: Vec<u8>,
    }

    impl Fetcher for StubFetcher {
        fn get(&self, _url: &str) -> Result<Vec<u8>, SignError> {
            Ok(self.crl.clone())
        }
        fn post(&self, _url: &str, _body: &[u8], _ct: &str) -> Result<Vec<u8>, SignError> {
            Ok(self.ocsp.clone())
        }
        fn now(&self) -> Time {
            self.now
        }
    }

    #[test]
    fn encode_request_round_trips_shape() {
        let id = CertId {
            hash_alg: DigestAlg::Sha1,
            issuer_name_hash: vec![0x01; 20],
            issuer_key_hash: vec![0x02; 20],
            serial: vec![0x00, 0x12, 0x34],
        };
        let req = id.encode_request();
        // The outer element is a SEQUENCE.
        assert_eq!(req[0], 0x30);
        // The CertID's serial should appear in the encoding.
        assert!(req.windows(2).any(|w| w == [0x12, 0x34]));
    }

    #[test]
    fn empty_chain_yields_empty_evidence() {
        let fetcher = StubFetcher { now: Time::from_unix(0), ocsp: vec![], crl: vec![] };
        let evidence = fetch_evidence(&[], &fetcher);
        assert!(evidence.certs.is_empty() && evidence.ocsps.is_empty() && evidence.crls.is_empty());
    }
}