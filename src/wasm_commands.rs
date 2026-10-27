//! Explicit typed conversions between extension components and shared commands.
//! Domain checks, permissions, and effects remain in the application dispatcher.

pub mod bindings {
    wasmtime::component::bindgen!({ path: "wit/v2", world: "compiled-extension", imports: { default: trappable } });
}
pub use bindings::tanglewood::extensions::host;

fn parse_uuid(value: String) -> Result<uuid::Uuid, host::CommandError> {
    uuid::Uuid::parse_str(&value)
        .map_err(|_| host::CommandError::InvalidArgument("invalid UUID or revision".into()))
}
fn parse_usize(value: u64) -> Result<usize, host::CommandError> {
    usize::try_from(value)
        .map_err(|_| host::CommandError::InvalidArgument("integer is out of range".into()))
}
fn decode_source_urls(
    values: Vec<host::SourceUrl>,
) -> Result<std::collections::BTreeMap<uuid::Uuid, String>, host::CommandError> {
    let mut result = std::collections::BTreeMap::new();
    for value in values {
        if result.insert(parse_uuid(value.id)?, value.url).is_some() {
            return Err(host::CommandError::InvalidArgument(
                "duplicate source URL ID".into(),
            ));
        }
    }
    Ok(result)
}

impl TryFrom<host::CodexProjectBinding> for crate::notes::CodexProjectBinding {
    type Error = host::CommandError;
    fn try_from(value: host::CodexProjectBinding) -> Result<Self, Self::Error> {
        Ok(Self {
            project_id: value
                .project_id
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            name: value.name,
            creation_key: value.creation_key,
        })
    }
}
impl From<crate::notes::CodexProjectBinding> for host::CodexProjectBinding {
    fn from(value: crate::notes::CodexProjectBinding) -> Self {
        Self {
            project_id: value.project_id.map(|item| item),
            name: value.name,
            creation_key: value.creation_key,
        }
    }
}

impl TryFrom<host::TextEdit> for crate::notes::TextEdit {
    type Error = host::CommandError;
    fn try_from(value: host::TextEdit) -> Result<Self, Self::Error> {
        Ok(Self {
            start: parse_usize(value.start)?,
            end: parse_usize(value.end)?,
            replacement: value.replacement,
        })
    }
}
impl From<crate::notes::TextEdit> for host::TextEdit {
    fn from(value: crate::notes::TextEdit) -> Self {
        Self {
            start: value.start as u64,
            end: value.end as u64,
            replacement: value.replacement,
        }
    }
}

impl TryFrom<host::NoteRecord> for crate::notes::NoteRecord {
    type Error = host::CommandError;
    fn try_from(value: host::NoteRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: crate::notes::NoteId(parse_uuid(value.id)?),
            path: value.path,
            text: value.text,
            revision: crate::notes::Revision(parse_uuid(value.revision)?),
            trashed: value.trashed,
        })
    }
}
impl From<crate::notes::NoteRecord> for host::NoteRecord {
    fn from(value: crate::notes::NoteRecord) -> Self {
        Self {
            id: value.id.0.to_string(),
            path: value.path,
            text: value.text,
            revision: value.revision.0.to_string(),
            trashed: value.trashed,
        }
    }
}

impl TryFrom<host::SaveState> for crate::notes::SaveState {
    type Error = host::CommandError;
    fn try_from(value: host::SaveState) -> Result<Self, Self::Error> {
        Ok(Self {
            accepted: crate::notes::Revision(parse_uuid(value.accepted)?),
            persisted: value
                .persisted
                .map(|item| -> Result<_, host::CommandError> {
                    Ok(crate::notes::Revision(parse_uuid(item)?))
                })
                .transpose()?,
            error: value
                .error
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
        })
    }
}
impl From<crate::notes::SaveState> for host::SaveState {
    fn from(value: crate::notes::SaveState) -> Self {
        Self {
            accepted: value.accepted.0.to_string(),
            persisted: value.persisted.map(|item| item.0.to_string()),
            error: value.error.map(|item| item),
        }
    }
}

impl TryFrom<host::NoteView> for crate::notes::NoteView {
    type Error = host::CommandError;
    fn try_from(value: host::NoteView) -> Result<Self, Self::Error> {
        Ok(Self {
            note: value.note.try_into()?,
            save: value.save.try_into()?,
        })
    }
}
impl From<crate::notes::NoteView> for host::NoteView {
    fn from(value: crate::notes::NoteView) -> Self {
        Self {
            note: value.note.into(),
            save: value.save.into(),
        }
    }
}

impl TryFrom<host::LanPeerStatus> for crate::commands::LanPeerStatus {
    type Error = host::CommandError;
    fn try_from(value: host::LanPeerStatus) -> Result<Self, Self::Error> {
        Ok(Self {
            fingerprint: value.fingerprint,
            connected: value.connected,
            session: value
                .session
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            last_receipt: value
                .last_receipt
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .transpose()?,
        })
    }
}
impl From<crate::commands::LanPeerStatus> for host::LanPeerStatus {
    fn from(value: crate::commands::LanPeerStatus) -> Self {
        Self {
            fingerprint: value.fingerprint,
            connected: value.connected,
            session: value.session.map(|item| item),
            last_receipt: value.last_receipt.map(|item| item.to_string()),
        }
    }
}

impl TryFrom<host::LanStatus> for crate::commands::LanStatus {
    type Error = host::CommandError;
    fn try_from(value: host::LanStatus) -> Result<Self, Self::Error> {
        Ok(Self {
            listening: value.listening,
            paused: value.paused,
            prepared: value.prepared,
            joining: value.joining,
            vault_id: value
                .vault_id
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .transpose()?,
            ticket: value
                .ticket
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            peers: value
                .peers
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            pending_messages: parse_usize(value.pending_messages)?,
            error: value
                .error
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            accepted_generation: value.accepted_generation,
            persisted_generation: value.persisted_generation,
        })
    }
}
impl From<crate::commands::LanStatus> for host::LanStatus {
    fn from(value: crate::commands::LanStatus) -> Self {
        Self {
            listening: value.listening,
            paused: value.paused,
            prepared: value.prepared,
            joining: value.joining,
            vault_id: value.vault_id.map(|item| item.to_string()),
            ticket: value.ticket.map(|item| item),
            peers: value.peers.into_iter().map(|item| item.into()).collect(),
            pending_messages: value.pending_messages as u64,
            error: value.error.map(|item| item),
            accepted_generation: value.accepted_generation,
            persisted_generation: value.persisted_generation,
        }
    }
}

impl TryFrom<host::SourceMetadata> for crate::research::SourceMetadata {
    type Error = host::CommandError;
    fn try_from(value: host::SourceMetadata) -> Result<Self, Self::Error> {
        Ok(Self {
            author: value
                .author
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            date: value
                .date
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            url: value
                .url
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            origin_label: value
                .origin_label
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
        })
    }
}
impl From<crate::research::SourceMetadata> for host::SourceMetadata {
    fn from(value: crate::research::SourceMetadata) -> Self {
        Self {
            author: value.author.map(|item| item),
            date: value.date.map(|item| item),
            url: value.url.map(|item| item),
            origin_label: value.origin_label.map(|item| item),
        }
    }
}

impl TryFrom<host::SourcePage> for crate::research::SourcePage {
    type Error = host::CommandError;
    fn try_from(value: host::SourcePage) -> Result<Self, Self::Error> {
        Ok(Self {
            number: value
                .number
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            text: value.text,
        })
    }
}
impl From<crate::research::SourcePage> for host::SourcePage {
    fn from(value: crate::research::SourcePage) -> Self {
        Self {
            number: value.number.map(|item| item),
            text: value.text,
        }
    }
}

impl TryFrom<host::SourceAssessment> for crate::research::SourceAssessment {
    type Error = host::CommandError;
    fn try_from(value: host::SourceAssessment) -> Result<Self, Self::Error> {
        Ok(Self {
            relevance: value.relevance,
            quality: value.quality,
            limitations: value.limitations,
        })
    }
}
impl From<crate::research::SourceAssessment> for host::SourceAssessment {
    fn from(value: crate::research::SourceAssessment) -> Self {
        Self {
            relevance: value.relevance,
            quality: value.quality,
            limitations: value.limitations,
        }
    }
}

impl TryFrom<host::ImportedSource> for crate::research::ImportedSource {
    type Error = host::CommandError;
    fn try_from(value: host::ImportedSource) -> Result<Self, Self::Error> {
        Ok(Self {
            title: value.title,
            kind: value.kind.try_into()?,
            metadata: value.metadata.try_into()?,
            pages: value
                .pages
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research::ImportedSource> for host::ImportedSource {
    fn from(value: crate::research::ImportedSource) -> Self {
        Self {
            title: value.title,
            kind: value.kind.into(),
            metadata: value.metadata.into(),
            pages: value.pages.into_iter().map(|item| item.into()).collect(),
        }
    }
}

impl TryFrom<host::PaperContent> for crate::research::PaperContent {
    type Error = host::CommandError;
    fn try_from(value: host::PaperContent) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact_id: parse_uuid(value.artifact_id)?,
            extraction_error: value
                .extraction_error
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            ocr: value.ocr,
        })
    }
}
impl From<crate::research::PaperContent> for host::PaperContent {
    fn from(value: crate::research::PaperContent) -> Self {
        Self {
            artifact_id: value.artifact_id.to_string(),
            extraction_error: value.extraction_error.map(|item| item),
            ocr: value.ocr,
        }
    }
}

impl TryFrom<host::ResearchSource> for crate::research::ResearchSource {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchSource) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            title: value.title,
            kind: value.kind.try_into()?,
            origin: value.origin.try_into()?,
            metadata: value.metadata.try_into()?,
            pages: value
                .pages
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            paper: value
                .paper
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
            selected: value.selected,
            human_assessment: value
                .human_assessment
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
            agent_assessment: value
                .agent_assessment
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
        })
    }
}
impl From<crate::research::ResearchSource> for host::ResearchSource {
    fn from(value: crate::research::ResearchSource) -> Self {
        Self {
            id: value.id.to_string(),
            title: value.title,
            kind: value.kind.into(),
            origin: value.origin.into(),
            metadata: value.metadata.into(),
            pages: value.pages.into_iter().map(|item| item.into()).collect(),
            paper: value.paper.map(|item| item.into()),
            selected: value.selected,
            human_assessment: value.human_assessment.map(|item| item.into()),
            agent_assessment: value.agent_assessment.map(|item| item.into()),
        }
    }
}

