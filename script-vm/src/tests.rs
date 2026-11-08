use super::EmbeddedRuntime;
use serde_json::json;

#[test]
fn bundled_compiler_accepts_typescript_without_a_toolchain_or_mod_evaluation() {
    let mut runtime = EmbeddedRuntime::bundled().expect("bundled runtime");
    let files = std::collections::BTreeMap::from([("scripts/index.ts".to_string(),
        "import {Component} from '@whatever-engine/api'; throw new Error('must not execute'); export class Health extends Component { value: number=3; }".to_string())]);
    let result = runtime
        .compile("test", &files)
        .expect("compile TypeScript inside Wasm");
    assert_eq!(result["definitions"][0]["id"], "test:Health");
    assert!(
        result["modules"]["scripts/index.ts"]
            .as_str()
            .expect("compiled source")
            .contains("class Health")
    );
}

#[test]
fn guests_share_no_globals_or_ambient_os_capabilities() {
    let mut runtime = EmbeddedRuntime::bundled().expect("runtime");
    runtime
        .evaluate("first", "var secret=42; null")
        .expect("first");
    let result = runtime.evaluate("second", "[typeof secret, typeof process, typeof require, typeof fetch, typeof WebSocket, typeof Worker, typeof __hostCall]").expect("second");
    assert_eq!(
        result,
        json!([
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined",
            "undefined"
        ])
    );
    assert!(
        runtime
            .evaluate(
                "second",
                "__engineCall(JSON.stringify({op:'entity_create'}))"
            )
            .is_err()
    );
}

#[test]
fn fuel_exhaustion_faults_only_the_affected_instance() {
    let mut runtime = EmbeddedRuntime::bundled().expect("runtime");
    assert!(runtime.evaluate("loop", "while(true) {}").is_err());
    assert!(runtime.evaluate("loop", "1").is_err());
    assert_eq!(
        runtime.evaluate("healthy", "1+1").expect("healthy"),
        json!(2)
    );
    runtime.reset("loop");
    assert_eq!(runtime.evaluate("loop", "1+1").expect("reset"), json!(2));
}

#[test]
fn promise_jobs_share_the_turn_budget_and_report_unhandled_rejections() {
    let mut runtime = EmbeddedRuntime::bundled().expect("runtime");
    runtime
        .evaluate(
            "jobs",
            "var done=false;Promise.resolve().then(()=>{done=true});null",
        )
        .expect("pump");
    assert_eq!(
        runtime.evaluate("jobs", "done").expect("result"),
        json!(true)
    );
    assert!(
        runtime
            .evaluate(
                "jobs",
                "function again(){Promise.resolve().then(again)};again()"
            )
            .is_err()
    );
    let error = runtime
        .evaluate(
            "rejection",
            "Promise.reject(new Error('rejection test'));null",
        )
        .expect_err("must surface rejection");
    assert!(format!("{error:#}").contains("rejection test"));
}

#[test]
fn host_call_and_memory_limits_are_enforced() {
    let mut runtime = EmbeddedRuntime::bundled().expect("runtime");
    assert!(
        runtime
            .evaluate("memory", "new ArrayBuffer(512*1024*1024)")
            .is_err()
    );
    let mut count = 0;
    let mut callback = |_: serde_json::Value| -> anyhow::Result<serde_json::Value> {
        count += 1;
        Ok(json!(null))
    };
    let result = runtime.scoped(
        "calls",
        "evaluate",
        "for(let i=0;i<100001;i++)__engineCall('{}');null",
        &mut callback,
    );
    assert!(result.is_err());
    assert_eq!(count, 100_000);
}
