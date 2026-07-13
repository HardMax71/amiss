use std::fs;
use std::path::PathBuf;

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

/// Deterministic byte-level mutants of one seed: flips, truncations,
/// duplications, and splices, seeded per target so every run replays.
fn mutants(seed: &[u8], rng: &mut XorShift, count: usize) -> Vec<Vec<u8>> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let mut mutant = seed.to_vec();
        match rng.next() % 4 {
            0 if !mutant.is_empty() => {
                let index = usize::try_from(rng.next()).unwrap_or(0) % mutant.len();
                let bit = u8::try_from(rng.next() % 8).unwrap_or(0);
                mutant[index] ^= 1 << bit;
            }
            1 if !mutant.is_empty() => {
                let keep = usize::try_from(rng.next()).unwrap_or(0) % mutant.len();
                mutant.truncate(keep);
            }
            2 => {
                let byte = u8::try_from(rng.next() % 256).unwrap_or(0);
                let index = usize::try_from(rng.next()).unwrap_or(0) % (mutant.len() + 1);
                mutant.insert(index, byte);
            }
            _ if mutant.len() >= 2 => {
                let from = usize::try_from(rng.next()).unwrap_or(0) % mutant.len();
                let to = usize::try_from(rng.next()).unwrap_or(0) % mutant.len();
                let span = 1 + usize::try_from(rng.next()).unwrap_or(0) % 16;
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
