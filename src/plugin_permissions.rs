//! Manifest-scoped application command authority. This is independent of WASI.
//!
//! A `Guarded` decision is not permission to submit a job to an unguarded
//! worker. Its rule must accompany the operation through file opening and every
//! network redirect. Saved-input operations additionally require resolution on
//! the owner thread before they can be dispatched.

use crate::commands::{Caller, Command, ImportDestination};
use crate::research::{ArtifactInput, ResearchCommand};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};
use thiserror::Error;
use url::Url;
use uuid::Uuid;

macro_rules! actions {
    ($( $variant:ident = $name:literal ),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum Action { $(#[serde(rename = $name)] $variant),+ }
        impl Action {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
        }
    };
}
actions! {
    LanListen = "lan.listen", LanTicket = "lan.ticket", LanPrepareJoin = "lan.prepare_join",
    LanCancelJoin = "lan.cancel_join", LanConfirm = "lan.confirm", LanUnpair = "lan.unpair",
    LanConnect = "lan.connect", LanStatus = "lan.status", LanPause = "lan.pause",
    LanResume = "lan.resume", SyncEnable = "sync.enable", MirrorConfigure = "mirror.configure",
    MirrorRescan = "mirror.rescan", MarkdownImport = "markdown.import",
    MarkdownExport = "markdown.export", NoteCreate = "note.create", NoteRead = "note.read",
    NoteEdit = "note.edit", NoteRename = "note.rename", NoteTrash = "note.trash",
    NoteRestore = "note.restore", NoteSearch = "note.search", CodexProjectRead = "codex.project.read",
    CodexProjectSet = "codex.project.set", AutomationSetWriteEnabled = "automation.set_write_enabled",
    ResearchImportSources = "research.import_sources", ResearchAcquireSources = "research.acquire_sources",
    ResearchRetryAcquisitions = "research.retry_acquisitions", ResearchCollectReferences = "research.collect_references",
    ResearchImportSource = "research.import_source", ResearchAcquireSource = "research.acquire_source",
    ResearchRetryAcquisition = "research.retry_acquisition", ResearchCancelAcquisition = "research.cancel_acquisition",
    ResearchCreateOutput = "research.create_output", ResearchSetArtifactRole = "research.set_artifact_role",
    ResearchFindPassages = "research.find_passages", ResearchEvidenceGaps = "research.evidence_gaps",
    ResearchCreate = "research.create", ResearchList = "research.list", ResearchRead = "research.read",
    ResearchReadIndex = "research.read_index", ResearchReadRound = "research.read_round",
    ResearchReadSource = "research.read_source", ResearchSavePaper = "research.save_paper",
    ResearchReadArtifact = "research.read_artifact", ResearchUpdate = "research.update",
    ResearchAddNote = "research.add_note", ResearchAddImported = "research.add_imported",
    ResearchRemoveSource = "research.remove_source", ResearchSelectSource = "research.select_source",
    ResearchAssessSource = "research.assess_source", ResearchLinkNote = "research.link_note",
    ResearchBuildPrompt = "research.build_prompt", ResearchSetBrief = "research.set_brief",
    ResearchAssociateThread = "research.associate_thread", ResearchRemoveThread = "research.remove_thread",
    ResearchAddRound = "research.add_round", ResearchCollectArtifact = "research.collect_artifact",
    ResearchRetryArtifact = "research.retry_artifact", ResearchBuildReport = "research.build_report",
    ResearchBuildContext = "research.build_context", ResearchDiscover = "research.discover"
}

/// Exhaustive mapping: adding a shared command requires assigning its action.
pub fn action(command: &Command) -> Option<Action> {
    use Action as A;
    use Command as C;
    use ResearchCommand as R;
    Some(match command {
        #[cfg(feature = "wasm-extensions")]
        C::Plugin(_) => return None,
        C::GetCodexProject => A::CodexProjectRead,
        C::SetAutomationWriteEnabled { .. } => A::AutomationSetWriteEnabled,
        C::SetCodexProject { .. } => A::CodexProjectSet,
        C::ListenLan { .. } => A::LanListen,
        C::GetLanTicket => A::LanTicket,
        C::PrepareJoin { .. } => A::LanPrepareJoin,
        C::CancelJoin => A::LanCancelJoin,
        C::ConfirmPeer { .. } => A::LanConfirm,
        C::UnpairPeer { .. } => A::LanUnpair,
        C::ConnectPeer { .. } => A::LanConnect,
        C::GetLanStatus => A::LanStatus,
        C::PauseLan => A::LanPause,
        C::ResumeLan => A::LanResume,
        C::EnableSync => A::SyncEnable,
        C::SetMirrorEnabled { .. } => A::MirrorConfigure,
        C::RescanMirror => A::MirrorRescan,
        C::ImportMarkdown { .. } => A::MarkdownImport,
        C::ExportMarkdown { .. } => A::MarkdownExport,
        C::CreateNote { .. } => A::NoteCreate,
        C::ReadNote { .. } => A::NoteRead,
        C::EditNote { .. } => A::NoteEdit,
        C::RenameNote { .. } => A::NoteRename,
        C::TrashNote { .. } => A::NoteTrash,
        C::RestoreNote { .. } => A::NoteRestore,
        C::SearchNotes { .. } => A::NoteSearch,
        C::Research(r) => match r {
            R::ImportSources { .. } => A::ResearchImportSources,
            R::AcquireSources { .. } => A::ResearchAcquireSources,
            R::RetryAcquisitions { .. } => A::ResearchRetryAcquisitions,
            R::CollectReferences { .. } => A::ResearchCollectReferences,
            R::ImportSource { .. } => A::ResearchImportSource,
            R::AcquireSource { .. } => A::ResearchAcquireSource,
            R::RetryAcquisition { .. } => A::ResearchRetryAcquisition,
            R::CancelAcquisition { .. } => A::ResearchCancelAcquisition,
            R::CreateOutput { .. } => A::ResearchCreateOutput,
            R::SetArtifactRole { .. } => A::ResearchSetArtifactRole,
            R::FindPassages { .. } => A::ResearchFindPassages,
            R::EvidenceGaps { .. } => A::ResearchEvidenceGaps,
            R::Create { .. } => A::ResearchCreate,
            R::List => A::ResearchList,
            R::Read { .. } => A::ResearchRead,
            R::ReadIndex { .. } => A::ResearchReadIndex,
            R::ReadRound { .. } => A::ResearchReadRound,
            R::ReadSource { .. } => A::ResearchReadSource,
            R::SavePaper { .. } => A::ResearchSavePaper,
            R::ReadArtifact { .. } => A::ResearchReadArtifact,
            R::Update { .. } => A::ResearchUpdate,
            R::AddNote { .. } => A::ResearchAddNote,
            R::AddImported { .. } => A::ResearchAddImported,
            R::RemoveSource { .. } => A::ResearchRemoveSource,
            R::SelectSource { .. } => A::ResearchSelectSource,
            R::AssessSource { .. } => A::ResearchAssessSource,
            R::LinkNote { .. } => A::ResearchLinkNote,
            R::BuildPrompt { .. } => A::ResearchBuildPrompt,
            R::SetBrief { .. } => A::ResearchSetBrief,
            R::AssociateThread { .. } => A::ResearchAssociateThread,
            R::RemoveThread { .. } => A::ResearchRemoveThread,
            R::AddRound { .. } => A::ResearchAddRound,
            R::CollectArtifact { .. } => A::ResearchCollectArtifact,
            R::RetryArtifact { .. } => A::ResearchRetryArtifact,
            R::BuildReport { .. } => A::ResearchBuildReport,
            R::BuildContext { .. } => A::ResearchBuildContext,
            R::Discover { .. } => A::ResearchDiscover,
        },
    })
}

/// Construct only from trusted adapter state, never from guest-supplied fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Origin {
    Local,
    AutomationReadOnly,
    AutomationReadWrite,
}
impl Origin {
    fn caller(self) -> Caller {
        match self {
            Self::Local => Caller::Local,
            Self::AutomationReadOnly => Caller::AutomationReadOnly,
            Self::AutomationReadWrite => Caller::AutomationReadWrite,
        }
    }
}

/// `all` is explicit. A finite list is exact for IDs/origins/targets and a
/// set of roots for paths. Empty lists are invalid, rather than implicit all.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "values",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum Scope<T> {
    All,
    Only(Vec<T>),
}
impl<T: PartialEq> Scope<T> {
    fn contains(&self, value: &T) -> bool {
        match self {
            Self::All => true,
            Self::Only(values) => values.contains(value),
        }
    }
    fn is_all(&self) -> bool {
        matches!(self, Self::All)
    }
    fn is_valid(&self) -> bool {
        !matches!(self, Self::Only(values) if values.is_empty())
    }
}

