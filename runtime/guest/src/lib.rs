//! Engine-owned QuickJS guest. No WASI, filesystem, sockets, processes or native modules.
use rquickjs::loader::{ImportAttributes, Loader, Resolver};
use rquickjs::{Coerced, Context, Ctx, FromJs, Function, Module, Runtime, function::Func};
use std::cell::RefCell;
use std::collections::BTreeMap;

const MAX_BUFFER: usize = 64 * 1024 * 1024;

#[link(wasm_import_module = "whatever")]
unsafe extern "C" {
    fn request(pointer: *const u8, length: usize) -> i32;
    fn response(pointer: *mut u8, length: usize) -> i32;
}

struct Guest {
    // Context must be dropped before the runtime.
    context: Context,
    runtime: Runtime,
    compiler_loaded: std::cell::Cell<bool>,
    unhandled: std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>,
}
thread_local! {
    static GUEST: RefCell<Option<Guest>> = const { RefCell::new(None) };
    static RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

fn host_call(input: String) -> rquickjs::Result<String> {
    if input.len() > MAX_BUFFER {
        return Err(rquickjs::Error::Unknown);
    }
    // These buffers belong to this guest. The Wasmtime host bounds-checks both directions.
    let length = unsafe { request(input.as_ptr(), input.len()) };
    if length < 0 || length as usize > MAX_BUFFER {
        return Err(rquickjs::Error::Unknown);
    }
    let mut output = vec![0; length as usize];
    if unsafe { response(output.as_mut_ptr(), output.len()) } != length {
        return Err(rquickjs::Error::Unknown);
    }
    String::from_utf8(output).map_err(|_| rquickjs::Error::Unknown)
}

#[unsafe(no_mangle)]
pub extern "C" fn allocate(length: usize) -> *mut u8 {
    if length > MAX_BUFFER {
        return std::ptr::null_mut();
    }
    let buffer = vec![0_u8; length].into_boxed_slice();
    Box::into_raw(buffer) as *mut u8
}

/// The host may free only a buffer returned by allocate, with its original length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn deallocate(pointer: *mut u8, length: usize) {
    if !pointer.is_null() {
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(pointer, length)) });
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn initialize() -> i32 {
    let result = (|| {
        let runtime = Runtime::new()?;
        let unhandled = std::sync::Arc::new(std::sync::Mutex::new(BTreeMap::new()));
        let failures = unhandled.clone();
        runtime.set_host_promise_rejection_tracker(Some(Box::new(
            move |ctx, promise, reason, handled| {
                let key = format!("{promise:?}");
                if let Ok(mut failures) = failures.lock() {
                    if handled {
                        failures.remove(&key);
                    } else {
                        let message = Coerced::<String>::from_js(&ctx, reason)
                            .map(|value| value.0)
                            .unwrap_or_else(|_| "Unhandled promise rejection".into());
                        failures.insert(key, message);
                    }
                }
            },
        )));
        runtime.set_memory_limit(256 * 1024 * 1024);
        runtime.set_max_stack_size(256 * 1024);
        let context = Context::full(&runtime)?;
        context.with(|ctx| {
            ctx.globals().set("__engineCall", Func::from(host_call))?;
            ctx.eval::<(), _>(include_str!("../polyfills.js"))?;
            ctx.eval::<(), _>("globalThis.__evaluate = source => { const value = (0,eval)(source); return JSON.stringify(value === undefined ? null : value); };")
        })?;
        GUEST.with_borrow_mut(|slot| {
            *slot = Some(Guest {
                context,
                runtime,
                compiler_loaded: std::cell::Cell::new(false),
                unhandled,
            })
        });
        Ok::<(), rquickjs::Error>(())
    })();
    if let Err(error) = result {
        set_result(format!("{error:?}"));
        return 1;
    }
    0
}