impl TryFrom<host::ResearchBrief> for crate::research::ResearchBrief {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchBrief) -> Result<Self, Self::Error> {
        Ok(Self {
            purpose: value.purpose,
            scope: value.scope,
            assumptions: value.assumptions,
            open_questions: value.open_questions,
        })
    }
}
impl From<crate::research::ResearchBrief> for host::ResearchBrief {
    fn from(value: crate::research::ResearchBrief) -> Self {
        Self {
            purpose: value.purpose,
            scope: value.scope,
            assumptions: value.assumptions,
            open_questions: value.open_questions,
        }
    }
}

impl TryFrom<host::ThreadRef> for crate::research::ThreadRef {
    type Error = host::CommandError;
    fn try_from(value: host::ThreadRef) -> Result<Self, Self::Error> {
        Ok(Self {
            provider: value.provider,
            project_id: value
                .project_id
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            thread_id: value.thread_id,
            title: value.title,
        })
    }
}
impl From<crate::research::ThreadRef> for host::ThreadRef {
    fn from(value: crate::research::ThreadRef) -> Self {
        Self {
            provider: value.provider,
            project_id: value.project_id.map(|item| item),
            thread_id: value.thread_id,
            title: value.title,
        }
    }
}

impl TryFrom<host::ResearchRound> for crate::research::ResearchRound {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchRound) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            captured_at: value.captured_at,
            question: value.question,
            summary: value.summary,
            open_questions: value.open_questions,
            discussion: value.discussion,
            thread: value
                .thread
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
            turn_id: value
                .turn_id
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            artifact_ids: value
                .artifact_ids
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research::ResearchRound> for host::ResearchRound {
    fn from(value: crate::research::ResearchRound) -> Self {
        Self {
            id: value.id.to_string(),
            captured_at: value.captured_at,
            question: value.question,
            summary: value.summary,
            open_questions: value.open_questions,
            discussion: value.discussion,
            thread: value.thread.map(|item| item.into()),
            turn_id: value.turn_id.map(|item| item),
            artifact_ids: value
                .artifact_ids
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
        }
    }
}

impl TryFrom<host::Acquisition> for crate::research::Acquisition {
    type Error = host::CommandError;
    fn try_from(value: host::Acquisition) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            source_id: parse_uuid(value.source_id)?,
            input: value.input.try_into()?,
            ocr: value.ocr,
            status: value.status.try_into()?,
            authority: None,
            error: value
                .error
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
        })
    }
}
impl From<crate::research::Acquisition> for host::Acquisition {
    fn from(value: crate::research::Acquisition) -> Self {
        Self {
            id: value.id.to_string(),
            source_id: value.source_id.to_string(),
            input: value.input.into(),
            ocr: value.ocr,
            status: value.status.into(),
            error: value.error.map(|item| item),
        }
    }
}

impl TryFrom<host::AssessmentEvent> for crate::research::AssessmentEvent {
    type Error = host::CommandError;
    fn try_from(value: host::AssessmentEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            source_id: parse_uuid(value.source_id)?,
            human: value.human,
            assessment: value.assessment.try_into()?,
        })
    }
}
impl From<crate::research::AssessmentEvent> for host::AssessmentEvent {
    fn from(value: crate::research::AssessmentEvent) -> Self {
        Self {
            source_id: value.source_id.to_string(),
            human: value.human,
            assessment: value.assessment.into(),
        }
    }
}

impl TryFrom<host::ResearchArtifact> for crate::research::ResearchArtifact {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchArtifact) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            title: value.title,
            kind: value.kind.try_into()?,
            url: value
                .url
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            final_url: value
                .final_url
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            managed_path: value
                .managed_path
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            markdown: value
                .markdown
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            captured_at: value.captured_at,
            thread: value
                .thread
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
            round_id: value
                .round_id
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .transpose()?,
            role: value.role.try_into()?,
            source_ids: value
                .source_ids
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research::ResearchArtifact> for host::ResearchArtifact {
    fn from(value: crate::research::ResearchArtifact) -> Self {
        Self {
            id: value.id.to_string(),
            title: value.title,
            kind: value.kind.into(),
            url: value.url.map(|item| item),
            final_url: value.final_url.map(|item| item),
            managed_path: value.managed_path.map(|item| item),
            markdown: value.markdown.map(|item| item),
            captured_at: value.captured_at,
            thread: value.thread.map(|item| item.into()),
            round_id: value.round_id.map(|item| item.to_string()),
            role: value.role.into(),
            source_ids: value
                .source_ids
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
        }
    }
}

impl TryFrom<host::ResearchWorkspace> for crate::research::ResearchWorkspace {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchWorkspace) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            revision: crate::notes::Revision(parse_uuid(value.revision)?),
            title: value.title,
            question: value.question,
            sources: value
                .sources
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            research_notes: value
                .research_notes
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> {
                    Ok(crate::notes::NoteId(parse_uuid(item)?))
                })
                .collect::<Result<Vec<_>, _>>()?,
            brief: value.brief.try_into()?,
            threads: value
                .threads
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            rounds: value
                .rounds
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            artifacts: value
                .artifacts
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            acquisitions: value
                .acquisitions
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
            assessment_history: value
                .assessment_history
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research::ResearchWorkspace> for host::ResearchWorkspace {
    fn from(value: crate::research::ResearchWorkspace) -> Self {
        Self {
            id: value.id.to_string(),
            revision: value.revision.0.to_string(),
            title: value.title,
            question: value.question,
            sources: value.sources.into_iter().map(|item| item.into()).collect(),
            research_notes: value
                .research_notes
                .into_iter()
                .map(|item| item.0.to_string())
                .collect(),
            brief: value.brief.into(),
            threads: value.threads.into_iter().map(|item| item.into()).collect(),
            rounds: value.rounds.into_iter().map(|item| item.into()).collect(),
            artifacts: value
                .artifacts
                .into_iter()
                .map(|item| item.into())
                .collect(),
            acquisitions: value
                .acquisitions
                .into_iter()
                .map(|item| item.into())
                .collect(),
            assessment_history: value
                .assessment_history
                .into_iter()
                .map(|item| item.into())
                .collect(),
        }
    }
}

impl TryFrom<host::ReferenceCandidate> for crate::research_desktop::ReferenceCandidate {
    type Error = host::CommandError;
    fn try_from(value: host::ReferenceCandidate) -> Result<Self, Self::Error> {
        Ok(Self {
            title: value.title,
            url: value.url,
            author: value
                .author
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            date: value
                .date
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
        })
    }
}
impl From<crate::research_desktop::ReferenceCandidate> for host::ReferenceCandidate {
    fn from(value: crate::research_desktop::ReferenceCandidate) -> Self {
        Self {
            title: value.title,
            url: value.url,
            author: value.author.map(|item| item),
            date: value.date.map(|item| item),
        }
    }
}

