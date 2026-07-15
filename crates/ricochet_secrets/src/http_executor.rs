use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::io::Read;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;

use reqwest::blocking::{Client, Response};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, HOST};
use ricochet_sandbox::DestinationGrant;
use serde_json::Value as JsonValue;
use zeroize::Zeroizing;

use crate::deferred_http::{DeferredHttpCredentials, DeferredSecretSourceRef};
use crate::{SecretSessionContext, SecurityDomainId};

const PERMISSION_ERROR: &str = "PermissionError";
const SECRET_REFERENCE_ERROR: &str = "SecretReferenceError";
const HTTP_HEADER_ERROR: &str = "HttpHeaderError";
const HTTP_ERROR: &str = "HttpError";

trait DeferredCredentialSource: Send + Sync {
    fn resolve_environment(&self, name: &str)
        -> Result<Zeroizing<String>, DeferredCredentialError>;
}

struct ProcessEnvironmentCredentialSource;

#[derive(Clone, Copy)]
enum DeferredCredentialError {
    MissingEnvironment,
    NonUnicodeEnvironment,
}

#[derive(Clone)]
pub struct EnvironmentCredentialPolicy {
    enabled: bool,
    allowed_names: Option<BTreeSet<String>>,
}

pub struct SecretHttpPolicySnapshot {
    http_enabled: bool,
    allowed_hosts: Option<BTreeSet<String>>,
    allowed_destinations: BTreeSet<DestinationGrant>,
    environment: EnvironmentCredentialPolicy,
    address_policy: SecretHttpAddressPolicy,
    secret_session: Option<SecretSessionContext>,
    security_domain_id: Option<SecurityDomainId>,
}

#[derive(Clone, Copy)]
enum SecretHttpAddressPolicy {
    PublicOnly,
}

#[derive(Clone)]
pub struct SecretsHttpExecutor {
    inner: Arc<SecretsHttpExecutorInner>,
}

struct SecretsHttpExecutorInner {
    source: Arc<dyn DeferredCredentialSource>,
    resolver: DestinationResolver,
    #[cfg(feature = "test-host")]
    metrics: Option<Arc<TestHostMetrics>>,
}

enum DestinationResolver {
    System,
    #[cfg(test)]
    FixedForAddressPolicyTest(SocketAddr),
    #[cfg(feature = "test-host")]
    Test(TestDestinationResolver),
}

#[cfg(feature = "test-host")]
struct TestDestinationResolver {
    host: String,
    address: SocketAddr,
}

#[cfg(feature = "test-host")]
#[derive(Default)]
struct TestHostMetrics {
    credential_resolutions: std::sync::atomic::AtomicUsize,
    environment_source_accesses: std::sync::atomic::AtomicUsize,
}

pub struct PreparedSecretHttpRequest {
    credentials: DeferredHttpCredentials,
    method: reqwest::Method,
    url: String,
    headers: HeaderMap,
    json: Option<JsonValue>,
    body: Option<String>,
    timeout: Duration,
    max_response_bytes: usize,
    resolved_host: String,
    resolved_addresses: Vec<SocketAddr>,
    secret_session: Option<SecretSessionContext>,
    security_domain_id: Option<SecurityDomainId>,
}

enum ResolvedHttpCredential {
    Bearer(Zeroizing<String>),
}

pub struct SecretHttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

pub struct SecretHttpResponseStream {
    status: u16,
    headers: BTreeMap<String, String>,
    response: Response,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SecretHttpError {
    kind: &'static str,
    message: String,
}

impl EnvironmentCredentialPolicy {
    pub fn new(enabled: bool, allowed_names: Option<BTreeSet<String>>) -> Self {
        Self {
            enabled,
            allowed_names,
        }
    }
}

impl fmt::Debug for EnvironmentCredentialPolicy {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<environment-credential-policy>")
    }
}

impl SecretHttpPolicySnapshot {
    pub fn new(
        http_enabled: bool,
        allowed_hosts: Option<BTreeSet<String>>,
        allowed_destinations: BTreeSet<DestinationGrant>,
        environment: EnvironmentCredentialPolicy,
    ) -> Self {
        Self {
            http_enabled,
            allowed_hosts: allowed_hosts.map(|hosts| {
                hosts
                    .into_iter()
                    .map(|host| host.to_ascii_lowercase())
                    .collect()
            }),
            allowed_destinations,
            environment,
            address_policy: SecretHttpAddressPolicy::PublicOnly,
            secret_session: None,
            security_domain_id: None,
        }
    }

    pub fn with_security_domain(mut self, security_domain_id: SecurityDomainId) -> Self {
        self.security_domain_id = Some(security_domain_id);
        self
    }

    pub fn with_secret_session(
        mut self,
        context: SecretSessionContext,
        security_domain_id: SecurityDomainId,
    ) -> Self {
        self.secret_session = Some(context);
        self.security_domain_id = Some(security_domain_id);
        self
    }
}

impl fmt::Debug for SecretHttpPolicySnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<secret-http-policy-snapshot>")
    }
}

