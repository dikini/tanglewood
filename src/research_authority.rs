//! Persisted authority for plugin-created research acquisition jobs.
//!
//! This metadata is never a guest-supplied command. The owner records the
//! originating principal and every complete rule that must remain satisfied.
use crate::plugin_permissions::{Guard, Origin};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResearchAuthority {
    pub plugin_id: String,
    pub component_sha256: String,
    pub origin: Origin,
    /// Intersect complete guards; never combine fields of different rules.
    pub guards: Vec<Guard>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin_permissions::{Action, Rule, Scope};
    use crate::research::{Acquisition, AcquisitionStatus, ArtifactInput};
    use uuid::Uuid;

    #[test]
    fn acquisition_authority_survives_serde_and_legacy_records_load() {
        let rule = Rule {
            action: Action::ResearchSavePaper,
            notes: None,
            workspaces: Some(Scope::All),
            note_paths: None,
            filesystem_roots: None,
            url_origins: Some(Scope::Only(vec!["https://example.org".into()])),
            administrative_targets: None,
            input_kinds: None,
            human: None,
        };
        let original = Acquisition {
            id: Uuid::new_v4(),
            source_id: Uuid::new_v4(),
            input: ArtifactInput::Url {
                url: "https://example.org/paper.pdf".into(),
                archive: true,
            },
            ocr: false,
            status: AcquisitionStatus::Queued,
            error: None,
            authority: Some(ResearchAuthority {
                plugin_id: "reader".into(),
                component_sha256: "a".repeat(64),
                origin: Origin::Local,
                guards: vec![Guard {
                    rule,
                    origin: Origin::Local,
                }],
            }),
        };
        let encoded = serde_json::to_value(&original).unwrap();
        let decoded: Acquisition = serde_json::from_value(encoded.clone()).unwrap();
        assert_eq!(decoded.authority.unwrap().guards.len(), 1);
        let mut legacy = encoded;
        legacy.as_object_mut().unwrap().remove("authority");
        assert!(
            serde_json::from_value::<Acquisition>(legacy)
                .unwrap()
                .authority
                .is_none()
        );

        #[cfg(feature = "wasm-extensions")]
        {
            use crate::commands::CommandResult;
            use crate::notes::Revision;
            use crate::research::{ResearchResult, ResearchWorkspace};
            use crate::wasm_commands::{host, project_result};
            let workspace = ResearchWorkspace {
                id: Uuid::new_v4(),
                revision: Revision(Uuid::new_v4()),
                title: "test".into(),
                question: String::new(),
                sources: vec![],
                research_notes: vec![],
                brief: Default::default(),
                threads: vec![],
                rounds: vec![],
                artifacts: vec![],
                acquisitions: vec![original],
                assessment_history: vec![],
            };
            let result = CommandResult::Research(ResearchResult::Workspace(workspace));
            let path = vec![
                host::PathElement::Field("Research".into()),
                host::PathElement::Field("Workspace".into()),
                host::PathElement::Field("acquisitions".into()),
                host::PathElement::Index(0),
            ];
            let node = project_result(&result, &path, 0, 32768).unwrap();
            match node {
                host::ResultNode::Fields(fields) => {
                    assert!(!fields.names.contains(&"authority".to_owned()))
                }
                other => panic!("unexpected node: {other:?}"),
            }
            let mut hidden = path;
            hidden.push(host::PathElement::Field("authority".into()));
            assert!(project_result(&result, &hidden, 0, 32768).is_err());
        }
    }
}