struct Sources(BTreeMap<String, String>);
impl Resolver for Sources {
    fn resolve<'js>(
        &mut self,
        _ctx: &Ctx<'js>,
        base: &str,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<String> {
        if name == "@whatever-engine/api" {
            return Ok(name.into());
        }
        if name.contains('\\') || (!name.starts_with('.') && base != "@engine/bootstrap") {
            return Err(rquickjs::Error::new_resolving_message(
                base,
                name,
                "Whatever module policy denied: only contained relative source imports and @whatever-engine/api are available",
            ));
        }
        let mut parts: Vec<&str> = if name.starts_with('/') {
            Vec::new()
        } else {
            base.rsplit_once('/')
                .map(|(parent, _)| parent.split('/').filter(|p| !p.is_empty()).collect())
                .unwrap_or_default()
        };
        for part in name.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err(rquickjs::Error::new_resolving_message(
                            base,
                            name,
                            "Whatever module policy denied: source import escapes mod root",
                        ));
                    }
                }
                _ => parts.push(part),
            }
        }
        let path = parts.join("/");
        for suffix in [
            "",
            ".ts",
            ".tsx",
            ".js",
            ".jsx",
            ".mts",
            ".cts",
            ".mjs",
            ".cjs",
            ".json",
            "/index.ts",
            "/index.js",
        ] {
            let candidate = format!("{path}{suffix}");
            if self.0.contains_key(&candidate) {
                return Ok(candidate);
            }
        }
        Err(rquickjs::Error::new_resolving_message(
            base,
            name,
            "local source not found",
        ))
    }
}
impl Loader for Sources {
    fn load<'js>(
        &mut self,
        ctx: &Ctx<'js>,
        name: &str,
        _attributes: Option<ImportAttributes<'js>>,
    ) -> rquickjs::Result<Module<'js>> {
        let source = if name == "@whatever-engine/api" {
            include_str!("../generated/api.js")
        } else {
            self.0
                .get(name)
                .ok_or_else(|| rquickjs::Error::new_loading_message(name, "source not found"))?
        };
        Module::declare(ctx.clone(), name, source)
    }
}

/// Parse/type-inspect/transpile mod sources inside the same Wasm sandbox. No mod code is evaluated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compile_sources(pointer: *const u8, length: usize) -> i32 {
    if length > MAX_BUFFER {
        set_result("Source package exceeds limit".into());
        return 1;
    }
    let input = unsafe { std::slice::from_raw_parts(pointer, length) };
    let result = GUEST.with_borrow(|slot| -> Result<String, String> {
        let guest = slot.as_ref().ok_or("Guest is not initialized")?;
        let input: serde_json::Value = serde_json::from_slice(input).map_err(|e| e.to_string())?;
        guest.context.with(|ctx| {
            if !guest.compiler_loaded.get() {
                ctx.eval::<(), _>(include_str!("../generated/compiler.js"))
                    .map_err(|e| exception(&ctx, e))?;
                guest.compiler_loaded.set(true);
            }
            let script = format!(
                "JSON.stringify(__compileSources({},{}))",
                input["files"], input["owner"]
            );
            ctx.eval::<String, _>(script)
                .map_err(|e| exception(&ctx, e))
        })
    });
    finish(result)
}