impl SecretsHttpExecutor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(SecretsHttpExecutorInner {
                source: Arc::new(ProcessEnvironmentCredentialSource),
                resolver: DestinationResolver::System,
                #[cfg(feature = "test-host")]
                metrics: None,
            }),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        &self,
        credentials: DeferredHttpCredentials,
        method: reqwest::Method,
        url: String,
        headers: HeaderMap,
        json: Option<JsonValue>,
        body: Option<String>,
        timeout: Duration,
        max_response_bytes: usize,
        request_allowed_hosts: Option<BTreeSet<String>>,
        request_allowed_schemes: Option<BTreeSet<String>>,
        policy: SecretHttpPolicySnapshot,
    ) -> Result<PreparedSecretHttpRequest, SecretHttpError> {
        if !policy.http_enabled {
            return Err(SecretHttpError::permission(
                "HTTP capability is not enabled for deferred credentials",
            ));
        }
        let parsed = reqwest::Url::parse(&url)
            .map_err(|_| SecretHttpError::permission("deferred HTTP credential URL is invalid"))?;
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(SecretHttpError::permission(
                "deferred HTTP credential URLs must not contain userinfo",
            ));
        }
        let scheme = parsed.scheme().to_ascii_lowercase();
        if scheme != "https" {
            return Err(SecretHttpError::permission(
                "deferred HTTP credentials require HTTPS",
            ));
        }
        if request_allowed_schemes
            .as_ref()
            .is_some_and(|schemes| !schemes.contains(&scheme))
        {
            return Err(SecretHttpError::permission(
                "deferred HTTP scheme is not allowed by request policy",
            ));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| SecretHttpError::permission("deferred HTTP credential URL has no host"))?
            .to_ascii_lowercase();
        let Some(allowed_hosts) = &policy.allowed_hosts else {
            return Err(SecretHttpError::permission(
                "deferred HTTP credentials require explicit HTTP host permission",
            ));
        };
        if !allowed_hosts.contains(&host) {
            return Err(SecretHttpError::permission(
                "deferred HTTP host is not allowed by HTTP policy",
            ));
        }
        if request_allowed_hosts
            .as_ref()
            .is_some_and(|hosts| !hosts.contains(&host))
        {
            return Err(SecretHttpError::permission(
                "deferred HTTP host is not allowed by request policy",
            ));
        }
        if headers.contains_key(HOST) {
            return Err(SecretHttpError::permission(
                "deferred HTTP requests must not supply a Host header",
            ));
        }
        if headers.contains_key(AUTHORIZATION) {
            return Err(SecretHttpError::permission(
                "deferred HTTP Authorization conflicts with an ordinary header",
            ));
        }
        if json.is_some() && body.is_some() {
            return Err(SecretHttpError::http(
                "deferred HTTP request body configuration is invalid",
            ));
        }
        let port = parsed.port_or_known_default().ok_or_else(|| {
            SecretHttpError::permission("deferred HTTP credential URL has no destination port")
        })?;
        let resolved_addresses = self.resolve_destination(&host, port)?;
        if resolved_addresses.is_empty()
            || resolved_addresses
                .iter()
                .any(|address| address.port() != port)
        {
            return Err(SecretHttpError::permission(
                "deferred HTTP destination address validation failed",
            ));
        }
        if !self.addresses_allowed(&host, &resolved_addresses, policy.address_policy) {
            return Err(SecretHttpError::permission(
                "deferred HTTP destination address validation failed",
            ));
        }
        let destination = DestinationGrant::new(&host, port)
            .map_err(|_| SecretHttpError::permission("deferred HTTP destination is invalid"))?;
        if !policy.allowed_destinations.contains(&destination) {
            return Err(SecretHttpError::permission(format!(
                "deferred HTTP credentials require an exact HTTP destination grant for {host}:{port}"
            )));
        }
        preflight_environment(&credentials, &policy.environment)?;
        preflight_secret_session(
            &credentials,
            policy.secret_session.as_ref(),
            policy.security_domain_id.as_ref(),
        )?;

        Ok(PreparedSecretHttpRequest {
            credentials,
            method,
            url,
            headers,
            json,
            body,
            timeout,
            max_response_bytes,
            resolved_host: host,
            resolved_addresses,
            secret_session: policy.secret_session,
            security_domain_id: policy.security_domain_id,
        })
    }

    pub fn execute(
        &self,
        request: PreparedSecretHttpRequest,
    ) -> Result<SecretHttpResponse, SecretHttpError> {
        let max_response_bytes = request.max_response_bytes;
        let response = self.send_once(request)?;
        let status = response.status().as_u16();
        let headers = sanitized_response_headers(response.headers());
        let mut body = Vec::new();
        response
            .take((max_response_bytes.saturating_add(1)) as u64)
            .read_to_end(&mut body)
            .map_err(|_| SecretHttpError::http("deferred HTTP response read failed"))?;
        if body.len() > max_response_bytes {
            return Err(SecretHttpError::http(
                "deferred HTTP response exceeded the configured byte limit",
            ));
        }
        Ok(SecretHttpResponse {
            status,
            headers,
            body,
        })
    }

    pub fn execute_stream(
        &self,
        request: PreparedSecretHttpRequest,
    ) -> Result<SecretHttpResponseStream, SecretHttpError> {
        let response = self.send_once(request)?;
        Ok(SecretHttpResponseStream {
            status: response.status().as_u16(),
            headers: sanitized_response_headers(response.headers()),
            response,
        })
    }

    fn send_once(&self, request: PreparedSecretHttpRequest) -> Result<Response, SecretHttpError> {
        let credential = self.resolve_credential(
            &request.credentials,
            request.secret_session.as_ref(),
            request.security_domain_id.as_ref(),
        )?;
        let mut headers = request.headers;
        credential.apply(&mut headers)?;
        let client = Client::builder()
            .timeout(request.timeout)
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .retry(reqwest::retry::never())
            .resolve_to_addrs(&request.resolved_host, &request.resolved_addresses);
        #[cfg(feature = "test-host")]
        let client = if matches!(self.inner.resolver, DestinationResolver::Test(_)) {
            client.danger_accept_invalid_certs(true)
        } else {
            client
        };
        let client = client
            .build()
            .map_err(|_| SecretHttpError::http("deferred HTTP client construction failed"))?;
        let mut builder = client.request(request.method, request.url).headers(headers);
        if let Some(json) = request.json {
            builder = builder.json(&json);
        } else if let Some(body) = request.body {
            builder = builder.body(body);
        }
        builder
            .send()
            .map_err(|_| SecretHttpError::http("deferred HTTP transport failed"))
    }

    fn resolve_credential(
        &self,
        credentials: &DeferredHttpCredentials,
        session: Option<&SecretSessionContext>,
        security_domain_id: Option<&SecurityDomainId>,
    ) -> Result<ResolvedHttpCredential, SecretHttpError> {
        #[cfg(feature = "test-host")]
        if let Some(metrics) = &self.inner.metrics {
            metrics
                .credential_resolutions
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
        let value = match credentials.bearer_source().source_ref() {
            DeferredSecretSourceRef::Environment { environment_key } => self
                .inner
                .source
                .resolve_environment(environment_key)
                .map_err(SecretHttpError::secret_reference)?,
            DeferredSecretSourceRef::Literal { value } => Zeroizing::new(value.to_string()),
            DeferredSecretSourceRef::Opaque { reference } => {
                let session = session.ok_or_else(|| {
                    SecretHttpError::secret_reference_message(
                        "session credential has no active host session",
                    )
                })?;
                let security_domain_id = security_domain_id.ok_or_else(|| {
                    SecretHttpError::secret_reference_message(
                        "session credential has no active security domain",
                    )
                })?;
                session
                    .resolve_reference(reference, security_domain_id)
                    .map_err(|_| {
                        SecretHttpError::secret_reference_message(
                            "session credential is unavailable",
                        )
                    })?
            }
        };
        if value.is_empty() {
            return Err(SecretHttpError::secret_reference_message(
                "deferred HTTP credential is empty",
            ));
        }
        Ok(ResolvedHttpCredential::Bearer(value))
    }

    fn resolve_destination(
        &self,
        host: &str,
        port: u16,
    ) -> Result<Vec<SocketAddr>, SecretHttpError> {
        match &self.inner.resolver {
            DestinationResolver::System => (host, port)
                .to_socket_addrs()
                .map(|addresses| addresses.collect())
                .map_err(|_| {
                    SecretHttpError::permission(
                        "deferred HTTP destination address validation failed",
                    )
                }),
            #[cfg(test)]
            DestinationResolver::FixedForAddressPolicyTest(address) => Ok(vec![*address]),
            #[cfg(feature = "test-host")]
            DestinationResolver::Test(resolver) => {
                if resolver.host == host && resolver.address.port() == port {
                    Ok(vec![resolver.address])
                } else {
                    Err(SecretHttpError::permission(
                        "deferred HTTP destination address validation failed",
                    ))
                }
            }
        }
    }

    fn addresses_allowed(
        &self,
        host: &str,
        addresses: &[SocketAddr],
        address_policy: SecretHttpAddressPolicy,
    ) -> bool {
        #[cfg(feature = "test-host")]
        if let DestinationResolver::Test(resolver) = &self.inner.resolver {
            return resolver.host == host
                && addresses.len() == 1
                && addresses[0] == resolver.address
                && resolver.address.ip().is_loopback();
        }
        let _ = host;
        match address_policy {
            SecretHttpAddressPolicy::PublicOnly => {
                addresses.iter().all(|address| is_public_ip(address.ip()))
            }
        }
    }
}

