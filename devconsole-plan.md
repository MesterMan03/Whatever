# Plan: Developer Console

## Context

The Whatever engine needs a developer console — a terminal-style panel that slides in from the top of the window on demand. It must have typed input, scrollable output history, tab autocomplete, an extensible command system, and a TypeScript API so mods can register their own commands. This is the primary developer/player debugging interface.

---

## Rendering: egui

Add three crates to `Cargo.toml`:
```toml
egui       = "0.29"
egui-wgpu  = "0.29"
egui-winit = "0.29"
```
> **Note**: egui-wgpu 0.29 targets wgpu 22 (verify on crates.io; if incompatible, bump wgpu to 23 and update `src/renderer/` for any breaking API changes).

egui renders as a second pass after the scene pass inside the existing `Renderer::render()` command encoder. The console panel is a `TopBottomPanel` anchored to the top, full-width, taking ~40% of window height when open.

---

## Toggle Key

**Ctrl + Alt + Enter** — detected in `engine.rs::window_event()` on `KeyboardInput` with `KeyCode::Enter` pressed, by checking `self.input.keys_pressed` for both Ctrl and Alt keys. When triggered: `self.console.toggle()`, and if opening, release cursor capture.

When the console is **open**, all keyboard and character input is routed exclusively to egui (via `egui_winit::State::on_window_event`). Camera controller receives no key events.

---

## New Module: `src/console/`

```
src/console/
├── mod.rs           re-exports
├── console.rs       DevConsole struct + egui render logic
├── registry.rs      CommandRegistry: register, lookup, namespace resolution
├── types.rs         CommandNode, ArgSpec, ArgType, ArgValue, ParsedArgs, CommandResult
├── parser.rs        parse raw string → (command_path: Vec<String>, raw_args: Vec<String>)
├── completer.rs     tab autocomplete: match partial tokens against the command tree
└── commands/
    ├── mod.rs
    ├── markbench.rs
    ├── mods_cmd.rs
    ├── vfs_cmd.rs
    └── engine_cmd.rs
```

### `types.rs`

```rust
pub enum ArgType { String, Int, Float, Bool }

pub struct ArgSpec {
    pub name: String,       // [a-z_]+ only
    pub arg_type: ArgType,
    pub required: bool,
    pub description: String,
}

pub enum ArgValue { String(String), Int(i64), Float(f64), Bool(bool) }

pub struct ParsedArgs { pub positional: Vec<ArgValue> }

pub type CommandResult = Result<Vec<String>, String>;  // lines of output or error message

pub struct CommandContext<'a> {
    pub registry: &'a crate::mods::ModRegistry,
    pub vfs: &'a dyn crate::vfs::Vfs,
    pub fps: f32,
}

// Handler type for engine built-in commands
pub type CommandHandler = Arc<dyn Fn(ParsedArgs, &CommandContext) -> CommandResult + Send + Sync>;

pub struct CommandNode {
    pub name: String,
    pub description: String,
    pub subcommands: Vec<CommandNode>,
    pub args: Vec<ArgSpec>,
    pub handler: Option<CommandHandler>,
    pub source: CommandSource,
}

pub enum CommandSource {
    Engine,
    Mod(String),  // mod_id
}
```

### `registry.rs` — Namespace Conflict Resolution

- Validate new command name matches `^[a-z_]+$` (reject on failure).
- If name already taken: register as `{mod_id}:{original_name}`.
- If even the namespaced form is taken: warn and skip (extremely unlikely).
- Engine commands are always registered first under their plain names; they cannot be displaced.

```rust
pub struct CommandRegistry {
    roots: Vec<CommandNode>,
}
impl CommandRegistry {
    pub fn register_engine(&mut self, node: CommandNode) { ... }
    pub fn register_mod(&mut self, mod_id: &str, node: CommandNode) -> String { ... } // returns final name
    pub fn find(&self, path: &[&str]) -> Option<&CommandNode> { ... }
    pub fn completions(&self, partial_path: &[&str]) -> Vec<String> { ... }
}
```

