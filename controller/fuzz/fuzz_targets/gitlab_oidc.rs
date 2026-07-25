#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    amiss_controller_fuzz::gitlab_oidc(data);
});
