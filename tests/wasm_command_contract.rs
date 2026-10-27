#![cfg(feature = "wasm-extensions")]

use tanglewood::wasm_commands::{bindings::tanglewood::extensions::host, decode_command};
use tanglewood::{
    commands::Command,
    notes::{NoteId, Revision},
};
use uuid::Uuid;

#[test]
fn typed_note_edit_preserves_revision_and_utf8_ranges() {
    let id = Uuid::new_v4();
    let revision = Uuid::new_v4();
    let wire = host::Command::EditNote(host::CommandEditNote {
        id: id.to_string(),
        expected: revision.to_string(),
        edits: vec![host::TextEdit {
            start: 1,
            end: 5,
            replacement: "🦀".into(),
        }],
    });
    match decode_command(wire).expect("typed command") {
        Command::EditNote {
            id: actual,
            expected,
            edits,
        } => {
            assert_eq!(actual, NoteId(id));
            assert_eq!(expected, Revision(revision));
            assert_eq!(
                (edits[0].start, edits[0].end, &edits[0].replacement),
                (1, 5, &"🦀".to_string())
            );
        }
        other => panic!("unexpected command: {other:?}"),
    }
}

#[test]
fn invalid_uuid_is_reported_instead_of_falling_through() {
    let wire = host::Command::ReadNote(host::CommandReadNote {
        id: "no uuid".into(),
    });
    assert!(matches!(
        decode_command(wire),
        Err(host::CommandError::InvalidArgument(_))
    ));
}

#[test]
fn oversized_result_text_is_retrieved_in_utf8_safe_parts() {
    use tanglewood::notes::{NoteRecord, NoteView, SaveState};
    use tanglewood::wasm_commands::RetainedResult;
    let id = NoteId(Uuid::new_v4());
    let revision = Revision(Uuid::new_v4());
    let result = tanglewood::commands::CommandResult::Note(NoteView {
        note: NoteRecord {
            id,
            path: "page.md".into(),
            text: "abc🦀!".into(),
            revision,
            trashed: false,
        },
        save: SaveState {
            accepted: revision,
            persisted: None,
            error: None,
        },
    });
    let result = RetainedResult::new(result, 1024).expect("retain result");
    let path = || {
        vec![
            host::PathElement::Field("Note".into()),
            host::PathElement::Field("note".into()),
            host::PathElement::Field("text".into()),
        ]
    };
    match result.read(path(), 0, 5).expect("first part") {
        host::ResultNode::Text(part) => {
            assert_eq!(part.text, "abc");
            assert_eq!(part.next_offset, Some(3));
        }
        other => panic!("unexpected node: {other:?}"),
    }
    match result.read(path(), 3, 4).expect("second part") {
        host::ResultNode::Text(part) => {
            assert_eq!(part.text, "🦀");
            assert_eq!(part.next_offset, Some(7));
        }
        other => panic!("unexpected node: {other:?}"),
    }
    assert!(matches!(
        result.read(path(), 4, 4),
        Err(host::CommandError::InvalidArgument(_))
    ));
}

#[test]
fn large_results_can_be_projected_without_retaining_them() {
    use tanglewood::notes::{NoteRecord, NoteView, SaveState};
    let id = NoteId(Uuid::new_v4());
    let revision = Revision(Uuid::new_v4());
    let result = tanglewood::commands::CommandResult::Note(NoteView {
        note: NoteRecord {
            id,
            path: "large.md".into(),
            text: "é".repeat(200_000),
            revision,
            trashed: false,
        },
        save: SaveState {
            accepted: revision,
            persisted: None,
            error: None,
        },
    });
    let path = vec![
        host::PathElement::Field("Note".into()),
        host::PathElement::Field("note".into()),
        host::PathElement::Field("text".into()),
    ];
    match tanglewood::wasm_commands::project_result(&result, &path, 399_996, 4).expect("project") {
        host::ResultNode::Text(part) => {
            assert_eq!(part.text, "éé");
            assert_eq!(part.next_offset, None);
        }
        other => panic!("unexpected node: {other:?}"),
    }
}

