//! `gtc worker` subcommand implementation: author and build agentic-worker
//! `.gtpack` archives from a `greentic-dw-authoring::WorkerSpec`, without any
//! of the Designer's DB/orchestration machinery.
//!
//! Subcommands:
//! - `init`     — scaffold a starter `WorkerSpec` YAML file for a given kind.
//! - `validate` — parse a `WorkerSpec` file and run structural validation.
//! - `build`    — parse, validate, bake local knowledge documents, and
//!   assemble a runner-loadable `.gtpack`.
//! - `new`      — non-interactive: load a `WorkerSpec` (or pre-filled
//!   "answers" file) and run the `build` path. A full interactive TTY wizard
//!   is out of scope for this task (see `run_new` below).

use std::fs;
use std::path::{Path, PathBuf};

use greentic_dw_authoring::assemble::build_worker_pack;
use greentic_dw_authoring::{
    AgentGraphSpec, AgentKind, Coordinator, DeepWorkerSpec, KnowledgeInput, LlmRef, Specialist,
    WorkerSpec, validate,
};

use crate::cli_types::{CliError, WorkerArgs, WorkerSub};

pub fn run_worker(args: WorkerArgs) -> Result<(), CliError> {
    match args.cmd {
        WorkerSub::Init { kind, out } => run_init(&kind, out),
        WorkerSub::Validate { spec } => run_validate(&spec),
        WorkerSub::Build { spec, out } => run_build(&spec, out.as_deref()),
        WorkerSub::New {
            answers,
            out,
            schema,
        } => run_new(answers.as_deref(), out.as_deref(), schema),
    }
}

/// Parse a free-form kind string (`single_turn`, `single-turn`, `agent_graph`,
/// `agent-graph`, `deep_worker`, `deep-worker`, case-insensitively) into an
/// [`AgentKind`].
fn parse_agent_kind(kind: &str) -> Result<AgentKind, CliError> {
    match kind.to_ascii_lowercase().replace('-', "_").as_str() {
        "single_turn" => Ok(AgentKind::SingleTurn),
        "agent_graph" => Ok(AgentKind::AgentGraph),
        "deep_worker" => Ok(AgentKind::DeepWorker),
        _ => Err(CliError::UnknownAgentKind(kind.to_string())),
    }
}

/// Build a minimal, already-valid starter [`WorkerSpec`] for `kind` — every
/// field required by `validate::validate` for that kind is pre-filled so the
/// scaffolded file can be built immediately.
fn starter_spec(kind: AgentKind) -> WorkerSpec {
    let mut spec = WorkerSpec {
        kind,
        name: "my-worker".to_string(),
        description: Some("Describe what this worker does.".to_string()),
        tenant: None,
        llm: LlmRef {
            provider: "openai".to_string(),
            model: "gpt-4o".to_string(),
            credential_ref: None,
        },
        instructions: "You are a helpful assistant.".to_string(),
        tools: Vec::new(),
        memory: None,
        knowledge: None,
        guardrails: Vec::new(),
        agent_graph: None,
        deep_worker: None,
        locale: None,
        icon: None,
        vertical: None,
        opening_message: None,
        extension_tools: Vec::new(),
    };

    match kind {
        AgentKind::SingleTurn => {}
        AgentKind::AgentGraph => {
            spec.agent_graph = Some(AgentGraphSpec {
                coordinator: Coordinator {
                    instructions: "Route each request to the right specialist.".to_string(),
                },
                specialists: vec![
                    Specialist {
                        name: "specialist_one".to_string(),
                        instructions: "Handle specialist_one tasks.".to_string(),
                        tools: Vec::new(),
                    },
                    Specialist {
                        name: "specialist_two".to_string(),
                        instructions: "Handle specialist_two tasks.".to_string(),
                        tools: Vec::new(),
                    },
                ],
            });
        }
        AgentKind::DeepWorker => {
            spec.deep_worker = Some(DeepWorkerSpec::default());
        }
    }

    spec
}

fn run_init(kind: &str, out: Option<PathBuf>) -> Result<(), CliError> {
    let agent_kind = parse_agent_kind(kind)?;
    let spec = starter_spec(agent_kind);
    let out_path = out.unwrap_or_else(|| PathBuf::from(format!("./{kind}-worker.yaml")));

    let yaml = serde_yaml_bw::to_string(&spec).map_err(CliError::WorkerSpecSerialize)?;
    fs::write(&out_path, yaml).map_err(|source| CliError::WorkerSpecWrite {
        path: out_path.display().to_string(),
        source,
    })?;

    println!("{}", out_path.display());
    Ok(())
}

