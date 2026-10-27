wit_bindgen::generate!({
    path: "../../../wit/v2",
    world: "compiled-extension",
});

use tanglewood::extensions::host::{self, Command, CommandResult, TextEdit};

struct NoteWorkflow;

fn note(command: Command) -> Result<host::NoteView, String> {
    let receipt = host::execute(&command).map_err(|error| format!("command failed: {error:?}"))?;
    match receipt.value {
        CommandResult::Note(note) => Ok(note),
        other => Err(format!("expected note result, got {other:?}")),
    }
}

impl exports::tanglewood::extensions::compiled::Guest for NoteWorkflow {
    fn run(input: String) -> Result<String, String> {
        let created = note(Command::CreateNote(host::CommandCreateNote {
            path: input,
            text: "Compiled extension".into(),
        }))?;
        let id = created.note.id;
        let edited = note(Command::EditNote(host::CommandEditNote {
            id: id.clone(),
            expected: created.note.revision,
            edits: vec![TextEdit {
                start: 18,
                end: 18,
                replacement: " edited".into(),
            }],
        }))?;
        let read = note(Command::ReadNote(host::CommandReadNote { id }))?;
        if read.note.revision != edited.note.revision {
            return Err("the note changed before it could be read".into());
        }
        Ok(format!("{}:{}", read.note.id, read.note.text))
    }
}

export!(NoteWorkflow);
