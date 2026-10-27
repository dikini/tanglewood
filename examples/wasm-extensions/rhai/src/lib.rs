wit_bindgen::generate!({
    path: "../../../wit",
    world: "interpreter-extension",
});

use rhai::{Dynamic, Engine, EvalAltResult, Map, Position, Scope};
use tanglewood::extensions::host::{self, LogLevel};

struct RhaiInterpreter;

fn host_error(operation: &str, error: host::HostError) -> Box<EvalAltResult> {
    EvalAltResult::ErrorRuntime(
        format!("{operation} failed: {error:?}").into(),
        Position::NONE,
    )
    .into()
}

impl exports::tanglewood::extensions::interpreter::Guest for RhaiInterpreter {
    fn evaluate(source: String, input: String) -> Result<String, String> {
        let mut engine = Engine::new();
        engine.register_fn("log", |message: &str| -> Result<(), Box<EvalAltResult>> {
            host::log(LogLevel::Info, message).map_err(|error| host_error("logging", error))
        });
        engine.register_fn("read_note", || -> Result<Map, Box<EvalAltResult>> {
            let note = host::read_note().map_err(|error| host_error("reading note", error))?;
            let mut result = Map::new();
            result.insert("id".into(), note.id.into());
            result.insert("path".into(), note.path.into());
            result.insert("revision".into(), note.revision.into());
            result.insert("text".into(), note.text.into());
            Ok(result)
        });

        let mut scope = Scope::new();
        scope.push("input", input);
        engine
            .eval_with_scope::<Dynamic>(&mut scope, &source)
            .map(|value| value.to_string())
            .map_err(|error| error.to_string())
    }
}

export!(RhaiInterpreter);
