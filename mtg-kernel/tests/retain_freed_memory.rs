//! `retain_freed_memory_v1` sets mimalloc's purge delay to -1 unless the
//! environment names one. Its contract (call first in `main`, before any other
//! thread) rules out libtest's harness, so this test is its own `main`: run
//! without arguments it starts itself once per case with a controlled
//! environment, and each child checks the option before starting any thread.
//! The child's case travels in an environment variable, so libtest arguments
//! a whole-crate `cargo test` passes (filters, `--nocapture`) are ignored;
//! `--list` lists nothing.

const CASE: &str = "MTG_KERNEL_RETAIN_FREED_MEMORY_CASE";

const CASES: [(&str, Option<(&str, &str)>); 4] = [
    ("default", None),
    ("override", Some(("MIMALLOC_PURGE_DELAY", "0"))),
    ("legacy", Some(("MIMALLOC_RESET_DELAY", "7"))),
    ("lowercase", Some(("mimalloc_purge_delay", "3"))),
];

fn main() {
    if std::env::args().any(|argument| argument == "--list") {
        return;
    }
    match std::env::var(CASE).ok().as_deref() {
        None => {
            for (case, variable) in CASES {
                let mut command = std::process::Command::new(std::env::current_exe().unwrap());
                command.env(CASE, case);
                for (name, _) in std::env::vars_os() {
                    if name
                        .to_str()
                        .is_some_and(|name| name.to_ascii_uppercase().starts_with("MIMALLOC_"))
                    {
                        command.env_remove(name);
                    }
                }
                if let Some((name, value)) = variable {
                    command.env(name, value);
                }
                let output = command.output().unwrap();
                assert!(
                    output.status.success(),
                    "case {case}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            println!("retain_freed_memory: {} cases passed", CASES.len());
        }
        Some(case) => {
            // Option 15 is the purge delay: its built-in default is 1000 ms,
            // and a delay the environment names, in any letter case, stands.
            let before = mtg_kernel::purge_delay_ms_v1();
            // SAFETY: this child has started no other thread.
            unsafe { mtg_kernel::retain_freed_memory_v1() };
            let after = mtg_kernel::purge_delay_ms_v1();
            let expected = match case {
                "default" => (1000, -1),
                "override" => (0, 0),
                "legacy" => (7, 7),
                "lowercase" => (3, 3),
                other => panic!("unknown case {other}"),
            };
            assert_eq!((before, after), expected, "case {case}");
        }
    }
}