impl TryFrom<host::OutputCard> for crate::research_desktop::OutputCard {
    type Error = host::CommandError;
    fn try_from(value: host::OutputCard) -> Result<Self, Self::Error> {
        Ok(Self {
            heading: value.heading,
            body: value.body,
            citations: value
                .citations
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research_desktop::OutputCard> for host::OutputCard {
    fn from(value: crate::research_desktop::OutputCard) -> Self {
        Self {
            heading: value.heading,
            body: value.body,
            citations: value.citations.into_iter().map(|item| item).collect(),
        }
    }
}

impl TryFrom<host::OutputSpec> for crate::research_desktop::OutputSpec {
    type Error = host::CommandError;
    fn try_from(value: host::OutputSpec) -> Result<Self, Self::Error> {
        Ok(Self {
            source_urls: decode_source_urls(value.source_urls)?,
            title: value.title,
            kind: value.kind.try_into()?,
            cards: value
                .cards
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research_desktop::OutputSpec> for host::OutputSpec {
    fn from(value: crate::research_desktop::OutputSpec) -> Self {
        Self {
            source_urls: value
                .source_urls
                .into_iter()
                .map(|(id, url)| host::SourceUrl {
                    id: id.to_string(),
                    url,
                })
                .collect(),
            title: value.title,
            kind: value.kind.into(),
            cards: value.cards.into_iter().map(|item| item.into()).collect(),
        }
    }
}

impl TryFrom<host::Passage> for crate::research_desktop::Passage {
    type Error = host::CommandError;
    fn try_from(value: host::Passage) -> Result<Self, Self::Error> {
        Ok(Self {
            source_id: parse_uuid(value.source_id)?,
            page: value
                .page
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            text: value.text,
            citation: value.citation,
            score: parse_usize(value.score)?,
        })
    }
}
impl From<crate::research_desktop::Passage> for host::Passage {
    fn from(value: crate::research_desktop::Passage) -> Self {
        Self {
            source_id: value.source_id.to_string(),
            page: value.page.map(|item| item),
            text: value.text,
            citation: value.citation,
            score: value.score as u64,
        }
    }
}

impl TryFrom<host::ResearchIndex> for crate::research::ResearchIndex {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchIndex) -> Result<Self, Self::Error> {
        Ok(Self {
            id: parse_uuid(value.id)?,
            revision: crate::notes::Revision(parse_uuid(value.revision)?),
            title: value.title,
            round_ids: value
                .round_ids
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .collect::<Result<Vec<_>, _>>()?,
            artifact_ids: value
                .artifact_ids
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .collect::<Result<Vec<_>, _>>()?,
            source_ids: value
                .source_ids
                .into_iter()
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                .collect::<Result<Vec<_>, _>>()?,
        })
    }
}
impl From<crate::research::ResearchIndex> for host::ResearchIndex {
    fn from(value: crate::research::ResearchIndex) -> Self {
        Self {
            id: value.id.to_string(),
            revision: value.revision.0.to_string(),
            title: value.title,
            round_ids: value
                .round_ids
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
            artifact_ids: value
                .artifact_ids
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
            source_ids: value
                .source_ids
                .into_iter()
                .map(|item| item.to_string())
                .collect(),
        }
    }
}

impl TryFrom<host::SourceExcerpt> for crate::research::SourceExcerpt {
    type Error = host::CommandError;
    fn try_from(value: host::SourceExcerpt) -> Result<Self, Self::Error> {
        Ok(Self {
            source_id: parse_uuid(value.source_id)?,
            title: value.title,
            paper: value
                .paper
                .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                .transpose()?,
            page: parse_usize(value.page)?,
            number: value
                .number
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            offset: parse_usize(value.offset)?,
            text: value.text,
            total_bytes: parse_usize(value.total_bytes)?,
            next_offset: value
                .next_offset
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_usize(item)?) })
                .transpose()?,
            next_page: value
                .next_page
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_usize(item)?) })
                .transpose()?,
        })
    }
}
impl From<crate::research::SourceExcerpt> for host::SourceExcerpt {
    fn from(value: crate::research::SourceExcerpt) -> Self {
        Self {
            source_id: value.source_id.to_string(),
            title: value.title,
            paper: value.paper.map(|item| item.into()),
            page: value.page as u64,
            number: value.number.map(|item| item),
            offset: value.offset as u64,
            text: value.text,
            total_bytes: value.total_bytes as u64,
            next_offset: value.next_offset.map(|item| item as u64),
            next_page: value.next_page.map(|item| item as u64),
        }
    }
}

impl TryFrom<host::ArtifactExcerpt> for crate::research::ArtifactExcerpt {
    type Error = host::CommandError;
    fn try_from(value: host::ArtifactExcerpt) -> Result<Self, Self::Error> {
        Ok(Self {
            artifact: value.artifact.try_into()?,
            markdown: value
                .markdown
                .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                .transpose()?,
            offset: parse_usize(value.offset)?,
            next_offset: value
                .next_offset
                .map(|item| -> Result<_, host::CommandError> { Ok(parse_usize(item)?) })
                .transpose()?,
            total_bytes: parse_usize(value.total_bytes)?,
        })
    }
}
impl From<crate::research::ArtifactExcerpt> for host::ArtifactExcerpt {
    fn from(value: crate::research::ArtifactExcerpt) -> Self {
        Self {
            artifact: value.artifact.into(),
            markdown: value.markdown.map(|item| item),
            offset: value.offset as u64,
            next_offset: value.next_offset.map(|item| item as u64),
            total_bytes: value.total_bytes as u64,
        }
    }
}

impl TryFrom<host::SourceKind> for crate::research::SourceKind {
    type Error = host::CommandError;
    fn try_from(value: host::SourceKind) -> Result<Self, Self::Error> {
        Ok(match value {
            host::SourceKind::Note => Self::Note,
            host::SourceKind::Text => Self::Text,
            host::SourceKind::Pdf => Self::Pdf,
            host::SourceKind::Web => Self::Web,
            host::SourceKind::Image => Self::Image,
            host::SourceKind::Video => Self::Video,
            host::SourceKind::Observation => Self::Observation,
        })
    }
}
impl From<crate::research::SourceKind> for host::SourceKind {
    fn from(value: crate::research::SourceKind) -> Self {
        match value {
            crate::research::SourceKind::Note => Self::Note,
            crate::research::SourceKind::Text => Self::Text,
            crate::research::SourceKind::Pdf => Self::Pdf,
            crate::research::SourceKind::Web => Self::Web,
            crate::research::SourceKind::Image => Self::Image,
            crate::research::SourceKind::Video => Self::Video,
            crate::research::SourceKind::Observation => Self::Observation,
        }
    }
}

impl TryFrom<host::ArtifactKind> for crate::research::ArtifactKind {
    type Error = host::CommandError;
    fn try_from(value: host::ArtifactKind) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ArtifactKind::File => Self::File,
            host::ArtifactKind::Pdf => Self::Pdf,
            host::ArtifactKind::Image => Self::Image,
            host::ArtifactKind::Web => Self::Web,
            host::ArtifactKind::Markdown => Self::Markdown,
        })
    }
}
impl From<crate::research::ArtifactKind> for host::ArtifactKind {
    fn from(value: crate::research::ArtifactKind) -> Self {
        match value {
            crate::research::ArtifactKind::File => Self::File,
            crate::research::ArtifactKind::Pdf => Self::Pdf,
            crate::research::ArtifactKind::Image => Self::Image,
            crate::research::ArtifactKind::Web => Self::Web,
            crate::research::ArtifactKind::Markdown => Self::Markdown,
        }
    }
}

impl TryFrom<host::ArtifactRole> for crate::research::ArtifactRole {
    type Error = host::CommandError;
    fn try_from(value: host::ArtifactRole) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ArtifactRole::Unclassified => Self::Unclassified,
            host::ArtifactRole::Input => Self::Input,
            host::ArtifactRole::Output => Self::Output,
        })
    }
}
impl From<crate::research::ArtifactRole> for host::ArtifactRole {
    fn from(value: crate::research::ArtifactRole) -> Self {
        match value {
            crate::research::ArtifactRole::Unclassified => Self::Unclassified,
            crate::research::ArtifactRole::Input => Self::Input,
            crate::research::ArtifactRole::Output => Self::Output,
        }
    }
}

impl TryFrom<host::AcquisitionStatus> for crate::research::AcquisitionStatus {
    type Error = host::CommandError;
    fn try_from(value: host::AcquisitionStatus) -> Result<Self, Self::Error> {
        Ok(match value {
            host::AcquisitionStatus::Queued => Self::Queued,
            host::AcquisitionStatus::Running => Self::Running,
            host::AcquisitionStatus::Ready => Self::Ready,
            host::AcquisitionStatus::Failed => Self::Failed,
            host::AcquisitionStatus::Cancelled => Self::Cancelled,
        })
    }
}
impl From<crate::research::AcquisitionStatus> for host::AcquisitionStatus {
    fn from(value: crate::research::AcquisitionStatus) -> Self {
        match value {
            crate::research::AcquisitionStatus::Queued => Self::Queued,
            crate::research::AcquisitionStatus::Running => Self::Running,
            crate::research::AcquisitionStatus::Ready => Self::Ready,
            crate::research::AcquisitionStatus::Failed => Self::Failed,
            crate::research::AcquisitionStatus::Cancelled => Self::Cancelled,
        }
    }
}

impl TryFrom<host::ContextMode> for crate::research::ContextMode {
    type Error = host::CommandError;
    fn try_from(value: host::ContextMode) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ContextMode::Continue => Self::Continue,
            host::ContextMode::Report => Self::Report,
        })
    }
}
impl From<crate::research::ContextMode> for host::ContextMode {
    fn from(value: crate::research::ContextMode) -> Self {
        match value {
            crate::research::ContextMode::Continue => Self::Continue,
            crate::research::ContextMode::Report => Self::Report,
        }
    }
}

impl TryFrom<host::SourceScope> for crate::research::SourceScope {
    type Error = host::CommandError;
    fn try_from(value: host::SourceScope) -> Result<Self, Self::Error> {
        Ok(match value {
            host::SourceScope::ExistingOnly => Self::ExistingOnly,
            host::SourceScope::ExternalGathering => Self::ExternalGathering,
        })
    }
}
impl From<crate::research::SourceScope> for host::SourceScope {
    fn from(value: crate::research::SourceScope) -> Self {
        match value {
            crate::research::SourceScope::ExistingOnly => Self::ExistingOnly,
            crate::research::SourceScope::ExternalGathering => Self::ExternalGathering,
        }
    }
}

impl TryFrom<host::OutputKind> for crate::research_desktop::OutputKind {
    type Error = host::CommandError;
    fn try_from(value: host::OutputKind) -> Result<Self, Self::Error> {
        Ok(match value {
            host::OutputKind::Slides => Self::Slides,
            host::OutputKind::Infographic => Self::Infographic,
            host::OutputKind::MindMap => Self::MindMap,
        })
    }
}
impl From<crate::research_desktop::OutputKind> for host::OutputKind {
    fn from(value: crate::research_desktop::OutputKind) -> Self {
        match value {
            crate::research_desktop::OutputKind::Slides => Self::Slides,
            crate::research_desktop::OutputKind::Infographic => Self::Infographic,
            crate::research_desktop::OutputKind::MindMap => Self::MindMap,
        }
    }
}

impl TryFrom<host::SourceOrigin> for crate::research::SourceOrigin {
    type Error = host::CommandError;
    fn try_from(value: host::SourceOrigin) -> Result<Self, Self::Error> {
        Ok(match value {
            host::SourceOrigin::NoteSnapshot(payload) => Self::NoteSnapshot {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                revision: crate::notes::Revision(parse_uuid(payload.revision)?),
            },
            host::SourceOrigin::Imported => Self::Imported,
        })
    }
}
impl From<crate::research::SourceOrigin> for host::SourceOrigin {
    fn from(value: crate::research::SourceOrigin) -> Self {
        match value {
            crate::research::SourceOrigin::NoteSnapshot { id, revision } => {
                Self::NoteSnapshot(host::SourceOriginNoteSnapshot {
                    id: id.0.to_string(),
                    revision: revision.0.to_string(),
                })
            }
            crate::research::SourceOrigin::Imported => Self::Imported,
        }
    }
}