#[test]
fn cursor_detects_changed_results_without_replaying_mutation() {
    use tanglewood::notes::{NoteRecord, NoteView, SaveState};
    use tanglewood::wasm_commands::{CursorSource, QueryCursor};
    let id = NoteId(Uuid::new_v4());
    let revision = Revision(Uuid::new_v4());
    let save = SaveState {
        accepted: revision,
        persisted: None,
        error: None,
    };
    let result = tanglewood::commands::CommandResult::Note(NoteView {
        note: NoteRecord {
            id,
            path: "page.md".into(),
            text: "large".into(),
            revision,
            trashed: false,
        },
        save: save.clone(),
    });
    let cursor = QueryCursor::new(
        &Command::EditNote {
            id,
            expected: revision,
            edits: vec![],
        },
        &result,
    )
    .expect("cursor");
    assert!(matches!(cursor.source, CursorSource::Note { .. }));
    let mut later = match result {
        tanglewood::commands::CommandResult::Note(note) => note,
        _ => unreachable!(),
    };
    later.save.persisted = Some(revision);
    assert!(
        cursor
            .verify(tanglewood::commands::CommandResult::Note(later.clone()))
            .is_ok()
    );
    later.note.revision = Revision(Uuid::new_v4());
    assert!(matches!(
        cursor.verify(tanglewood::commands::CommandResult::Note(later)),
        Err(host::CommandError::ResultChanged)
    ));
}

