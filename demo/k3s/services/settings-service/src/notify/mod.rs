//! The `notify` slice — a config-change notification port and its MQTT adapter.
//!
//! It is deliberately a **separate slice** from `settings`: the settings core
//! stays free of `rumqttc`/OTel, and the notification is attached at the
//! boundary by a decorator (`NotifyingSettingsService`) — the pattern the
//! settings model documents for domain events ("the reference adapter attaches
//! them at the boundary").

mod demo_rest;
mod model;
mod notifier;
pub mod otel;
mod runtime;

pub use demo_rest::demo_routes;
pub use model::ConfigChange;
pub use notifier::{MqttNotifier, NotifyingSettingsService, SettingsNotifier};
pub use runtime::{demo_publisher, mqtt_client, run_event_loop, subscribe_notifications};
