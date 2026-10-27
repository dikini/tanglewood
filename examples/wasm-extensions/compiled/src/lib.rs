wit_bindgen::generate!({
    path: "../../../wit",
    world: "compiled-extension",
});

use tanglewood::extensions::host::{self, LogLevel};

struct NoteSummary;

impl exports::tanglewood::extensions::compiled::Guest for NoteSummary {
    fn run(input: String) -> Result<String, String> {
        host::log(LogLevel::Info, "summarizing the selected note")
            .map_err(|error| format!("logging failed: {error:?}"))?;
        let note = host::read_note().map_err(|error| format!("reading note failed: {error:?}"))?;
        let words = note.text.split_whitespace().count();
        Ok(format!("{input}{}: {words} words", note.path))
    }
}

export!(NoteSummary);