impl TryFrom<host::ArtifactInput> for crate::research::ArtifactInput {
    type Error = host::CommandError;
    fn try_from(value: host::ArtifactInput) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ArtifactInput::LocalFile(payload) => Self::LocalFile { path: payload.path },
            host::ArtifactInput::Url(payload) => Self::Url {
                url: payload.url,
                archive: payload.archive,
            },
            host::ArtifactInput::Generated(payload) => Self::Generated {
                spec: payload.spec.try_into()?,
            },
            host::ArtifactInput::Markdown(payload) => Self::Markdown {
                markdown: payload.markdown,
                url: payload
                    .url
                    .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                    .transpose()?,
            },
        })
    }
}
impl From<crate::research::ArtifactInput> for host::ArtifactInput {
    fn from(value: crate::research::ArtifactInput) -> Self {
        match value {
            crate::research::ArtifactInput::LocalFile { path } => {
                Self::LocalFile(host::ArtifactInputLocalFile { path: path })
            }
            crate::research::ArtifactInput::Url { url, archive } => {
                Self::Url(host::ArtifactInputUrl {
                    url: url,
                    archive: archive,
                })
            }
            crate::research::ArtifactInput::Generated { spec } => {
                Self::Generated(host::ArtifactInputGenerated { spec: spec.into() })
            }
            crate::research::ArtifactInput::Markdown { markdown, url } => {
                Self::Markdown(host::ArtifactInputMarkdown {
                    markdown: markdown,
                    url: url.map(|item| item),
                })
            }
        }
    }
}

impl TryFrom<host::ImportDestination> for crate::commands::ImportDestination {
    type Error = host::CommandError;
    fn try_from(value: host::ImportDestination) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ImportDestination::Create(payload) => Self::Create { path: payload.path },
            host::ImportDestination::Replace(payload) => Self::Replace {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
            },
        })
    }
}
impl From<crate::commands::ImportDestination> for host::ImportDestination {
    fn from(value: crate::commands::ImportDestination) -> Self {
        match value {
            crate::commands::ImportDestination::Create { path } => {
                Self::Create(host::ImportDestinationCreate { path: path })
            }
            crate::commands::ImportDestination::Replace { id, expected } => {
                Self::Replace(host::ImportDestinationReplace {
                    id: id.0.to_string(),
                    expected: expected.0.to_string(),
                })
            }
        }
    }
}