/// Load a `WorkerSpec` from a YAML (or JSON, which is valid YAML) file.
fn load_spec(path: &Path) -> Result<WorkerSpec, CliError> {
    let raw = fs::read_to_string(path).map_err(|source| CliError::WorkerSpecRead {
        path: path.display().to_string(),
        source,
    })?;
    serde_yaml_bw::from_str(&raw).map_err(|source| CliError::WorkerSpecParse {
        path: path.display().to_string(),
        source,
    })
}

/// Join `validate::validate`'s errors into a `"field: message"`-per-line
/// report, printing each line to stdout as it goes.
fn report_validation_errors(errors: &[validate::ValidationError]) -> String {
    let mut lines = Vec::with_capacity(errors.len());
    for error in errors {
        let line = format!("{}: {}", error.field, error.message);
        println!("{line}");
        lines.push(line);
    }
    lines.join("\n")
}

fn run_validate(spec_path: &Path) -> Result<(), CliError> {
    let spec = load_spec(spec_path)?;
    match validate::validate(&spec) {
        Ok(()) => {
            println!("valid");
            Ok(())
        }
        Err(errors) => {
            let joined = report_validation_errors(&errors);
            Err(CliError::WorkerSpecInvalid(joined))
        }
    }
}

/// Resolve `spec.knowledge.documents` (local file paths, relative to
/// `spec_dir` when not absolute) into [`KnowledgeInput`]s. Resolution is
/// deterministic and independent of the process's current working
/// directory: a relative document path is always joined onto `spec_dir`
/// first, and only that resolved path's existence is consulted — it is
/// never shadowed by a coincidentally same-named file elsewhere (e.g. in
/// the CWD). A missing document at the resolved path is a hard error.
fn load_knowledge_inputs(
    spec: &WorkerSpec,
    spec_dir: &Path,
) -> Result<Vec<KnowledgeInput>, CliError> {
    let Some(knowledge) = spec.knowledge.as_ref() else {
        return Ok(Vec::new());
    };

    let mut inputs = Vec::with_capacity(knowledge.documents.len());
    for document in &knowledge.documents {
        let document_path = Path::new(document);
        let resolved = if document_path.is_absolute() {
            document_path.to_path_buf()
        } else {
            spec_dir.join(document_path)
        };

        let text = extract_document_text(&resolved)?;
        let id = resolved
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_string())
            .unwrap_or_else(|| document.clone());
        inputs.push(KnowledgeInput { id, text });
    }

    Ok(inputs)
}

/// Extract text from a local knowledge document. `.pdf` files go through
/// `pdf-extract`; every other extension is read as UTF-8 text (`.txt`,
/// `.md`, and anything else that is actually text).
fn extract_document_text(path: &Path) -> Result<String, CliError> {
    let is_pdf = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.eq_ignore_ascii_case("pdf"))
        .unwrap_or(false);

    if is_pdf {
        let bytes = fs::read(path).map_err(|source| CliError::KnowledgeDocumentRead {
            path: path.display().to_string(),
            source,
        })?;
        pdf_extract::extract_text_from_mem(&bytes).map_err(|error| {
            CliError::KnowledgeDocumentExtract {
                path: path.display().to_string(),
                message: error.to_string(),
            }
        })
    } else {
        fs::read_to_string(path).map_err(|source| CliError::KnowledgeDocumentRead {
            path: path.display().to_string(),
            source,
        })
    }
}

/// The directory `build_worker_pack` should write into: the parent of `out`
/// when given (so `--out somewhere/name.gtpack` writes under `somewhere/`),
/// the current directory otherwise. `build_worker_pack` names the file
/// itself from the spec's own `pack_id`, so the emitted file may not share
/// `out`'s basename — [`finalize_pack_path`] renames it into place afterward
/// when `out` names an explicit file.
fn resolve_out_dir(out: Option<&Path>) -> PathBuf {
    match out {
        Some(path) => match path.parent() {
            Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
            _ => PathBuf::from("."),
        },
        None => PathBuf::from("."),
    }
}

/// Whether `out` names a directory to write into, as opposed to naming the
/// pack file itself: true when the path is empty, ends in a path separator
/// (e.g. `somewhere/`), or already exists as a directory on disk.
fn out_names_directory(out: &Path) -> bool {
    if out.as_os_str().is_empty() {
        return true;
    }
    if out.to_string_lossy().ends_with(std::path::MAIN_SEPARATOR) {
        return true;
    }
    out.is_dir()
}