/// Relevant modes of an artifact input. These are application-command
/// constraints and grant no WASI authority.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    LocalFile,
    UrlArchive,
    UrlReference,
    Generated,
    Markdown,
}

/// Each rule couples an action to its resource constraints. A request must
/// match one complete rule; fields from separate rules are never combined.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub action: Action,
    #[serde(default)]
    pub notes: Option<Scope<Uuid>>,
    #[serde(default)]
    pub workspaces: Option<Scope<Uuid>>,
    #[serde(default)]
    pub note_paths: Option<Scope<String>>,
    #[serde(default)]
    pub filesystem_roots: Option<Scope<PathBuf>>,
    #[serde(default)]
    pub url_origins: Option<Scope<String>>,
    #[serde(default)]
    pub administrative_targets: Option<Scope<String>>,
    #[serde(default)]
    pub input_kinds: Option<Vec<InputKind>>,
    #[serde(default)]
    pub human: Option<bool>,
}

#[derive(Debug, Error)]
pub enum PolicyError {
    #[error("command is unavailable")]
    Unavailable,
    #[error("command is not authorized")]
    Denied,
    #[error("invalid resource scope: {0}")]
    InvalidScope(String),
}

impl Action {
    // Reject fields that could otherwise look like restrictions while being ignored.
    fn scope_mask(self) -> u8 {
        use Action::*;
        const NOTES: u8 = 1;
        const WORKSPACES: u8 = 2;
        const NOTE_PATHS: u8 = 4;
        const FILES: u8 = 8;
        const ORIGINS: u8 = 16;
        const TARGETS: u8 = 32;
        match self {
            NoteRead | NoteEdit | NoteTrash | NoteRestore | NoteSearch => NOTES,
            NoteCreate => NOTE_PATHS,
            NoteRename => NOTES | NOTE_PATHS,
            MarkdownImport => NOTES | NOTE_PATHS | FILES,
            MarkdownExport => NOTES | FILES,
            ResearchAddNote | ResearchLinkNote => WORKSPACES | NOTES,
            ResearchImportSource | ResearchImportSources => WORKSPACES | FILES,
            ResearchAcquireSource
            | ResearchSavePaper
            | ResearchCollectArtifact
            | ResearchAcquireSources
            | ResearchRetryAcquisition
            | ResearchRetryAcquisitions
            | ResearchRetryArtifact => WORKSPACES | FILES | ORIGINS,
            ResearchCancelAcquisition
            | ResearchCollectReferences
            | ResearchCreateOutput
            | ResearchSetArtifactRole
            | ResearchFindPassages
            | ResearchEvidenceGaps
            | ResearchCreate
            | ResearchList
            | ResearchRead
            | ResearchReadIndex
            | ResearchReadRound
            | ResearchReadSource
            | ResearchReadArtifact
            | ResearchUpdate
            | ResearchAddImported
            | ResearchRemoveSource
            | ResearchSelectSource
            | ResearchAssessSource
            | ResearchBuildPrompt
            | ResearchSetBrief
            | ResearchAssociateThread
            | ResearchRemoveThread
            | ResearchAddRound
            | ResearchBuildReport
            | ResearchBuildContext
            | ResearchDiscover => WORKSPACES,
            LanListen | LanPrepareJoin | LanConfirm | LanUnpair | LanConnect | CodexProjectSet => {
                TARGETS
            }
            LanTicket
            | LanCancelJoin
            | LanStatus
            | LanPause
            | LanResume
            | SyncEnable
            | MirrorConfigure
            | MirrorRescan
            | CodexProjectRead
            | AutomationSetWriteEnabled => 0,
        }
    }
}