impl TryFrom<host::ResearchCommand> for crate::research::ResearchCommand {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchCommand) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ResearchCommand::ImportSources(payload) => Self::ImportSources {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                paths: payload
                    .paths
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            host::ResearchCommand::AcquireSources(payload) => Self::AcquireSources {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_ids: payload
                    .source_ids
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            host::ResearchCommand::RetryAcquisitions(payload) => Self::RetryAcquisitions {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                job_ids: payload
                    .job_ids
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            host::ResearchCommand::CollectReferences(payload) => Self::CollectReferences {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                candidates: payload
                    .candidates
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .collect::<Result<Vec<_>, _>>()?,
                thread: payload
                    .thread
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .transpose()?,
                turn_id: payload
                    .turn_id
                    .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                    .transpose()?,
            },
            host::ResearchCommand::ImportSource(payload) => Self::ImportSource {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                path: payload.path,
            },
            host::ResearchCommand::AcquireSource(payload) => Self::AcquireSource {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_id: parse_uuid(payload.source_id)?,
                input: payload.input.try_into()?,
                ocr: payload.ocr,
            },
            host::ResearchCommand::RetryAcquisition(payload) => Self::RetryAcquisition {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                job_id: parse_uuid(payload.job_id)?,
            },
            host::ResearchCommand::CancelAcquisition(payload) => Self::CancelAcquisition {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                job_id: parse_uuid(payload.job_id)?,
            },
            host::ResearchCommand::CreateOutput(payload) => Self::CreateOutput {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                spec: payload.spec.try_into()?,
            },
            host::ResearchCommand::SetArtifactRole(payload) => Self::SetArtifactRole {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                artifact_id: parse_uuid(payload.artifact_id)?,
                role: payload.role.try_into()?,
            },
            host::ResearchCommand::FindPassages(payload) => Self::FindPassages {
                id: parse_uuid(payload.id)?,
                query: payload.query,
                limit: parse_usize(payload.limit)?,
            },
            host::ResearchCommand::EvidenceGaps(payload) => Self::EvidenceGaps {
                id: parse_uuid(payload.id)?,
            },
            host::ResearchCommand::Create(payload) => Self::Create {
                title: payload.title,
                question: payload.question,
            },
            host::ResearchCommand::List => Self::List,
            host::ResearchCommand::Read(payload) => Self::Read {
                id: parse_uuid(payload.id)?,
            },
            host::ResearchCommand::ReadIndex(payload) => Self::ReadIndex {
                id: parse_uuid(payload.id)?,
            },
            host::ResearchCommand::ReadRound(payload) => Self::ReadRound {
                id: parse_uuid(payload.id)?,
                round_id: parse_uuid(payload.round_id)?,
            },
            host::ResearchCommand::ReadSource(payload) => Self::ReadSource {
                id: parse_uuid(payload.id)?,
                source_id: parse_uuid(payload.source_id)?,
                page: parse_usize(payload.page)?,
                offset: parse_usize(payload.offset)?,
                limit: parse_usize(payload.limit)?,
            },
            host::ResearchCommand::SavePaper(payload) => Self::SavePaper {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_id: parse_uuid(payload.source_id)?,
                input: payload.input.try_into()?,
                ocr: payload.ocr,
            },
            host::ResearchCommand::ReadArtifact(payload) => Self::ReadArtifact {
                id: parse_uuid(payload.id)?,
                artifact_id: parse_uuid(payload.artifact_id)?,
                offset: parse_usize(payload.offset)?,
                limit: parse_usize(payload.limit)?,
            },
            host::ResearchCommand::Update(payload) => Self::Update {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                title: payload.title,
                question: payload.question,
            },
            host::ResearchCommand::AddNote(payload) => Self::AddNote {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                note_id: crate::notes::NoteId(parse_uuid(payload.note_id)?),
            },
            host::ResearchCommand::AddImported(payload) => Self::AddImported {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source: payload.source.try_into()?,
            },
            host::ResearchCommand::RemoveSource(payload) => Self::RemoveSource {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_id: parse_uuid(payload.source_id)?,
            },
            host::ResearchCommand::SelectSource(payload) => Self::SelectSource {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_id: parse_uuid(payload.source_id)?,
                selected: payload.selected,
            },
            host::ResearchCommand::AssessSource(payload) => Self::AssessSource {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                source_id: parse_uuid(payload.source_id)?,
                human: payload.human,
                assessment: payload.assessment.try_into()?,
            },
            host::ResearchCommand::LinkNote(payload) => Self::LinkNote {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                note_id: crate::notes::NoteId(parse_uuid(payload.note_id)?),
            },
            host::ResearchCommand::BuildPrompt(payload) => Self::BuildPrompt {
                id: parse_uuid(payload.id)?,
            },
            host::ResearchCommand::SetBrief(payload) => Self::SetBrief {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                brief: payload.brief.try_into()?,
            },
            host::ResearchCommand::AssociateThread(payload) => Self::AssociateThread {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                thread: payload.thread.try_into()?,
            },
            host::ResearchCommand::RemoveThread(payload) => Self::RemoveThread {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                thread: payload.thread.try_into()?,
            },
            host::ResearchCommand::AddRound(payload) => Self::AddRound {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                round: payload.round.try_into()?,
            },
            host::ResearchCommand::CollectArtifact(payload) => Self::CollectArtifact {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                title: payload.title,
                input: payload.input.try_into()?,
                thread: payload
                    .thread
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .transpose()?,
                round_id: payload
                    .round_id
                    .map(|item| -> Result<_, host::CommandError> { Ok(parse_uuid(item)?) })
                    .transpose()?,
            },
            host::ResearchCommand::RetryArtifact(payload) => Self::RetryArtifact {
                id: parse_uuid(payload.id)?,
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                receipt_id: parse_uuid(payload.receipt_id)?,
            },
            host::ResearchCommand::BuildReport(payload) => Self::BuildReport {
                id: parse_uuid(payload.id)?,
            },
            host::ResearchCommand::BuildContext(payload) => Self::BuildContext {
                id: parse_uuid(payload.id)?,
                mode: payload.mode.try_into()?,
                scope: payload.scope.try_into()?,
            },
            host::ResearchCommand::Discover(payload) => Self::Discover {
                id: parse_uuid(payload.id)?,
            },
        })
    }
}
impl From<crate::research::ResearchCommand> for host::ResearchCommand {
    fn from(value: crate::research::ResearchCommand) -> Self {
        match value {
            crate::research::ResearchCommand::ImportSources {
                id,
                expected,
                paths,
            } => Self::ImportSources(host::ResearchCommandImportSources {
                id: id.to_string(),
                expected: expected.0.to_string(),
                paths: paths.into_iter().map(|item| item).collect(),
            }),
            crate::research::ResearchCommand::AcquireSources {
                id,
                expected,
                source_ids,
            } => Self::AcquireSources(host::ResearchCommandAcquireSources {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_ids: source_ids
                    .into_iter()
                    .map(|item| item.to_string())
                    .collect(),
            }),
            crate::research::ResearchCommand::RetryAcquisitions {
                id,
                expected,
                job_ids,
            } => Self::RetryAcquisitions(host::ResearchCommandRetryAcquisitions {
                id: id.to_string(),
                expected: expected.0.to_string(),
                job_ids: job_ids.into_iter().map(|item| item.to_string()).collect(),
            }),
            crate::research::ResearchCommand::CollectReferences {
                id,
                expected,
                candidates,
                thread,
                turn_id,
            } => Self::CollectReferences(host::ResearchCommandCollectReferences {
                id: id.to_string(),
                expected: expected.0.to_string(),
                candidates: candidates.into_iter().map(|item| item.into()).collect(),
                thread: thread.map(|item| item.into()),
                turn_id: turn_id.map(|item| item),
            }),
            crate::research::ResearchCommand::ImportSource { id, expected, path } => {
                Self::ImportSource(host::ResearchCommandImportSource {
                    id: id.to_string(),
                    expected: expected.0.to_string(),
                    path: path,
                })
            }
            crate::research::ResearchCommand::AcquireSource {
                id,
                expected,
                source_id,
                input,
                ocr,
            } => Self::AcquireSource(host::ResearchCommandAcquireSource {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_id: source_id.to_string(),
                input: input.into(),
                ocr: ocr,
            }),
            crate::research::ResearchCommand::RetryAcquisition {
                id,
                expected,
                job_id,
            } => Self::RetryAcquisition(host::ResearchCommandRetryAcquisition {
                id: id.to_string(),
                expected: expected.0.to_string(),
                job_id: job_id.to_string(),
            }),
            crate::research::ResearchCommand::CancelAcquisition {
                id,
                expected,
                job_id,
            } => Self::CancelAcquisition(host::ResearchCommandCancelAcquisition {
                id: id.to_string(),
                expected: expected.0.to_string(),
                job_id: job_id.to_string(),
            }),
            crate::research::ResearchCommand::CreateOutput { id, expected, spec } => {
                Self::CreateOutput(host::ResearchCommandCreateOutput {
                    id: id.to_string(),
                    expected: expected.0.to_string(),
                    spec: spec.into(),
                })
            }
            crate::research::ResearchCommand::SetArtifactRole {
                id,
                expected,
                artifact_id,
                role,
            } => Self::SetArtifactRole(host::ResearchCommandSetArtifactRole {
                id: id.to_string(),
                expected: expected.0.to_string(),
                artifact_id: artifact_id.to_string(),
                role: role.into(),
            }),
            crate::research::ResearchCommand::FindPassages { id, query, limit } => {
                Self::FindPassages(host::ResearchCommandFindPassages {
                    id: id.to_string(),
                    query: query,
                    limit: limit as u64,
                })
            }
            crate::research::ResearchCommand::EvidenceGaps { id } => {
                Self::EvidenceGaps(host::ResearchCommandEvidenceGaps { id: id.to_string() })
            }
            crate::research::ResearchCommand::Create { title, question } => {
                Self::Create(host::ResearchCommandCreate {
                    title: title,
                    question: question,
                })
            }
            crate::research::ResearchCommand::List => Self::List,
            crate::research::ResearchCommand::Read { id } => {
                Self::Read(host::ResearchCommandRead { id: id.to_string() })
            }
            crate::research::ResearchCommand::ReadIndex { id } => {
                Self::ReadIndex(host::ResearchCommandReadIndex { id: id.to_string() })
            }
            crate::research::ResearchCommand::ReadRound { id, round_id } => {
                Self::ReadRound(host::ResearchCommandReadRound {
                    id: id.to_string(),
                    round_id: round_id.to_string(),
                })
            }
            crate::research::ResearchCommand::ReadSource {
                id,
                source_id,
                page,
                offset,
                limit,
            } => Self::ReadSource(host::ResearchCommandReadSource {
                id: id.to_string(),
                source_id: source_id.to_string(),
                page: page as u64,
                offset: offset as u64,
                limit: limit as u64,
            }),
            crate::research::ResearchCommand::SavePaper {
                id,
                expected,
                source_id,
                input,
                ocr,
            } => Self::SavePaper(host::ResearchCommandSavePaper {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_id: source_id.to_string(),
                input: input.into(),
                ocr: ocr,
            }),
            crate::research::ResearchCommand::ReadArtifact {
                id,
                artifact_id,
                offset,
                limit,
            } => Self::ReadArtifact(host::ResearchCommandReadArtifact {
                id: id.to_string(),
                artifact_id: artifact_id.to_string(),
                offset: offset as u64,
                limit: limit as u64,
            }),
            crate::research::ResearchCommand::Update {
                id,
                expected,
                title,
                question,
            } => Self::Update(host::ResearchCommandUpdate {
                id: id.to_string(),
                expected: expected.0.to_string(),
                title: title,
                question: question,
            }),
            crate::research::ResearchCommand::AddNote {
                id,
                expected,
                note_id,
            } => Self::AddNote(host::ResearchCommandAddNote {
                id: id.to_string(),
                expected: expected.0.to_string(),
                note_id: note_id.0.to_string(),
            }),
            crate::research::ResearchCommand::AddImported {
                id,
                expected,
                source,
            } => Self::AddImported(host::ResearchCommandAddImported {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source: source.into(),
            }),
            crate::research::ResearchCommand::RemoveSource {
                id,
                expected,
                source_id,
            } => Self::RemoveSource(host::ResearchCommandRemoveSource {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_id: source_id.to_string(),
            }),
            crate::research::ResearchCommand::SelectSource {
                id,
                expected,
                source_id,
                selected,
            } => Self::SelectSource(host::ResearchCommandSelectSource {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_id: source_id.to_string(),
                selected: selected,
            }),
            crate::research::ResearchCommand::AssessSource {
                id,
                expected,
                source_id,
                human,
                assessment,
            } => Self::AssessSource(host::ResearchCommandAssessSource {
                id: id.to_string(),
                expected: expected.0.to_string(),
                source_id: source_id.to_string(),
                human: human,
                assessment: assessment.into(),
            }),
            crate::research::ResearchCommand::LinkNote {
                id,
                expected,
                note_id,
            } => Self::LinkNote(host::ResearchCommandLinkNote {
                id: id.to_string(),
                expected: expected.0.to_string(),
                note_id: note_id.0.to_string(),
            }),
            crate::research::ResearchCommand::BuildPrompt { id } => {
                Self::BuildPrompt(host::ResearchCommandBuildPrompt { id: id.to_string() })
            }
            crate::research::ResearchCommand::SetBrief {
                id,
                expected,
                brief,
            } => Self::SetBrief(host::ResearchCommandSetBrief {
                id: id.to_string(),
                expected: expected.0.to_string(),
                brief: brief.into(),
            }),
            crate::research::ResearchCommand::AssociateThread {
                id,
                expected,
                thread,
            } => Self::AssociateThread(host::ResearchCommandAssociateThread {
                id: id.to_string(),
                expected: expected.0.to_string(),
                thread: thread.into(),
            }),
            crate::research::ResearchCommand::RemoveThread {
                id,
                expected,
                thread,
            } => Self::RemoveThread(host::ResearchCommandRemoveThread {
                id: id.to_string(),
                expected: expected.0.to_string(),
                thread: thread.into(),
            }),
            crate::research::ResearchCommand::AddRound {
                id,
                expected,
                round,
            } => Self::AddRound(host::ResearchCommandAddRound {
                id: id.to_string(),
                expected: expected.0.to_string(),
                round: round.into(),
            }),
            crate::research::ResearchCommand::CollectArtifact {
                id,
                expected,
                title,
                input,
                thread,
                round_id,
            } => Self::CollectArtifact(host::ResearchCommandCollectArtifact {
                id: id.to_string(),
                expected: expected.0.to_string(),
                title: title,
                input: input.into(),
                thread: thread.map(|item| item.into()),
                round_id: round_id.map(|item| item.to_string()),
            }),
            crate::research::ResearchCommand::RetryArtifact {
                id,
                expected,
                receipt_id,
            } => Self::RetryArtifact(host::ResearchCommandRetryArtifact {
                id: id.to_string(),
                expected: expected.0.to_string(),
                receipt_id: receipt_id.to_string(),
            }),
            crate::research::ResearchCommand::BuildReport { id } => {
                Self::BuildReport(host::ResearchCommandBuildReport { id: id.to_string() })
            }
            crate::research::ResearchCommand::BuildContext { id, mode, scope } => {
                Self::BuildContext(host::ResearchCommandBuildContext {
                    id: id.to_string(),
                    mode: mode.into(),
                    scope: scope.into(),
                })
            }
            crate::research::ResearchCommand::Discover { id } => {
                Self::Discover(host::ResearchCommandDiscover { id: id.to_string() })
            }
        }
    }
}

