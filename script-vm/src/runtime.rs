//! Engine-owned Wasm VM instances and bounded synchronous capability calls.
use anyhow::{Context, ensure};
use serde_json::{Value, json};
use std::collections::HashMap;
use wasmtime::{
    Caller, Config, Engine, Instance, Linker, Memory, Module, Store, StoreLimits,
    StoreLimitsBuilder,
};

const MAX_BUFFER: usize = 64 * 1024 * 1024;
pub fn valid_mod_id(value: &str) -> bool {
    !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}
scoped_tls_hkt::scoped_thread_local!(static mut HOST: for<'a> &'a mut (dyn FnMut(Value) -> anyhow::Result<Value> + 'a));

#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
enum HostResponse {
    Value(Value),
    Error(String),
}

#[derive(Clone)]
pub struct EmbeddedLimits {
    pub memory_bytes: usize,
    pub fuel_per_turn: u64,
    pub calls_per_turn: usize,
}
impl Default for EmbeddedLimits {
    fn default() -> Self {
        Self {
            memory_bytes: 384 * 1024 * 1024,
            fuel_per_turn: 2_000_000_000,
            calls_per_turn: 100_000,
        }
    }
}
struct HostState {
    active: bool,
    remaining_calls: usize,
    response: Vec<u8>,
    limits: StoreLimits,
    started: std::time::Instant,
}
struct Guest {
    store: Store<HostState>,
    instance: Instance,
    faulted: bool,
}
pub struct EmbeddedRuntime {
    engine: Engine,
    module: Module,
    guests: HashMap<String, Guest>,
    limits: EmbeddedLimits,
}
fn memory(caller: &mut Caller<'_, HostState>) -> anyhow::Result<Memory> {
    caller
        .get_export("memory")
        .and_then(|export| export.into_memory())
        .context("guest memory is missing")
}
fn range(pointer: i32, length: i32) -> anyhow::Result<(usize, usize)> {
    ensure!(
        pointer >= 0 && length >= 0 && length as usize <= MAX_BUFFER,
        "guest buffer exceeds limit"
    );
    Ok((pointer as usize, length as usize))
}
impl EmbeddedRuntime {
    /// The single bundled VM image, also used by the engine's source-cache key.
    pub fn bundled_bytes() -> &'static [u8] {
        include_bytes!("../../runtime/dist/whatever-script-guest.wasm")
    }

    pub fn bundled() -> anyhow::Result<Self> {
        static COMPILED: std::sync::OnceLock<Result<(Engine, Module), String>> =
            std::sync::OnceLock::new();
        let (engine, module) = COMPILED
            .get_or_init(|| {
                Self::new(Self::bundled_bytes(), EmbeddedLimits::default())
                    .map(|runtime| (runtime.engine, runtime.module))
                    .map_err(|error| format!("{error:#}"))
            })
            .as_ref()
            .map_err(|error| anyhow::anyhow!(error.clone()))?;
        Ok(Self {
            engine: engine.clone(),
            module: module.clone(),
            guests: HashMap::new(),
            limits: EmbeddedLimits::default(),
        })
    }

    pub fn scoped(
        &mut self,
        mod_id: &str,
        export: &str,
        input: &str,
        handler: &mut dyn FnMut(Value) -> anyhow::Result<Value>,
    ) -> anyhow::Result<Value> {
        let previous = self.limits.fuel_per_turn;
        if export == "load_package" {
            self.limits.fuel_per_turn = 10_000_000_000;
        }
        let result = HOST.set(handler, || self.execute(mod_id, export, input));
        self.limits.fuel_per_turn = previous;
        result
    }

    pub fn compile(
        &mut self,
        owner: &str,
        files: &std::collections::BTreeMap<String, String>,
    ) -> anyhow::Result<Value> {
        let previous = self.limits.fuel_per_turn;
        self.limits.fuel_per_turn = 100_000_000_000;
        let result = self.execute(
            "compiler",
            "compile_sources",
            &serde_json::to_string(&json!({"owner":owner,"files":files}))?,
        );
        self.limits.fuel_per_turn = previous;
        if result.is_err() {
            self.reset("compiler");
        }
        result
    }
    /// Accept a portable Wasm guest, never a serialized native-code cache from a mod.
    pub fn new(guest: &[u8], limits: EmbeddedLimits) -> anyhow::Result<Self> {
        ensure!(
            guest.len() <= 16 * 1024 * 1024,
            "guest module exceeds size limit"
        );
        let mut config = Config::new();
        config.consume_fuel(true);
        config.max_wasm_stack(1024 * 1024);
        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, guest)
            .map_err(anyhow::Error::from)
            .context("compile JavaScript Wasm guest")?;
        for import in module.imports() {
            ensure!(
                matches!(
                    (import.module(), import.name()),
                    ("whatever", "request" | "response") | ("env", "__rquickjs_host_now_us")
                ),
                "unsupported guest import '{}::{}'",
                import.module(),
                import.name()
            );
        }
        Ok(Self {
            engine,
            module,
            guests: HashMap::new(),
            limits,
        })
    }

    fn create_guest(&self) -> anyhow::Result<Guest> {
        let state = HostState {
            active: false,
            remaining_calls: 0,
            response: Vec::new(),
            limits: StoreLimitsBuilder::new()
                .memory_size(self.limits.memory_bytes)
                .memories(1)
                .instances(1)
                .tables(1)
                .build(),
            started: std::time::Instant::now(),
        };
        let mut store = Store::new(&self.engine, state);
        store.limiter(|state| &mut state.limits);
        store.set_fuel(self.limits.fuel_per_turn)?;
        let mut linker = Linker::new(&self.engine);
        linker.func_wrap("env", "__rquickjs_host_now_us", || {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0.0, |time| time.as_secs_f64() * 1_000_000.0)
        })?;
        linker.func_wrap(
            "whatever",
            "request",
            |mut caller: Caller<'_, HostState>,
             pointer: i32,
             length: i32|
             -> wasmtime::Result<i32> {
                let result = (|| -> anyhow::Result<i32> {
                    let (pointer, length) = range(pointer, length)?;
                    ensure!(
                        caller.data().active,
                        "engine calls require an active simulation turn"
                    );
                    ensure!(
                        caller.data().remaining_calls > 0,
                        "host call budget exhausted"
                    );
                    caller.data_mut().remaining_calls -= 1;
                    let input = memory(&mut caller)?
                        .data(&caller)
                        .get(pointer..pointer + length)
                        .context("engine request is outside guest memory")?;
                    let operation: Value = serde_json::from_slice(input)?;
                    let output = match operation["op"].as_str() {
                        Some("clock") => Ok(json!(
                            caller.data().started.elapsed().as_secs_f64() * 1000.0
                        )),
                        Some("memory") => {
                            let size = memory(&mut caller)?.data_size(&caller);
                            Ok(json!({"heap_used":size,"rss":size}))
                        }
                        _ if HOST.is_set() => HOST.with(|callback| callback(operation)),
                        _ => {
                            anyhow::bail!("engine capability requires an active simulation context")
                        }
                    };
                    let response = match output {
                        Ok(value) => HostResponse::Value(value),
                        Err(error) => HostResponse::Error(format!("{error:#}")),
                    };
                    let bytes = serde_json::to_vec(&response)?;
                    ensure!(bytes.len() <= MAX_BUFFER, "engine response exceeds limit");
                    let length = bytes.len() as i32;
                    caller.data_mut().response = bytes;
                    Ok(length)
                })();
                result.map_err(|error| wasmtime::Error::msg(format!("{error:#}")))
            },
        )?;
        linker.func_wrap(
            "whatever",
            "response",
            |mut caller: Caller<'_, HostState>,
             pointer: i32,
             length: i32|
             -> wasmtime::Result<i32> {
                let result = (|| -> anyhow::Result<i32> {
                    let (pointer, length) = range(pointer, length)?;
                    ensure!(
                        caller.data().active && length == caller.data().response.len(),
                        "invalid response buffer"
                    );
                    let bytes = std::mem::take(&mut caller.data_mut().response);
                    memory(&mut caller)?.write(&mut caller, pointer, &bytes)?;
                    Ok(length as i32)
                })();
                result.map_err(|error| wasmtime::Error::msg(format!("{error:#}")))
            },
        )?;
        let instance = linker.instantiate(&mut store, &self.module)?;
        let initialize = instance.get_typed_func::<(), i32>(&mut store, "initialize")?;
        ensure!(
            initialize.call(&mut store, ())? == 0,
            "JavaScript guest initialization failed"
        );
        Ok(Guest {
            store,
            instance,
            faulted: false,
        })
    }

    #[cfg(test)]
    pub fn evaluate(&mut self, mod_id: &str, source: &str) -> anyhow::Result<Value> {
        self.execute(mod_id, "evaluate", source)
    }
    fn execute(&mut self, mod_id: &str, export: &str, source: &str) -> anyhow::Result<Value> {
        ensure!(valid_mod_id(mod_id), "invalid mod identity");
        ensure!(source.len() <= MAX_BUFFER, "script source exceeds limit");
        if !self.guests.contains_key(mod_id) {
            self.guests.insert(mod_id.into(), self.create_guest()?);
        }
        let guest = self.guests.get_mut(mod_id).context("guest disappeared")?;
        ensure!(
            !guest.faulted,
            "mod is faulted; reset it before executing again"
        );
        guest.store.set_fuel(self.limits.fuel_per_turn)?;
        guest.store.data_mut().remaining_calls = self.limits.calls_per_turn;
        guest.store.data_mut().active = true;
        let result = (|| -> anyhow::Result<Value> {
            let allocate = guest
                .instance
                .get_typed_func::<i32, i32>(&mut guest.store, "allocate")?;
            let pointer = allocate.call(&mut guest.store, source.len() as i32)?;
            ensure!(pointer > 0, "guest source allocation failed");
            let memory = guest
                .instance
                .get_memory(&mut guest.store, "memory")
                .context("guest memory")?;
            memory.write(&mut guest.store, pointer as usize, source.as_bytes())?;
            let evaluate = guest
                .instance
                .get_typed_func::<(i32, i32), i32>(&mut guest.store, export)?;
            let status = evaluate.call(&mut guest.store, (pointer, source.len() as i32))?;
            let deallocate = guest
                .instance
                .get_typed_func::<(i32, i32), ()>(&mut guest.store, "deallocate")?;
            deallocate.call(&mut guest.store, (pointer, source.len() as i32))?;
            let result_pointer = guest
                .instance
                .get_typed_func::<(), i32>(&mut guest.store, "result_pointer")?
                .call(&mut guest.store, ())?;
            let result_length = guest
                .instance
                .get_typed_func::<(), i32>(&mut guest.store, "result_length")?
                .call(&mut guest.store, ())?;
            let (result_pointer, result_length) = range(result_pointer, result_length)?;
            let mut output = vec![0; result_length];
            memory.read(&guest.store, result_pointer, &mut output)?;
            ensure!(
                status == 0,
                "JavaScript exception: {}",
                String::from_utf8_lossy(&output)
            );
            let drain = guest
                .instance
                .get_typed_func::<(), i32>(&mut guest.store, "drain_jobs")?;
            if drain.call(&mut guest.store, ())? != 0 {
                let pointer = guest
                    .instance
                    .get_typed_func::<(), i32>(&mut guest.store, "result_pointer")?
                    .call(&mut guest.store, ())?;
                let length = guest
                    .instance
                    .get_typed_func::<(), i32>(&mut guest.store, "result_length")?
                    .call(&mut guest.store, ())?;
                let (pointer, length) = range(pointer, length)?;
                let mut message = vec![0; length];
                memory.read(&guest.store, pointer, &mut message)?;
                anyhow::bail!(
                    "JavaScript microtask failed: {}",
                    String::from_utf8_lossy(&message)
                );
            }
            serde_json::from_slice(&output).context("invalid JavaScript result")
        })();
        guest.store.data_mut().active = false;
        guest.store.data_mut().response.clear();
        if result.is_err() {
            guest.faulted = true;
        }
        result
    }

    pub fn reset(&mut self, mod_id: &str) {
        self.guests.remove(mod_id);
    }
}
