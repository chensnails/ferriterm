//! Sandboxed WASM plugin runner.
//!
//! Guest modules are interpreted by wasmi with a fuel budget, a capped linear
//! memory, and a host import surface limited to the `ferriterm` namespace. Any
//! other import (WASI, filesystem, network, ...) is rejected before the module
//! is instantiated, so a plugin cannot reach the host through imports.

use std::sync::Arc;

use wasmi::{
    core::TrapCode, Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits,
    StoreLimitsBuilder,
};

pub const DEFAULT_FUEL: u64 = 5_000_000;
pub const DEFAULT_MEMORY_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_MODULE_BYTES: usize = 2 * 1024 * 1024;
const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_LOG_LINES: usize = 256;
const MAX_LOG_BYTES: usize = 32 * 1024;
const MAX_READ_BYTES: usize = 64 * 1024;

const HOST_MODULE: &str = "ferriterm";
const ALLOWED_IMPORTS: &[&str] = &["log", "emit", "payload_len", "payload_byte"];

#[derive(Debug, Clone)]
pub struct WasmLimits {
    pub fuel: u64,
    pub memory_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for WasmLimits {
    fn default() -> Self {
        Self {
            fuel: DEFAULT_FUEL,
            memory_bytes: DEFAULT_MEMORY_BYTES,
            max_output_bytes: MAX_OUTPUT_BYTES,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WasmRun {
    pub output: String,
    pub logs: Vec<String>,
    pub fuel_used: u64,
}

struct SandboxState {
    limits: StoreLimits,
    logs: Vec<String>,
    log_bytes: usize,
    output: String,
    max_output_bytes: usize,
}

/// Runs a WASM plugin entry point (`run`) with the given payload.
///
/// The guest can import only the four `ferriterm.*` functions; everything else
/// fails validation. A module that keeps running until the fuel budget is spent
/// traps with a fuel error instead of hanging the host.
pub fn run(bytes: &[u8], payload: &[u8], limits: &WasmLimits) -> Result<WasmRun, String> {
    if bytes.len() > MAX_MODULE_BYTES {
        return Err(format!(
            "wasm module is {} bytes (limit {MAX_MODULE_BYTES})",
            bytes.len()
        ));
    }
    if payload.len() > MAX_PAYLOAD_BYTES {
        return Err(format!(
            "payload is {} bytes (limit {MAX_PAYLOAD_BYTES})",
            payload.len()
        ));
    }

    let mut config = Config::default();
    config.consume_fuel(true);
    let engine = Engine::new(&config);
    let module =
        Module::new(&engine, bytes).map_err(|err| format!("invalid wasm module: {err}"))?;
    reject_forbidden_imports(&module)?;

    let store_limits = StoreLimitsBuilder::new()
        .memory_size(limits.memory_bytes)
        .table_elements(1024)
        .instances(1)
        .tables(1)
        .memories(1)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(
        &engine,
        SandboxState {
            limits: store_limits,
            logs: Vec::new(),
            log_bytes: 0,
            output: String::new(),
            max_output_bytes: limits.max_output_bytes.min(MAX_OUTPUT_BYTES),
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(limits.fuel)
        .map_err(|err| format!("cannot enable fuel metering: {err}"))?;

    let payload = Arc::new(payload.to_vec());
    let mut linker = Linker::new(&engine);
    let payload_len = Arc::clone(&payload);
    linker
        .func_wrap(HOST_MODULE, "payload_len", move || payload_len.len() as i32)
        .map_err(|err| err.to_string())?;
    let payload_byte = Arc::clone(&payload);
    linker
        .func_wrap(HOST_MODULE, "payload_byte", move |index: i32| -> i32 {
            usize::try_from(index)
                .ok()
                .and_then(|index| payload_byte.get(index).copied())
                .map_or(-1, i32::from)
        })
        .map_err(|err| err.to_string())?;
    linker
        .func_wrap(
            HOST_MODULE,
            "log",
            |mut caller: Caller<'_, SandboxState>, ptr: i32, len: i32| {
                if let Some(text) = read_guest(&caller, ptr, len) {
                    push_log(caller.data_mut(), text);
                }
            },
        )
        .map_err(|err| err.to_string())?;
    linker
        .func_wrap(
            HOST_MODULE,
            "emit",
            |mut caller: Caller<'_, SandboxState>, ptr: i32, len: i32| {
                if let Some(text) = read_guest(&caller, ptr, len) {
                    let cap = caller.data().max_output_bytes;
                    caller.data_mut().output = truncate(text, cap);
                }
            },
        )
        .map_err(|err| err.to_string())?;

    let instance = linker
        .instantiate(&mut store, &module)
        .and_then(|pre| pre.start(&mut store))
        .map_err(|err| describe_trap(&err))?;

    let run = instance
        .get_typed_func::<(), ()>(&store, "run")
        .map_err(|_| "plugin must export a `run` function with no arguments".to_string())?;
    if let Err(err) = run.call(&mut store, ()) {
        return Err(describe_trap(&err));
    }

    let state = store.data();
    Ok(WasmRun {
        output: state.output.clone(),
        logs: state.logs.clone(),
        fuel_used: limits.fuel.saturating_sub(store.get_fuel().unwrap_or(0)),
    })
}

fn reject_forbidden_imports(module: &Module) -> Result<(), String> {
    for import in module.imports() {
        let module_name = import.module();
        let field = import.name();
        if module_name != HOST_MODULE || !ALLOWED_IMPORTS.contains(&field) {
            return Err(format!(
                "forbidden import {module_name}.{field}: plugins may only import ferriterm.{{log,emit,payload_len,payload_byte}}"
            ));
        }
    }
    Ok(())
}

fn guest_memory(caller: &Caller<'_, SandboxState>) -> Option<wasmi::Memory> {
    caller.get_export("memory").and_then(Extern::into_memory)
}

fn read_guest(caller: &Caller<'_, SandboxState>, ptr: i32, len: i32) -> Option<String> {
    let memory = guest_memory(caller)?;
    let start = usize::try_from(ptr).ok()?;
    let len = usize::try_from(len).ok()?;
    if len > MAX_READ_BYTES {
        return None;
    }
    let end = start.checked_add(len)?;
    if end > memory.data_size(caller) {
        return None;
    }
    let mut buf = vec![0u8; len];
    memory.read(caller, start, &mut buf).ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn push_log(state: &mut SandboxState, text: String) {
    if state.logs.len() >= MAX_LOG_LINES {
        return;
    }
    let text = truncate(text, MAX_READ_BYTES);
    if state.log_bytes + text.len() > MAX_LOG_BYTES {
        return;
    }
    state.log_bytes += text.len();
    state.logs.push(text);
}

fn truncate(mut text: String, cap: usize) -> String {
    if text.len() <= cap {
        return text;
    }
    let mut boundary = cap;
    while !text.is_char_boundary(boundary) {
        boundary -= 1;
    }
    text.truncate(boundary);
    text
}

fn describe_trap(err: &wasmi::Error) -> String {
    if err.as_trap_code() == Some(TrapCode::OutOfFuel) {
        "plugin exhausted its fuel budget".to_string()
    } else {
        format!("plugin trapped: {err}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wat(source: &str) -> Vec<u8> {
        wat::parse_str(source).unwrap()
    }

    #[test]
    fn runs_exported_function_and_captures_output() {
        let module = wat(r#"
            (module
              (import "ferriterm" "emit" (func $emit (param i32 i32)))
              (import "ferriterm" "log" (func $log (param i32 i32)))
              (memory (export "memory") 1)
              (data (i32.const 0) "hello plugin")
              (data (i32.const 32) "log line")
              (func (export "run")
                (call $log (i32.const 32) (i32.const 8))
                (call $emit (i32.const 0) (i32.const 12))))
            "#);
        let run = run(&module, b"", &WasmLimits::default()).unwrap();
        assert_eq!(run.output, "hello plugin");
        assert_eq!(run.logs, vec!["log line".to_string()]);
        assert!(run.fuel_used > 0);
    }

    #[test]
    fn reads_payload_bytes() {
        let module = wat(r#"
            (module
              (import "ferriterm" "payload_len" (func $len (result i32)))
              (import "ferriterm" "payload_byte" (func $byte (param i32) (result i32)))
              (import "ferriterm" "emit" (func $emit (param i32 i32)))
              (memory (export "memory") 1)
              (func (export "run")
                (local $i i32)
                (block $done
                  (loop $loop
                    (br_if $done (i32.ge_u (local.get $i) (call $len)))
                    (i32.store8 (local.get $i) (call $byte (local.get $i)))
                    (local.set $i (i32.add (local.get $i) (i32.const 1)))
                    (br $loop)))
                (call $emit (i32.const 0) (local.get $i))))
            "#);
        let run = run(&module, b"payload", &WasmLimits::default()).unwrap();
        assert_eq!(run.output, "payload");
    }

    #[test]
    fn rejects_wasi_imports() {
        let module = wat(r#"
            (module
              (import "wasi_snapshot_preview1" "fd_write"
                (func (param i32 i32 i32 i32) (result i32)))
              (memory (export "memory") 1)
              (func (export "run")))
            "#);
        let err = run(&module, b"", &WasmLimits::default()).unwrap_err();
        assert!(err.contains("forbidden import wasi_snapshot_preview1.fd_write"));
    }

    #[test]
    fn rejects_unknown_host_imports() {
        let module = wat(r#"
            (module
              (import "ferriterm" "shell" (func $shell (param i32 i32)))
              (func (export "run")))
            "#);
        assert!(run(&module, b"", &WasmLimits::default()).is_err());
    }

    #[test]
    fn traps_when_fuel_is_exhausted() {
        let module = wat(r#"
            (module
              (func (export "run")
                (loop $spin (br $spin))))
            "#);
        let limits = WasmLimits {
            fuel: 10_000,
            ..WasmLimits::default()
        };
        let err = run(&module, b"", &limits).unwrap_err();
        assert!(err.contains("fuel"), "{err}");
    }

    #[test]
    fn caps_memory_growth() {
        let module = wat(r#"
            (module
              (memory (export "memory") 1)
              (func (export "run")
                (drop (memory.grow (i32.const 5000)))))
            "#);
        let err = run(&module, b"", &WasmLimits::default()).unwrap_err();
        assert!(err.contains("growth"), "{err}");
    }

    #[test]
    fn rejects_oversized_modules_and_payloads() {
        let limits = WasmLimits::default();
        let module = wat("(module (func (export \"run\")))");
        assert!(run(&module, b"", &limits).is_ok());
        let huge = vec![0u8; MAX_MODULE_BYTES + 1];
        assert!(run(&huge, b"", &limits).is_err());
        assert!(run(&module, &vec![0u8; MAX_PAYLOAD_BYTES + 1], &limits).is_err());
    }

    #[test]
    fn requires_run_export() {
        let module = wat("(module (memory (export \"memory\") 1))");
        let err = run(&module, b"", &WasmLimits::default()).unwrap_err();
        assert!(err.contains("run"));
    }
}