impl TryFrom<host::Command> for crate::commands::Command {
    type Error = host::CommandError;
    fn try_from(value: host::Command) -> Result<Self, Self::Error> {
        Ok(match value {
            host::Command::Research(payload) => Self::Research(payload.try_into()?),
            host::Command::GetCodexProject => Self::GetCodexProject,
            host::Command::SetAutomationWriteEnabled(payload) => Self::SetAutomationWriteEnabled {
                enabled: payload.enabled,
            },
            host::Command::SetCodexProject(payload) => Self::SetCodexProject {
                expected: payload
                    .expected
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .transpose()?,
                project: payload.project.try_into()?,
            },
            host::Command::ListenLan(payload) => Self::ListenLan {
                bind: payload.bind,
                advertised_ip: payload
                    .advertised_ip
                    .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                    .transpose()?,
            },
            host::Command::GetLanTicket => Self::GetLanTicket,
            host::Command::PrepareJoin(payload) => Self::PrepareJoin {
                ticket: payload.ticket,
            },
            host::Command::CancelJoin => Self::CancelJoin,
            host::Command::ConfirmPeer(payload) => Self::ConfirmPeer {
                ticket: payload.ticket,
            },
            host::Command::UnpairPeer(payload) => Self::UnpairPeer {
                fingerprint: payload.fingerprint,
            },
            host::Command::ConnectPeer(payload) => Self::ConnectPeer {
                fingerprint: payload.fingerprint,
            },
            host::Command::GetLanStatus => Self::GetLanStatus,
            host::Command::PauseLan => Self::PauseLan,
            host::Command::ResumeLan => Self::ResumeLan,
            host::Command::EnableSync => Self::EnableSync,
            host::Command::SetMirrorEnabled(payload) => Self::SetMirrorEnabled {
                enabled: payload.enabled,
            },
            host::Command::ImportMarkdown(payload) => Self::ImportMarkdown {
                source_path: payload.source_path,
                destination: payload.destination.try_into()?,
            },
            host::Command::ExportMarkdown(payload) => Self::ExportMarkdown {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                path: payload.path,
            },
            host::Command::RescanMirror => Self::RescanMirror,
            host::Command::CreateNote(payload) => Self::CreateNote {
                path: payload.path,
                text: payload.text,
            },
            host::Command::ReadNote(payload) => Self::ReadNote {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
            },
            host::Command::EditNote(payload) => Self::EditNote {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                edits: payload
                    .edits
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .collect::<Result<Vec<_>, _>>()?,
            },
            host::Command::RenameNote(payload) => Self::RenameNote {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
                path: payload.path,
            },
            host::Command::TrashNote(payload) => Self::TrashNote {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
            },
            host::Command::RestoreNote(payload) => Self::RestoreNote {
                id: crate::notes::NoteId(parse_uuid(payload.id)?),
                expected: crate::notes::Revision(parse_uuid(payload.expected)?),
            },
            host::Command::SearchNotes(payload) => Self::SearchNotes {
                query: payload.query,
                include_trash: payload.include_trash,
            },
        })
    }
}
impl TryFrom<crate::commands::Command> for host::Command {
    type Error = host::CommandError;
    fn try_from(value: crate::commands::Command) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::commands::Command::Plugin(_) => return Err(host::CommandError::Unauthorized),
            crate::commands::Command::Research(payload) => Self::Research(payload.into()),
            crate::commands::Command::GetCodexProject => Self::GetCodexProject,
            crate::commands::Command::SetAutomationWriteEnabled { enabled } => {
                Self::SetAutomationWriteEnabled(host::CommandSetAutomationWriteEnabled {
                    enabled: enabled,
                })
            }
            crate::commands::Command::SetCodexProject { expected, project } => {
                Self::SetCodexProject(host::CommandSetCodexProject {
                    expected: expected.map(|item| item.into()),
                    project: project.into(),
                })
            }
            crate::commands::Command::ListenLan {
                bind,
                advertised_ip,
            } => Self::ListenLan(host::CommandListenLan {
                bind: bind,
                advertised_ip: advertised_ip.map(|item| item),
            }),
            crate::commands::Command::GetLanTicket => Self::GetLanTicket,
            crate::commands::Command::PrepareJoin { ticket } => {
                Self::PrepareJoin(host::CommandPrepareJoin { ticket: ticket })
            }
            crate::commands::Command::CancelJoin => Self::CancelJoin,
            crate::commands::Command::ConfirmPeer { ticket } => {
                Self::ConfirmPeer(host::CommandConfirmPeer { ticket: ticket })
            }
            crate::commands::Command::UnpairPeer { fingerprint } => {
                Self::UnpairPeer(host::CommandUnpairPeer {
                    fingerprint: fingerprint,
                })
            }
            crate::commands::Command::ConnectPeer { fingerprint } => {
                Self::ConnectPeer(host::CommandConnectPeer {
                    fingerprint: fingerprint,
                })
            }
            crate::commands::Command::GetLanStatus => Self::GetLanStatus,
            crate::commands::Command::PauseLan => Self::PauseLan,
            crate::commands::Command::ResumeLan => Self::ResumeLan,
            crate::commands::Command::EnableSync => Self::EnableSync,
            crate::commands::Command::SetMirrorEnabled { enabled } => {
                Self::SetMirrorEnabled(host::CommandSetMirrorEnabled { enabled: enabled })
            }
            crate::commands::Command::ImportMarkdown {
                source_path,
                destination,
            } => Self::ImportMarkdown(host::CommandImportMarkdown {
                source_path: source_path,
                destination: destination.into(),
            }),
            crate::commands::Command::ExportMarkdown { id, path } => {
                Self::ExportMarkdown(host::CommandExportMarkdown {
                    id: id.0.to_string(),
                    path: path,
                })
            }
            crate::commands::Command::RescanMirror => Self::RescanMirror,
            crate::commands::Command::CreateNote { path, text } => {
                Self::CreateNote(host::CommandCreateNote {
                    path: path,
                    text: text,
                })
            }
            crate::commands::Command::ReadNote { id } => Self::ReadNote(host::CommandReadNote {
                id: id.0.to_string(),
            }),
            crate::commands::Command::EditNote {
                id,
                expected,
                edits,
            } => Self::EditNote(host::CommandEditNote {
                id: id.0.to_string(),
                expected: expected.0.to_string(),
                edits: edits.into_iter().map(|item| item.into()).collect(),
            }),
            crate::commands::Command::RenameNote { id, expected, path } => {
                Self::RenameNote(host::CommandRenameNote {
                    id: id.0.to_string(),
                    expected: expected.0.to_string(),
                    path: path,
                })
            }
            crate::commands::Command::TrashNote { id, expected } => {
                Self::TrashNote(host::CommandTrashNote {
                    id: id.0.to_string(),
                    expected: expected.0.to_string(),
                })
            }
            crate::commands::Command::RestoreNote { id, expected } => {
                Self::RestoreNote(host::CommandRestoreNote {
                    id: id.0.to_string(),
                    expected: expected.0.to_string(),
                })
            }
            crate::commands::Command::SearchNotes {
                query,
                include_trash,
            } => Self::SearchNotes(host::CommandSearchNotes {
                query: query,
                include_trash: include_trash,
            }),
        })
    }
}

impl TryFrom<host::ResearchResult> for crate::research::ResearchResult {
    type Error = host::CommandError;
    fn try_from(value: host::ResearchResult) -> Result<Self, Self::Error> {
        Ok(match value {
            host::ResearchResult::Passages(payload) => Self::Passages(
                payload
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            host::ResearchResult::Suggestions(payload) => Self::Suggestions(
                payload
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item) })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            host::ResearchResult::Workspace(payload) => Self::Workspace(payload.try_into()?),
            host::ResearchResult::Workspaces(payload) => Self::Workspaces(
                payload
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            host::ResearchResult::Prompt(payload) => Self::Prompt(payload),
            host::ResearchResult::Report(payload) => Self::Report(payload),
            host::ResearchResult::Index(payload) => Self::Index(payload.try_into()?),
            host::ResearchResult::Round(payload) => Self::Round(payload.try_into()?),
            host::ResearchResult::Artifact(payload) => Self::Artifact(payload.try_into()?),
            host::ResearchResult::Source(payload) => Self::Source(payload.try_into()?),
        })
    }
}
impl From<crate::research::ResearchResult> for host::ResearchResult {
    fn from(value: crate::research::ResearchResult) -> Self {
        match value {
            crate::research::ResearchResult::Passages(payload) => {
                Self::Passages(payload.into_iter().map(|item| item.into()).collect())
            }
            crate::research::ResearchResult::Suggestions(payload) => {
                Self::Suggestions(payload.into_iter().map(|item| item).collect())
            }
            crate::research::ResearchResult::Workspace(payload) => Self::Workspace(payload.into()),
            crate::research::ResearchResult::Workspaces(payload) => {
                Self::Workspaces(payload.into_iter().map(|item| item.into()).collect())
            }
            crate::research::ResearchResult::Prompt(payload) => Self::Prompt(payload),
            crate::research::ResearchResult::Report(payload) => Self::Report(payload),
            crate::research::ResearchResult::Index(payload) => Self::Index(payload.into()),
            crate::research::ResearchResult::Round(payload) => Self::Round(payload.into()),
            crate::research::ResearchResult::Artifact(payload) => Self::Artifact(payload.into()),
            crate::research::ResearchResult::Source(payload) => Self::Source(payload.into()),
        }
    }
}

impl TryFrom<host::CommandResult> for crate::commands::CommandResult {
    type Error = host::CommandError;
    fn try_from(value: host::CommandResult) -> Result<Self, Self::Error> {
        Ok(match value {
            host::CommandResult::Research(payload) => Self::Research(payload.try_into()?),
            host::CommandResult::CodexProject(payload) => Self::CodexProject(
                payload
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .transpose()?,
            ),
            host::CommandResult::LanTicket(payload) => Self::LanTicket {
                ticket: payload.ticket,
            },
            host::CommandResult::LanStatus(payload) => Self::LanStatus(payload.try_into()?),
            host::CommandResult::Accepted(payload) => Self::Accepted {
                request_id: parse_uuid(payload.request_id)?,
            },
            host::CommandResult::Note(payload) => Self::Note(payload.try_into()?),
            host::CommandResult::Notes(payload) => Self::Notes(
                payload
                    .into_iter()
                    .map(|item| -> Result<_, host::CommandError> { Ok(item.try_into()?) })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
        })
    }
}
impl TryFrom<crate::commands::CommandResult> for host::CommandResult {
    type Error = host::CommandError;
    fn try_from(value: crate::commands::CommandResult) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::commands::CommandResult::Plugin(_) => {
                return Err(host::CommandError::Unauthorized);
            }
            crate::commands::CommandResult::Research(payload) => Self::Research(payload.into()),
            crate::commands::CommandResult::CodexProject(payload) => {
                Self::CodexProject(payload.map(|item| item.into()))
            }
            crate::commands::CommandResult::LanTicket { ticket } => {
                Self::LanTicket(host::CommandResultLanTicket { ticket: ticket })
            }
            crate::commands::CommandResult::LanStatus(payload) => Self::LanStatus(payload.into()),
            crate::commands::CommandResult::Accepted { request_id } => {
                Self::Accepted(host::CommandResultAccepted {
                    request_id: request_id.to_string(),
                })
            }
            crate::commands::CommandResult::Note(payload) => Self::Note(payload.into()),
            crate::commands::CommandResult::Notes(payload) => {
                Self::Notes(payload.into_iter().map(|item| item.into()).collect())
            }
        })
    }
}

