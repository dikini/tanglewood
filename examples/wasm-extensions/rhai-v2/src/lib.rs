wit_bindgen::generate!({
    path: "../../../wit/v2",
    world: "interpreter-extension",
    additional_derives: [serde::Serialize, serde::Deserialize],
});

use rhai::{Array, Dynamic, Engine, EvalAltResult, Map, Position, Scope};
use tanglewood::extensions::host::{self, Command, CommandResult, TextEdit};

struct RhaiInterpreter;

fn failure(message: impl Into<String>) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(message.into().into(), Position::NONE).into()
}

fn execute(command: Command) -> Result<host::Receipt, Box<EvalAltResult>> {
    host::execute(&command).map_err(|error| failure(format!("command failed: {error:?}")))
}

fn note(command: Command) -> Result<Map, Box<EvalAltResult>> {
    match execute(command)?.value {
        CommandResult::Note(note) => {
            let mut result = Map::new();
            result.insert("id".into(), note.note.id.into());
            result.insert("path".into(), note.note.path.into());
            result.insert("revision".into(), note.note.revision.into());
            result.insert("text".into(), note.note.text.into());
            result.insert("trashed".into(), note.note.trashed.into());
            result.insert("accepted".into(), note.save.accepted.into());
            if let Some(persisted) = note.save.persisted {
                result.insert("persisted".into(), persisted.into());
            }
            if let Some(error) = note.save.error {
                result.insert("save_error".into(), error.into());
            }
            Ok(result)
        }
        other => Err(failure(format!("expected note result, got {other:?}"))),
    }
}

fn scope_value(scope: Option<host::Scope>) -> Dynamic {
    match scope {
        None => Dynamic::UNIT,
        Some(host::Scope::All) => "all".into(),
        Some(host::Scope::Only(values)) => values
            .into_iter()
            .map(Dynamic::from)
            .collect::<Array>()
            .into(),
    }
}

fn capabilities() -> Array {
    host::capabilities()
        .into_iter()
        .map(|rule| {
            let mut item = Map::new();
            item.insert("name".into(), rule.name.into());
            item.insert("notes".into(), scope_value(rule.notes));
            item.insert("workspaces".into(), scope_value(rule.workspaces));
            item.insert("note_paths".into(), scope_value(rule.note_paths));
            item.insert(
                "filesystem_roots".into(),
                scope_value(rule.filesystem_roots),
            );
            item.insert("url_origins".into(), scope_value(rule.url_origins));
            item.insert(
                "administrative_targets".into(),
                scope_value(rule.administrative_targets),
            );
            let kinds = rule
                .input_kinds
                .map(|kinds| {
                    kinds
                        .into_iter()
                        .map(|kind| {
                            match kind {
                                host::InputKind::LocalFile => "local-file",
                                host::InputKind::UrlArchive => "url-archive",
                                host::InputKind::UrlReference => "url-reference",
                                host::InputKind::Generated => "generated",
                                host::InputKind::Markdown => "markdown",
                            }
                            .into()
                        })
                        .collect::<Array>()
                        .into()
                })
                .unwrap_or(Dynamic::UNIT);
            item.insert("input_kinds".into(), kinds);
            item.insert(
                "human".into(),
                rule.human.map(Dynamic::from).unwrap_or(Dynamic::UNIT),
            );
            Dynamic::from(item)
        })
        .collect()
}

impl exports::tanglewood::extensions::interpreter::Guest for RhaiInterpreter {
    fn evaluate(source: String, input: String) -> Result<String, String> {
        let mut engine = Engine::new();
        engine.register_fn("capabilities", capabilities);
        // Scripts construct a tagged, typed WIT command as a Rhai value. No
        // serialized RPC payload crosses the host boundary.
        engine.register_fn(
            "command",
            |value: Dynamic| -> Result<Dynamic, Box<EvalAltResult>> {
                let command: Command = rhai::serde::from_dynamic(&value)?;
                rhai::serde::to_dynamic(host::execute(&command))
            },
        );
        engine.register_fn(
            "read_result",
            |token: &str,
             path: Dynamic,
             offset: i64,
             limit: i64|
             -> Result<Dynamic, Box<EvalAltResult>> {
                let path: Vec<host::PathElement> = rhai::serde::from_dynamic(&path)?;
                let offset = u64::try_from(offset).map_err(|_| failure("negative offset"))?;
                let limit = u32::try_from(limit).map_err(|_| failure("invalid limit"))?;
                rhai::serde::to_dynamic(host::read_result(token, &path, offset, limit))
            },
        );
        engine.register_fn("log", |message: &str| -> Result<(), Box<EvalAltResult>> {
            host::log(host::LogLevel::Info, message)
                .map_err(|error| failure(format!("logging failed: {error:?}")))
        });
        engine.register_fn("create_note", |path: &str, text: &str| {
            note(Command::CreateNote(host::CommandCreateNote {
                path: path.into(),
                text: text.into(),
            }))
        });
        engine.register_fn("read_note", |id: &str| {
            note(Command::ReadNote(host::CommandReadNote { id: id.into() }))
        });
        engine.register_fn(
            "edit_note",
            |id: &str, revision: &str, start: i64, end: i64, replacement: &str| {
                let start = u64::try_from(start).map_err(|_| failure("negative start offset"))?;
                let end = u64::try_from(end).map_err(|_| failure("negative end offset"))?;
                note(Command::EditNote(host::CommandEditNote {
                    id: id.into(),
                    expected: revision.into(),
                    edits: vec![TextEdit {
                        start,
                        end,
                        replacement: replacement.into(),
                    }],
                }))
            },
        );
        engine.register_fn(
            "set_automation_write_enabled",
            |enabled: bool| -> Result<String, Box<EvalAltResult>> {
                let receipt = execute(Command::SetAutomationWriteEnabled(
                    host::CommandSetAutomationWriteEnabled { enabled },
                ))?;
                match receipt.value {
                    CommandResult::Accepted(accepted) => Ok(accepted.request_id),
                    other => Err(failure(format!("expected receipt, got {other:?}"))),
                }
            },
        );
        let mut scope = Scope::new();
        scope.push("input", input);
        engine
            .eval_with_scope::<Dynamic>(&mut scope, &source)
            .map(|value| value.to_string())
            .map_err(|error| error.to_string())
    }
}

export!(RhaiInterpreter);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generic_command_accepts_nested_research_variants() {
        let request = Engine::new()
            .eval::<Dynamic>(r#"#{ Research: #{ BuildReport: #{ id: "11111111-1111-4111-8111-111111111111" } } }"#)
            .expect("Rhai request");
        let command: Command = rhai::serde::from_dynamic(&request).expect("typed WIT command");
        assert!(matches!(
            command,
            Command::Research(host::ResearchCommand::BuildReport(_))
        ));
    }

    #[test]
    fn generic_command_preserves_too_large_token() {
        let reply: Result<host::Receipt, host::CommandError> =
            Err(host::CommandError::TooLarge(host::Oversized {
                token: "cursor".into(),
                completion: host::Completion::Pending,
            }));
        let value = rhai::serde::to_dynamic(reply).expect("typed error as Rhai value");
        let mut scope = Scope::new();
        scope.push_dynamic("reply", value);
        let token = Engine::new()
            .eval_with_scope::<String>(&mut scope, "reply.Err.TooLarge.token")
            .expect("token remains structured");
        assert_eq!(token, "cursor");
    }
}
