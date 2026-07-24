use std::path::Path;

use amiss_controller_files::read_bounded;
use tempfile::TempDir;

#[test]
fn reads_only_absolute_bounded_regular_files() {
    let root = TempDir::new().unwrap();
    let regular = root.path().join("regular");
    std::fs::write(&regular, b"trusted").unwrap();
    assert_eq!(read_bounded(&regular, 7).unwrap(), b"trusted");
    assert!(read_bounded(&regular, 6).is_err());
    assert!(read_bounded(Path::new("relative"), 32).is_err());

    let directory = root.path().join("directory");
    std::fs::create_dir(&directory).unwrap();
    assert!(read_bounded(&directory, 32).is_err());
}

#[cfg(unix)]
#[test]
fn does_not_follow_the_final_file_entry() {
    let root = TempDir::new().unwrap();
    std::fs::write(root.path().join("target"), b"replacement").unwrap();
    let linked = root.path().join("linked");
    std::os::unix::fs::symlink("target", &linked).unwrap();
    assert!(read_bounded(&linked, 32).is_err());
}
