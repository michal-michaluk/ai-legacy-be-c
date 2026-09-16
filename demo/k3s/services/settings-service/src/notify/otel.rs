//! OpenTelemetry trace wiring: OTLP/HTTP span export to the collector, plus
//! W3C trace-context inject/extract over MQTT 5 User Properties.
//!
//! Trace context crosses the broker as the de-facto MQTT convention documented
//! in `docs/specs/kubernetes-native/trace-context-convention.md`: two MQTT 5
//! User Properties named exactly `traceparent` and `tracestate`. Mosquitto
//! forwards them unmodified; this module produces them on publish and consumes
//! them on delivery, so a producer span and a consumer span share one trace id.

use std::collections::HashMap;
use std::sync::OnceLock;

use opentelemetry::global;
use opentelemetry::trace::{SpanBuilder, SpanKind, TraceContextExt, Tracer};
use opentelemetry::{Context, KeyValue};
use opentelemetry_otlp::{SpanExporter, WithExportConfig};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::trace::SdkTracerProvider;
use opentelemetry_sdk::Resource;

const SCOPE: &str = "settings-service";

static PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();

// Carries an explicit parent context across `await` points (an HTTP request's
// SERVER span), so the MQTT PRODUCER span created deeper in the call stack
// becomes its child instead of a new root. A thread-local `ContextGuard` cannot
// be held across awaits in an axum handler, so a task-local is used instead.
tokio::task_local! {
    static PARENT_CX: Context;
}

/// Install the global tracer provider (OTLP/HTTP → `<base>/v1/traces`) and the
/// W3C `traceparent`/`tracestate` propagator. Failure to build the exporter is
/// non-fatal: the service runs, only span export is disabled.
pub fn init(endpoint_base: &str, service_name: &str) {
    global::set_text_map_propagator(TraceContextPropagator::new());

    let endpoint = format!("{}/v1/traces", endpoint_base.trim_end_matches('/'));
    let exporter = match SpanExporter::builder()
        .with_http()
        .with_endpoint(endpoint)
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(error = %e, "OTLP span exporter init failed; spans disabled");
            return;
        }
    };
    let provider = SdkTracerProvider::builder()
        .with_resource(
            Resource::builder()
                .with_service_name(service_name.to_string())
                .build(),
        )
        .with_batch_exporter(exporter)
        .build();
    global::set_tracer_provider(provider.clone());
    let _ = PROVIDER.set(provider);
}

/// Start a `Producer` span and inject its W3C trace context into `carrier`
/// (which becomes the publish's MQTT 5 User Properties). When a parent context
/// is scoped for the current task (`with_parent`), the producer becomes its
/// child; otherwise it is a new root.
pub fn producer_context(name: &str, carrier: &mut HashMap<String, String>) -> Context {
    let tracer = global::tracer(SCOPE);
    let parent = PARENT_CX
        .try_with(|cx| cx.clone())
        .unwrap_or_else(|_| Context::current());
    let span = tracer.build_with_context(
        SpanBuilder::from_name(name.to_string())
            .with_kind(SpanKind::Producer)
            .with_attributes([KeyValue::new("messaging.system", "mqtt")]),
        &parent,
    );
    let cx = parent.with_span(span);
    global::get_text_map_propagator(|p| p.inject_context(&cx, carrier));
    cx
}

/// Continue an incoming request's trace (from its HTTP headers) and start a
/// `Server` span, or start a new root when no valid `traceparent` is present.
pub fn server_context(name: &str, carrier: &HashMap<String, String>) -> Context {
    let parent = global::get_text_map_propagator(|p| p.extract(carrier));
    let tracer = global::tracer(SCOPE);
    let span = tracer.build_with_context(
        SpanBuilder::from_name(name.to_string())
            .with_kind(SpanKind::Server)
            .with_attributes([KeyValue::new("span.kind.name", "server")]),
        &parent,
    );
    parent.with_span(span)
}

/// Run `fut` with `cx` as the task's parent context (see `PARENT_CX`).
pub async fn with_parent<F: std::future::Future>(cx: Context, fut: F) -> F::Output {
    PARENT_CX.scope(cx, fut).await
}

/// Extract the parent from `carrier` and start a `Consumer` span as its child,
/// so the receiving side continues the producer's trace.
pub fn consumer_context(carrier: &HashMap<String, String>) -> Context {
    let parent = global::get_text_map_propagator(|p| p.extract(carrier));
    let tracer = global::tracer(SCOPE);
    let span = tracer.build_with_context(
        SpanBuilder::from_name("mqtt.receive".to_string())
            .with_kind(SpanKind::Consumer)
            .with_attributes([KeyValue::new("messaging.system", "mqtt")]),
        &parent,
    );
    parent.with_span(span)
}

pub fn trace_id(cx: &Context) -> String {
    cx.span().span_context().trace_id().to_string()
}

pub fn end(cx: &Context) {
    cx.span().end();
}

/// Flush buffered spans so a short-lived demo observes them promptly.
pub fn flush() {
    if let Some(p) = PROVIDER.get() {
        let _ = p.force_flush();
    }
}
