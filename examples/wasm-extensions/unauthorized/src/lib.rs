//! Test-only guest requesting an interface the host does not register.

wit_bindgen::generate!({ world: "compiled-extension" });

use tanglewood::extensions::{host, unregistered};

struct UnauthorizedFixture;

impl exports::tanglewood::extensions::compiled::Guest for UnauthorizedFixture {
    fn run(input: String) -> Result<String, String> {
        host::log(host::LogLevel::Info, "calling an unregistered interface")
            .map_err(|error| format!("logging failed: {error:?}"))?;
        unregistered::ping();
        Ok(input)
    }
}

export!(UnauthorizedFixture);
