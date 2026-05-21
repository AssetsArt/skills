use std::path::PathBuf;
use std::process::Command;

fn bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_codegraph"))
}

#[test]
fn cli_help_lists_all_four_subcommands() {
    let out = Command::new(bin()).args(["--help"]).output().expect("run");
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    for sub in ["find-refs", "callers", "callees", "impact"] {
        assert!(
            stdout.contains(sub),
            "missing subcommand {sub} in --help:\n{stdout}"
        );
    }
}
