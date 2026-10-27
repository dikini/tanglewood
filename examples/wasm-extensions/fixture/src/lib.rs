//! Test-only guest used to exercise runtime isolation and resource limits.

wit_bindgen::generate!({
    path: "../../../wit",
    world: "compiled-extension",
});

use std::sync::atomic::{AtomicU32, Ordering};

static CALLS: AtomicU32 = AtomicU32::new(0);

struct Fixture;

impl exports::tanglewood::extensions::compiled::Guest for Fixture {
    fn run(input: String) -> Result<String, String> {
        match input.as_str() {
            "trap" => panic!("intentional guest trap"),
            "spin" => loop {
                std::hint::black_box(());
            },
            "grow" => loop {
                if std::arch::wasm32::memory_grow::<0>(256) == usize::MAX {
                    panic!("guest memory growth denied");
                }
            },
            "sleep" => {
                std::thread::sleep(std::time::Duration::from_secs(120));
                Ok("sleep returned".into())
            }
            "bump" => Ok(CALLS.fetch_add(1, Ordering::Relaxed).to_string()),
            _ => Err("unknown fixture operation".into()),
        }
    }
}

export!(Fixture);
