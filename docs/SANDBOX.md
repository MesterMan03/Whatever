# Mod Sandboxing

Each scripted mod runs in a dedicated Bun subprocess. The sandbox limits what that process can do — which files it can read or write, whether it can open network connections, and which syscalls it can invoke. Restrictions are applied around `spawn`: some hooks run in the forked child before `exec` (pre-spawn), others configure OS objects after the process is created (post-spawn).

---

## Linux

**Security status: strong**

Three independent layers are applied in the forked child before `exec`:

### 1. Landlock (filesystem + network)

[Landlock](https://landlock.io/) is a Linux kernel access-control mechanism available since kernel 5.13. The engine uses `ABI::V4` in best-effort mode, so on older kernels rules that are not supported are silently skipped rather than failing.

| Path | Access |
|---|---|
| `/usr`, `/lib`, `/lib64`, `/lib32`, `/etc`, `/dev`, `/proc/self`, `/run`, `/sys` | read + execute (needed by the dynamic linker and libc) |
| Bun install directory (`~/.bun` or `$BUN_INSTALL`) | read + execute |
| Engine root (contains `node_modules/@whatever/api`) | read-only |
| Mod root (script entry and sibling imports) | read-only |
| Mod data directory | full read-write (the only persistent storage a mod has) |
| `/tmp` | read-write (Bun's transpilation cache) |
| Everything else | denied |

No `AccessNet` allow-rules are added, so all TCP bind and connect calls are blocked at the Landlock layer.

### 2. seccomp-BPF (syscall denylist)

A BPF filter runs in the child and blocks the following syscalls with `EPERM`:

| Syscall | Reason |
|---|---|
| `ptrace` | prevent process inspection / code injection |
| `mount` / `umount2` | prevent filesystem manipulation |
| `pivot_root` / `chroot` | prevent namespace escapes |
| `kexec_load` / `kexec_file_load` | prevent kernel replacement |
| `reboot` | prevent host reboot |
| `socket(AF_INET, ...)` | block IPv4 (belt-and-suspenders over Landlock) |
| `socket(AF_INET6, ...)` | block IPv6 (belt-and-suspenders over Landlock) |

All other syscalls are allowed. `AF_UNIX` sockets are not blocked because Bun uses them for internal IPC between its main thread and worker threads.

The filter uses a denylist rather than an allowlist to avoid breaking Bun's internal runtime while still blocking the most dangerous privilege-escalation vectors.

### 3. Capability drop (prctl)

`PR_SET_SECUREBITS` is called with:

- `SECBIT_NOROOT` + `SECBIT_NOROOT_LOCKED` — effective root inside the process no longer grants extra capabilities.
- `SECBIT_NO_SETUID_FIXUP` + `SECBIT_NO_SETUID_FIXUP_LOCKED` — setuid/setgid execution inside the sandbox cannot re-elevate capabilities.

`PR_CAP_AMBIENT_CLEAR_ALL` removes all ambient capabilities so they cannot be inherited by child processes.

### Known limitations

- Landlock and seccomp failures are non-fatal — the engine logs a warning and the mod continues with weaker (or no) isolation. This is intentional so that mods remain runnable on kernel versions that predate these features.
- The seccomp filter is a denylist. A sufficiently creative exploit could use an unblocked syscall to escape. A future allowlist-based filter would be stronger but risks breaking Bun updates.
- If the Bun binary cannot be located (none of `$BUN_INSTALL`, `~/.bun/bin/bun`, or `PATH` resolve), Landlock is skipped entirely for that mod. Check the `[sandbox]` lines on stderr if isolation seems absent.
- Mods that spawn child processes inherit the same Landlock and seccomp restrictions, so they cannot escape by exec-ing another binary.

---

## macOS

**Security status: partial / best-effort**

The engine uses Apple's **Seatbelt** (`sandbox_init`) to apply a Scheme-like profile in the forked child before `exec`. Seatbelt is a private Apple API that has been formally deprecated but remains functional on current macOS releases.

### Profile summary

```
(deny default)                       ; deny everything not explicitly allowed
(allow file-read* /usr /System /Library /private/etc /private/tmp /private/var/folders)
(allow file-read* <bun_dir>)         ; Bun install directory
(allow file-read* <engine_root>)     ; node_modules/@whatever/api
(allow file-read* <mod_root>)        ; mod script files
(allow file-write* <mod_data_dir>)   ; mod persistent storage
(allow file-write* /private/tmp)     ; Bun transpilation cache
(deny network*)                      ; no network access
(deny process-exec*)                 ; no exec of other binaries
(allow ipc-posix-shm)                ; Bun shared memory
(allow mach-lookup)                  ; macOS system services
(allow process-info* (target self))
(allow signal (target self))
```

Bun detection tries `$BUN_INSTALL`, `~/.bun`, `/opt/homebrew/bin/bun`, and `/usr/local/bin/bun` in that order; if none is found, `/usr/local` is used as a fallback.

### Known limitations

- `sandbox_init` is deprecated. Apple may remove it in a future macOS version without notice, at which point the sandbox would silently be skipped and the mod would run unrestricted.
- There is no syscall filter layer on macOS (no seccomp equivalent exposed to userspace). The Seatbelt profile is the only enforcement mechanism.
- Profile paths containing `"` or `\` are stripped before insertion into the profile string. Paths with those characters will not be accessible inside the sandbox.
- `sandbox_init` failure is non-fatal: the engine logs a warning and the mod continues without Seatbelt.

---

## Windows

**Security status: partial**

Windows does not expose a pre-exec hook for applying restrictions before process creation (there is no `fork`/`exec` split). Sandbox restrictions are applied **post-spawn** via a **Windows Job Object**.

### What the Job Object does

A new anonymous Job Object is created and the child process is immediately assigned to it. The Job Object is configured with:

| Limit | Value |
|---|---|
| `KILL_ON_JOB_CLOSE` | when the engine drops the `SandboxGuard`, all processes in the job are killed |
| `ACTIVE_PROCESS_LIMIT` | 4 (enough for Bun's internal worker threads) |

The `SandboxGuard` RAII wrapper holds the Job Object `HANDLE`. It is stored in `ScriptProcess` and dropped when the script process is cleaned up, ensuring the mod process tree is always terminated.

### Known limitations

- **No filesystem isolation.** The mod process can read and write arbitrary paths on the filesystem, limited only by the OS user's ACLs. Landlock and Seatbelt have no Windows equivalent that can be applied from a parent process.
- **No network isolation.** The mod can open network connections without restriction.
- **No syscall filter.** There is no seccomp equivalent available in this configuration.
- The Job Object `KILL_ON_JOB_CLOSE` limit only prevents the process tree from outliving the engine; it does not constrain what the process can do while it runs.
- If `CreateJobObjectW` or `AssignProcessToJobObject` fail, the guard is returned with an invalid handle and no enforcement is applied. A warning is logged but the mod continues running.
- `ACTIVE_PROCESS_LIMIT: 4` is a rough heuristic for Bun's internal thread count. Bun updates that spawn more worker threads could hit this limit and fail to start.

---

## Comparison

| Feature | Linux | macOS | Windows |
|---|---|---|---|
| Filesystem read restriction | Landlock | Seatbelt | none |
| Filesystem write restriction | Landlock | Seatbelt | none |
| Network block | Landlock + seccomp | Seatbelt (`deny network*`) | none |
| Syscall filter | seccomp-BPF | none | none |
| Capability drop | prctl | n/a | n/a |
| Process-tree cleanup | n/a | n/a | Job Object `KILL_ON_JOB_CLOSE` |
| Graceful degradation | yes (warns, continues) | yes (warns, continues) | yes (warns, continues) |

**Bottom line:** Linux provides the strongest isolation. macOS provides meaningful filesystem and network restrictions through Seatbelt but no syscall layer, and depends on a deprecated API. Windows provides only process-tree lifetime management; filesystem and network access are fully unrestricted. Do not run untrusted mods on Windows.