/// Decode a component request without bypassing shared domain validation.
pub fn decode_command(
    command: host::Command,
) -> Result<crate::commands::Command, host::CommandError> {
    command.try_into()
}

/// Map a native command failure to the stable typed extension error.
pub fn encode_error(error: crate::commands::CommandError) -> host::CommandError {
    use crate::commands::CommandError as Native;
    match error {
        Native::InvalidResearch(message) => host::CommandError::InvalidResearch(message),
        Native::CodexProjectChanged => host::CommandError::CodexProjectChanged,
        Native::InvalidCodexProject(message) => host::CommandError::InvalidCodexProject(message),
        Native::Unauthorized => host::CommandError::Unauthorized,
        Native::Busy => host::CommandError::Busy,
        Native::NotFound => host::CommandError::NotFound,
        Native::StaleRevision { current } => {
            host::CommandError::StaleRevision(current.0.to_string())
        }
        Native::InvalidPath => host::CommandError::InvalidPath,
        Native::PathCollision => host::CommandError::PathCollision,
        Native::InvalidEdits => host::CommandError::InvalidEdits,
        Native::Trashed => host::CommandError::Trashed,
        Native::Unavailable(message) => host::CommandError::Unavailable(message),
    }
}

/// Convert a successfully dispatched domain result to its typed component form.
pub fn encode_result(
    result: crate::commands::CommandResult,
) -> Result<host::CommandResult, host::CommandError> {
    result.try_into()
}

/// A run-owned result retained only if it fits the configured byte budget.
pub struct RetainedResult {
    value: crate::commands::CommandResult,
}

/// Count serialized bytes without allocating a second copy of the result.
pub fn result_fits(result: &crate::commands::CommandResult, max_bytes: usize) -> bool {
    struct Counter {
        bytes: usize,
        limit: usize,
    }
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.limit.saturating_sub(self.bytes) {
                return Err(std::io::Error::other("result limit exceeded"));
            }
            self.bytes += bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(
        Counter {
            bytes: 0,
            limit: max_bytes,
        },
        result,
    )
    .is_ok()
}

impl RetainedResult {
    /// Retain one result within the serialized byte budget.
    pub fn new(
        result: crate::commands::CommandResult,
        max_bytes: usize,
    ) -> Result<Self, host::CommandError> {
        if !result_fits(&result, max_bytes) {
            return Err(host::CommandError::Unavailable(
                "result exceeds the retained-result limit".into(),
            ));
        }
        Ok(Self { value: result })
    }
    /// Read one typed node; strings and field names are returned in pages.
    pub fn read(
        &self,
        path: Vec<host::PathElement>,
        offset: u64,
        limit: u32,
    ) -> Result<host::ResultNode, host::CommandError> {
        project_result(&self.value, &path, offset, limit)
    }
}

/// Read-only replay information for a result too large to retain. Only `Query`
/// can be executed again; `Note` needs a separately authorized note read and
/// preserves the original save state. Neither permits replaying a mutation.
#[derive(Clone, Debug)]
pub enum CursorSource {
    Query(crate::commands::Command),
    Note {
        id: crate::notes::NoteId,
        revision: crate::notes::Revision,
        save: crate::notes::SaveState,
    },
}

/// The owner binds this cursor to a run, its original authority, and an expiry.
#[derive(Clone, Debug)]
pub struct QueryCursor {
    pub source: CursorSource,
    fingerprint: Option<[u8; 32]>,
}

impl QueryCursor {
    /// Construct a non-replaying cursor for a result, when a safe source exists.
    pub fn new(
        command: &crate::commands::Command,
        result: &crate::commands::CommandResult,
    ) -> Option<Self> {
        use crate::commands::{Command, CommandResult};
        use crate::research::ResearchCommand as R;
        let source = match (command, result) {
            (
                Command::CreateNote { .. }
                | Command::ReadNote { .. }
                | Command::EditNote { .. }
                | Command::RenameNote { .. }
                | Command::TrashNote { .. }
                | Command::RestoreNote { .. },
                CommandResult::Note(note),
            ) => CursorSource::Note {
                id: note.note.id,
                revision: note.note.revision,
                save: note.save.clone(),
            },
            (
                Command::GetCodexProject
                | Command::GetLanTicket
                | Command::GetLanStatus
                | Command::SearchNotes { .. },
                _,
            ) => CursorSource::Query(command.clone()),
            (
                Command::Research(
                    R::List
                    | R::FindPassages { .. }
                    | R::EvidenceGaps { .. }
                    | R::Read { .. }
                    | R::ReadIndex { .. }
                    | R::ReadRound { .. }
                    | R::ReadSource { .. }
                    | R::ReadArtifact { .. }
                    | R::BuildPrompt { .. }
                    | R::BuildReport { .. }
                    | R::BuildContext { .. }
                    | R::Discover { .. },
                ),
                _,
            ) => CursorSource::Query(command.clone()),
            _ => return None,
        };
        let fingerprint = match &source {
            CursorSource::Note { .. } => None,
            CursorSource::Query(_) => Some(result_fingerprint(result).ok()?),
        };
        Some(Self {
            source,
            fingerprint,
        })
    }

    /// Verify a fresh query result and restore the original note save state.
    /// A changed or replaced source never yields pieces from mixed generations.
    pub fn verify(
        &self,
        mut result: crate::commands::CommandResult,
    ) -> Result<crate::commands::CommandResult, host::CommandError> {
        if let CursorSource::Note { id, revision, save } = &self.source {
            match &mut result {
                crate::commands::CommandResult::Note(note)
                    if note.note.id == *id && note.note.revision == *revision =>
                {
                    note.save = save.clone()
                }
                _ => return Err(host::CommandError::ResultChanged),
            }
        }
        if let Some(fingerprint) = self.fingerprint
            && result_fingerprint(&result).ok() != Some(fingerprint)
        {
            return Err(host::CommandError::ResultChanged);
        }
        Ok(result)
    }
}

fn result_fingerprint(
    result: &crate::commands::CommandResult,
) -> Result<[u8; 32], serde_json::Error> {
    use sha2::{Digest, Sha256};
    struct HashWriter(Sha256);
    impl std::io::Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Sha256::new());
    serde_json::to_writer(&mut writer, result)?;
    Ok(writer.0.finalize().into())
}

/// Project one node from a result without building or retaining a second copy.
/// Paths use the shared Serde record and variant names. Offsets are UTF-8 bytes
/// for text and field indices for records. List elements use `Index` steps.
pub fn project_result(
    result: &crate::commands::CommandResult,
    path: &[host::PathElement],
    offset: u64,
    limit: u32,
) -> Result<host::ResultNode, host::CommandError> {
    use serde::Serialize;
    if path.len() > 64
        || limit == 0
        || limit > 32_768
        || path
            .iter()
            .any(|p| matches!(p, host::PathElement::Field(name) if name.len() > 256))
    {
        return Err(host::CommandError::InvalidArgument(
            "invalid result path or page".into(),
        ));
    }
    let offset = usize::try_from(offset)
        .map_err(|_| host::CommandError::InvalidArgument("invalid result page offset".into()))?;
    result
        .serialize(Projector {
            path,
            offset,
            limit: limit as usize,
        })
        .map_err(|_| host::CommandError::InvalidArgument("invalid result path or page".into()))
}

#[derive(Debug)]
struct ProjectionError;
impl std::fmt::Display for ProjectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("invalid result path or page")
    }
}
impl std::error::Error for ProjectionError {}
impl serde::ser::Error for ProjectionError {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self
    }
}

type Projected = Result<host::ResultNode, ProjectionError>;
#[derive(Clone, Copy)]
struct Projector<'a> {
    path: &'a [host::PathElement],
    offset: usize,
    limit: usize,
}
impl<'a> Projector<'a> {
    fn child(self) -> Self {
        Self {
            path: &self.path[1..],
            ..self
        }
    }
    fn scalar(self, value: host::ResultNode) -> Projected {
        if self.path.is_empty() && self.offset == 0 {
            Ok(value)
        } else {
            Err(ProjectionError)
        }
    }
    fn text(self, value: &str) -> Projected {
        if !self.path.is_empty()
            || self.offset > value.len()
            || !value.is_char_boundary(self.offset)
        {
            return Err(ProjectionError);
        }
        let mut end = self.offset.saturating_add(self.limit).min(value.len());
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        if end == self.offset && end < value.len() {
            return Err(ProjectionError);
        }
        Ok(host::ResultNode::Text(host::TextPart {
            text: value[self.offset..end].to_owned(),
            total_bytes: value.len() as u64,
            next_offset: (end < value.len()).then_some(end as u64),
        }))
    }
    fn variant<T: ?Sized + serde::Serialize>(self, name: &'static str, value: &T) -> Projected {
        if self.path.is_empty() {
            let mut fields = ObjectProjector::new(self);
            fields.field(name, value)?;
            fields.finish()
        } else {
            match &self.path[0] {
                host::PathElement::Field(field) if field == name => value.serialize(self.child()),
                _ => Err(ProjectionError),
            }
        }
    }
}