impl Rule {
    pub fn validate(&self) -> Result<(), PolicyError> {
        let supplied = u8::from(self.notes.is_some())
            | (u8::from(self.workspaces.is_some()) << 1)
            | (u8::from(self.note_paths.is_some()) << 2)
            | (u8::from(self.filesystem_roots.is_some()) << 3)
            | (u8::from(self.url_origins.is_some()) << 4)
            | (u8::from(self.administrative_targets.is_some()) << 5);
        if supplied & !self.action.scope_mask() != 0 {
            return Err(PolicyError::InvalidScope(
                "scope does not apply to this action".into(),
            ));
        }
        // A mandatory resource dimension may not be omitted and accidentally
        // look like an effective grant in discovery.
        let mut required = self.action.scope_mask() & (1 | 2 | 32);
        if self.action == Action::MarkdownImport {
            required &= !1;
        }
        if matches!(self.action, Action::NoteCreate | Action::NoteRename) {
            required |= 4;
        }
        if matches!(
            self.action,
            Action::MarkdownImport
                | Action::MarkdownExport
                | Action::ResearchImportSource
                | Action::ResearchImportSources
        ) {
            required |= 8;
        }
        if supplied & required != required {
            return Err(PolicyError::InvalidScope(
                "a required resource scope is missing".into(),
            ));
        }
        if self.action == Action::MarkdownImport
            && self.notes.is_none()
            && self.note_paths.is_none()
        {
            return Err(PolicyError::InvalidScope(
                "import requires a destination scope".into(),
            ));
        }
        if matches!(
            self.action,
            Action::ResearchAcquireSource
                | Action::ResearchSavePaper
                | Action::ResearchAcquireSources
                | Action::ResearchRetryAcquisition
                | Action::ResearchRetryAcquisitions
        ) && self.filesystem_roots.is_none()
            && self.url_origins.is_none()
        {
            return Err(PolicyError::InvalidScope(
                "saved or external input requires a resource scope".into(),
            ));
        }
        if self.human.is_some() && self.action != Action::ResearchAssessSource {
            return Err(PolicyError::InvalidScope(
                "human restriction does not apply to this action".into(),
            ));
        }
        if let Some(kinds) = &self.input_kinds {
            if kinds.is_empty()
                || !matches!(
                    self.action,
                    Action::ResearchAcquireSource
                        | Action::ResearchSavePaper
                        | Action::ResearchCollectArtifact
                        | Action::ResearchAcquireSources
                        | Action::ResearchRetryAcquisition
                        | Action::ResearchRetryAcquisitions
                        | Action::ResearchRetryArtifact
                        | Action::ResearchImportSource
                        | Action::ResearchImportSources
                )
            {
                return Err(PolicyError::InvalidScope(
                    "invalid input kind restriction".into(),
                ));
            }
        }
        fn nonempty<T: PartialEq>(s: &Option<Scope<T>>) -> bool {
            s.as_ref().is_none_or(Scope::is_valid)
        }
        if !nonempty(&self.notes)
            || !nonempty(&self.workspaces)
            || !nonempty(&self.note_paths)
            || !nonempty(&self.filesystem_roots)
            || !nonempty(&self.url_origins)
            || !nonempty(&self.administrative_targets)
        {
            return Err(PolicyError::InvalidScope("empty scope".into()));
        }
        if let Some(Scope::Only(paths)) = &self.note_paths {
            if paths.iter().any(|p| !safe_relative(Path::new(p))) {
                return Err(PolicyError::InvalidScope(
                    "note path roots must be relative and normalized".into(),
                ));
            }
        }
        if let Some(Scope::Only(paths)) = &self.filesystem_roots {
            if paths.iter().any(|p| !safe_absolute(p)) {
                return Err(PolicyError::InvalidScope(
                    "filesystem roots must be absolute and normalized".into(),
                ));
            }
        }
        if let Some(Scope::Only(origins)) = &self.url_origins {
            if origins
                .iter()
                .any(|v| normalized_origin(v).as_deref() != Some(v.as_str()))
            {
                return Err(PolicyError::InvalidScope(
                    "URL origins must be canonical HTTP(S) origins".into(),
                ));
            }
        }
        if let Some(Scope::Only(targets)) = &self.administrative_targets {
            if targets
                .iter()
                .any(|v| v.is_empty() || v.chars().any(char::is_control))
            {
                return Err(PolicyError::InvalidScope(
                    "invalid administrative target".into(),
                ));
            }
        }
        Ok(())
    }
}

fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty() && path.components().all(|c| matches!(c, Component::Normal(_)))
}
fn safe_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path
            .components()
            .all(|c| matches!(c, Component::RootDir | Component::Normal(_)))
}
fn normalized_origin(value: &str) -> Option<String> {
    let url = Url::parse(value).ok()?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    Some(url.origin().ascii_serialization())
}

#[derive(Clone, Copy)]
enum IdTarget {
    One(Uuid),
    All,
}
#[derive(Default)]
struct Requirements {
    note: Option<IdTarget>,
    workspace: Option<IdTarget>,
    note_paths: Vec<String>,
    file_paths: Vec<PathBuf>,
    origins: Vec<String>,
    targets: Vec<String>,
    input_kind: Option<InputKind>,
    human: Option<bool>,
    unresolved: bool,
}
impl Requirements {
    fn input(&mut self, input: &ArtifactInput) -> Result<(), PolicyError> {
        self.input_kind = Some(match input {
            ArtifactInput::LocalFile { .. } => InputKind::LocalFile,
            ArtifactInput::Url { archive: true, .. } => InputKind::UrlArchive,
            ArtifactInput::Url { archive: false, .. } => InputKind::UrlReference,
            ArtifactInput::Generated { .. } => InputKind::Generated,
            ArtifactInput::Markdown { .. } => InputKind::Markdown,
        });
        match input {
            ArtifactInput::LocalFile { path } => self.file_paths.push(PathBuf::from(path)),
            ArtifactInput::Url { url, archive: true } => self
                .origins
                .push(normalized_origin(url).ok_or(PolicyError::Denied)?),
            ArtifactInput::Url { .. }
            | ArtifactInput::Generated { .. }
            | ArtifactInput::Markdown { .. } => {}
        }
        Ok(())
    }
}

