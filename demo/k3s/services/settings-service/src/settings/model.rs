//! The `settings` bounded context's contextual object model: aggregate + value
//! objects + domain events + snapshots.
//!
//! **Pure synchronous domain code** — no `async`, no `axum`, `sqlx`, `tokio`, or
//! `serde` derive. `Ownership` comes from the `auth` kernel and is checked by
//! the service here. Time is passed in (`now`); there is no clock mocking
//! (ADR-006).

use std::time::Instant;

use serde_json::Value;

use crate::auth::Ownership;
use crate::settings::error::SettingsError;

/// A config `key`. Self-validating value object.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SettingKey(String);

impl SettingKey {
    pub fn new(s: &str) -> Result<Self, SettingsError> {
        let s = s.trim();
        if s.is_empty() || s.len() > 128 {
            return Err(SettingsError::InvalidKey);
        }
        Ok(Self(s.to_string()))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A config `value` — a JSON **object** (`jsonb`). Self-validating value object;
/// equality is by value (JSON has no NaN, so `Eq` is total).
#[derive(Clone, Debug)]
pub struct SettingValue(Value);

impl SettingValue {
    pub fn new(v: Value) -> Result<Self, SettingsError> {
        if !v.is_object() {
            return Err(SettingsError::InvalidValue);
        }
        if v.to_string().len() > MAX_VALUE_BYTES {
            return Err(SettingsError::InvalidValue);
        }
        Ok(Self(v))
    }

    pub fn as_value(&self) -> &Value {
        &self.0
    }

    pub fn into_value(self) -> Value {
        self.0
    }

    /// RFC 7396 JSON merge-patch: object merges recursively (`null` deletes a
    /// key); a non-object patch replaces the whole value.
    pub fn merge(&self, patch: &SettingValue) -> Result<Self, SettingsError> {
        Self::new(merge_patch(&self.0, &patch.0))
    }
}

/// `Eq` is sound because serde_json cannot construct `NaN`/`Infinity`.
impl PartialEq for SettingValue {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for SettingValue {}

const MAX_VALUE_BYTES: usize = 64 * 1024;

fn merge_patch(base: &Value, patch: &Value) -> Value {
    match (base, patch) {
        (Value::Object(b), Value::Object(p)) => {
            let mut out = b.clone();
            for (k, pv) in p {
                match pv {
                    Value::Null => {
                        out.remove(k);
                    }
                    _ => {
                        let merged = match out.get(k) {
                            Some(bv) => merge_patch(bv, pv),
                            None => pv.clone(),
                        };
                        out.insert(k.clone(), merged);
                    }
                }
            }
            Value::Object(out)
        }
        _ => patch.clone(),
    }
}

/// The settings **aggregate root** — the invariant owner; every change goes
/// through a business method that advances the optimistic `version` (§10).
#[derive(Clone, Debug)]
pub struct Setting {
    key: SettingKey,
    value: SettingValue,
    ownership: Ownership,
    version: u64,
    updated_at: Instant,
}

impl Setting {
    /// Construct a new aggregate (`version = 1`).
    pub fn new(
        key: SettingKey,
        value: SettingValue,
        ownership: Ownership,
        now: Instant,
    ) -> Result<Self, SettingsError> {
        Ok(Self {
            key,
            value,
            ownership,
            version: 1,
            updated_at: now,
        })
    }

    /// Reconstruct an aggregate from a persistence row (the adapter maps record
    /// -> aggregate). Invariants are re-checked at construction.
    pub fn restore(
        key: SettingKey,
        value: SettingValue,
        ownership: Ownership,
        version: u64,
        updated_at: Instant,
    ) -> Result<Self, SettingsError> {
        Ok(Self {
            key,
            value,
            ownership,
            version: version.max(1),
            updated_at,
        })
    }

    pub fn key(&self) -> &SettingKey {
        &self.key
    }
    pub fn value(&self) -> &SettingValue {
        &self.value
    }
    pub fn ownership(&self) -> &Ownership {
        &self.ownership
    }
    pub fn version(&self) -> u64 {
        self.version
    }
    pub fn updated_at(&self) -> Instant {
        self.updated_at
    }

    /// PUT: replace the whole value, advancing `version`.
    pub fn set_value(&mut self, value: SettingValue, now: Instant) -> Result<(), SettingsError> {
        self.value = value;
        self.version += 1;
        self.updated_at = now;
        Ok(())
    }

    /// PATCH: partial merge into the existing value (business rule on the
    /// model), advancing `version`.
    pub fn apply_partial(
        &mut self,
        patch: SettingValue,
        now: Instant,
    ) -> Result<(), SettingsError> {
        self.value = self.value.merge(&patch)?;
        self.version += 1;
        self.updated_at = now;
        Ok(())
    }

    /// Read-model snapshot — a **value object** (immutable, equality by value),
    /// never a transport type.
    pub fn snapshot(&self) -> SettingSnapshot {
        SettingSnapshot {
            key: self.key.clone(),
            value: self.value.clone(),
            ownership: self.ownership.clone(),
            version: self.version,
            updated_at: self.updated_at,
        }
    }
}

/// Immutable read model — a value object, `Clone + PartialEq + Eq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingSnapshot {
    pub key: SettingKey,
    pub value: SettingValue,
    pub ownership: Ownership,
    pub version: u64,
    pub updated_at: Instant,
}

/// Domain events — significant state changes (context-to-context reactivity via
/// a projection/mediator adapter, never a cross-context import). Kept as the
/// documented DDD pattern; the reference adapter attaches them at the boundary.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsEvent {
    SettingUpserted(SettingKey),
    SettingDeleted(SettingKey),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{Tenant, UserId};

    fn now() -> Instant {
        Instant::now()
    }
    fn own(tenant: &str, user: Option<&str>) -> Ownership {
        match user {
            Some(u) => Ownership::for_user(Tenant::new(tenant), UserId::new(u)),
            None => Ownership::for_tenant(Tenant::new(tenant)),
        }
    }

    #[test]
    fn empty_key_is_rejected() {
        assert!(matches!(
            SettingKey::new("  "),
            Err(SettingsError::InvalidKey)
        ));
    }

    #[test]
    fn overlong_key_is_rejected() {
        assert!(matches!(
            SettingKey::new(&"a".repeat(129)),
            Err(SettingsError::InvalidKey)
        ));
    }

    #[test]
    fn value_must_be_json_object() {
        assert!(SettingValue::new(serde_json::json!({"a": 1})).is_ok());
        assert!(matches!(
            SettingValue::new(serde_json::json!([1, 2, 3])),
            Err(SettingsError::InvalidValue)
        ));
        assert!(matches!(
            SettingValue::new(serde_json::json!("scalar")),
            Err(SettingsError::InvalidValue)
        ));
    }

    #[test]
    fn oversize_value_is_rejected() {
        let big = serde_json::json!({ "blob": "x".repeat(70 * 1024) });
        assert!(matches!(
            SettingValue::new(big),
            Err(SettingsError::InvalidValue)
        ));
    }

    #[test]
    fn merge_patch_deep_merges_and_deletes() {
        let base =
            SettingValue::new(serde_json::json!({"a": 1, "nested": {"x": 1, "y": 2}})).unwrap();
        let patch = SettingValue::new(serde_json::json!({"nested": {"y": null, "z": 3}})).unwrap();
        let merged = base.merge(&patch).unwrap();
        assert_eq!(
            merged.as_value(),
            &serde_json::json!({"a": 1, "nested": {"x": 1, "z": 3}})
        );
    }

    #[test]
    fn object_patch_merges_into_base() {
        let base = SettingValue::new(serde_json::json!({"a": 1})).unwrap();
        let patch = SettingValue::new(serde_json::json!({"b": 2})).unwrap();
        assert_eq!(
            base.merge(&patch).unwrap().as_value(),
            &serde_json::json!({"a": 1, "b": 2})
        );
    }

    #[test]
    fn aggregate_version_advances_on_mutation() {
        let mut s = Setting::new(
            SettingKey::new("theme").unwrap(),
            SettingValue::new(serde_json::json!({"color": "dark"})).unwrap(),
            own("acme", None),
            now(),
        )
        .unwrap();
        assert_eq!(s.version(), 1);
        s.set_value(
            SettingValue::new(serde_json::json!({"color": "light"})).unwrap(),
            now(),
        )
        .unwrap();
        assert_eq!(s.version(), 2);
        assert_eq!(s.snapshot().version, 2);
    }

    #[test]
    fn setting_value_equality_by_value() {
        let a = SettingValue::new(serde_json::json!({ "a": 1 })).unwrap();
        let b = SettingValue::new(serde_json::json!({ "a": 1 })).unwrap();
        assert_eq!(a, b);
        assert_ne!(a, SettingValue::new(serde_json::json!({ "a": 2 })).unwrap());
    }

    #[test]
    fn into_value_consumes() {
        let s = SettingValue::new(serde_json::json!({ "a": 1 })).unwrap();
        assert_eq!(s.into_value(), serde_json::json!({ "a": 1 }));
    }

    #[test]
    fn non_object_patch_replaces_child_value() {
        // base child is a scalar; patch child is an object -> RFC 7396 replaces
        // the whole child, exercising the `_ => patch.clone()` arm.
        let base = SettingValue::new(serde_json::json!({ "a": 1 })).unwrap();
        let patch = SettingValue::new(serde_json::json!({ "a": { "b": 2 } })).unwrap();
        assert_eq!(
            base.merge(&patch).unwrap().as_value(),
            &serde_json::json!({ "a": { "b": 2 } })
        );
    }

    #[test]
    fn merge_overflow_is_rejected() {
        // A merge that pushes the combined value past MAX_VALUE_BYTES must be
        // rejected by `SettingValue::new` (the `?` error path in `apply_partial`).
        let big = "x".repeat(63 * 1024);
        let base = SettingValue::new(serde_json::json!({ "a": big })).unwrap();
        let patch = SettingValue::new(serde_json::json!({ "b": "y".repeat(8 * 1024) })).unwrap();
        assert!(base.merge(&patch).is_err());
    }

    #[test]
    fn user_scoped_helper_produces_user_ownership() {
        let o = own("acme", Some("bob"));
        assert_eq!(o.user(), Some(&UserId::new("bob")));
        assert!(!o.is_tenant_scoped());
    }

    #[test]
    fn partial_merge_advances_version() {
        let mut s = Setting::new(
            SettingKey::new("app").unwrap(),
            SettingValue::new(serde_json::json!({"a": 1})).unwrap(),
            own("acme", None),
            now(),
        )
        .unwrap();
        s.apply_partial(
            SettingValue::new(serde_json::json!({"b": 2})).unwrap(),
            now(),
        )
        .unwrap();
        assert_eq!(
            s.snapshot().value.as_value(),
            &serde_json::json!({"a": 1, "b": 2})
        );
    }
}