struct SequenceProjector<'a> {
    projector: Projector<'a>,
    count: usize,
    selected: Option<host::ResultNode>,
}
impl<'a> SequenceProjector<'a> {
    fn new(projector: Projector<'a>) -> Self {
        Self {
            projector,
            count: 0,
            selected: None,
        }
    }
    fn element<T: ?Sized + serde::Serialize>(&mut self, value: &T) -> Result<(), ProjectionError> {
        if matches!(self.projector.path.first(), Some(host::PathElement::Index(index)) if *index == self.count as u64)
        {
            self.selected = Some(value.serialize(self.projector.child())?);
        }
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Projected {
        if self.projector.path.is_empty() {
            self.projector
                .scalar(host::ResultNode::ListLength(self.count as u64))
        } else {
            self.selected.ok_or(ProjectionError)
        }
    }
}
macro_rules! sequence_impl {
    ($trait:ident, $method:ident) => {
        impl<'a> serde::ser::$trait for SequenceProjector<'a> {
            type Ok = host::ResultNode;
            type Error = ProjectionError;
            fn $method<T: ?Sized + serde::Serialize>(
                &mut self,
                value: &T,
            ) -> Result<(), Self::Error> {
                self.element(value)
            }
            fn end(self) -> Projected {
                self.finish()
            }
        }
    };
}
sequence_impl!(SerializeSeq, serialize_element);
sequence_impl!(SerializeTuple, serialize_element);
sequence_impl!(SerializeTupleStruct, serialize_field);

struct ObjectProjector<'a> {
    projector: Projector<'a>,
    count: usize,
    used: usize,
    full: bool,
    hide_authority: bool,
    names: Vec<String>,
    selected: Option<host::ResultNode>,
    pending: Option<String>,
}
impl<'a> ObjectProjector<'a> {
    fn new(projector: Projector<'a>) -> Self {
        Self {
            projector,
            count: 0,
            used: 0,
            full: false,
            hide_authority: false,
            names: Vec::new(),
            selected: None,
            pending: None,
        }
    }
    fn field<T: ?Sized + serde::Serialize>(
        &mut self,
        name: &str,
        value: &T,
    ) -> Result<(), ProjectionError> {
        if self.hide_authority && name == "authority" {
            return Ok(());
        }
        if self.projector.path.is_empty() {
            if self.count >= self.projector.offset {
                let bytes = name.len().saturating_add(8);
                if !self.full && self.used.saturating_add(bytes) <= self.projector.limit {
                    self.names.push(name.to_owned());
                    self.used += bytes;
                } else {
                    self.full = true;
                }
            }
        } else if matches!(&self.projector.path[0], host::PathElement::Field(field) if field == name)
        {
            self.selected = Some(value.serialize(self.projector.child())?);
        }
        self.count += 1;
        Ok(())
    }
    fn finish(self) -> Projected {
        if !self.projector.path.is_empty() {
            return self.selected.ok_or(ProjectionError);
        }
        if self.projector.offset > self.count
            || (self.names.is_empty() && self.projector.offset < self.count)
        {
            return Err(ProjectionError);
        }
        let next = self.projector.offset + self.names.len();
        Ok(host::ResultNode::Fields(host::FieldPart {
            names: self.names,
            total_fields: self.count as u64,
            next_offset: (next < self.count).then_some(next as u64),
        }))
    }
}
impl<'a> serde::ser::SerializeStruct for ObjectProjector<'a> {
    type Ok = host::ResultNode;
    type Error = ProjectionError;
    fn serialize_field<T: ?Sized + serde::Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        self.field(key, value)
    }
    fn end(self) -> Projected {
        self.finish()
    }
}
impl<'a> serde::ser::SerializeMap for ObjectProjector<'a> {
    type Ok = host::ResultNode;
    type Error = ProjectionError;
    fn serialize_key<T: ?Sized + serde::Serialize>(&mut self, key: &T) -> Result<(), Self::Error> {
        self.pending = match serde_json::to_value(key).map_err(|_| ProjectionError)? {
            serde_json::Value::String(key) => Some(key),
            _ => return Err(ProjectionError),
        };
        Ok(())
    }
    fn serialize_value<T: ?Sized + serde::Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), Self::Error> {
        let key = self.pending.take().ok_or(ProjectionError)?;
        self.field(&key, value)
    }
    fn end(self) -> Projected {
        self.finish()
    }
}

struct TupleVariantProjector<'a> {
    name: &'static str,
    parent: Projector<'a>,
    sequence: Option<SequenceProjector<'a>>,
}
impl<'a> serde::ser::SerializeTupleVariant for TupleVariantProjector<'a> {
    type Ok = host::ResultNode;
    type Error = ProjectionError;
    fn serialize_field<T: ?Sized + serde::Serialize>(
        &mut self,
        value: &T,
    ) -> Result<(), Self::Error> {
        if let Some(sequence) = &mut self.sequence {
            sequence.element(value)?;
        }
        Ok(())
    }
    fn end(self) -> Projected {
        if let Some(sequence) = self.sequence {
            sequence.finish()
        } else if self.parent.path.is_empty() {
            let mut object = ObjectProjector::new(self.parent);
            object.field(self.name, &())?;
            object.finish()
        } else {
            Err(ProjectionError)
        }
    }
}
struct StructVariantProjector<'a> {
    name: &'static str,
    parent: Projector<'a>,
    object: Option<ObjectProjector<'a>>,
}
impl<'a> serde::ser::SerializeStructVariant for StructVariantProjector<'a> {
    type Ok = host::ResultNode;
    type Error = ProjectionError;
    fn serialize_field<T: ?Sized + serde::Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        if let Some(object) = &mut self.object {
            object.field(key, value)?;
        }
        Ok(())
    }
    fn end(self) -> Projected {
        if let Some(object) = self.object {
            object.finish()
        } else if self.parent.path.is_empty() {
            let mut object = ObjectProjector::new(self.parent);
            object.field(self.name, &())?;
            object.finish()
        } else {
            Err(ProjectionError)
        }
    }
}

impl<'a> serde::Serializer for Projector<'a> {
    type Ok = host::ResultNode;
    type Error = ProjectionError;
    type SerializeSeq = SequenceProjector<'a>;
    type SerializeTuple = SequenceProjector<'a>;
    type SerializeTupleStruct = SequenceProjector<'a>;
    type SerializeTupleVariant = TupleVariantProjector<'a>;
    type SerializeMap = ObjectProjector<'a>;
    type SerializeStruct = ObjectProjector<'a>;
    type SerializeStructVariant = StructVariantProjector<'a>;
    fn serialize_bool(self, value: bool) -> Projected {
        self.scalar(host::ResultNode::Boolean(value))
    }
    fn serialize_i8(self, value: i8) -> Projected {
        self.serialize_i64(value.into())
    }
    fn serialize_i16(self, value: i16) -> Projected {
        self.serialize_i64(value.into())
    }
    fn serialize_i32(self, value: i32) -> Projected {
        self.serialize_i64(value.into())
    }
    fn serialize_i64(self, value: i64) -> Projected {
        self.scalar(host::ResultNode::Signed(value))
    }
    fn serialize_u8(self, value: u8) -> Projected {
        self.serialize_u64(value.into())
    }
    fn serialize_u16(self, value: u16) -> Projected {
        self.serialize_u64(value.into())
    }
    fn serialize_u32(self, value: u32) -> Projected {
        self.serialize_u64(value.into())
    }
    fn serialize_u64(self, value: u64) -> Projected {
        self.scalar(host::ResultNode::Unsigned(value))
    }
    fn serialize_f32(self, value: f32) -> Projected {
        self.serialize_f64(value.into())
    }
    fn serialize_f64(self, value: f64) -> Projected {
        self.scalar(host::ResultNode::Float(value))
    }
    fn serialize_char(self, value: char) -> Projected {
        self.text(&value.to_string())
    }
    fn serialize_str(self, value: &str) -> Projected {
        self.text(value)
    }
    fn serialize_bytes(self, _: &[u8]) -> Projected {
        Err(ProjectionError)
    }
    fn serialize_none(self) -> Projected {
        self.scalar(host::ResultNode::Null)
    }
    fn serialize_some<T: ?Sized + serde::Serialize>(self, value: &T) -> Projected {
        value.serialize(self)
    }
    fn serialize_unit(self) -> Projected {
        self.scalar(host::ResultNode::Null)
    }
    fn serialize_unit_struct(self, _: &'static str) -> Projected {
        self.serialize_unit()
    }
    fn serialize_unit_variant(self, _: &'static str, _: u32, variant: &'static str) -> Projected {
        self.text(variant)
    }
    fn serialize_newtype_struct<T: ?Sized + serde::Serialize>(
        self,
        _: &'static str,
        value: &T,
    ) -> Projected {
        value.serialize(self)
    }
    fn serialize_newtype_variant<T: ?Sized + serde::Serialize>(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        value: &T,
    ) -> Projected {
        self.variant(variant, value)
    }
    fn serialize_seq(self, _: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SequenceProjector::new(self))
    }
    fn serialize_tuple(self, _: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(SequenceProjector::new(self))
    }
    fn serialize_tuple_struct(
        self,
        _: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(SequenceProjector::new(self))
    }
    fn serialize_tuple_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        let sequence = if matches!(self.path.first(), Some(host::PathElement::Field(name)) if name == variant)
        {
            Some(SequenceProjector::new(self.child()))
        } else {
            None
        };
        Ok(TupleVariantProjector {
            name: variant,
            parent: self,
            sequence,
        })
    }
    fn serialize_map(self, _: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(ObjectProjector::new(self))
    }
    fn serialize_struct(
        self,
        name: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        let mut object = ObjectProjector::new(self);
        object.hide_authority = name == "Acquisition";
        Ok(object)
    }
    fn serialize_struct_variant(
        self,
        _: &'static str,
        _: u32,
        variant: &'static str,
        _: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let object = if matches!(self.path.first(), Some(host::PathElement::Field(name)) if name == variant)
        {
            Some(ObjectProjector::new(self.child()))
        } else {
            None
        };
        Ok(StructVariantProjector {
            name: variant,
            parent: self,
            object,
        })
    }
}
