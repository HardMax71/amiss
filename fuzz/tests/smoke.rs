use std::fs;
use std::path::PathBuf;

#[expect(
    clippy::expect_used,
    clippy::panic,
    reason = "missing committed seeds are test setup failures"
)]
fn seeds(target: &str) -> Vec<Vec<u8>> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("seeds")
        .join(target);
    let mut seeds: Vec<(PathBuf, Vec<u8>)> = fs::read_dir(&dir)
        .unwrap_or_else(|_| panic!("seeds dir {}", dir.display()))
        .map(|entry| {
            let path = entry.expect("seed entry").path();
            let bytes = fs::read(&path).expect("seed bytes");
            (path, bytes)
        })
        .collect();
    assert!(!seeds.is_empty(), "no seeds for {target}");
    seeds.sort_by(|a, b| a.0.cmp(&b.0));
    seeds.into_iter().map(|(_, bytes)| bytes).collect()
}

struct XorShift(u64);

impl XorShift {
    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }
}

fn bounded_index(random: u64, bound: usize) -> usize {
    let bound = u64::try_from(bound).unwrap_or(u64::MAX);
    usize::try_from(random.checked_rem(bound).unwrap_or(0)).unwrap_or(0)
}

/// Deterministic byte-level mutants of one seed: flips, truncations,
/// duplications, and splices, seeded per target so every run replays.
fn mutants(seed: &[u8], rng: &mut XorShift, count: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut mutant = seed.to_vec();
        match rng.next() % 4 {
            0 if !mutant.is_empty() => {
                let index = bounded_index(rng.next(), mutant.len());
                let bit = u8::try_from(rng.next() % 8).unwrap_or(0);
                if let Some(byte) = mutant.get_mut(index) {
                    *byte ^= 1 << bit;
                }
            }
            1 if !mutant.is_empty() => {
                let keep = bounded_index(rng.next(), mutant.len());
                mutant.truncate(keep);
            }
            2 => {
                let byte = u8::try_from(rng.next() % 256).unwrap_or(0);
                let index = bounded_index(rng.next(), mutant.len().saturating_add(1));
                mutant.insert(index, byte);
            }
            _ if mutant.len() >= 2 => {
                let from = bounded_index(rng.next(), mutant.len());
                let to = bounded_index(rng.next(), mutant.len());
                let span = bounded_index(rng.next(), 16).saturating_add(1);
                let chunk: Vec<u8> = mutant
                    .iter()
                    .cycle()
                    .skip(from)
                    .take(span)
                    .copied()
                    .collect();
                let at = to.min(mutant.len());
                mutant.splice(at..at, chunk);
            }
            _ => {}
        }
        out.push(mutant);
    }
    out
}

fn sweep(target: &str, body: fn(&[u8]), seed_state: u64, per_seed: usize) {
    static QUIET: std::sync::Once = std::sync::Once::new();
    QUIET.call_once(|| std::panic::set_hook(Box::new(|_info| {})));
    let mut rng = XorShift(seed_state);
    for seed in seeds(target) {
        body(&seed);
        for mutant in mutants(&seed, &mut rng, per_seed) {
            body(&mutant);
        }
    }
}

#[test]
fn json_smoke() {
    sweep("json", amiss_fuzz::json, 0x9E37_79B9_7F4A_7C15, 400);
}

#[test]
fn controls_smoke() {
    sweep("controls", amiss_fuzz::controls, 0xBF58_476D_1CE4_E5B9, 200);
}

#[test]
fn requests_smoke() {
    sweep("requests", amiss_fuzz::requests, 0x94D0_49BB_1331_11EB, 400);
}

#[test]
fn markdown_smoke() {
    sweep("markdown", amiss_fuzz::markdown, 0xD6E8_FEB8_6659_FD93, 25);
}

#[test]
fn git_index_smoke() {
    sweep(
        "git_index",
        amiss_fuzz::git_index,
        0xA076_1D64_78BD_642F,
        400,
    );
}

#[test]
fn git_objects_smoke() {
    sweep(
        "git_objects",
        amiss_fuzz::git_objects,
        0xE703_7ED1_A0B4_28DB,
        400,
    );
}

#[test]
fn human_smoke() {
    sweep("human", amiss_fuzz::human, 0x8EBC_6AF0_9C88_C6E3, 400);
}