/// After `build_worker_pack` writes its pack-id-named file, rename it to the
/// exact path the user asked for via `--out`, when `--out` named a file
/// rather than a bare directory. Returns the path that should be reported to
/// the user as the final pack location.
fn finalize_pack_path(built_path: PathBuf, out: Option<&Path>) -> Result<PathBuf, CliError> {
    match out {
        Some(out_path) if !out_names_directory(out_path) => {
            fs::rename(&built_path, out_path).map_err(|source| CliError::WorkerPackWrite {
                path: out_path.display().to_string(),
                source,
            })?;
            Ok(out_path.to_path_buf())
        }
        _ => Ok(built_path),
    }
}

fn build_from_spec(spec: &WorkerSpec, spec_dir: &Path, out: Option<&Path>) -> Result<(), CliError> {
    validate::validate(spec).map_err(|errors| {
        let joined = report_validation_errors(&errors);
        CliError::WorkerSpecInvalid(joined)
    })?;

    let knowledge = load_knowledge_inputs(spec, spec_dir)?;
    let out_dir = resolve_out_dir(out);
    let pack = build_worker_pack(spec, &knowledge, &out_dir)?;
    let final_path = finalize_pack_path(pack.pack_path, out)?;

    println!("{}", final_path.display());
    Ok(())
}

fn run_build(spec_path: &Path, out: Option<&Path>) -> Result<(), CliError> {
    let spec = load_spec(spec_path)?;
    let spec_dir = spec_path.parent().unwrap_or_else(|| Path::new("."));
    build_from_spec(&spec, spec_dir, out)
}

