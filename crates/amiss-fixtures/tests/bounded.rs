amiss_fixtures::bounded_memory!();

use amiss_fixtures::MEMORY_CEILING;

#[test]
fn the_ceiling_refuses_what_would_pass_it() {
    let mut runaway: Vec<u8> = Vec::new();
    assert!(runaway.try_reserve(MEMORY_CEILING + 1).is_err());
    let mut modest: Vec<u8> = Vec::new();
    assert!(modest.try_reserve(8 * 1024 * 1024).is_ok());
}
