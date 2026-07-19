//! Boa implementation of the backend-neutral JavaScript contract.

use std::{cell::RefCell, fmt, rc::Rc};

use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue, NativeFunction, Source};
use meow_js_runtime::{
    BackendKind, FailureMode, HostApi, JsRuntime, RuntimeError, RuntimeFactory, RuntimeValue,
    ScriptSource,
};

type SharedHost = Rc<RefCell<Box<dyn HostApi>>>;

thread_local! {
    static ACTIVE_HOST: RefCell<Option<SharedHost>> = const { RefCell::new(None) };
}

pub struct BoaRuntime {
    context: Context,
    host: SharedHost,
}

impl fmt::Debug for BoaRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("BoaRuntime").finish_non_exhaustive()
    }
}

impl BoaRuntime {
    fn new(host: Box<dyn HostApi>) -> Result<Self, RuntimeError> {
        let mut context = Context::default();
        context
            .register_global_builtin_callable(
                JsString::from("__meow_host_call"),
                2,
                NativeFunction::from_fn_ptr(host_call),
            )
            .map_err(|error| RuntimeError {
                backend: BackendKind::Boa,
                mode: FailureMode::Initialization,
                message: error.to_string(),
                source_name: "<initialization>".to_owned(),
            })?;
        Ok(Self {
            context,
            host: Rc::new(RefCell::new(host)),
        })
    }
}

impl JsRuntime for BoaRuntime {
    fn backend(&self) -> BackendKind {
        BackendKind::Boa
    }

    fn execute(&mut self, source: &ScriptSource) -> Result<RuntimeValue, RuntimeError> {
        let _guard = ActiveHostGuard::install(Rc::clone(&self.host))
            .map_err(|error| map_error(error.to_string(), source.name.clone()))?;
        let value = self
            .context
            .eval(Source::from_bytes(source.code.as_bytes()))
            .and_then(|value| {
                self.context.run_jobs()?;
                Ok(value)
            })
            .map_err(|error| map_error(error.to_string(), source.name.clone()))?;
        Ok(runtime_value(value))
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BoaFactory;

impl RuntimeFactory for BoaFactory {
    type Runtime = BoaRuntime;

    fn backend(&self) -> BackendKind {
        BackendKind::Boa
    }

    fn create(&self, host: Box<dyn HostApi>) -> Result<Self::Runtime, RuntimeError> {
        BoaRuntime::new(host)
    }
}

struct ActiveHostGuard;

impl ActiveHostGuard {
    fn install(host: SharedHost) -> JsResult<Self> {
        ACTIVE_HOST.with(|slot| {
            let mut active = slot.borrow_mut();
            if active.is_some() {
                return Err(JsNativeError::error()
                    .with_message("nested MeowEngine host activation")
                    .into());
            }
            *active = Some(host);
            Ok(Self)
        })
    }
}

impl Drop for ActiveHostGuard {
    fn drop(&mut self) {
        ACTIVE_HOST.with(|slot| *slot.borrow_mut() = None);
    }
}

fn host_call(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let operation = argument_string(args, 0, context)?;
    let payload = argument_string(args, 1, context)?;
    ACTIVE_HOST.with(|slot| {
        let host = slot
            .borrow()
            .clone()
            .ok_or_else(|| JsNativeError::error().with_message("JavaScript host is not active"))?;
        let result = host
            .try_borrow_mut()
            .map_err(|_| JsNativeError::error().with_message("JavaScript host is borrowed"))?
            .call(&operation, &payload)
            .map_err(|error| JsNativeError::error().with_message(error))?;
        Ok(JsValue::from(JsString::from(result)))
    })
}

fn argument_string(args: &[JsValue], index: usize, context: &mut Context) -> JsResult<String> {
    args.get(index)
        .ok_or_else(|| JsNativeError::typ().with_message(format!("missing argument {index}")))?
        .to_string(context)
        .map(|value| value.to_std_string_escaped())
}

fn runtime_value(value: JsValue) -> RuntimeValue {
    if value.is_undefined() {
        RuntimeValue::Undefined
    } else if value.is_null() {
        RuntimeValue::Null
    } else if let Some(value) = value.as_boolean() {
        RuntimeValue::Boolean(value)
    } else if let Some(value) = value.as_number() {
        RuntimeValue::Number(value)
    } else if let Some(value) = value.as_string() {
        RuntimeValue::String(value.to_std_string_escaped())
    } else {
        RuntimeValue::Object
    }
}

fn map_error(message: String, source_name: String) -> RuntimeError {
    let lowercase = message.to_ascii_lowercase();
    let mode = if lowercase.contains("syntaxerror") || lowercase.contains("parser") {
        FailureMode::Compile
    } else if lowercase.contains("host") {
        FailureMode::Host
    } else {
        FailureMode::Exception
    };
    RuntimeError {
        backend: BackendKind::Boa,
        mode,
        message,
        source_name,
    }
}

#[cfg(test)]
mod tests {
    use meow_js_runtime::{ExpectedAvailability, run_conformance_suite};

    use super::*;

    #[test]
    fn shared_host_conformance_suite_passes_for_boa() {
        let report = run_conformance_suite(&BoaFactory, ExpectedAvailability::Ready).unwrap();
        assert_eq!(report.assertions, 9);
    }
}
