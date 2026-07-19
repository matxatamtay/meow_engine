//! W3 V8 contract scaffold. W4 replaces the fail-closed factory with a real isolate.

use meow_js_runtime::{
    BackendKind, HostApi, JsRuntime, RuntimeError, RuntimeFactory, RuntimeValue, ScriptSource,
};

#[derive(Debug)]
pub struct V8Runtime;

impl JsRuntime for V8Runtime {
    fn backend(&self) -> BackendKind {
        BackendKind::V8
    }

    fn execute(&mut self, source: &ScriptSource) -> Result<RuntimeValue, RuntimeError> {
        Err(RuntimeError::backend_unavailable(
            BackendKind::V8,
            format!("V8 isolate is not linked until Y2-W4 ({})", source.name),
        ))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct V8Factory;

impl RuntimeFactory for V8Factory {
    type Runtime = V8Runtime;

    fn backend(&self) -> BackendKind {
        BackendKind::V8
    }

    fn create(&self, _host: Box<dyn HostApi>) -> Result<Self::Runtime, RuntimeError> {
        Err(RuntimeError::backend_unavailable(
            BackendKind::V8,
            "V8 isolate support is intentionally fail-closed until Y2-W4",
        ))
    }
}

#[cfg(test)]
mod tests {
    use meow_js_runtime::{ExpectedAvailability, run_conformance_suite};

    use super::*;

    #[test]
    fn shared_host_conformance_suite_records_w3_v8_unavailability() {
        let report = run_conformance_suite(&V8Factory, ExpectedAvailability::Unavailable).unwrap();
        assert_eq!(report.assertions, 2);
    }
}