### `console.rs` — DevConsole

```rust
pub struct DevConsole {
    pub is_open: bool,
    input_buf: String,
    output: Vec<OutputLine>,
    history: Vec<String>,      // submitted inputs, oldest first
    history_pos: Option<usize>,
    scroll_to_bottom: bool,
    pub registry: CommandRegistry,
    pub fps: f32,              // updated by engine each frame
    pending_invoke: Option<PendingInvoke>,  // for async mod commands
}

pub enum OutputLine {
    Input(String),    // "> command text"
    Text(String),     // command output
    Error(String),    // styled red
}

pub struct PendingInvoke {
    pub request_id: String,
    pub mod_id: String,
}

// ConsoleAction returned from execute() so engine can route IPC
pub enum ConsoleAction {
    None,
    SendIpc { mod_id: String, message: crate::script::ipc::EngineMessage },
}
```

**`DevConsole::render(ctx, window_size)`** — called from the render loop:
- Builds egui `TopBottomPanel` (top-anchored, scrollable output, fixed input row at bottom).
- Tab key → call `completer::complete(registry, &input_buf)` → replace input or show suggestions.
- Up/Down arrows → navigate history.
- Enter → call `self.execute()`.
- Completions shown as a small tooltip/popup above the input line.

**`DevConsole::execute(input)`**:
1. Append `OutputLine::Input("> {input}")`.
2. Push input to history.
3. Parse via `parser::parse(input)` → `(path, args)`.
4. Look up `registry.find(&path)`.
5. If engine command: call `handler(parsed_args, &ctx)`, append output lines.
6. If mod command: return `ConsoleAction::SendIpc` with a `CommandInvoke` message.
7. If not found: append `OutputLine::Error("unknown command: …")`.

**Mod command response**: when engine receives `CommandResponse` IPC, it calls `console.handle_command_response(output)` which appends the lines.

---

## Engine Changes (`src/engine.rs`)

### New fields in `Engine`

```rust
console: DevConsole,
egui_ctx: egui::Context,
egui_state: egui_winit::State,  // initialized in resumed()
```

### `resumed()` additions
After renderer is created: initialize `egui_winit::State` with the event loop and window, initialize the console with all engine commands registered.

### `window_event()` changes

```rust
// Before any existing match arm, feed to egui:
let egui_consumed = self.egui_state.on_window_event(&self.window, &event);

// Toggle on Ctrl+Alt+Enter
if KeyboardInput with Enter pressed && ctrl && alt in keys_pressed {
    self.console.toggle();
    if self.console.is_open { self.set_cursor_captured(false); }
}

// If console open and egui consumed: return early (don't pass to camera/input)
if self.console.is_open && egui_consumed { return; }
```

### `dispatch_messages()` interception
Before calling `dispatch()`, pattern-match and intercept console-specific messages:
```rust
ScriptMessage::RegisterCommand { .. } => { self.console.registry.register_mod(&mod_id, ...) }
ScriptMessage::CommandResponse { .. } => { self.console.handle_command_response(..) }
```

### `frame()` additions
```rust
self.console.fps = 1.0 / dt;

// After scene render, run egui pass:
let input = self.egui_state.take_egui_input(&window);
let full_output = self.egui_ctx.run(input, |ctx| {
    self.console.render(ctx, window_size);
});
// egui_wgpu_renderer renders into the same command encoder, presented in same frame
```

---

## Renderer Changes (`src/renderer/mod.rs`)

Add `egui_wgpu::Renderer` field. In `render()`:
1. Existing scene render pass (unchanged).
2. After: update egui buffers, run egui render pass into same command encoder.
3. Present.

---

## IPC Protocol Additions (`src/script/ipc.rs`)

### New Script → Engine messages
```rust
RegisterCommand {
    name: String,
    description: String,
    subcommands: Vec<CommandNodeSpec>,  // recursive DTO
    args: Vec<ArgSpecDto>,
}

CommandResponse {
    request_id: String,
    output: Vec<String>,
    error: Option<String>,
}
```