fn requirements(command: &Command) -> Result<Requirements, PolicyError> {
    use Command as C;
    use ResearchCommand as R;
    let mut req = Requirements::default();
    match command {
        #[cfg(feature = "wasm-extensions")]
        C::Plugin(_) => return Err(PolicyError::Denied),
        C::GetCodexProject
        | C::SetAutomationWriteEnabled { .. }
        | C::GetLanTicket
        | C::CancelJoin
        | C::GetLanStatus
        | C::PauseLan
        | C::ResumeLan
        | C::EnableSync
        | C::SetMirrorEnabled { .. }
        | C::RescanMirror => {}
        C::SetCodexProject { project, .. } => req.targets.push(format!(
            "project:{}",
            project
                .project_id
                .as_deref()
                .unwrap_or(&project.creation_key)
        )),
        C::ListenLan {
            bind,
            advertised_ip,
        } => {
            req.targets.push(format!("bind:{bind}"));
            if let Some(ip) = advertised_ip {
                req.targets.push(format!("advertised:{ip}"));
            }
        }
        C::PrepareJoin { ticket } | C::ConfirmPeer { ticket } => {
            req.targets.push(format!("ticket:{ticket}"))
        }
        C::UnpairPeer { fingerprint } | C::ConnectPeer { fingerprint } => {
            req.targets.push(format!("peer:{fingerprint}"))
        }
        C::ImportMarkdown {
            source_path,
            destination,
        } => {
            req.file_paths.push(PathBuf::from(source_path));
            match destination {
                ImportDestination::Create { path } => req.note_paths.push(path.clone()),
                ImportDestination::Replace { id, .. } => req.note = Some(IdTarget::One(id.0)),
            }
        }
        C::ExportMarkdown { id, path } => {
            req.note = Some(IdTarget::One(id.0));
            req.file_paths.push(PathBuf::from(path));
        }
        C::CreateNote { path, .. } => req.note_paths.push(path.clone()),
        C::ReadNote { id }
        | C::EditNote { id, .. }
        | C::TrashNote { id, .. }
        | C::RestoreNote { id, .. } => req.note = Some(IdTarget::One(id.0)),
        C::RenameNote { id, path, .. } => {
            req.note = Some(IdTarget::One(id.0));
            req.note_paths.push(path.clone());
        }
        C::SearchNotes { .. } => req.note = Some(IdTarget::All),
        C::Research(r) => {
            // Every workspace-bearing variant is explicitly listed so additions fail compilation.
            let workspace = match r {
                R::Create { .. } | R::List => IdTarget::All,
                R::ImportSources { id, .. }
                | R::AcquireSources { id, .. }
                | R::RetryAcquisitions { id, .. }
                | R::CollectReferences { id, .. }
                | R::ImportSource { id, .. }
                | R::AcquireSource { id, .. }
                | R::RetryAcquisition { id, .. }
                | R::CancelAcquisition { id, .. }
                | R::CreateOutput { id, .. }
                | R::SetArtifactRole { id, .. }
                | R::FindPassages { id, .. }
                | R::EvidenceGaps { id, .. }
                | R::Read { id, .. }
                | R::ReadIndex { id, .. }
                | R::ReadRound { id, .. }
                | R::ReadSource { id, .. }
                | R::SavePaper { id, .. }
                | R::ReadArtifact { id, .. }
                | R::Update { id, .. }
                | R::AddNote { id, .. }
                | R::AddImported { id, .. }
                | R::RemoveSource { id, .. }
                | R::SelectSource { id, .. }
                | R::AssessSource { id, .. }
                | R::LinkNote { id, .. }
                | R::BuildPrompt { id, .. }
                | R::SetBrief { id, .. }
                | R::AssociateThread { id, .. }
                | R::RemoveThread { id, .. }
                | R::AddRound { id, .. }
                | R::CollectArtifact { id, .. }
                | R::RetryArtifact { id, .. }
                | R::BuildReport { id, .. }
                | R::BuildContext { id, .. }
                | R::Discover { id, .. } => IdTarget::One(*id),
            };
            req.workspace = Some(workspace);
            match r {
                R::AddNote { note_id, .. } | R::LinkNote { note_id, .. } => {
                    req.note = Some(IdTarget::One(note_id.0))
                }
                R::ImportSources { paths, .. } => {
                    req.input_kind = Some(InputKind::LocalFile);
                    req.file_paths.extend(paths.iter().map(PathBuf::from));
                }
                R::ImportSource { path, .. } => {
                    req.input_kind = Some(InputKind::LocalFile);
                    req.file_paths.push(PathBuf::from(path));
                }
                R::AssessSource { human, .. } => req.human = Some(*human),
                R::AcquireSource { input, .. }
                | R::SavePaper { input, .. }
                | R::CollectArtifact { input, .. } => req.input(input)?,
                // The owner must resolve existing source/job/receipt inputs and retain their original authority.
                R::AcquireSources { .. }
                | R::RetryAcquisitions { .. }
                | R::RetryAcquisition { .. }
                | R::RetryArtifact { .. } => req.unresolved = true,
                _ => {}
            }
        }
    }
    Ok(req)
}
fn matches_id(scope: &Option<Scope<Uuid>>, target: Option<IdTarget>) -> bool {
    match target {
        None => true,
        Some(IdTarget::All) => scope.as_ref().is_some_and(Scope::is_all),
        Some(IdTarget::One(id)) => scope.as_ref().is_some_and(|s| s.contains(&id)),
    }
}
fn matches_paths<T: AsRef<Path>>(
    scope: &Option<Scope<T>>,
    paths: &[PathBuf],
    absolute: bool,
) -> bool {
    paths.iter().all(|path| {
        (if absolute {
            safe_absolute(path)
        } else {
            safe_relative(path)
        }) && scope.as_ref().is_some_and(|s| match s {
            Scope::All => true,
            Scope::Only(roots) => roots.iter().any(|root| path.starts_with(root.as_ref())),
        })
    })
}
fn matches_rule(rule: &Rule, req: &Requirements) -> bool {
    let notes = req.note_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
    matches_id(&rule.notes, req.note)
        && matches_id(&rule.workspaces, req.workspace)
        && matches_paths(&rule.note_paths, &notes, false)
        && matches_paths(&rule.filesystem_roots, &req.file_paths, true)
        && req
            .origins
            .iter()
            .all(|v| rule.url_origins.as_ref().is_some_and(|s| s.contains(v)))
        && req.targets.iter().all(|v| {
            rule.administrative_targets
                .as_ref()
                .is_some_and(|s| s.contains(v))
        })
        && req.input_kind.is_none_or(|kind| {
            rule.input_kinds
                .as_ref()
                .is_none_or(|kinds| kinds.contains(&kind))
        })
        && req
            .human
            .is_none_or(|human| rule.human.is_none_or(|allowed| allowed == human))
}

