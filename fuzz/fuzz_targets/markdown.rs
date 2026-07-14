#![no_main]

use std::sync::Once;

static QUIET: Once = Once::new();

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    // The contract classifies caught parser panics; the default hook would
    // abort at panic time before the sanctioned catch_unwind runs. A panic
    // escaping the harness still aborts through the unwind boundary.
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_info| {})));
    amiss_fuzz::markdown(data);
});
