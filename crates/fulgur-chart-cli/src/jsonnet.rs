use std::{
    cell::RefCell,
    collections::HashMap,
    fmt::Write as FmtWrite,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    ops::ControlFlow,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use clap::Args;
use jrsonnet_evaluator::{
    AsPathLike, IStr, ImportResolver, InitialContextBuilder, ObjValueBuilder, Source, SourcePath,
    State, Thunk, Val,
    error::Result as JrsonnetResult,
    manifest::{JsonFormat, ManifestFormat},
    trace::PathResolver,
};
use jrsonnet_stdlib::{Settings, StdTracePrinter, stdlib_uncached};

const MAX_JSONNET_SOURCE_BYTES: usize = 1024 * 1024;
const MAX_JSONNET_IMPORTS: usize = 128;
const MAX_JSONNET_TOTAL_SOURCE_BYTES: usize = 16 * 1024 * 1024;
const MAX_JSONNET_OUTPUT_DEPTH: usize = 128;
const MAX_JSONNET_OUTPUT_VALUES: usize = 8_000_000;
const MAX_JSONNET_OUTPUT_FIELDS: usize = 4_000_000;
const MAX_JSONNET_RAW_FIELDS: usize = 4_000_000;
const MAX_JSONNET_FIELDS_PER_OBJECT: usize = 1_000_000;
const MAX_JSONNET_OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const MAX_JSONNET_STDERR_BYTES: usize = 64 * 1024;
const MAX_JSONNET_WORKER_MEMORY_BYTES: usize = 512 * 1024 * 1024;
const MAX_JSONNET_WORKER_CPU_SECONDS: u64 = 30;
const MAX_JSONNET_WORKER_WALL_SECONDS: u64 = 45;
const WORKER_PROTOCOL_BYTE: u8 = 0x01;

#[derive(Args)]
pub struct WorkerArgs {
    #[arg(long)]
    file: Option<PathBuf>,
}

/// std.parseYaml を除去したカスタム ContextInitializer。
/// parseYaml は YAML anchor bomb による DoS を起こし得るため公開しない。
#[derive(jrsonnet_gcmodule::Trace, Clone)]
struct FulgurContextInitializer {
    inner: jrsonnet_stdlib::ContextInitializer,
    patched_stdlib_obj: jrsonnet_evaluator::ObjValue,
}

impl FulgurContextInitializer {
    fn new(resolver: PathResolver) -> Self {
        let settings = Settings {
            ext_vars: HashMap::new(),
            ext_natives: HashMap::new(),
            trace_printer: Rc::new(StdTracePrinter::new(resolver.clone())),
            path_resolver: resolver.clone(),
        };
        let settings_cc = jrsonnet_gcmodule::Cc::new(RefCell::new(settings));
        let base_stdlib = stdlib_uncached(settings_cc);

        let mut b = ObjValueBuilder::new();
        b.with_super(base_stdlib);
        // IStr は interior mutability を持つが、インターン済みで実質不変。
        // jrsonnet の with_fields_omitted は FxHashSet<IStr> を要求するため抑制する。
        #[allow(clippy::mutable_key_type)]
        let mut omit = jrsonnet_evaluator::rustc_hash::FxHashSet::default();
        omit.insert(IStr::from("parseYaml"));
        b.with_fields_omitted(omit);
        let patched_stdlib_obj = b.build();

        Self {
            inner: jrsonnet_stdlib::ContextInitializer::new(resolver),
            patched_stdlib_obj,
        }
    }
}

impl jrsonnet_evaluator::ContextInitializer for FulgurContextInitializer {
    fn populate(&self, source: Source, builder: &mut InitialContextBuilder) {
        let mut b = ObjValueBuilder::new();
        b.with_super(self.patched_stdlib_obj.clone());
        b.field("thisFile").hide().value({
            let sp = source.source_path();
            sp.path().map_or_else(
                || sp.to_string(),
                |p| self.inner.settings().path_resolver.resolve(p),
            )
        });
        builder.bind("std", Thunk::evaluated(Val::Obj(b.build())));
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[derive(Default)]
struct SourceBudget {
    total_bytes: usize,
    import_count: usize,
    root_loaded: bool,
}

/// .jsonnet のディレクトリ外の import と、過大な source 読込を拒否する。
struct SandboxedImportResolver {
    root: PathBuf,
    entry: PathBuf,
    budget: RefCell<SourceBudget>,
}

impl SandboxedImportResolver {
    fn new(jsonnet_path: &Path) -> std::io::Result<Self> {
        let root = jsonnet_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."))
            .canonicalize()?;
        let entry = jsonnet_path.canonicalize()?;
        Ok(Self {
            root,
            entry,
            budget: RefCell::new(SourceBudget::default()),
        })
    }
}

// SandboxedImportResolver は GC 管理ヒープを持たない（パスと整数カウンタのみ）。
impl jrsonnet_gcmodule::Trace for SandboxedImportResolver {
    fn trace(&self, _tracer: &mut jrsonnet_gcmodule::Tracer) {}
    fn is_type_tracked() -> bool {
        false
    }
}
// SAFETY: GC ヒープへの参照を持たないため循環しない。
unsafe impl jrsonnet_gcmodule::Acyclic for SandboxedImportResolver {}

impl ImportResolver for SandboxedImportResolver {
    fn resolve_from(&self, from: &SourcePath, path: &dyn AsPathLike) -> JrsonnetResult<SourcePath> {
        let base = match from.path() {
            Some(path)
                if from
                    .downcast_ref::<jrsonnet_ir::SourceDirectory>()
                    .is_some() =>
            {
                path.to_path_buf()
            }
            Some(path) => path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            None => std::env::current_dir().map_err(|e| {
                jsonnet_runtime_error(format!("failed to resolve import base directory: {e}"))
            })?,
        };
        let candidate = base.join(path.as_path());
        let metadata = fs::metadata(&candidate).map_err(|e| {
            jsonnet_runtime_error(format!(
                "failed to inspect import path '{}': {e}",
                candidate.display()
            ))
        })?;
        if !metadata.is_file() {
            return Err(jsonnet_runtime_error(format!(
                "import '{}' is not a regular file",
                candidate.display()
            )));
        }

        let canonical = candidate.canonicalize().map_err(|e| {
            jsonnet_runtime_error(format!(
                "failed to canonicalize import path '{}': {e}",
                candidate.display()
            ))
        })?;
        if canonical != self.entry && !canonical.starts_with(&self.root) {
            return Err(jsonnet_runtime_error(format!(
                "import '{}' escapes the sandbox root '{}'",
                canonical.display(),
                self.root.display()
            )));
        }
        Ok(SourcePath::new(jrsonnet_ir::SourceFile::new(canonical)))
    }

    fn load_file_contents(&self, resolved: &SourcePath) -> JrsonnetResult<Vec<u8>> {
        let path = resolved.path().ok_or_else(|| {
            jsonnet_runtime_error("import of non-filesystem source is not allowed in sandbox mode")
        })?;
        let is_root = path == self.entry;
        let (remaining_bytes, is_import) = {
            let budget = self.budget.borrow();
            let is_first_root_load = is_root && !budget.root_loaded;
            if !is_first_root_load && budget.import_count >= MAX_JSONNET_IMPORTS {
                return Err(jsonnet_runtime_error(format!(
                    "Jsonnet import count limit exceeded ({MAX_JSONNET_IMPORTS})"
                )));
            }
            (
                MAX_JSONNET_TOTAL_SOURCE_BYTES.saturating_sub(budget.total_bytes),
                !is_first_root_load,
            )
        };
        if remaining_bytes == 0 {
            return Err(jsonnet_runtime_error(format!(
                "Jsonnet total source byte limit exceeded ({MAX_JSONNET_TOTAL_SOURCE_BYTES})"
            )));
        }

        let max_file_bytes = MAX_JSONNET_SOURCE_BYTES.min(remaining_bytes);
        let read = read_bounded_file(path, max_file_bytes).map_err(|e| {
            jsonnet_runtime_error(format!(
                "failed to read Jsonnet source '{}': {e}",
                path.display()
            ))
        })?;
        if read.too_large {
            if read.observed_len > MAX_JSONNET_SOURCE_BYTES as u64 {
                return Err(jsonnet_runtime_error(format!(
                    "Jsonnet source exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte per-file limit"
                )));
            }
            return Err(jsonnet_runtime_error(format!(
                "Jsonnet total source byte limit exceeded ({MAX_JSONNET_TOTAL_SOURCE_BYTES})"
            )));
        }

        let mut budget = self.budget.borrow_mut();
        budget.total_bytes = budget.total_bytes.saturating_add(read.bytes.len());
        if is_root {
            budget.root_loaded = true;
        }
        if is_import {
            budget.import_count += 1;
        }
        Ok(read.bytes)
    }
}

fn jsonnet_runtime_error(message: impl Into<IStr>) -> jrsonnet_evaluator::Error {
    jrsonnet_evaluator::Error::new(jrsonnet_evaluator::RuntimeError(message.into()))
}

struct BoundedFileRead {
    bytes: Vec<u8>,
    too_large: bool,
    observed_len: u64,
}

fn read_bounded_file(path: &Path, limit: usize) -> io::Result<BoundedFileRead> {
    let file = open_jsonnet_file(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Jsonnet source is not a regular file",
        ));
    }
    if metadata.len() > limit as u64 {
        return Ok(BoundedFileRead {
            bytes: Vec::new(),
            too_large: true,
            observed_len: metadata.len(),
        });
    }

    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1) as u64)
        .read_to_end(&mut bytes)?;
    let too_large = bytes.len() > limit;
    let observed_len = bytes.len() as u64;
    if too_large {
        bytes.truncate(limit);
    }
    Ok(BoundedFileRead {
        bytes,
        too_large,
        observed_len,
    })
}