/// `gtc worker new`: non-interactive path only (see module docs). `--schema`
/// prints a short explanation instead of a JSON schema — `WorkerSpec` does
/// not derive `schemars::JsonSchema` (its `extension_tools` field embeds a
/// vendored type with no `schemars` support), so there is no schema to hand
/// out without hand-authoring and maintaining one separately from the type.
/// `--answers <file>` loads a `WorkerSpec` from that file and runs the same
/// path as `build`. With neither flag, this prints guidance instead of
/// prompting — a full interactive TTY wizard is left for a follow-up task.
fn run_new(answers: Option<&Path>, out: Option<&Path>, schema: bool) -> Result<(), CliError> {
    if schema {
        println!(
            "JSON schema not available for WorkerSpec: its extension_tools field embeds \
             greentic_extension_sdk_contract::AgenticWorkerMetadata, a vendored type with no \
             schemars support. Author a WorkerSpec YAML/JSON file by hand or via `worker init` \
             instead."
        );
        return Ok(());
    }

    match answers {
        Some(answers_path) => {
            let spec = load_spec(answers_path)?;
            let spec_dir = answers_path.parent().unwrap_or_else(|| Path::new("."));
            build_from_spec(&spec, spec_dir, out)
        }
        None => {
            println!(
                "interactive mode is not implemented; provide --answers <file> with a WorkerSpec \
                 YAML/JSON document (see `gtc worker init`)."
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn read_zip_entry(pack: &Path, name: &str) -> Option<Vec<u8>> {
        let f = fs::File::open(pack).ok()?;
        let mut zip = zip::ZipArchive::new(f).ok()?;
        let mut file = zip.by_name(name).ok()?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf).ok()?;
        Some(buf)
    }

    #[test]
    fn init_single_turn_writes_reparseable_yaml() {
        let dir = tempfile::tempdir().unwrap();
        let out_path = dir.path().join("worker.yaml");

        run_init("single_turn", Some(out_path.clone())).expect("init succeeds");

        let raw = fs::read_to_string(&out_path).expect("scaffold file written");
        let spec: WorkerSpec = serde_yaml_bw::from_str(&raw).expect("reparses as WorkerSpec");
        assert_eq!(spec.kind, AgentKind::SingleTurn);
        assert!(validate::validate(&spec).is_ok());
    }

    #[test]
    fn init_agent_graph_writes_valid_spec() {
        let dir = tempfile::tempdir().unwrap();
        let out_path = dir.path().join("worker.yaml");

        run_init("agent_graph", Some(out_path.clone())).expect("init succeeds");

        let raw = fs::read_to_string(&out_path).unwrap();
        let spec: WorkerSpec = serde_yaml_bw::from_str(&raw).unwrap();
        assert_eq!(spec.kind, AgentKind::AgentGraph);
        assert!(validate::validate(&spec).is_ok());
    }

    #[test]
    fn init_deep_worker_writes_valid_spec() {
        let dir = tempfile::tempdir().unwrap();
        let out_path = dir.path().join("worker.yaml");

        run_init("deep_worker", Some(out_path.clone())).expect("init succeeds");

        let raw = fs::read_to_string(&out_path).unwrap();
        let spec: WorkerSpec = serde_yaml_bw::from_str(&raw).unwrap();
        assert_eq!(spec.kind, AgentKind::DeepWorker);
        assert!(validate::validate(&spec).is_ok());
    }

    #[test]
    fn init_write_failure_reports_worker_spec_write_error() {
        let dir = tempfile::tempdir().unwrap();
        // A path that is itself an existing directory cannot be `fs::write`n
        // to as a file.
        let err = run_init("single_turn", Some(dir.path().to_path_buf())).unwrap_err();
        assert!(matches!(err, CliError::WorkerSpecWrite { .. }));
    }

    #[test]
    fn init_rejects_unknown_kind() {
        let dir = tempfile::tempdir().unwrap();
        let out_path = dir.path().join("worker.yaml");

        let err = run_init("not_a_kind", Some(out_path)).unwrap_err();
        assert!(matches!(err, CliError::UnknownAgentKind(k) if k == "not_a_kind"));
    }

    fn write_spec_file(dir: &Path, name: &str, spec: &WorkerSpec) -> PathBuf {
        let path = dir.join(name);
        let yaml = serde_yaml_bw::to_string(spec).unwrap();
        fs::write(&path, yaml).unwrap();
        path
    }

    #[test]
    fn validate_accepts_valid_spec() {
        let dir = tempfile::tempdir().unwrap();
        let spec = starter_spec(AgentKind::SingleTurn);
        let path = write_spec_file(dir.path(), "spec.yaml", &spec);

        run_validate(&path).expect("valid spec passes");
    }

    #[test]
    fn validate_reports_invalid_spec_field() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = starter_spec(AgentKind::SingleTurn);
        spec.name = String::new();
        let path = write_spec_file(dir.path(), "spec.yaml", &spec);

        let err = run_validate(&path).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("name"),
            "expected error to name the `name` field, got: {message}"
        );
    }

    #[test]
    fn build_single_turn_produces_runner_loadable_pack() {
        let dir = tempfile::tempdir().unwrap();
        let spec = starter_spec(AgentKind::SingleTurn);
        let spec_path = write_spec_file(dir.path(), "spec.yaml", &spec);
        let out_path = dir.path().join("out.gtpack");

        run_build(&spec_path, Some(&out_path)).expect("build succeeds");

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("gtpack"))
            .collect();
        assert_eq!(entries.len(), 1, "expected exactly one .gtpack written");
        let pack_path = &entries[0];

        let manifest_bytes =
            read_zip_entry(pack_path, "manifest.cbor").expect("manifest.cbor present");
        greentic_types::decode_pack_manifest(&manifest_bytes).expect("manifest.cbor decodes");
        assert!(read_zip_entry(pack_path, "dw-agents.json").is_some());
    }

    #[test]
    fn build_with_out_filename_renames_pack_to_exact_name() {
        let dir = tempfile::tempdir().unwrap();
        let spec = starter_spec(AgentKind::SingleTurn);
        let spec_path = write_spec_file(dir.path(), "spec.yaml", &spec);
        let out_path = dir.path().join("custom-name.gtpack");

        let args = WorkerArgs {
            cmd: WorkerSub::Build {
                spec: spec_path,
                out: Some(out_path.clone()),
            },
        };
        run_worker(args).expect("build succeeds");

        assert!(
            out_path.is_file(),
            "expected pack to be written at exactly {}",
            out_path.display()
        );
        let manifest_bytes =
            read_zip_entry(&out_path, "manifest.cbor").expect("manifest.cbor present");
        greentic_types::decode_pack_manifest(&manifest_bytes).expect("manifest.cbor decodes");
    }

    #[test]
    fn build_rejects_invalid_spec_before_assembling() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = starter_spec(AgentKind::SingleTurn);
        spec.name = String::new();
        let spec_path = write_spec_file(dir.path(), "spec.yaml", &spec);

        let err = run_build(&spec_path, None).unwrap_err();
        assert!(matches!(err, CliError::WorkerSpecInvalid(_)));
    }

    #[test]
    fn build_with_knowledge_document_bakes_corpus() {
        let dir = tempfile::tempdir().unwrap();
        let doc_path = dir.path().join("policy.txt");
        fs::write(&doc_path, "our refund policy is 30 days").unwrap();

        let mut spec = starter_spec(AgentKind::SingleTurn);
        spec.knowledge = Some(greentic_dw_authoring::KnowledgeSpec {
            provider: "acme.knowledge".to_string(),
            embedding: greentic_dw_authoring::EmbeddingRef {
                provider: "acme.embedding".to_string(),
                model: "text-embedding-3-small".to_string(),
                credential_ref: None,
            },
            top_k: 5,
            documents: vec!["policy.txt".to_string()],
        });
        let spec_path = write_spec_file(dir.path(), "spec.yaml", &spec);
        let out_path = dir.path().join("out.gtpack");

        run_build(&spec_path, Some(&out_path)).expect("build succeeds");

        let entries: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("gtpack"))
            .collect();
        assert_eq!(entries.len(), 1);
        let pack_path = &entries[0];

        assert!(read_zip_entry(pack_path, "knowledge_corpus.json").is_some());
        assert!(read_zip_entry(pack_path, "assets/knowledge/policy.txt").is_some());
    }

    #[test]
    fn build_missing_knowledge_document_is_hard_error() {
        let dir = tempfile::tempdir().unwrap();
        let mut spec = starter_spec(AgentKind::SingleTurn);
        spec.knowledge = Some(greentic_dw_authoring::KnowledgeSpec {
            provider: "acme.knowledge".to_string(),
            embedding: greentic_dw_authoring::EmbeddingRef {
                provider: "acme.embedding".to_string(),
                model: "text-embedding-3-small".to_string(),
                credential_ref: None,
            },
            top_k: 5,
            documents: vec!["does-not-exist.txt".to_string()],
        });
        let spec_path = write_spec_file(dir.path(), "spec.yaml", &spec);

        let err = run_build(&spec_path, None).unwrap_err();
        assert!(matches!(err, CliError::KnowledgeDocumentRead { .. }));
    }

    /// Serializes tests that temporarily change the process's current
    /// working directory, since it is process-global state.
    static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// RAII guard restoring the original CWD on drop (including on panic),
    /// so a failed assertion never leaves the test process pointed at a
    /// temp dir that is about to be deleted.
    struct CwdGuard {
        original: PathBuf,
    }

    impl CwdGuard {
        fn change_to(dir: &Path) -> Self {
            let original = std::env::current_dir().expect("read current dir");
            std::env::set_current_dir(dir).expect("set current dir");
            CwdGuard { original }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn knowledge_document_next_to_spec_is_found_regardless_of_cwd() {
        let _lock = CWD_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let spec_dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(
            spec_dir.path().join("policy.txt"),
            "our refund policy is 30 days",
        )
        .unwrap();

        let mut spec = starter_spec(AgentKind::SingleTurn);
        spec.knowledge = Some(greentic_dw_authoring::KnowledgeSpec {
            provider: "acme.knowledge".to_string(),
            embedding: greentic_dw_authoring::EmbeddingRef {
                provider: "acme.embedding".to_string(),
                model: "text-embedding-3-small".to_string(),
                credential_ref: None,
            },
            top_k: 5,
            documents: vec!["policy.txt".to_string()],
        });
        let spec_path = write_spec_file(spec_dir.path(), "spec.yaml", &spec);
        let out_path = spec_dir.path().join("out.gtpack");

        // CWD does not contain policy.txt at all; resolution must not
        // depend on it, only on the spec's own directory.
        let _cwd_guard = CwdGuard::change_to(elsewhere.path());

        run_build(&spec_path, Some(&out_path)).expect("build succeeds without relying on cwd");

        assert!(read_zip_entry(&out_path, "knowledge_corpus.json").is_some());
        let baked = read_zip_entry(&out_path, "assets/knowledge/policy.txt")
            .expect("policy.txt baked from spec dir");
        assert_eq!(baked, b"our refund policy is 30 days");
    }

    #[test]
    fn new_with_answers_runs_build_path() {
        let dir = tempfile::tempdir().unwrap();
        let spec = starter_spec(AgentKind::SingleTurn);
        let answers_path = write_spec_file(dir.path(), "answers.yaml", &spec);
        let out_path = dir.path().join("out.gtpack");

        run_new(Some(&answers_path), Some(&out_path), false).expect("new succeeds");

        let has_pack = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.path().extension().and_then(|e| e.to_str()) == Some("gtpack"));
        assert!(has_pack, "expected a .gtpack to be written");
    }

    #[test]
    fn new_without_answers_or_schema_does_not_error() {
        run_new(None, None, false).expect("prints guidance instead of failing");
    }

    #[test]
    fn new_with_schema_flag_does_not_error() {
        run_new(None, None, true).expect("prints schema-not-available message");
    }
}