impl Default for SecretsHttpExecutor {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for SecretsHttpExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<secrets-http-executor>")
    }
}

impl fmt::Debug for PreparedSecretHttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<prepared-secret-http-request>")
    }
}

impl ResolvedHttpCredential {
    fn apply(self, headers: &mut HeaderMap) -> Result<(), SecretHttpError> {
        match self {
            Self::Bearer(value) => {
                let mut bearer = Zeroizing::new(String::from("Bearer "));
                bearer.push_str(&value);
                let mut header = HeaderValue::from_str(&bearer).map_err(|_| {
                    SecretHttpError::header("deferred HTTP credential is not header-safe")
                })?;
                header.set_sensitive(true);
                headers.insert(AUTHORIZATION, header);
            }
        }
        Ok(())
    }
}

impl SecretHttpResponse {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn body(&self) -> &[u8] {
        &self.body
    }
}

impl fmt::Debug for SecretHttpResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<secret-http-response>")
    }
}

impl SecretHttpResponseStream {
    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn headers(&self) -> &BTreeMap<String, String> {
        &self.headers
    }

    pub fn read_chunk(&mut self, buffer: &mut [u8]) -> Result<usize, SecretHttpError> {
        self.response
            .read(buffer)
            .map_err(|_| SecretHttpError::http("deferred HTTP stream read failed"))
    }
}

impl fmt::Debug for SecretHttpResponseStream {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let _ = self;
        formatter.write_str("<secret-http-response-stream>")
    }
}

impl SecretHttpError {
    fn permission(message: impl Into<String>) -> Self {
        Self {
            kind: PERMISSION_ERROR,
            message: message.into(),
        }
    }

    fn secret_reference(error: DeferredCredentialError) -> Self {
        let message = match error {
            DeferredCredentialError::MissingEnvironment => {
                "deferred environment credential is missing"
            }
            DeferredCredentialError::NonUnicodeEnvironment => {
                "deferred environment credential is not Unicode"
            }
        };
        Self::secret_reference_message(message)
    }

    fn secret_reference_message(message: impl Into<String>) -> Self {
        Self {
            kind: SECRET_REFERENCE_ERROR,
            message: message.into(),
        }
    }

    fn header(message: impl Into<String>) -> Self {
        Self {
            kind: HTTP_HEADER_ERROR,
            message: message.into(),
        }
    }

    fn http(message: impl Into<String>) -> Self {
        Self {
            kind: HTTP_ERROR,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Debug for SecretHttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretHttpError")
            .field("kind", &self.kind)
            .field("message", &self.message)
            .finish()
    }
}

impl fmt::Display for SecretHttpError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind, self.message)
    }
}

impl std::error::Error for SecretHttpError {}