fn open_jsonnet_file(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    options.open(path)
}

struct JsonnetOutputBudget {
    values: usize,
    fields: usize,
    raw_fields: usize,
    estimated_bytes: usize,
}

impl JsonnetOutputBudget {
    fn add_bytes(&mut self, count: usize) -> Result<(), String> {
        let total = self.estimated_bytes.saturating_add(count);
        if total > MAX_JSONNET_OUTPUT_BYTES {
            return Err(format!(
                "Jsonnet output budget exceeded ({MAX_JSONNET_OUTPUT_BYTES} bytes)"
            ));
        }
        self.estimated_bytes = total;
        Ok(())
    }

    fn add_value(&mut self, depth: usize) -> Result<(), String> {
        if depth > MAX_JSONNET_OUTPUT_DEPTH {
            return Err(format!(
                "Jsonnet output depth limit exceeded ({MAX_JSONNET_OUTPUT_DEPTH})"
            ));
        }
        if self.values >= MAX_JSONNET_OUTPUT_VALUES {
            return Err(format!(
                "Jsonnet output value limit exceeded ({MAX_JSONNET_OUTPUT_VALUES})"
            ));
        }
        self.values += 1;
        Ok(())
    }

    fn ensure_child_values_fit(&self, count: usize) -> Result<(), String> {
        if count > MAX_JSONNET_OUTPUT_VALUES.saturating_sub(self.values) {
            return Err(format!(
                "Jsonnet output value limit exceeded ({MAX_JSONNET_OUTPUT_VALUES})"
            ));
        }
        Ok(())
    }