/// Static caller-class and host-availability ceiling for discovery. Actual
/// dispatch also checks parameters, resource scopes, and saved-job authority.
pub fn action_origin_allowed(action: Action, origin: Origin, vault_write_enabled: bool) -> bool {
    use Action::*;
    let available = match action {
        LanListen | LanTicket | LanPrepareJoin | LanCancelJoin | LanConfirm | LanUnpair
        | LanConnect | LanStatus | LanPause | LanResume => cfg!(feature = "lan"),
        SyncEnable => cfg!(feature = "sync"),
        MirrorConfigure | MirrorRescan => cfg!(target_os = "linux"),
        _ => true,
    };
    if !available {
        return false;
    }
    if origin == Origin::Local {
        return true;
    }
    match action {
        LanListen
        | LanTicket
        | LanPrepareJoin
        | LanCancelJoin
        | LanConfirm
        | LanUnpair
        | LanConnect
        | LanStatus
        | LanPause
        | LanResume
        | SyncEnable
        | MirrorConfigure
        | MirrorRescan
        | MarkdownImport
        | MarkdownExport
        | CodexProjectSet
        | AutomationSetWriteEnabled
        | ResearchImportSources
        | ResearchImportSource
        | ResearchRetryArtifact => false,
        NoteRead | NoteSearch | CodexProjectRead | ResearchFindPassages | ResearchEvidenceGaps
        | ResearchList | ResearchRead | ResearchReadIndex | ResearchReadRound
        | ResearchReadSource | ResearchReadArtifact | ResearchBuildPrompt | ResearchBuildReport
        | ResearchBuildContext | ResearchDiscover => true,
        NoteCreate
        | NoteEdit
        | NoteRename
        | NoteTrash
        | NoteRestore
        | ResearchAcquireSources
        | ResearchRetryAcquisitions
        | ResearchCollectReferences
        | ResearchAcquireSource
        | ResearchRetryAcquisition
        | ResearchCancelAcquisition
        | ResearchCreateOutput
        | ResearchSetArtifactRole
        | ResearchCreate
        | ResearchSavePaper
        | ResearchUpdate
        | ResearchAddNote
        | ResearchAddImported
        | ResearchRemoveSource
        | ResearchSelectSource
        | ResearchAssessSource
        | ResearchLinkNote
        | ResearchSetBrief
        | ResearchAssociateThread
        | ResearchRemoveThread
        | ResearchAddRound
        | ResearchCollectArtifact => origin == Origin::AutomationReadWrite && vault_write_enabled,
    }
}

/// Resource-coupled rules after intersection with the currently authenticated
/// origin class and vault-write switch. Never use discovery as authorization.
pub fn effective_rules(rules: &[Rule], origin: Origin, vault_write_enabled: bool) -> Vec<Rule> {
    rules
        .iter()
        .filter_map(|rule| {
            if rule.validate().is_err()
                || !action_origin_allowed(rule.action, origin, vault_write_enabled)
            {
                return None;
            }
            let mut effective = rule.clone();
            if origin != Origin::Local {
                if effective.action == Action::ResearchAssessSource {
                    if effective.human == Some(true) {
                        return None;
                    }
                    effective.human = Some(false);
                }
                let kinds: Option<&[InputKind]> = match effective.action {
                    Action::ResearchAcquireSource
                    | Action::ResearchSavePaper
                    | Action::ResearchAcquireSources
                    | Action::ResearchRetryAcquisition
                    | Action::ResearchRetryAcquisitions => Some(&[InputKind::UrlArchive]),
                    Action::ResearchCollectArtifact => Some(&[
                        InputKind::UrlReference,
                        InputKind::Generated,
                        InputKind::Markdown,
                    ]),
                    _ => None,
                };
                if let Some(kinds) = kinds {
                    effective.filesystem_roots = None;
                    let narrowed: Vec<_> = kinds
                        .iter()
                        .copied()
                        .filter(|kind| {
                            effective
                                .input_kinds
                                .as_ref()
                                .is_none_or(|requested| requested.contains(kind))
                        })
                        .collect();
                    if narrowed.is_empty() {
                        return None;
                    }
                    if narrowed == [InputKind::UrlArchive] && effective.url_origins.is_none() {
                        return None;
                    }
                    effective.input_kinds = Some(narrowed);
                    // Archive capture is local-only for CollectArtifact.
                    if effective.action == Action::ResearchCollectArtifact {
                        effective.url_origins = None;
                    }
                }
            }
            Some(effective)
        })
        .collect()
}