fn describe_exception<'js>(ctx: &Ctx<'js>, value: rquickjs::Value<'js>) -> String {
    let stack = rquickjs::Object::from_js(ctx, value.clone())
        .ok()
        .and_then(|object| object.get::<_, String>("stack").ok())
        .unwrap_or_default();
    let message = Coerced::<String>::from_js(ctx, value)
        .map(|value| value.0)
        .unwrap_or_else(|_| "unprintable JavaScript exception".into());
    if stack.is_empty() {
        message
    } else {
        format!("{message}\n{stack}")
    }
}
fn exception(ctx: &Ctx<'_>, error: rquickjs::Error) -> String {
    format!("{error}: {}", describe_exception(ctx, ctx.catch()))
}
fn finish(result: Result<String, String>) -> i32 {
    match result {
        Ok(value) => {
            set_result(value);
            0
        }
        Err(error) => {
            set_result(error);
            1
        }
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn load_package(pointer: *const u8, length: usize) -> i32 {
    if length > MAX_BUFFER {
        set_result("Source package exceeds limit".into());
        return 1;
    }
    let input = unsafe { std::slice::from_raw_parts(pointer, length) };
    let result=GUEST.with_borrow(|slot|->Result<String,String>{
        let guest=slot.as_ref().ok_or("Guest is not initialized")?;
        let input:serde_json::Value=serde_json::from_slice(input).map_err(|e|e.to_string())?;
        let modules:BTreeMap<String,String>=serde_json::from_value(input["modules"].clone()).map_err(|e|e.to_string())?;
        guest.runtime.set_loader(Sources(modules.clone()),Sources(modules));
        let script=format!(r#"import * as api from '@whatever-engine/api';
            api.__mode({mode});globalThis.__deliver=api.__deliver;
            globalThis.console=Object.freeze(Object.fromEntries(['log','info','warn','error','debug','trace'].map(level=>[level,(...values)=>api.Engine.log(level==='error'?'error':level==='warn'?'warn':'info',values.map(v=>typeof v==='string'?v:JSON.stringify(v)).join(' '))])));
            try{{for(const definition of {definitions}){{const module=await import('/'+definition.source);api.__register(module[definition.export],definition);}}
            await import('/'+{entry});globalThis.__ready=true;}}
            catch(error){{globalThis.__bootError=String(error)+'\n'+(error.stack??'');}}
        "#,mode=input["mode"],definitions=input["definitions"],entry=input["entry"]);
        guest.context.with(|ctx|{
            Module::evaluate(ctx.clone(),"@engine/bootstrap",script).map_err(|e|exception(&ctx,e))?;
            Ok("null".into())
        })
    });
    finish(result)
}

fn set_result(value: String) {
    RESULT.with_borrow_mut(|result| *result = value.into_bytes());
}

/// Source is always UTF-8 JavaScript, never untrusted QuickJS bytecode.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn evaluate(pointer: *const u8, length: usize) -> i32 {
    if length > MAX_BUFFER {
        set_result("Source exceeds limit".into());
        return 1;
    }
    let source = unsafe { std::slice::from_raw_parts(pointer, length) };
    let Ok(source) = std::str::from_utf8(source) else {
        set_result("Invalid UTF-8 source".into());
        return 1;
    };
    GUEST.with_borrow(|slot| {
        let Some(guest) = slot else {
            set_result("Guest is not initialized".into());
            return 1;
        };
        let result = guest.context.with(|ctx| {
            let evaluate: Function = ctx.globals().get("__evaluate")?;
            match evaluate.call::<_, String>((source,)) {
                Ok(value) => Ok(value),
                Err(error) => Err(exception(&ctx, error)),
            }
            .map_err(|error| rquickjs::Error::new_from_js_message("exception", "string", error))
        });
        match result {
            Ok(value) => {
                set_result(value);
                0
            }
            Err(error) => {
                set_result(error.to_string());
                1
            }
        }
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn drain_jobs() -> i32 {
    GUEST.with_borrow(|slot| {
        let Some(guest) = slot else {
            return 1;
        };
        // Fuel covers the complete pump, including self-rescheduling microtasks.
        while guest.runtime.is_job_pending() {
            if guest.runtime.execute_pending_job().is_err() {
                set_result("Unhandled JavaScript job exception".into());
                return 1;
            }
        }
        if let Ok(failures) = guest.unhandled.lock() {
            if let Some(error) = failures.values().next() {
                set_result(error.clone());
                return 1;
            }
        }
        0
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn result_pointer() -> *const u8 {
    RESULT.with_borrow(|result| result.as_ptr())
}
#[unsafe(no_mangle)]
pub extern "C" fn result_length() -> usize {
    RESULT.with_borrow(Vec::len)
}
