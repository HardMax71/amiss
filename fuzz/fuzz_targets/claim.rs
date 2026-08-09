#![no_main]

use std::sync::Once;

static QUIET: Once = Once::new();

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_info| {})));
    amiss_fuzz::claim(data);
});
