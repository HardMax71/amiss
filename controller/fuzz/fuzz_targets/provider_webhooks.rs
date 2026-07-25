#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    amiss_controller_fuzz::provider_webhooks(data);
});
