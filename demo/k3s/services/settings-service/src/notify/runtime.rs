//! Runtime glue: build the MQTT client, drive its event loop, and (optionally)
//! emit a periodic demo config change per service instance.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rumqttc::v5::mqttbytes::v5::{Packet, Publish};
use rumqttc::v5::mqttbytes::QoS;
use rumqttc::v5::{AsyncClient, Event, EventLoop, MqttOptions};

use crate::auth::{Authority, Role, Tenant, UserId};
use crate::common::config::Config;
use crate::notify::otel;
use crate::settings::{SettingKey, SettingValue, SettingsServicePort};

/// Build the MQTT 5 client and its event loop from `Config`.
pub fn mqtt_client(config: &Config) -> (AsyncClient, EventLoop) {
    let mut options = MqttOptions::new(
        config.mqtt_client_id.clone(),
        config.mqtt_host.clone(),
        config.mqtt_port,
    );
    options.set_keep_alive(Duration::from_secs(30));
    AsyncClient::new(options, 64)
}

/// Subscribe to every config-change notification topic. The request is queued
/// and sent once the connection is established, so it is safe before `ConnAck`.
pub async fn subscribe_notifications(client: &AsyncClient) {
    if let Err(e) = client.subscribe("settings/#", QoS::AtLeastOnce).await {
        tracing::warn!(error = %e, "mqtt subscribe request failed");
    }
}

/// Drive the event loop forever; (re)subscribe on every `ConnAck` (the canonical
/// rumqttc pattern, robust across reconnects), and on every delivered PUBLISH
/// extract the W3C trace context from the MQTT 5 User Properties and start a
/// `Consumer` span, so the receiving service continues the producer's trace.
pub async fn run_event_loop(mut eventloop: EventLoop, client: AsyncClient, service: String) {
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                subscribe_notifications(&client).await;
            }
            Ok(Event::Incoming(Packet::Publish(publish))) => handle_publish(&publish, &service),
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(error = %e, "mqtt connection error; retrying in 2s");
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

fn handle_publish(publish: &Publish, service: &str) {
    let mut carrier: HashMap<String, String> = HashMap::new();
    if let Some(props) = &publish.properties {
        for (key, value) in &props.user_properties {
            carrier.insert(key.to_lowercase(), value.clone());
        }
    }
    let cx = otel::consumer_context(&carrier);
    let trace_id = otel::trace_id(&cx);
    let topic = String::from_utf8_lossy(&publish.topic);
    let payload = String::from_utf8_lossy(&publish.payload);
    tracing::info!(
        service = %service,
        topic = %topic,
        trace_id = %trace_id,
        traceparent = ?carrier.get("traceparent"),
        payload = %payload,
        "mqtt config-change notification received"
    );
    otel::end(&cx);
}

/// Periodic config-change producer used by the demo: each service instance
/// writes its own config key through the real service port (so the decorator
/// fires) at a fixed interval.
pub async fn demo_publisher<S>(
    settings: Arc<S>,
    interval: Duration,
    tenant: String,
    key: String,
    service: String,
) where
    S: SettingsServicePort + 'static,
{
    let authority = Authority::new(Tenant::new(&tenant), UserId::new("demo-loop"), Role::Admin);
    let mut counter: u64 = 0;
    loop {
        tokio::time::sleep(interval).await;
        counter += 1;
        let Ok(key_vo) = SettingKey::new(&key) else {
            continue;
        };
        let value = match SettingValue::new(
            serde_json::json!({ "counter": counter, "service": service }),
        ) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let expected = settings
            .get(&key_vo, &authority)
            .await
            .ok()
            .map(|s| s.version());
        match settings
            .put(key_vo, value, &authority, expected, Instant::now())
            .await
        {
            Ok(setting) => tracing::info!(
                service = %service,
                key = %key,
                version = setting.version(),
                "demo config change applied"
            ),
            Err(e) => tracing::warn!(service = %service, error = %e, "demo config change failed"),
        }
    }
}
