//! The notification port and its MQTT adapter, plus the decorator that attaches
//! notifications to the settings service at the boundary.

use std::collections::HashMap;
use std::future::Future;
use std::time::Instant;

use rumqttc::v5::mqttbytes::v5::PublishProperties;
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::AsyncClient;

use crate::auth::Authority;
use crate::common::pagination::Pagination;
use crate::notify::model::ConfigChange;
use crate::notify::otel;
use crate::settings::{
    SearchPage, Setting, SettingKey, SettingValue, SettingsError, SettingsServicePort,
};

/// A sink for config-change notifications. Native async (RPITIT), generic
/// static dispatch — same convention as the settings ports.
pub trait SettingsNotifier: Send + Sync {
    fn notify(&self, change: ConfigChange) -> impl Future<Output = ()> + Send;
}

/// MQTT 5 notifier: publishes each change to `settings/<tenant>/<key>` and
/// injects the W3C trace context (`traceparent`/`tracestate`) as User
/// Properties on the publish (the broker forwards them unmodified — C3).
#[derive(Clone)]
pub struct MqttNotifier {
    client: AsyncClient,
}

impl MqttNotifier {
    pub fn new(client: AsyncClient) -> Self {
        Self { client }
    }
}

impl SettingsNotifier for MqttNotifier {
    async fn notify(&self, change: ConfigChange) {
        let topic = format!("settings/{}/{}", change.tenant, change.key);
        let mut carrier = HashMap::new();
        let cx = otel::producer_context("mqtt.publish", &mut carrier);
        let trace_id = otel::trace_id(&cx);
        tracing::info!(
            topic = %topic,
            trace_id = %trace_id,
            traceparent = ?carrier.get("traceparent"),
            op = %change.op,
            key = %change.key,
            version = change.version,
            "mqtt config-change notification published"
        );

        let mut props = PublishProperties::default();
        for (key, value) in &carrier {
            props.user_properties.push((key.clone(), value.clone()));
        }

        let payload = serde_json::to_vec(&change).unwrap_or_default();
        if let Err(e) = self
            .client
            .publish_with_properties(topic, QoS::AtLeastOnce, false, payload, props)
            .await
        {
            tracing::warn!(error = %e, "MQTT config-change publish failed");
        }
        otel::end(&cx);
    }
}

/// Decorator over any `SettingsServicePort` that emits a notification after a
/// successful mutation. Reads pass through untouched; only `put`/`patch`/
/// `delete` that succeed produce an event.
pub struct NotifyingSettingsService<S, N> {
    inner: S,
    notifier: N,
    service: String,
}

impl<S, N> NotifyingSettingsService<S, N> {
    pub fn new(inner: S, notifier: N, service: String) -> Self {
        Self {
            inner,
            notifier,
            service,
        }
    }

    fn upsert_change(&self, setting: &Setting) -> ConfigChange {
        let snapshot = setting.snapshot();
        ConfigChange {
            service: self.service.clone(),
            op: "upsert",
            tenant: snapshot.ownership.tenant().as_str().to_string(),
            key: snapshot.key.as_str().to_string(),
            value: Some(snapshot.value.as_value().clone()),
            version: snapshot.version,
        }
    }
}

impl<S, N> SettingsServicePort for NotifyingSettingsService<S, N>
where
    S: SettingsServicePort,
    N: SettingsNotifier,
{
    async fn get(&self, key: &SettingKey, authority: &Authority) -> Result<Setting, SettingsError> {
        self.inner.get(key, authority).await
    }

    async fn search(
        &self,
        query: &str,
        authority: &Authority,
        pagination: &Pagination,
    ) -> Result<SearchPage, SettingsError> {
        self.inner.search(query, authority, pagination).await
    }

    async fn put(
        &self,
        key: SettingKey,
        value: SettingValue,
        authority: &Authority,
        expected: Option<u64>,
        now: Instant,
    ) -> Result<Setting, SettingsError> {
        let result = self.inner.put(key, value, authority, expected, now).await;
        if let Ok(setting) = &result {
            self.notifier.notify(self.upsert_change(setting)).await;
        }
        result
    }

    async fn patch(
        &self,
        key: SettingKey,
        patch: SettingValue,
        authority: &Authority,
        expected: u64,
        now: Instant,
    ) -> Result<Setting, SettingsError> {
        let result = self.inner.patch(key, patch, authority, expected, now).await;
        if let Ok(setting) = &result {
            self.notifier.notify(self.upsert_change(setting)).await;
        }
        result
    }

    async fn delete(
        &self,
        key: SettingKey,
        authority: &Authority,
        expected: u64,
    ) -> Result<(), SettingsError> {
        self.inner.delete(key.clone(), authority, expected).await?;
        self.notifier
            .notify(ConfigChange {
                service: self.service.clone(),
                op: "delete",
                tenant: authority.tenant().as_str().to_string(),
                key: key.as_str().to_string(),
                value: None,
                version: 0,
            })
            .await;
        Ok(())
    }

    async fn ready(&self) -> Result<(), SettingsError> {
        self.inner.ready().await
    }
}