impl DeferredCredentialSource for ProcessEnvironmentCredentialSource {
    fn resolve_environment(
        &self,
        name: &str,
    ) -> Result<Zeroizing<String>, DeferredCredentialError> {
        match std::env::var(name) {
            Ok(value) => Ok(Zeroizing::new(value)),
            Err(std::env::VarError::NotPresent) => Err(DeferredCredentialError::MissingEnvironment),
            Err(std::env::VarError::NotUnicode(_)) => {
                Err(DeferredCredentialError::NonUnicodeEnvironment)
            }
        }
    }
}

fn preflight_environment(
    credentials: &DeferredHttpCredentials,
    policy: &EnvironmentCredentialPolicy,
) -> Result<(), SecretHttpError> {
    let DeferredSecretSourceRef::Environment { environment_key } =
        credentials.bearer_source().source_ref()
    else {
        return Ok(());
    };
    if !policy.enabled {
        return Err(SecretHttpError::permission(
            "environment capability is not enabled for deferred credentials",
        ));
    }
    if policy
        .allowed_names
        .as_ref()
        .is_some_and(|names| !names.contains(environment_key))
    {
        return Err(SecretHttpError::permission(
            "environment credential name is not allowed by policy",
        ));
    }
    Ok(())
}

fn preflight_secret_session(
    credentials: &DeferredHttpCredentials,
    session: Option<&SecretSessionContext>,
    security_domain_id: Option<&SecurityDomainId>,
) -> Result<(), SecretHttpError> {
    let DeferredSecretSourceRef::Opaque { reference } = credentials.bearer_source().source_ref()
    else {
        return Ok(());
    };
    let session = session.ok_or_else(|| {
        SecretHttpError::secret_reference_message("session credential has no active host session")
    })?;
    let security_domain_id = security_domain_id.ok_or_else(|| {
        SecretHttpError::secret_reference_message(
            "session credential has no active security domain",
        )
    })?;
    session
        .validate_reference(reference, security_domain_id)
        .map_err(|_| SecretHttpError::secret_reference_message("session credential is unavailable"))
}

fn sanitized_response_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (name.as_str().to_string(), value.to_string()))
        })
        .collect()
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

// Static, fail-closed snapshot of IPv4 space whose /8 status is ALLOCATED or
// LEGACY, with longest-prefix overrides from the complete special-purpose
// registry. Unlisted, reserved, and future-use space stays denied until these
// tables and their one-to-one representative tests are deliberately updated.
//
// Sources (registry last-updated dates at the time of review):
// - IANA IPv4 Address Space, 2025-10-10
//   https://www.iana.org/assignments/ipv4-address-space/
// - IANA IPv4 Special-Purpose Address Space, 2025-10-09
//   https://www.iana.org/assignments/iana-ipv4-special-registry/
const IANA_ALLOCATED_OR_LEGACY_IPV4_FIRST_OCTET_RANGES: &[(u8, u8)] =
    &[(1, 9), (11, 126), (128, 223)];

// The boolean is the registry's Globally Reachable value. A blank value, as
// on deprecated 192.88.99.0/24, is not affirmative and therefore fails closed.
// The two true /32 entries inside non-global 192.0.0.0/24 are intentional
// globally reachable exceptions selected by longest-prefix match.
const IANA_IPV4_SPECIAL_PURPOSE_PREFIXES: &[(Ipv4Addr, u8, bool)] = &[
    (Ipv4Addr::new(0, 0, 0, 0), 8, false),
    (Ipv4Addr::new(0, 0, 0, 0), 32, false),
    (Ipv4Addr::new(10, 0, 0, 0), 8, false),
    (Ipv4Addr::new(100, 64, 0, 0), 10, false),
    (Ipv4Addr::new(127, 0, 0, 0), 8, false),
    (Ipv4Addr::new(169, 254, 0, 0), 16, false),
    (Ipv4Addr::new(172, 16, 0, 0), 12, false),
    (Ipv4Addr::new(192, 0, 0, 0), 24, false),
    (Ipv4Addr::new(192, 0, 0, 0), 29, false),
    (Ipv4Addr::new(192, 0, 0, 8), 32, false),
    // Globally reachable exceptions within 192.0.0.0/24.
    (Ipv4Addr::new(192, 0, 0, 9), 32, true),
    (Ipv4Addr::new(192, 0, 0, 10), 32, true),
    // IANA lists these two /32s in one NAT64/DNS64 Discovery row.
    (Ipv4Addr::new(192, 0, 0, 170), 32, false),
    (Ipv4Addr::new(192, 0, 0, 171), 32, false),
    (Ipv4Addr::new(192, 0, 2, 0), 24, false),
    (Ipv4Addr::new(192, 31, 196, 0), 24, true),
    (Ipv4Addr::new(192, 52, 193, 0), 24, true),
    (Ipv4Addr::new(192, 88, 99, 0), 24, false),
    (Ipv4Addr::new(192, 88, 99, 2), 32, false),
    (Ipv4Addr::new(192, 168, 0, 0), 16, false),
    (Ipv4Addr::new(192, 175, 48, 0), 24, true),
    (Ipv4Addr::new(198, 18, 0, 0), 15, false),
    (Ipv4Addr::new(198, 51, 100, 0), 24, false),
    (Ipv4Addr::new(203, 0, 113, 0), 24, false),
    (Ipv4Addr::new(240, 0, 0, 0), 4, false),
    (Ipv4Addr::new(255, 255, 255, 255), 32, false),
];

fn ipv4_prefix_contains(prefix: Ipv4Addr, prefix_length: u8, candidate: Ipv4Addr) -> bool {
    let shift = 32_u32 - u32::from(prefix_length);
    u32::from_be_bytes(prefix.octets()) >> shift == u32::from_be_bytes(candidate.octets()) >> shift
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let first_octet = ip.octets()[0];
    let is_allocated_or_legacy = IANA_ALLOCATED_OR_LEGACY_IPV4_FIRST_OCTET_RANGES
        .iter()
        .any(|(start, end)| (*start..=*end).contains(&first_octet));
    if !is_allocated_or_legacy {
        return false;
    }

    IANA_IPV4_SPECIAL_PURPOSE_PREFIXES
        .iter()
        .filter(|(prefix, prefix_length, _)| ipv4_prefix_contains(*prefix, *prefix_length, ip))
        .max_by_key(|(_, prefix_length, _)| *prefix_length)
        .is_none_or(|(_, _, globally_reachable)| *globally_reachable)
}