/// The complete matching rule is carried to the owner/worker; never merge it
/// with another rule. Unresolved requests must resolve every saved input under
/// the same caller and rule before any work or new jobs are accepted.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Guard {
    pub rule: Rule,
    pub origin: Origin,
}

#[derive(Clone, Debug)]
pub enum Decision {
    Direct,
    Guarded(Guard),
    ResolveSavedInputs(Guard),
}

pub fn authorize(
    rules: &[Rule],
    command: &Command,
    origin: Origin,
    vault_write_enabled: bool,
) -> Result<Decision, PolicyError> {
    let action = action(command).ok_or(PolicyError::Denied)?;
    let descriptor = command.descriptor();
    if !descriptor.available {
        return Err(PolicyError::Unavailable);
    }
    if !action_origin_allowed(action, origin, vault_write_enabled)
        || (origin != Origin::Local
            && (!descriptor.allowed(origin.caller())
                || (command.is_mutation() && !vault_write_enabled)))
    {
        return Err(PolicyError::Denied);
    }
    let required = requirements(command)?;
    let rule = rules
        .iter()
        .find(|rule| {
            rule.action == action && rule.validate().is_ok() && matches_rule(rule, &required)
        })
        .ok_or(PolicyError::Denied)?;
    if required.unresolved {
        Ok(Decision::ResolveSavedInputs(Guard {
            rule: rule.clone(),
            origin,
        }))
    } else if !required.file_paths.is_empty() || !required.origins.is_empty() {
        Ok(Decision::Guarded(Guard {
            rule: rule.clone(),
            origin,
        }))
    } else {
        Ok(Decision::Direct)
    }
}