    fn add_field(&mut self, key: &str, comma: bool) -> Result<(), String> {
        if self.fields >= MAX_JSONNET_OUTPUT_FIELDS {
            return Err(format!(
                "Jsonnet output field limit exceeded ({MAX_JSONNET_OUTPUT_FIELDS})"
            ));
        }
        self.fields += 1;
        if comma {
            self.add_bytes(1)?;
        }
        self.add_bytes(1)?; // colon
        self.add_json_string(key.as_bytes())
    }

    fn add_json_string(&mut self, bytes: &[u8]) -> Result<(), String> {
        let remaining = MAX_JSONNET_OUTPUT_BYTES.saturating_sub(self.estimated_bytes);
        if bytes.len().saturating_add(2) > remaining {
            return Err(format!(
                "Jsonnet output budget exceeded ({MAX_JSONNET_OUTPUT_BYTES} bytes)"
            ));
        }
        let mut encoded = 2usize;
        for byte in bytes {
            encoded = encoded.saturating_add(json_escape_width(*byte));
            if encoded > remaining {
                return Err(format!(
                    "Jsonnet output budget exceeded ({MAX_JSONNET_OUTPUT_BYTES} bytes)"
                ));
            }
        }
        self.add_bytes(encoded)
    }

    fn add_json_str_value(
        &mut self,
        value: &jrsonnet_evaluator::val::StrValue,
    ) -> Result<(), String> {
        let remaining = MAX_JSONNET_OUTPUT_BYTES.saturating_sub(self.estimated_bytes);
        if value.len().saturating_add(2) > remaining {
            return Err(format!(
                "Jsonnet output budget exceeded ({MAX_JSONNET_OUTPUT_BYTES} bytes)"
            ));
        }
        let mut encoded = 2usize;
        value.chunks(&mut |chunk| {
            for byte in chunk.as_bytes() {
                encoded = encoded.saturating_add(json_escape_width(*byte));
                if encoded > remaining {
                    break;
                }
            }
        });
        if encoded > remaining {
            return Err(format!(
                "Jsonnet output budget exceeded ({MAX_JSONNET_OUTPUT_BYTES} bytes)"
            ));
        }
        self.add_bytes(encoded)
    }
}

