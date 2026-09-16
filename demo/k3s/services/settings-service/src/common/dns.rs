//! Pure-Rust DNS resolver for the `FROM scratch` runtime.
//!
//! On a scratch image there is **no** `/etc/nsswitch.conf` and, when run outside
//! Kubernetes, no `/etc/resolv.conf` either — so the C library's
//! `getaddrinfo` cannot name-resolve. musl's resolver reads `/etc/resolv.conf`
//! (glibc would also touch `nsswitch.conf`), which is absent from scratch.
//!
//! This module wraps a `hickory-resolver` (pure-Rust DNS) behind the
//! `reqwest::dns::Resolve` trait, so the JWKS and OTLP client never call libc
//! name resolution. Only meaningful on Linux/musl (scratch); on a normal host
//! the system resolver is fine and cheaper, but this is harmless there too.
//!
//! In Kubernetes the kubelet injects `/etc/resolv.conf` for every pod (scratch
//! included), so system DNS *would* work there — this covers the bare-container
//! / non-K8s scratch case, which is the one that actually breaks.

use std::net::SocketAddr;
use std::sync::Arc;

use hickory_resolver::config::ResolverConfig;
use hickory_resolver::TokioResolver;

/// reqwest DNS resolver backed by hickory (pure-Rust, no libc `getaddrinfo`).
struct HickoryResolver {
    resolver: TokioResolver,
}

impl reqwest::dns::Resolve for HickoryResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let resolver = self.resolver.clone();
        let name = name.as_str().to_string();
        Box::pin(async move {
            let lookup = resolver.lookup_ip(name.as_str()).await.map_err(
                |e| -> Box<dyn std::error::Error + Send + Sync> {
                    std::io::Error::other(e.to_string()).into()
                },
            )?;
            let addrs = lookup
                .iter()
                .map(|ip| SocketAddr::new(ip, 0))
                .collect::<Vec<_>>();
            Ok(Box::new(addrs.into_iter()) as Box<dyn Iterator<Item = SocketAddr> + Send>)
        })
    }
}

/// Build the pure-Rust resolver for `reqwest::ClientBuilder::dns_resolver`.
///
/// Uses the system nameservers (`/etc/resolv.conf`) when present; otherwise
/// falls back to Google Public DNS (`ResolverConfig::udp_and_tcp(&GOOGLE)`) —
/// the pure-Rust "no system config" path available on scratch. Never fails.
pub fn reqwest_resolver() -> Arc<impl reqwest::dns::Resolve + 'static> {
    let resolver = TokioResolver::builder_tokio()
        .ok()
        .and_then(|b| b.build().ok())
        .unwrap_or_else(|| {
            TokioResolver::builder_with_config(
                ResolverConfig::udp_and_tcp(&hickory_resolver::config::GOOGLE),
                hickory_resolver::net::runtime::TokioRuntimeProvider::default(),
            )
            .build()
            .expect("google DNS resolver config is valid")
        });
    Arc::new(HickoryResolver { resolver })
}