// Static, fail-closed snapshot of allocated IPv6 global-unicast space that is
// also globally reachable. Unlisted space stays denied until this table and
// its exhaustive representative-address test are deliberately updated.
//
// Sources (registry last-updated dates at the time of review):
// - IANA IPv6 Global Unicast Address Assignments, 2025-10-10
//   https://www.iana.org/assignments/ipv6-unicast-address-assignments/
// - IANA IPv6 Special-Purpose Address Space, 2025-10-09
//   https://www.iana.org/assignments/iana-ipv6-special-registry/
const IANA_GLOBALLY_REACHABLE_IPV6_PREFIXES: &[(Ipv6Addr, u8)] = &[
    // Globally reachable special-purpose allocations within 2001::/23.
    (Ipv6Addr::new(0x2001, 0x0001, 0, 0, 0, 0, 0, 1), 128),
    (Ipv6Addr::new(0x2001, 0x0001, 0, 0, 0, 0, 0, 2), 128),
    (Ipv6Addr::new(0x2001, 0x0001, 0, 0, 0, 0, 0, 3), 128),
    (Ipv6Addr::new(0x2001, 0x0003, 0, 0, 0, 0, 0, 0), 32),
    (Ipv6Addr::new(0x2001, 0x0004, 0x0112, 0, 0, 0, 0, 0), 48),
    (Ipv6Addr::new(0x2001, 0x0020, 0, 0, 0, 0, 0, 0), 28),
    (Ipv6Addr::new(0x2001, 0x0030, 0, 0, 0, 0, 0, 0), 28),
    // Allocated global-unicast prefixes outside 2001::/23. The parent
    // 2001::/23 and 2002::/16 are intentionally absent because their IANA
    // global-reachability status is not true as a whole.
    (Ipv6Addr::new(0x2001, 0x0200, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x0400, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x0600, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x0800, 0, 0, 0, 0, 0, 0), 22),
    (Ipv6Addr::new(0x2001, 0x0c00, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x0e00, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x1200, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x1400, 0, 0, 0, 0, 0, 0), 22),
    (Ipv6Addr::new(0x2001, 0x1800, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x1a00, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x1c00, 0, 0, 0, 0, 0, 0), 22),
    (Ipv6Addr::new(0x2001, 0x2000, 0, 0, 0, 0, 0, 0), 19),
    (Ipv6Addr::new(0x2001, 0x4000, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4200, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4400, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4600, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4800, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4a00, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x4c00, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2001, 0x5000, 0, 0, 0, 0, 0, 0), 20),
    (Ipv6Addr::new(0x2001, 0x8000, 0, 0, 0, 0, 0, 0), 19),
    (Ipv6Addr::new(0x2001, 0xa000, 0, 0, 0, 0, 0, 0), 20),
    (Ipv6Addr::new(0x2001, 0xb000, 0, 0, 0, 0, 0, 0), 20),
    (Ipv6Addr::new(0x2003, 0, 0, 0, 0, 0, 0, 0), 18),
    (Ipv6Addr::new(0x2400, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2410, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2600, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2610, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2620, 0, 0, 0, 0, 0, 0, 0), 23),
    (Ipv6Addr::new(0x2630, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2800, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2a00, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2a10, 0, 0, 0, 0, 0, 0, 0), 12),
    (Ipv6Addr::new(0x2c00, 0, 0, 0, 0, 0, 0, 0), 12),
];

fn ipv6_prefix_contains(prefix: Ipv6Addr, prefix_length: u8, candidate: Ipv6Addr) -> bool {
    let shift = 128_u32 - u32::from(prefix_length);
    u128::from_be_bytes(prefix.octets()) >> shift
        == u128::from_be_bytes(candidate.octets()) >> shift
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4() {
        return is_public_ipv4(ipv4);
    }

    let is_globally_reachable_allocation = IANA_GLOBALLY_REACHABLE_IPV6_PREFIXES
        .iter()
        .any(|(prefix, prefix_length)| ipv6_prefix_contains(*prefix, *prefix_length, ip));
    let is_documentation =
        ipv6_prefix_contains(Ipv6Addr::new(0x2001, 0x0db8, 0, 0, 0, 0, 0, 0), 32, ip);

    is_globally_reachable_allocation
        && !is_documentation
        && !ip.is_loopback()
        && !ip.is_unspecified()
        && !ip.is_multicast()
        && !ip.is_unique_local()
        && !ip.is_unicast_link_local()
}

#[cfg(feature = "test-host")]
pub mod test_host {
    use super::*;

    pub struct TestEnvironmentValue(TestEnvironmentValueInner);

    enum TestEnvironmentValueInner {
        Unicode(Zeroizing<String>),
        Missing,
        NonUnicode,
    }

    pub struct TestSecretsHttpHost {
        executor: SecretsHttpExecutor,
        metrics: Arc<TestHostMetrics>,
    }

    struct TestCredentialSource {
        values: BTreeMap<String, TestEnvironmentValueInner>,
        metrics: Arc<TestHostMetrics>,
    }

    impl TestEnvironmentValue {
        pub fn unicode(value: String) -> Self {
            Self(TestEnvironmentValueInner::Unicode(Zeroizing::new(value)))
        }

        pub fn missing() -> Self {
            Self(TestEnvironmentValueInner::Missing)
        }

        pub fn non_unicode() -> Self {
            Self(TestEnvironmentValueInner::NonUnicode)
        }
    }