fn json_escape_width(byte: u8) -> usize {
    match byte {
        b'"' | b'\\' | 0x08 | 0x09 | 0x0a | 0x0c | 0x0d => 2,
        0x00..=0x1f => 6,
        _ => 1,
    }
}

fn preflight_output(
    value: &Val,
    depth: usize,
    budget: &mut JsonnetOutputBudget,
) -> Result<(), String> {
    budget.add_value(depth)?;
    match value {
        Val::Null => budget.add_bytes(4),
        Val::Bool(true) => budget.add_bytes(4),
        Val::Bool(false) => budget.add_bytes(5),
        Val::Str(value) => budget.add_json_str_value(value),
        Val::Num(number) => {
            let mut count = FmtByteCounter(0);
            write!(&mut count, "{number}").map_err(|e| e.to_string())?;
            budget.add_bytes(count.0)
        }
        Val::Arr(array) => {
            let len = array.len() as usize;
            budget.ensure_child_values_fit(len)?;
            budget.add_bytes(2usize.saturating_add(len.saturating_sub(1)))?;
            for item in array.iter() {
                let item = item.map_err(|e| format!("{e}"))?;
                preflight_output(&item, depth.saturating_add(1), budget)?;
            }
            Ok(())
        }
        Val::Obj(object) => {
            preflight_raw_object_fields(object, budget)?;
            budget.add_bytes(2)?; // braces
            for (field_index, (key, field)) in object.iter().enumerate() {
                budget.add_field(&key, field_index != 0)?;
                let field = field.map_err(|e| format!("{e}"))?;
                preflight_output(&field, depth.saturating_add(1), budget)?;
            }
            Ok(())
        }
        Val::Func(_) => Err("Jsonnet output contains a function which cannot be manifested".into()),
    }
}