### New Engine → Script message
```rust
CommandInvoke {
    request_id: String,
    command_path: Vec<String>,
    args: Vec<serde_json::Value>,  // positional
}
```

Add corresponding DTOs for `CommandNodeSpec` and `ArgSpecDto` (mirror `CommandNode`/`ArgSpec` without the handler).

---

## Runtime API (`runtime/index.ts`)

New `Console` namespace:
```typescript
export const Console = {
  register(spec: CommandSpec): void
}

type ArgType = "string" | "int" | "float" | "bool"

type ArgSpec = {
  name: string        // [a-z_]+ only
  type: ArgType
  required?: boolean
  description?: string
}

type CommandSpec = {
  name: string
  description?: string
  subcommands?: CommandSpec[]
  args?: ArgSpec[]
  handler?: (args: Record<string, string | number | boolean>) => string | string[] | Promise<string | string[]>
}
```

Internally:
- `register()` sends `RegisterCommand` (spec without handler).
- Handlers stored in a local `Map<string, handler>` keyed by the full dotted command path.
- On `CommandInvoke` from engine: look up handler, call it, send `CommandResponse` (or error on exception/timeout).
- Rebuild `mods/core/scripts/index.js` after editing: `bun build src/index.ts --outfile scripts/index.js --minify --external=@whatever/api`.

---

## Engine-Provided Commands

| Command | Args | Description |
|---|---|---|
| `help [command]` | optional command name | List all commands, or show help for one |
| `clear` | — | Clear console output |
| `echo <text…>` | variadic string | Echo args back |
| `engine version` | — | Print engine version from `CARGO_PKG_VERSION` |
| `engine fps` | — | Print current FPS (from rolling dt) |
| `mods list` | — | Table of id, name, version for all loaded mods |
| `mods get <mod_id>` | required string | Full manifest details for one mod |
| `vfs list <mod_id> [prefix]` | required mod_id, optional prefix | List VFS paths for a mod |
| `vfs read <path>` | `mod_id://path` | Print first 50 lines of a VFS text file |
| `markbench [thread_count]` | optional int | Benchmark: sum 1..=1,000,000,000 using N threads |

### `markbench` implementation (`commands/markbench.rs`)

```rust
fn run(thread_count: usize) -> CommandResult {
    let n = 1_000_000_000u64;
    let chunk = n / thread_count as u64;
    let start = std::time::Instant::now();
    let handles: Vec<_> = (0..thread_count).map(|i| {
        let lo = i as u64 * chunk + 1;
        let hi = if i + 1 == thread_count { n } else { lo + chunk - 1 };
        std::thread::spawn(move || (lo..=hi).fold(0u64, |acc, x| acc.wrapping_add(x)))
    }).collect();
    let total: u64 = handles.into_iter().filter_map(|h| h.join().ok()).sum();
    let elapsed = start.elapsed();
    Ok(vec![
        format!("sum(1..={n}) = {total}"),
        format!("time: {:.3}s  threads: {thread_count}", elapsed.as_secs_f64()),
    ])
}
// thread_count defaults to std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)
```

---

## Validation / Verification

1. `cargo build` — no errors.
2. `cargo run` — window opens normally with no console visible.
3. Press **Ctrl+Alt+Enter** — console panel slides in from top.
4. Type `help` → all built-in commands listed.
5. Type `markbench` → runs on all available threads, prints sum and time.
6. Type `markbench 1` → single-thread run (compare times).
7. Type `mods list` → table shows core and any user mods.
8. Type `mods get core` → shows core manifest.
9. Type `vfs list core` → lists core assets.
10. Type `vfs read core://shaders/sprite.wgsl` → prints shader source.
11. Type `engine fps` → current FPS.
12. Tab autocomplete on partial input (`mo` → `mods`, `mods ` → `list get`).
13. Up/Down arrow → navigate input history.
14. Press **Ctrl+Alt+Enter** again → console closes; camera controls resume.
15. In a mod's TypeScript: call `Console.register(...)`, run engine, open console, invoke the mod command → correct output returned.
