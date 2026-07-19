//! Backend-neutral JavaScript runtime, host-call, and selection contracts.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

/// Runtime selected for one document realm.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendKind {
    /// Production default.
    #[default]
    V8,
    /// Reference and explicit fallback backend.
    Boa,
}

impl BackendKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::V8 => "v8",
            Self::Boa => "boa",
        }
    }
}

/// Explicit policy used when the selected backend cannot initialize.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendPolicy {
    pub preferred: BackendKind,
    pub fallback: Option<BackendKind>,
}

impl Default for BackendPolicy {
    fn default() -> Self {
        Self {
            preferred: BackendKind::V8,
            fallback: None,
        }
    }
}

/// Failure categories shared by every backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureMode {
    BackendUnavailable,
    Initialization,
    Compile,
    Exception,
    Host,
    ResourceLimit,
}

/// Backend-neutral JavaScript failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeError {
    pub backend: BackendKind,
    pub mode: FailureMode,
    pub message: String,
    pub source_name: String,
}

impl RuntimeError {
    #[must_use]
    pub fn backend_unavailable(backend: BackendKind, message: impl Into<String>) -> Self {
        Self {
            backend,
            mode: FailureMode::BackendUnavailable,
            message: message.into(),
            source_name: "<initialization>".to_owned(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} {:?} failure in {}: {}",
            self.backend.as_str(),
            self.mode,
            self.source_name,
            self.message
        )
    }
}

impl Error for RuntimeError {}

/// One JavaScript source unit with stable diagnostic identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptSource {
    pub code: String,
    pub name: String,
}

impl ScriptSource {
    #[must_use]
    pub fn new(code: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            name: name.into(),
        }
    }
}

/// Values allowed to cross the engine/backend boundary.
#[derive(Clone, Debug, PartialEq)]
pub enum RuntimeValue {
    Undefined,
    Null,
    Boolean(bool),
    Number(f64),
    String(String),
    Object,
}

/// Backend-independent native host surface.
pub trait HostApi: fmt::Debug {
    fn call(&mut self, operation: &str, payload: &str) -> Result<String, String>;
}

/// One persistent JavaScript realm.
pub trait JsRuntime: fmt::Debug {
    fn backend(&self) -> BackendKind;
    fn execute(&mut self, source: &ScriptSource) -> Result<RuntimeValue, RuntimeError>;
}

/// Creates a runtime with one owned host API instance.
pub trait RuntimeFactory {
    type Runtime: JsRuntime;

    fn backend(&self) -> BackendKind;
    fn create(&self, host: Box<dyn HostApi>) -> Result<Self::Runtime, RuntimeError>;
}

/// Expected state used by the shared backend conformance suite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExpectedAvailability {
    Ready,
    Unavailable,
}

/// Compact result from the shared host/runtime conformance suite.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConformanceReport {
    pub backend: BackendKind,
    pub availability: ExpectedAvailability,
    pub assertions: usize,
}

#[derive(Debug, Default)]
struct EchoHost;

impl HostApi for EchoHost {
    fn call(&mut self, operation: &str, payload: &str) -> Result<String, String> {
        match operation {
            "echo" => Ok(payload.to_owned()),
            value => Err(format!("unsupported host operation {value}")),
        }
    }
}

/// Runs the same primitive, exception, and host-call probes for every adapter.
pub fn run_conformance_suite<F: RuntimeFactory>(
    factory: &F,
    expected: ExpectedAvailability,
) -> Result<ConformanceReport, RuntimeError> {
    let backend = factory.backend();
    let runtime = factory.create(Box::<EchoHost>::default());
    if expected == ExpectedAvailability::Unavailable {
        let error = runtime.expect_err("unavailable backend unexpectedly initialized");
        assert_eq!(error.backend, backend);
        assert_eq!(error.mode, FailureMode::BackendUnavailable);
        return Ok(ConformanceReport {
            backend,
            availability: expected,
            assertions: 2,
        });
    }

    let mut runtime = runtime?;
    assert_eq!(runtime.backend(), backend);
    assert_eq!(
        runtime.execute(&ScriptSource::new("undefined", "undefined.js"))?,
        RuntimeValue::Undefined
    );
    assert_eq!(
        runtime.execute(&ScriptSource::new("null", "null.js"))?,
        RuntimeValue::Null
    );
    assert_eq!(
        runtime.execute(&ScriptSource::new("true", "boolean.js"))?,
        RuntimeValue::Boolean(true)
    );
    assert_eq!(
        runtime.execute(&ScriptSource::new("40 + 2", "number.js"))?,
        RuntimeValue::Number(42.0)
    );
    assert_eq!(
        runtime.execute(&ScriptSource::new("'meow'", "string.js"))?,
        RuntimeValue::String("meow".to_owned())
    );
    assert_eq!(
        runtime.execute(&ScriptSource::new(
            "__meow_host_call('echo', 'ping')",
            "host.js",
        ))?,
        RuntimeValue::String("ping".to_owned())
    );
    let exception = runtime
        .execute(&ScriptSource::new("throw new Error('boom')", "throw.js"))
        .expect_err("throw must fail");
    assert_eq!(exception.mode, FailureMode::Exception);
    assert!(exception.message.contains("boom"));

    Ok(ConformanceReport {
        backend,
        availability: expected,
        assertions: 9,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_policy_defaults_to_v8_without_implicit_fallback() {
        assert_eq!(BackendPolicy::default().preferred, BackendKind::V8);
        assert_eq!(BackendPolicy::default().fallback, None);
    }
}