#[test]
fn command_inventory_round_trips_every_effective_action() {
    let cases = [
        r#"{"Research":{"ImportSources":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","paths":[]}}}"#,
        r#"{"Research":{"AcquireSources":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_ids":[]}}}"#,
        r#"{"Research":{"RetryAcquisitions":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","job_ids":[]}}}"#,
        r#"{"Research":{"CollectReferences":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","candidates":[],"thread":null,"turn_id":null}}}"#,
        r#"{"Research":{"ImportSource":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","path":"sample"}}}"#,
        r#"{"Research":{"AcquireSource":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111","input":{"LocalFile":{"path":"sample"}},"ocr":false}}}"#,
        r#"{"Research":{"RetryAcquisition":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","job_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"CancelAcquisition":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","job_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"CreateOutput":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","spec":{"source_urls":{},"title":"sample","kind":"Slides","cards":[]}}}}"#,
        r#"{"Research":{"SetArtifactRole":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","artifact_id":"11111111-1111-4111-8111-111111111111","role":"Unclassified"}}}"#,
        r#"{"Research":{"FindPassages":{"id":"11111111-1111-4111-8111-111111111111","query":"sample","limit":1}}}"#,
        r#"{"Research":{"EvidenceGaps":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"Create":{"title":"sample","question":"sample"}}}"#,
        r#"{"Research":"List"}"#,
        r#"{"Research":{"Read":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"ReadIndex":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"ReadRound":{"id":"11111111-1111-4111-8111-111111111111","round_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"ReadSource":{"id":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111","page":1,"offset":1,"limit":1}}}"#,
        r#"{"Research":{"SavePaper":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111","input":{"LocalFile":{"path":"sample"}},"ocr":false}}}"#,
        r#"{"Research":{"ReadArtifact":{"id":"11111111-1111-4111-8111-111111111111","artifact_id":"11111111-1111-4111-8111-111111111111","offset":1,"limit":1}}}"#,
        r#"{"Research":{"Update":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","title":"sample","question":"sample"}}}"#,
        r#"{"Research":{"AddNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","note_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"AddImported":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source":{"title":"sample","kind":"Note","metadata":{"author":null,"date":null,"url":null,"origin_label":null},"pages":[]}}}}"#,
        r#"{"Research":{"RemoveSource":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"SelectSource":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111","selected":false}}}"#,
        r#"{"Research":{"AssessSource":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","source_id":"11111111-1111-4111-8111-111111111111","human":false,"assessment":{"relevance":"sample","quality":"sample","limitations":"sample"}}}}"#,
        r#"{"Research":{"LinkNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","note_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"BuildPrompt":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"SetBrief":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","brief":{"purpose":"sample","scope":"sample","assumptions":"sample","open_questions":"sample"}}}}"#,
        r#"{"Research":{"AssociateThread":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","thread":{"provider":"sample","project_id":null,"thread_id":"sample","title":"sample"}}}}"#,
        r#"{"Research":{"RemoveThread":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","thread":{"provider":"sample","project_id":null,"thread_id":"sample","title":"sample"}}}}"#,
        r#"{"Research":{"AddRound":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","round":{"id":"11111111-1111-4111-8111-111111111111","captured_at":1,"question":"sample","summary":"sample","open_questions":"sample","discussion":"sample","thread":null,"turn_id":null,"artifact_ids":[]}}}}"#,
        r#"{"Research":{"CollectArtifact":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","title":"sample","input":{"LocalFile":{"path":"sample"}},"thread":null,"round_id":null}}}"#,
        r#"{"Research":{"RetryArtifact":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","receipt_id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"BuildReport":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#"{"Research":{"BuildContext":{"id":"11111111-1111-4111-8111-111111111111","mode":"Continue","scope":"ExistingOnly"}}}"#,
        r#"{"Research":{"Discover":{"id":"11111111-1111-4111-8111-111111111111"}}}"#,
        r#""GetCodexProject""#,
        r#"{"SetAutomationWriteEnabled":{"enabled":false}}"#,
        r#"{"SetCodexProject":{"expected":null,"project":{"project_id":null,"name":"sample","creation_key":"sample"}}}"#,
        r#"{"ListenLan":{"bind":"sample","advertised_ip":null}}"#,
        r#""GetLanTicket""#,
        r#"{"PrepareJoin":{"ticket":"sample"}}"#,
        r#""CancelJoin""#,
        r#"{"ConfirmPeer":{"ticket":"sample"}}"#,
        r#"{"UnpairPeer":{"fingerprint":"sample"}}"#,
        r#"{"ConnectPeer":{"fingerprint":"sample"}}"#,
        r#""GetLanStatus""#,
        r#""PauseLan""#,
        r#""ResumeLan""#,
        r#""EnableSync""#,
        r#"{"SetMirrorEnabled":{"enabled":false}}"#,
        r#"{"ImportMarkdown":{"source_path":"sample","destination":{"Create":{"path":"sample"}}}}"#,
        r#"{"ExportMarkdown":{"id":"11111111-1111-4111-8111-111111111111","path":"sample"}}"#,
        r#""RescanMirror""#,
        r#"{"CreateNote":{"path":"sample","text":"sample"}}"#,
        r#"{"ReadNote":{"id":"11111111-1111-4111-8111-111111111111"}}"#,
        r#"{"EditNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","edits":[]}}"#,
        r#"{"RenameNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111","path":"sample"}}"#,
        r#"{"TrashNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111"}}"#,
        r#"{"RestoreNote":{"id":"11111111-1111-4111-8111-111111111111","expected":"11111111-1111-4111-8111-111111111111"}}"#,
        r#"{"SearchNotes":{"query":"sample","include_trash":false}}"#,
    ];
    let mut actions = std::collections::HashSet::new();
    for encoded in cases {
        let native: Command = serde_json::from_str(encoded).expect("native inventory fixture");
        actions.insert(tanglewood::plugin_permissions::action(&native).expect("domain action"));
        let wire = host::Command::try_from(native.clone()).expect("typed command");
        let decoded = decode_command(wire).expect("typed round trip");
        assert_eq!(
            serde_json::to_value(native).unwrap(),
            serde_json::to_value(decoded).unwrap()
        );
    }
    let expected: std::collections::HashSet<_> = tanglewood::plugin_permissions::Action::ALL
        .iter()
        .copied()
        .collect();
    assert_eq!(actions, expected);
}

#[test]
fn recursive_plugin_controls_and_results_are_not_guest_commands() {
    use tanglewood::plugin_jobs::{PluginCommand, PluginResult};
    let command = Command::Plugin(PluginCommand::List);
    assert!(matches!(
        host::Command::try_from(command),
        Err(host::CommandError::Unauthorized)
    ));
    let result = tanglewood::commands::CommandResult::Plugin(PluginResult::Started {
        run_id: Uuid::new_v4(),
    });
    assert!(matches!(
        tanglewood::wasm_commands::encode_result(result),
        Err(host::CommandError::Unauthorized)
    ));
}
