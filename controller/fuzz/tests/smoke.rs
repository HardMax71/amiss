use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn seeds(target: &str) -> io::Result<Vec<(PathBuf, Vec<u8>)>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("seeds")
        .join(target);
    let mut seeds = fs::read_dir(&root)?
        .map(|entry| {
            let path = entry?.path();
            let bytes = fs::read(&path)?;
            Ok((path, bytes))
        })
        .collect::<io::Result<Vec<_>>>()?;
    seeds.sort_by(|left, right| left.0.cmp(&right.0));
    if seeds.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("no seeds for {target}"),
        ));
    }
    Ok(seeds)
}

fn sweep(target: &str, harness: fn(&[u8])) -> io::Result<()> {
    for (_path, seed) in seeds(target)? {
        harness(&seed);
        harness(&[]);
        for index in 0..seed.len() {
            for bit in 0..8 {
                let mut changed = seed.clone();
                if let Some(byte) = changed.get_mut(index) {
                    *byte ^= 1_u8.checked_shl(bit).unwrap_or_default();
                }
                harness(&changed);
            }
            let mut shortened = seed.clone();
            shortened.truncate(index);
            harness(&shortened);
        }
        for suffix in [0, 0x7f, 0xff] {
            let mut extended = seed.clone();
            extended.push(suffix);
            harness(&extended);
        }
    }
    Ok(())
}

#[test]
fn provider_webhooks_smoke() -> io::Result<()> {
    sweep(
        "provider_webhooks",
        amiss_controller_fuzz::provider_webhooks,
    )
}

#[test]
fn gitlab_oidc_smoke() -> io::Result<()> {
    sweep("gitlab_oidc", amiss_controller_fuzz::gitlab_oidc)
}