fn preflight_raw_object_fields(
    object: &jrsonnet_evaluator::ObjValue,
    budget: &mut JsonnetOutputBudget,
) -> Result<(), String> {
    let remaining = MAX_JSONNET_RAW_FIELDS.saturating_sub(budget.raw_fields);
    let mut raw_fields = 0usize;
    let mut over_limit = false;
    object.enum_fields(&mut |_, _, _, _field| {
        // ObjValue::iter builds its visibility map from every raw declaration,
        // including hidden and omitted fields, so cap all of them first.
        raw_fields += 1;
        if raw_fields > MAX_JSONNET_FIELDS_PER_OBJECT || raw_fields > remaining {
            over_limit = true;
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    });
    if over_limit {
        return Err("Jsonnet object field materialization limit exceeded".into());
    }
    budget.raw_fields += raw_fields;
    Ok(())
}

struct FmtByteCounter(usize);

impl std::fmt::Write for FmtByteCounter {
    fn write_str(&mut self, value: &str) -> std::fmt::Result {
        self.0 = self.0.saturating_add(value.len());
        Ok(())
    }
}

fn manifest_with_budget(value: Val) -> Result<String, String> {
    let mut budget = JsonnetOutputBudget {
        values: 0,
        fields: 0,
        raw_fields: 0,
        estimated_bytes: 0,
    };
    preflight_output(&value, 0, &mut budget)?;
    JsonFormat::minify()
        .manifest(value)
        .map_err(|e| format!("{e}"))
}

fn build_jrsonnet_file_state(jsonnet_path: &Path) -> Result<State, String> {
    let resolver = SandboxedImportResolver::new(jsonnet_path)
        .map_err(|e| format!("failed to resolve sandbox root: {e}"))?;
    let mut builder = State::builder();
    builder.import_resolver(resolver);
    builder.context_initializer(FulgurContextInitializer::new(
        PathResolver::new_cwd_fallback(),
    ));
    Ok(builder.build())
}

fn build_jrsonnet_snippet_state() -> State {
    let mut builder = State::builder();
    builder.context_initializer(FulgurContextInitializer::new(
        PathResolver::new_cwd_fallback(),
    ));
    builder.build()
}

fn evaluate_snippet(source_name: &str, source: &str) -> Result<String, String> {
    if source.len() > MAX_JSONNET_SOURCE_BYTES {
        return Err(format!(
            "Jsonnet source exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte limit ({})",
            source.len()
        ));
    }
    let state = build_jrsonnet_snippet_state();
    let _guard = state.enter();
    let value = state
        .evaluate_snippet(IStr::from(source_name), IStr::from(source))
        .map_err(|e| format!("{e}"))?;
    manifest_with_budget(value)
}

fn evaluate_file(path: &Path) -> Result<String, String> {
    let state = build_jrsonnet_file_state(path)?;
    let _guard = state.enter();
    let value = state.import(path).map_err(|e| format!("{e}"))?;
    manifest_with_budget(value)
}

fn read_bounded_source(reader: impl Read) -> Result<String, String> {
    let mut bytes = Vec::with_capacity(64 * 1024);
    reader
        .take(MAX_JSONNET_SOURCE_BYTES.saturating_add(1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("{e}"))?;
    if bytes.len() > MAX_JSONNET_SOURCE_BYTES {
        return Err(format!(
            "Jsonnet source exceeds the {MAX_JSONNET_SOURCE_BYTES}-byte limit ({})",
            bytes.len()
        ));
    }
    String::from_utf8(bytes).map_err(|e| format!("Jsonnet source is not valid UTF-8: {e}"))
}

#[derive(Clone, Copy)]
enum WorkerInput<'a> {
    File(&'a Path),
    Stdin,
}

/// CLI 側から Jsonnet を制限付き worker で評価する。
pub fn evaluate_stdin() -> Result<String, String> {
    run_worker_process(WorkerInput::Stdin)
}

/// CLI 側から Jsonnet ファイルを制限付き worker で評価する。
pub fn evaluate_file_in_worker(path: &Path) -> Result<String, String> {
    run_worker_process(WorkerInput::File(path))
}

struct CapturedPipe {
    bytes: Vec<u8>,
    truncated: bool,
}

fn drain_pipe(
    mut reader: impl Read,
    limit: usize,
    exceeded: Option<Arc<AtomicBool>>,
) -> io::Result<CapturedPipe> {
    let mut bytes = Vec::with_capacity(limit.min(64 * 1024));
    let mut buffer = [0u8; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        let keep = count.min(remaining);
        bytes.extend_from_slice(&buffer[..keep]);
        if keep < count {
            truncated = true;
            if let Some(exceeded) = &exceeded {
                exceeded.store(true, Ordering::Relaxed);
            }
        }
    }
    Ok(CapturedPipe { bytes, truncated })
}

fn start_pipe_reader(
    reader: impl Read + Send + 'static,
    limit: usize,
    exceeded: Option<Arc<AtomicBool>>,
) -> JoinHandle<io::Result<CapturedPipe>> {
    thread::spawn(move || drain_pipe(reader, limit, exceeded))
}

fn join_pipe_reader(handle: JoinHandle<io::Result<CapturedPipe>>) -> io::Result<CapturedPipe> {
    handle
        .join()
        .map_err(|_| io::Error::other("Jsonnet worker output reader panicked"))?
}

fn run_worker_process(input: WorkerInput<'_>) -> Result<String, String> {
    let executable = std::env::current_exe().map_err(|e| format!("failed to locate CLI: {e}"))?;
    let mut command = Command::new(executable);
    command
        .arg("__jsonnet-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let WorkerInput::File(path) = input {
        command.arg("--file").arg(path);
    }
    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|e| format!("failed to start Jsonnet worker: {e}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "failed to capture Jsonnet worker output".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "failed to capture Jsonnet worker errors".to_string())?;
    let output_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_reader = start_pipe_reader(
        stdout,
        MAX_JSONNET_OUTPUT_BYTES,
        Some(output_exceeded.clone()),
    );
    let stderr_reader = start_pipe_reader(stderr, MAX_JSONNET_STDERR_BYTES, None);

    let mut child_stdin = child
        .stdin
        .take()
        .ok_or_else(|| "failed to open Jsonnet worker input".to_string())?;
    let input_writer = match input {
        WorkerInput::File(_) => thread::spawn(move || {
            let result = child_stdin.write_all(&[WORKER_PROTOCOL_BYTE]);
            drop(child_stdin);
            result
        }),
        WorkerInput::Stdin => thread::spawn(move || {
            let mut input = io::stdin().lock();
            child_stdin.write_all(&[WORKER_PROTOCOL_BYTE])?;
            let mut remaining = MAX_JSONNET_SOURCE_BYTES.saturating_add(1);
            let mut buffer = [0u8; 8192];
            while remaining != 0 {
                let chunk_len = buffer.len().min(remaining);
                let count = input.read(&mut buffer[..chunk_len])?;
                if count == 0 {
                    break;
                }
                child_stdin.write_all(&buffer[..count])?;
                remaining -= count;
            }
            Ok(())
        }),
    };

    let mut timed_out = false;
    let mut output_too_large = false;
    let status = loop {
        if output_exceeded.load(Ordering::Relaxed) {
            output_too_large = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| format!("failed to stop Jsonnet worker: {e}"))?;
        }
        if started.elapsed() >= Duration::from_secs(MAX_JSONNET_WORKER_WALL_SECONDS) {
            timed_out = true;
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| format!("failed to stop Jsonnet worker: {e}"))?;
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|e| format!("failed to wait for Jsonnet worker: {e}"))?
        {
            break status;
        }
        thread::sleep(Duration::from_millis(10));
    };
    let input_result = if timed_out || output_too_large || !status.success() {
        // The input thread may be blocked waiting for more stdin bytes. The CLI
        // exits after returning this error, so leave that thread detached.
        None
    } else {
        Some(
            input_writer
                .join()
                .map_err(|_| "Jsonnet worker input writer panicked".to_string())?,
        )
    };
    let stdout = join_pipe_reader(stdout_reader)
        .map_err(|e| format!("failed to read Jsonnet output: {e}"))?;
    let stderr = join_pipe_reader(stderr_reader)
        .map_err(|e| format!("failed to read Jsonnet errors: {e}"))?;
    if timed_out {
        return Err(format!(
            "Jsonnet worker exceeded the {MAX_JSONNET_WORKER_WALL_SECONDS}-second wall-clock limit"
        ));
    }
    if output_too_large || stdout.truncated {
        return Err(format!(
            "Jsonnet output exceeds the {MAX_JSONNET_OUTPUT_BYTES}-byte limit"
        ));
    }
    if !status.success() {
        let detail = String::from_utf8_lossy(&stderr.bytes);
        let suffix = if stderr.truncated {
            " [stderr truncated]"
        } else {
            ""
        };
        return Err(format!("worker exited with {status}: {detail}{suffix}"));
    }
    if let Some(input_result) = input_result {
        input_result.map_err(|e| format!("failed to send source to Jsonnet worker: {e}"))?;
    }
    String::from_utf8(stdout.bytes)
        .map_err(|e| format!("Jsonnet worker returned invalid UTF-8: {e}"))
}

/// Hidden worker entry point. Limits are installed before reading/evaluating source.
pub fn run_worker(args: WorkerArgs) -> Result<(), String> {
    let _resource_guard = platform_limits::apply()
        .map_err(|e| format!("failed to apply Jsonnet worker resource limits: {e}"))?;
    let mut input = io::stdin().lock();
    let mut protocol = [0u8; 1];
    input
        .read_exact(&mut protocol)
        .map_err(|e| format!("failed to read worker protocol: {e}"))?;
    if protocol[0] != WORKER_PROTOCOL_BYTE {
        return Err("invalid Jsonnet worker protocol".into());
    }

    let json = match args.file {
        Some(path) => evaluate_file(&path)?,
        None => {
            let source = read_bounded_source(input)?;
            evaluate_snippet("(stdin)", &source)?
        }
    };
    io::stdout()
        .lock()
        .write_all(json.as_bytes())
        .map_err(|e| format!("failed to return Jsonnet output: {e}"))
}

#[cfg(unix)]
mod platform_limits {
    use std::io;

    #[cfg(target_env = "gnu")]
    type RLimitResource = libc::__rlimit_resource_t;
    #[cfg(not(target_env = "gnu"))]
    type RLimitResource = libc::c_int;

    pub struct ResourceGuard;

    pub fn apply() -> io::Result<ResourceGuard> {
        let memory = super::MAX_JSONNET_WORKER_MEMORY_BYTES as u64;
        set_limit(libc::RLIMIT_AS, memory, memory)?;
        let cpu = super::MAX_JSONNET_WORKER_CPU_SECONDS;
        set_limit(libc::RLIMIT_CPU, cpu.saturating_sub(1), cpu)?;
        // Hidden worker invocations also receive a wall-clock bound.
        // SAFETY: alarm has no pointer arguments and installs a process timer.
        unsafe {
            libc::alarm(super::MAX_JSONNET_WORKER_WALL_SECONDS as libc::c_uint);
        }
        Ok(ResourceGuard)
    }

    fn set_limit(resource: RLimitResource, soft: u64, hard: u64) -> io::Result<()> {
        let limit = libc::rlimit {
            rlim_cur: soft as libc::rlim_t,
            rlim_max: hard as libc::rlim_t,
        };
        // SAFETY: setrlimit only reads the initialized rlimit structure.
        if unsafe { libc::setrlimit(resource, &limit) } == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(all(test, unix))]
mod resolver_tests {
    use std::{
        ffi::CString,
        fs,
        os::unix::ffi::OsStrExt,
        path::Path,
        time::{Instant, SystemTime, UNIX_EPOCH},
    };

    use super::SandboxedImportResolver;
    use jrsonnet_evaluator::{ImportResolver, SourcePath};

    #[test]
    fn resolver_rejects_fifo_without_opening_or_reading_it() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("fulgur-jsonnet-fifo-{nonce}"));
        fs::create_dir(&directory).unwrap();
        let entry = directory.join("chart.jsonnet");
        fs::write(&entry, "{}\n").unwrap();
        let fifo = directory.join("input.fifo");
        let fifo_c = CString::new(fifo.as_os_str().as_bytes()).unwrap();
        // SAFETY: the path is a valid, nul-terminated temporary path.
        assert_eq!(unsafe { libc::mkfifo(fifo_c.as_ptr(), 0o600) }, 0);

        let resolver = SandboxedImportResolver::new(&entry).unwrap();
        let start = Instant::now();
        let result = resolver.resolve_from(&SourcePath::default(), &Path::new(&fifo));
        assert!(result.is_err(), "FIFO import should be rejected");
        assert!(start.elapsed().as_secs() < 1, "FIFO resolution blocked");

        fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(windows)]
mod platform_limits {
    use std::{
        io,
        mem::size_of,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        ptr::null,
        thread,
        time::Duration,
    };

    use windows_sys::Win32::{
        Foundation::{GetLastError, HANDLE},
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
                JOB_OBJECT_LIMIT_PROCESS_MEMORY, JOB_OBJECT_LIMIT_PROCESS_TIME,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
                SetInformationJobObject,
            },
            Threading::{GetCurrentProcess, TerminateProcess},
        },
    };

    pub struct ResourceGuard {
        _job: OwnedHandle,
    }

    pub fn apply() -> io::Result<ResourceGuard> {
        // The handle stays alive for the worker lifetime; KILL_ON_JOB_CLOSE also covers crashes.
        let raw = unsafe { CreateJobObjectW(null(), null()) };
        if raw.is_null() {
            return Err(last_error());
        }
        let job = unsafe { OwnedHandle::from_raw_handle(raw) };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
            | JOB_OBJECT_LIMIT_PROCESS_MEMORY
            | JOB_OBJECT_LIMIT_PROCESS_TIME;
        limits.BasicLimitInformation.PerProcessUserTimeLimit =
            (super::MAX_JSONNET_WORKER_CPU_SECONDS as i64) * 10_000_000;
        limits.ProcessMemoryLimit = super::MAX_JSONNET_WORKER_MEMORY_BYTES;
        let updated = unsafe {
            SetInformationJobObject(
                job.as_raw_handle() as HANDLE,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if updated == 0 {
            return Err(last_error());
        }
        let assigned =
            unsafe { AssignProcessToJobObject(job.as_raw_handle() as HANDLE, GetCurrentProcess()) };
        if assigned == 0 {
            return Err(last_error());
        }
        thread::Builder::new()
            .name("jsonnet-wall-limit".into())
            .spawn(|| {
                thread::sleep(Duration::from_secs(super::MAX_JSONNET_WORKER_WALL_SECONDS));
                // SAFETY: the watchdog terminates only its own worker process.
                unsafe {
                    TerminateProcess(GetCurrentProcess(), 1);
                }
            })?;
        Ok(ResourceGuard { _job: job })
    }

    fn last_error() -> io::Error {
        io::Error::from_raw_os_error(unsafe { GetLastError() } as i32)
    }
}

#[cfg(not(any(unix, windows)))]
mod platform_limits {
    use std::io;

    pub struct ResourceGuard;

    pub fn apply() -> io::Result<ResourceGuard> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Jsonnet resource limits are unsupported on this platform",
        ))
    }
}