    impl fmt::Debug for TestEnvironmentValue {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            let _ = self;
            formatter.write_str("<test-environment-value>")
        }
    }

    impl TestSecretsHttpHost {
        pub fn new(
            host: &str,
            address: SocketAddr,
            environment: BTreeMap<String, TestEnvironmentValue>,
        ) -> Self {
            assert!(
                address.ip().is_loopback(),
                "test-host resolver may authorize only its unique loopback fixture"
            );
            let canonical = DestinationGrant::new(host, address.port())
                .expect("test-host destination must be a valid exact destination");
            let metrics = Arc::new(TestHostMetrics::default());
            let values = environment
                .into_iter()
                .map(|(name, value)| (name, value.0))
                .collect();
            let source = Arc::new(TestCredentialSource {
                values,
                metrics: Arc::clone(&metrics),
            });
            let executor = SecretsHttpExecutor {
                inner: Arc::new(SecretsHttpExecutorInner {
                    source,
                    resolver: DestinationResolver::Test(TestDestinationResolver {
                        host: canonical.host().to_string(),
                        address,
                    }),
                    metrics: Some(Arc::clone(&metrics)),
                }),
            };
            Self { executor, metrics }
        }

        pub fn executor(&self) -> SecretsHttpExecutor {
            self.executor.clone()
        }

        pub fn credential_resolution_count(&self) -> usize {
            self.metrics
                .credential_resolutions
                .load(std::sync::atomic::Ordering::Acquire)
        }

        pub fn environment_source_access_count(&self) -> usize {
            self.metrics
                .environment_source_accesses
                .load(std::sync::atomic::Ordering::Acquire)
        }
    }

    impl DeferredCredentialSource for TestCredentialSource {
        fn resolve_environment(
            &self,
            name: &str,
        ) -> Result<Zeroizing<String>, DeferredCredentialError> {
            self.metrics
                .environment_source_accesses
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
            match self.values.get(name) {
                Some(TestEnvironmentValueInner::Unicode(value)) => {
                    Ok(Zeroizing::new(value.to_string()))
                }
                Some(TestEnvironmentValueInner::NonUnicode) => {
                    Err(DeferredCredentialError::NonUnicodeEnvironment)
                }
                Some(TestEnvironmentValueInner::Missing) | None => {
                    Err(DeferredCredentialError::MissingEnvironment)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use ricochet_application::SecretName;

    use super::*;
    use crate::DeferredSecretSource;

    struct CountingCredentialSource {
        source_accesses: Arc<AtomicUsize>,
    }

    impl DeferredCredentialSource for CountingCredentialSource {
        fn resolve_environment(
            &self,
            _name: &str,
        ) -> Result<Zeroizing<String>, DeferredCredentialError> {
            self.source_accesses.fetch_add(1, Ordering::AcqRel);
            Ok(Zeroizing::new("must-not-resolve".to_string()))
        }
    }

    #[test]
    fn deferred_http_address_policy_admits_only_public_destinations() {
        for address in ["93.184.216.34", "2606:4700:4700::1111"] {
            assert!(
                is_public_ip(address.parse().expect("public fixture should parse")),
                "public address should be admitted: {address}"
            );
        }
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "100.64.0.1",
            "192.0.2.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "fec0::1",
            "2001:db8::1",
            "3fff::",
            "3fff::1",
            "3fff:fff:ffff:ffff:ffff:ffff:ffff:ffff",
        ] {
            assert!(
                !is_public_ip(address.parse().expect("restricted fixture should parse")),
                "restricted address should be denied: {address}"
            );
        }
    }

    #[test]
    fn deferred_http_ipv4_non_global_destination_is_denied_before_credential_resolution_or_send() {
        let source_accesses = Arc::new(AtomicUsize::new(0));
        let host = "non-global-ipv4.example.test";
        let port = 443;
        let resolved_address = SocketAddr::new(Ipv4Addr::new(192, 88, 99, 2).into(), port);
        let executor = SecretsHttpExecutor {
            inner: Arc::new(SecretsHttpExecutorInner {
                source: Arc::new(CountingCredentialSource {
                    source_accesses: Arc::clone(&source_accesses),
                }),
                resolver: DestinationResolver::FixedForAddressPolicyTest(resolved_address),
                #[cfg(feature = "test-host")]
                metrics: None,
            }),
        };
        let credentials = DeferredHttpCredentials::bearer(DeferredSecretSource::environment(
            SecretName::parse("provider.api-key").expect("fixture name should parse"),
        ));
        let allowed_hosts = Some(BTreeSet::from([host.to_string()]));
        let allowed_schemes = Some(BTreeSet::from(["https".to_string()]));
        let allowed_destinations = BTreeSet::from([
            DestinationGrant::new(host, port).expect("fixture destination should be valid")
        ]);
        let policy = SecretHttpPolicySnapshot::new(
            true,
            allowed_hosts.clone(),
            allowed_destinations,
            EnvironmentCredentialPolicy::new(true, None),
        );

        let error = executor
            .prepare(
                credentials,
                reqwest::Method::GET,
                format!("https://{host}:{port}/must-not-send"),
                HeaderMap::new(),
                None,
                None,
                Duration::from_millis(50),
                1024,
                allowed_hosts,
                allowed_schemes,
                policy,
            )
            .expect_err("non-global IPv4 must be rejected before a request can be executed");

        assert_eq!(error.kind(), PERMISSION_ERROR);
        assert_eq!(
            source_accesses.load(Ordering::Acquire),
            0,
            "address denial must happen before credential source access"
        );
    }

    #[test]
    fn deferred_http_ipv4_policy_fails_closed_to_iana_registries() {
        let allocated_or_legacy_representatives = [
            ((1, 9), Ipv4Addr::new(8, 8, 8, 8)),
            ((11, 126), Ipv4Addr::new(93, 184, 216, 34)),
            ((128, 223), Ipv4Addr::new(203, 1, 1, 1)),
        ];
        assert_eq!(
            IANA_ALLOCATED_OR_LEGACY_IPV4_FIRST_OCTET_RANGES.len(),
            allocated_or_legacy_representatives.len(),
            "updating the IANA allocation ranges requires updating their representatives"
        );
        for (expected_range, representative) in allocated_or_legacy_representatives {
            assert_eq!(
                IANA_ALLOCATED_OR_LEGACY_IPV4_FIRST_OCTET_RANGES
                    .iter()
                    .filter(|range| **range == expected_range)
                    .count(),
                1,
                "allocated range should have exactly one representative: {expected_range:?}"
            );
            assert!(
                is_public_ipv4(representative),
                "ordinary allocated address should be admitted: {representative}"
            );
        }

        for first_octet in 0_u8..=u8::MAX {
            let address = Ipv4Addr::new(first_octet, 1, 1, 1);
            let allocated_or_legacy = matches!(first_octet, 1..=9 | 11..=126 | 128..=223);
            assert_eq!(
                is_public_ipv4(address),
                allocated_or_legacy,
                "ordinary representative must follow the IANA /8 status: {address}"
            );
        }

        let special_purpose_representatives = [
            (
                "this network",
                Ipv4Addr::new(0, 0, 0, 0),
                8,
                Ipv4Addr::new(0, 1, 2, 3),
                false,
            ),
            (
                "this host",
                Ipv4Addr::new(0, 0, 0, 0),
                32,
                Ipv4Addr::new(0, 0, 0, 0),
                false,
            ),
            (
                "private-use",
                Ipv4Addr::new(10, 0, 0, 0),
                8,
                Ipv4Addr::new(10, 0, 0, 1),
                false,
            ),
            (
                "shared address space",
                Ipv4Addr::new(100, 64, 0, 0),
                10,
                Ipv4Addr::new(100, 64, 0, 1),
                false,
            ),
            (
                "loopback",
                Ipv4Addr::new(127, 0, 0, 0),
                8,
                Ipv4Addr::new(127, 0, 0, 1),
                false,
            ),
            (
                "link-local",
                Ipv4Addr::new(169, 254, 0, 0),
                16,
                Ipv4Addr::new(169, 254, 0, 1),
                false,
            ),
            (
                "private-use",
                Ipv4Addr::new(172, 16, 0, 0),
                12,
                Ipv4Addr::new(172, 16, 0, 1),
                false,
            ),
            (
                "IETF protocol assignments",
                Ipv4Addr::new(192, 0, 0, 0),
                24,
                Ipv4Addr::new(192, 0, 0, 11),
                false,
            ),
            (
                "service continuity",
                Ipv4Addr::new(192, 0, 0, 0),
                29,
                Ipv4Addr::new(192, 0, 0, 1),
                false,
            ),
            (
                "dummy",
                Ipv4Addr::new(192, 0, 0, 8),
                32,
                Ipv4Addr::new(192, 0, 0, 8),
                false,
            ),
            (
                "PCP anycast",
                Ipv4Addr::new(192, 0, 0, 9),
                32,
                Ipv4Addr::new(192, 0, 0, 9),
                true,
            ),
            (
                "TURN anycast",
                Ipv4Addr::new(192, 0, 0, 10),
                32,
                Ipv4Addr::new(192, 0, 0, 10),
                true,
            ),
            (
                "NAT64 discovery",
                Ipv4Addr::new(192, 0, 0, 170),
                32,
                Ipv4Addr::new(192, 0, 0, 170),
                false,
            ),
            (
                "NAT64 discovery",
                Ipv4Addr::new(192, 0, 0, 171),
                32,
                Ipv4Addr::new(192, 0, 0, 171),
                false,
            ),
            (
                "TEST-NET-1",
                Ipv4Addr::new(192, 0, 2, 0),
                24,
                Ipv4Addr::new(192, 0, 2, 1),
                false,
            ),
            (
                "AS112-v4",
                Ipv4Addr::new(192, 31, 196, 0),
                24,
                Ipv4Addr::new(192, 31, 196, 1),
                true,
            ),
            (
                "AMT",
                Ipv4Addr::new(192, 52, 193, 0),
                24,
                Ipv4Addr::new(192, 52, 193, 1),
                true,
            ),
            (
                "deprecated 6to4",
                Ipv4Addr::new(192, 88, 99, 0),
                24,
                Ipv4Addr::new(192, 88, 99, 1),
                false,
            ),
            (
                "6a44 relay",
                Ipv4Addr::new(192, 88, 99, 2),
                32,
                Ipv4Addr::new(192, 88, 99, 2),
                false,
            ),
            (
                "private-use",
                Ipv4Addr::new(192, 168, 0, 0),
                16,
                Ipv4Addr::new(192, 168, 0, 1),
                false,
            ),
            (
                "direct delegation AS112",
                Ipv4Addr::new(192, 175, 48, 0),
                24,
                Ipv4Addr::new(192, 175, 48, 1),
                true,
            ),
            (
                "benchmarking",
                Ipv4Addr::new(198, 18, 0, 0),
                15,
                Ipv4Addr::new(198, 18, 0, 1),
                false,
            ),
            (
                "TEST-NET-2",
                Ipv4Addr::new(198, 51, 100, 0),
                24,
                Ipv4Addr::new(198, 51, 100, 1),
                false,
            ),
            (
                "TEST-NET-3",
                Ipv4Addr::new(203, 0, 113, 0),
                24,
                Ipv4Addr::new(203, 0, 113, 1),
                false,
            ),
            (
                "reserved",
                Ipv4Addr::new(240, 0, 0, 0),
                4,
                Ipv4Addr::new(240, 0, 0, 1),
                false,
            ),
            (
                "limited broadcast",
                Ipv4Addr::new(255, 255, 255, 255),
                32,
                Ipv4Addr::new(255, 255, 255, 255),
                false,
            ),
        ];
        assert_eq!(
            IANA_IPV4_SPECIAL_PURPOSE_PREFIXES.len(),
            special_purpose_representatives.len(),
            "updating the IANA special-purpose table requires updating its representatives"
        );
        for (classification, prefix, prefix_length, address, globally_reachable) in
            special_purpose_representatives
        {
            let actual = IANA_IPV4_SPECIAL_PURPOSE_PREFIXES
                .iter()
                .filter(|(candidate_prefix, candidate_length, _)| {
                    ipv4_prefix_contains(*candidate_prefix, *candidate_length, address)
                })
                .max_by_key(|(_, candidate_length, _)| *candidate_length)
                .expect("special-purpose representative should match its registry entry");
            assert_eq!(
                *actual,
                (prefix, prefix_length, globally_reachable),
                "fixture should represent exactly one most-specific IANA entry: {classification} ({address})"
            );
            assert_eq!(
                is_public_ipv4(address),
                globally_reachable,
                "special-purpose reachability must match IANA: {classification} ({address})"
            );
        }
    }

    #[test]
    fn deferred_http_ipv6_policy_fails_closed_to_iana_allocations() {
        let allocated_and_globally_reachable = [
            ("2001:1::1/128 PCP anycast", "2001:1::1"),
            ("2001:1::2/128 TURN anycast", "2001:1::2"),
            ("2001:1::3/128 DNS-SD anycast", "2001:1::3"),
            ("2001:3::/32 AMT", "2001:3::1"),
            ("2001:4:112::/48 AS112", "2001:4:112::1"),
            ("2001:20::/28 ORCHIDv2", "2001:20::1"),
            ("2001:30::/28 DET", "2001:30::1"),
            ("2001:200::/23 APNIC", "2001:200::1"),
            ("2001:400::/23 ARIN", "2001:400::1"),
            ("2001:600::/23 RIPE", "2001:600::1"),
            ("2001:800::/22 RIPE", "2001:800::1"),
            ("2001:c00::/23 APNIC", "2001:c00::1"),
            ("2001:e00::/23 APNIC", "2001:e00::1"),
            ("2001:1200::/23 LACNIC", "2001:1200::1"),
            ("2001:1400::/22 RIPE", "2001:1400::1"),
            ("2001:1800::/23 ARIN", "2001:1800::1"),
            ("2001:1a00::/23 RIPE", "2001:1a00::1"),
            ("2001:1c00::/22 RIPE", "2001:1c00::1"),
            ("2001:2000::/19 RIPE", "2001:2000::1"),
            ("2001:4000::/23 RIPE", "2001:4000::1"),
            ("2001:4200::/23 AFRINIC", "2001:4200::1"),
            ("2001:4400::/23 APNIC", "2001:4400::1"),
            ("2001:4600::/23 RIPE", "2001:4600::1"),
            ("2001:4800::/23 ARIN", "2001:4800::1"),
            ("2001:4a00::/23 RIPE", "2001:4a00::1"),
            ("2001:4c00::/23 RIPE", "2001:4c00::1"),
            ("2001:5000::/20 RIPE", "2001:5000::1"),
            ("2001:8000::/19 APNIC", "2001:8000::1"),
            ("2001:a000::/20 APNIC", "2001:a000::1"),
            ("2001:b000::/20 APNIC", "2001:b000::1"),
            ("2003::/18 RIPE", "2003::1"),
            ("2400::/12 APNIC", "2400::1"),
            ("2410::/12 APNIC", "2410::1"),
            ("2600::/12 ARIN", "2600::1"),
            ("2610::/23 ARIN", "2610::1"),
            ("2620::/23 ARIN", "2620::1"),
            ("2630::/12 ARIN", "2630::1"),
            ("2800::/12 LACNIC", "2800::1"),
            ("2a00::/12 RIPE", "2a00::1"),
            ("2a10::/12 RIPE", "2a10::1"),
            ("2c00::/12 AFRINIC", "2c00::1"),
        ];
        assert_eq!(
            IANA_GLOBALLY_REACHABLE_IPV6_PREFIXES.len(),
            allocated_and_globally_reachable.len(),
            "updating the IANA allocation table requires updating its representative fixtures"
        );
        for (allocation, address) in allocated_and_globally_reachable {
            let address: Ipv6Addr = address.parse().expect("allocated fixture should parse");
            let matching_prefixes = IANA_GLOBALLY_REACHABLE_IPV6_PREFIXES
                .iter()
                .filter(|(prefix, prefix_length)| {
                    ipv6_prefix_contains(*prefix, *prefix_length, address)
                })
                .count();
            assert_eq!(
                matching_prefixes, 1,
                "fixture should represent exactly one admitted IANA prefix: {allocation} ({address})"
            );
            assert!(
                is_public_ipv6(address),
                "allocated globally reachable address should be admitted: {allocation} ({address})"
            );
        }

        let reserved_or_not_globally_reachable = [
            ("returned 6bone space", "3ffe::1"),
            ("unallocated space above documentation /20", "3fff:1000::1"),
            ("broad IANA reserved block", "3000::1"),
            ("unallocated hole inside 2001::/16", "2001:1000::1"),
            ("Teredo not globally reachable", "2001::1"),
            ("benchmarking", "2001:2::1"),
            ("deprecated ORCHID", "2001:10::1"),
            ("RFC 3849 documentation", "2001:db8::1"),
            ("6to4 not globally reachable", "2002::1"),
            ("RFC 9637 documentation", "3fff::1"),
        ];
        for (classification, address) in reserved_or_not_globally_reachable {
            assert!(
                !is_public_ip(address.parse().expect("reserved fixture should parse")),
                "reserved or non-global address should be denied: {classification} ({address})"
            );
        }
    }
}