/// Resolve a saved acquisition or receipt's input on the owner thread. The
/// original job authority must also be intersected before any retry; this
/// function checks only the current rule's resource scope.
pub fn authorize_saved_input(
    guard: &Guard,
    input: &ArtifactInput,
) -> Result<Decision, PolicyError> {
    if guard.origin != Origin::Local && matches!(input, ArtifactInput::LocalFile { .. }) {
        return Err(PolicyError::Denied);
    }
    let mut required = Requirements::default();
    required.input(input)?;
    if !matches_rule(&guard.rule, &required) {
        return Err(PolicyError::Denied);
    }
    if !required.file_paths.is_empty() || !required.origins.is_empty() {
        Ok(Decision::Guarded(guard.clone()))
    } else {
        Ok(Decision::Direct)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::{NoteId, Revision};

    fn rule(action: Action) -> Rule {
        Rule {
            action,
            notes: None,
            workspaces: None,
            note_paths: None,
            filesystem_roots: None,
            url_origins: None,
            administrative_targets: None,
            input_kinds: None,
            human: None,
        }
    }
    #[test]
    fn coupled_rules_never_expand_scoped_edits() {
        let first = NoteId(Uuid::new_v4());
        let second = NoteId(Uuid::new_v4());
        let mut read = rule(Action::NoteRead);
        read.notes = Some(Scope::All);
        let mut edit = rule(Action::NoteEdit);
        edit.notes = Some(Scope::Only(vec![first.0]));
        let rules = vec![read, edit];
        let change = |id| Command::EditNote {
            id,
            expected: Revision(Uuid::new_v4()),
            edits: vec![],
        };
        assert!(matches!(
            authorize(
                &rules,
                &Command::ReadNote { id: second },
                Origin::AutomationReadOnly,
                false
            ),
            Ok(Decision::Direct)
        ));
        assert!(matches!(
            authorize(&rules, &change(first), Origin::AutomationReadWrite, true),
            Ok(Decision::Direct)
        ));
        assert!(matches!(
            authorize(&rules, &change(second), Origin::AutomationReadWrite, true),
            Err(PolicyError::Denied)
        ));
        assert!(matches!(
            authorize(&rules, &change(first), Origin::AutomationReadOnly, true),
            Err(PolicyError::Denied)
        ));
        assert!(matches!(
            authorize(&rules, &change(first), Origin::AutomationReadWrite, false),
            Err(PolicyError::Denied)
        ));
    }
    #[test]
    fn local_admin_grant_cannot_be_used_to_self_enable_agent_writes() {
        let rules = vec![rule(Action::AutomationSetWriteEnabled)];
        let command = Command::SetAutomationWriteEnabled { enabled: true };
        assert!(matches!(
            authorize(&rules, &command, Origin::Local, false),
            Ok(Decision::Direct)
        ));
        assert!(matches!(
            authorize(&rules, &command, Origin::AutomationReadWrite, true),
            Err(PolicyError::Denied)
        ));
    }
    #[cfg(feature = "wasm-extensions")]
    #[test]
    fn guest_cannot_invoke_nested_plugin_control() {
        let command = Command::Plugin(crate::plugin_jobs::PluginCommand::List);
        assert!(action(&command).is_none());
        assert!(matches!(
            authorize(&[], &command, Origin::Local, true),
            Err(PolicyError::Denied)
        ));
    }
    #[test]
    fn retries_require_resolving_and_scoping_saved_input() {
        let mut retry = rule(Action::ResearchRetryAcquisition);
        let workspace = Uuid::new_v4();
        retry.workspaces = Some(Scope::Only(vec![workspace]));
        retry.filesystem_roots = Some(Scope::Only(vec![PathBuf::from("/approved")]));
        let command = Command::Research(ResearchCommand::RetryAcquisition {
            id: workspace,
            expected: Revision(Uuid::new_v4()),
            job_id: Uuid::new_v4(),
        });
        let Decision::ResolveSavedInputs(grant) = authorize(
            &[retry.clone()],
            &command,
            Origin::AutomationReadWrite,
            true,
        )
        .unwrap() else {
            panic!("saved job must be resolved");
        };
        assert!(matches!(
            authorize_saved_input(
                &grant,
                &ArtifactInput::LocalFile {
                    path: "/approved/paper.pdf".into()
                }
            ),
            Err(PolicyError::Denied)
        ));
        let Decision::ResolveSavedInputs(grant) =
            authorize(&[retry], &command, Origin::Local, false).unwrap()
        else {
            panic!("saved job must be resolved");
        };
        assert!(matches!(
            authorize_saved_input(
                &grant,
                &ArtifactInput::LocalFile {
                    path: "/approved/paper.pdf".into()
                }
            ),
            Ok(Decision::Guarded(_))
        ));
        assert!(matches!(
            authorize_saved_input(
                &grant,
                &ArtifactInput::LocalFile {
                    path: "/outside/paper.pdf".into()
                }
            ),
            Err(PolicyError::Denied)
        ));
    }
    #[test]
    fn path_and_url_matches_are_guarded_for_actual_workers() {
        let id = NoteId(Uuid::new_v4());
        let mut export = rule(Action::MarkdownExport);
        export.notes = Some(Scope::Only(vec![id.0]));
        export.filesystem_roots = Some(Scope::Only(vec![PathBuf::from("/approved")]));
        let cmd = Command::ExportMarkdown {
            id,
            path: "/approved/output.md".into(),
        };
        assert!(matches!(
            authorize(&[export], &cmd, Origin::Local, false),
            Ok(Decision::Guarded(_))
        ));
        let mut save = rule(Action::ResearchSavePaper);
        save.workspaces = Some(Scope::All);
        save.url_origins = Some(Scope::Only(vec!["https://example.org".into()]));
        let saved = ArtifactInput::Url {
            url: "https://example.org/paper.pdf".into(),
            archive: true,
        };
        assert!(matches!(
            authorize_saved_input(
                &Guard {
                    rule: save.clone(),
                    origin: Origin::Local
                },
                &saved
            ),
            Ok(Decision::Guarded(_))
        ));
        let outside = ArtifactInput::Url {
            url: "https://other.org/paper.pdf".into(),
            archive: true,
        };
        assert!(matches!(
            authorize_saved_input(
                &Guard {
                    rule: save,
                    origin: Origin::Local
                },
                &outside
            ),
            Err(PolicyError::Denied)
        ));
    }
    #[test]
    fn effective_metadata_preserves_rule_scopes_and_caller_parameter_limits() {
        let mut read = rule(Action::NoteRead);
        read.notes = Some(Scope::All);
        let mut edit = rule(Action::NoteEdit);
        let note = Uuid::new_v4();
        edit.notes = Some(Scope::Only(vec![note]));
        let mut assess = rule(Action::ResearchAssessSource);
        assess.workspaces = Some(Scope::All);
        let mut acquire = rule(Action::ResearchAcquireSource);
        acquire.workspaces = Some(Scope::All);
        acquire.filesystem_roots = Some(Scope::Only(vec![PathBuf::from("/local")]));
        acquire.url_origins = Some(Scope::Only(vec!["https://example.org".into()]));
        let rules = vec![read, edit, assess, acquire];
        let readonly = effective_rules(&rules, Origin::AutomationReadOnly, true);
        assert_eq!(readonly.len(), 1);
        assert_eq!(readonly[0].action, Action::NoteRead);
        let enabled = effective_rules(&rules, Origin::AutomationReadWrite, true);
        assert_eq!(enabled.len(), 4);
        assert_eq!(enabled[1].notes, Some(Scope::Only(vec![note])));
        assert_eq!(enabled[2].human, Some(false));
        assert_eq!(enabled[3].filesystem_roots, None);
        assert_eq!(enabled[3].input_kinds, Some(vec![InputKind::UrlArchive]));
        assert_eq!(
            effective_rules(&rules, Origin::AutomationReadWrite, false).len(),
            1
        );
    }
}
