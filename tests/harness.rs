#![allow(non_camel_case_types)]

// harness.rs - tpack integration test harness.
//
// Runs the tpack binary (built by Cargo) across a matrix of output
// formats and packing features. Each case asserts the child's process exit
// code and that every expected output file was produced (text formats are
// additionally checked for expected sprite paths).
//
// Run via `make harness` / `cargo test --test harness -- --nocapture` from
// the project root (the harness runs the built binary and uses test/sprites
// as its input).

use std::fs;
use std::path::Path;
use std::process::Command;

// Expected_File is an output file the tool must have produced.
struct Expected_File {
    path: &'static str,
    must_contain: &'static str, // "" = existence + non-empty is enough
}

// Test_Case describes one invocation of the tpack binary.
struct Test_Case {
    name: &'static str,
    args: &'static [&'static str],
    outputs: &'static [Expected_File],
    expect_exit: i32, // required process exit code; -1 = any non-zero
    // setup, when non-nil, prepares fixture files after stale-output removal.
    setup: Option<fn()>,
    // verify, when non-nil, runs extra assertions after the output checks and
    // must succeed for the case to pass.
    verify: Option<fn() -> bool>,
}

fn main() {
    let mut failures: Vec<String> = Vec::new();

    println!("tpack harness");
    println!("-------------");

    let tests = test_cases();

    // The --no-overwrite case needs its outputs to already exist. Ignore
    // (but consume) the write result: a failure surfaces as a failed case.
    let noovw_w1 = fs::write("test/textures/harness_noovw.png", [0u8]);
    let noovw_w2 = fs::write("test/textures/harness_noovw.c", [0u8]);
    if noovw_w1.is_err() || noovw_w2.is_err() {
        eprintln!("harness: could not prepare --no-overwrite fixture outputs");
        std::process::exit(1);
    }

    let mut passed = 0;
    let mut failed = 0;
    for t in &tests {
        if run_test(t, &mut failures) {
            passed += 1;
        } else {
            failed += 1;
        }
    }

    println!("\n{}/{} tests passed", passed, tests.len());
    if failed > 0 {
        println!("failed:");
        for f in &failures {
            println!("  - {f}");
        }
        std::process::exit(1);
    }
}

fn test_cases() -> Vec<Test_Case> {
    vec![
        Test_Case {
            name: "c map (auto-size)",
            args: &[
                "--map-format",
                "c",
                "test/sprites",
                "test/textures/harness_c.png",
                "test/textures/harness_c.c",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_c.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_c.c",
                    must_contain: "test/sprites/wieldhand.png",
                },
                Expected_File {
                    path: "test/textures/harness_c.c",
                    must_contain: "test/sprites/zuma/green-ball.png",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "threads 1 (matches default)",
            args: &[
                "--threads",
                "1",
                "--map-format",
                "c",
                "test/sprites",
                "target/harness_threads1.png",
                "target/harness_threads1.c",
            ],
            outputs: &[
                Expected_File {
                    path: "target/harness_threads1.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "target/harness_threads1.c",
                    must_contain: "test/sprites/wieldhand.png",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: Some(|| {
                files_equal("target/harness_threads1.png", "test/textures/harness_c.png")
                    && files_equal("target/harness_threads1.c", "test/textures/harness_c.c")
            }),
        },
        Test_Case {
            name: "threads 4 (matches default)",
            args: &[
                "--threads",
                "4",
                "--map-format",
                "c",
                "test/sprites",
                "target/harness_threads4.png",
                "target/harness_threads4.c",
            ],
            outputs: &[
                Expected_File {
                    path: "target/harness_threads4.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "target/harness_threads4.c",
                    must_contain: "test/sprites/wieldhand.png",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: Some(|| {
                files_equal("target/harness_threads4.png", "test/textures/harness_c.png")
                    && files_equal("target/harness_threads4.c", "test/textures/harness_c.c")
            }),
        },
        Test_Case {
            name: "json map (auto-size)",
            args: &[
                "--map-format",
                "json",
                "test/sprites",
                "test/textures/harness_json.png",
                "test/textures/harness_json.json",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_json.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_json.json",
                    must_contain: "\"test/sprites/wieldhand.png\"",
                },
                Expected_File {
                    path: "test/textures/harness_json.json",
                    must_contain: "\"test/sprites/zuma/zuma.png\"",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "ssm map (auto-size)",
            args: &[
                "--map-format",
                "ssm",
                "test/sprites",
                "test/textures/harness_ssm.png",
                "test/textures/harness_ssm.ssm",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_ssm.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_ssm.ssm",
                    must_contain: "test/sprites/wieldhand.png",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "spr map (spr-root)",
            args: &[
                "--map-format",
                "spr",
                "--spr-root",
                "test/sprites",
                "test/sprites",
                "test/textures/harness_spr.png",
                "test/textures/harness_spr.spr",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_spr.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_spr.spr",
                    must_contain: "SPRS",
                },
            ],
            expect_exit: 0,
            setup: None,
            // Beyond a bare "SPRS" exists-check, validate the header layout
            // against docs/spr-format-spec.md: the implicit padding after
            // scale_mode (bytes 9..11) must be zero, or the file is
            // byte-nondeterministic across runs (uninitialized struct
            // padding).
            verify: Some(|| {
                let data = match fs::read("test/textures/harness_spr.spr") {
                    Ok(d) => d,
                    Err(_) => return false,
                };
                if data.len() < 16 {
                    return false;
                }
                if &data[0..4] != b"SPRS" {
                    return false;
                }
                let version = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
                if version != 1 {
                    return false;
                }
                if data[8] != 1 {
                    // default --scale-mode is linear
                    return false;
                }
                if data[9] != 0 || data[10] != 0 || data[11] != 0 {
                    return false;
                }
                let count = u32::from_le_bytes([data[12], data[13], data[14], data[15]]);
                if count == 0 {
                    return false;
                }
                if data.len() != 16 + count as usize * 12 {
                    return false;
                }
                true
            }),
        },
        Test_Case {
            name: "structured-c map",
            args: &[
                "--map-format",
                "structured-c",
                "--map-enum-prefix",
                "HARNESS_",
                "--map-array-name",
                "harness_sprites",
                "--map-array-type",
                "Harness_Rect",
                "test/sprites",
                "test/textures/harness_struct.png",
                "test/textures/harness_struct.c",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_struct.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_struct.c",
                    must_contain: "HARNESS_Wieldhand",
                },
                Expected_File {
                    path: "test/textures/harness_struct.c",
                    must_contain: "HARNESS_Green_Ball",
                },
                Expected_File {
                    path: "test/textures/harness_struct.c",
                    must_contain: "Harness_Rect harness_sprites[]",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "explicit size + bf + padding",
            args: &[
                "--width",
                "2048",
                "--height",
                "2048",
                "--no-auto-size",
                "--heuristic",
                "bf",
                "--sprite-padding",
                "4",
                "--map-format",
                "c",
                "test/sprites",
                "test/textures/harness_bf.png",
                "test/textures/harness_bf.c",
            ],
            outputs: &[
                Expected_File {
                    path: "test/textures/harness_bf.png",
                    must_contain: "",
                },
                Expected_File {
                    path: "test/textures/harness_bf.c",
                    must_contain: "test/sprites/air.png",
                },
            ],
            expect_exit: 0,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "negative: --no-overwrite refuses existing output",
            args: &[
                "--no-overwrite",
                "--map-format",
                "c",
                "test/sprites",
                "test/textures/harness_noovw.png",
                "test/textures/harness_noovw.c",
            ],
            outputs: &[],
            expect_exit: 1,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "negative: invalid map format",
            args: &[
                "--map-format",
                "bogus",
                "test/sprites",
                "test/textures/harness_bogus.png",
                "test/textures/harness_bogus.c",
            ],
            outputs: &[],
            expect_exit: 2,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "negative: zero threads",
            args: &[
                "--threads",
                "0",
                "test/sprites",
                "test/textures/harness_threads0.png",
                "test/textures/harness_threads0.c",
            ],
            outputs: &[],
            expect_exit: 2,
            setup: Some(|| {
                let _ = fs::remove_file("test/textures/harness_threads0.png");
                let _ = fs::remove_file("test/textures/harness_threads0.c");
            }),
            verify: Some(|| {
                !Path::new("test/textures/harness_threads0.png").exists()
                    && !Path::new("test/textures/harness_threads0.c").exists()
            }),
        },
        Test_Case {
            name: "negative: invalid heuristic",
            args: &[
                "--heuristic",
                "nope",
                "test/sprites",
                "test/textures/harness_heur.png",
                "test/textures/harness_heur.c",
            ],
            outputs: &[],
            expect_exit: 2,
            setup: None,
            verify: None,
        },
        Test_Case {
            name: "negative: auto format with unknown extension",
            // No --map-format: auto is the default, and .dat is not a known
            // sprite-map extension. The failure must happen BEFORE any file
            // is touched: the pre-existing .dat may not be truncated, and
            // the atlas PNG must not be written at all.
            args: &[
                "test/sprites",
                "test/textures/harness_auto.png",
                "test/textures/harness_auto.dat",
            ],
            outputs: &[],
            expect_exit: 1,
            setup: Some(|| {
                let _ = fs::remove_file("test/textures/harness_auto.png");
                let _ = fs::remove_file("test/textures/harness_auto.dat");
                let _ = fs::write("test/textures/harness_auto.dat", "PRECIOUS\n");
            }),
            verify: Some(|| {
                let data = match fs::read("test/textures/harness_auto.dat") {
                    Ok(d) => d,
                    Err(_) => return false,
                };
                if data != b"PRECIOUS\n" {
                    return false;
                }
                if Path::new("test/textures/harness_auto.png").exists() {
                    return false;
                }
                true
            }),
        },
    ]
}

fn files_equal(a: &str, b: &str) -> bool {
    match (fs::read(a), fs::read(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

fn run_test(t: &Test_Case, failures: &mut Vec<String>) -> bool {
    print!("  {:<38} ", t.name);

    // Remove stale outputs from earlier runs so a stopped writing-but-exiting-0
    // tool cannot be masked by leftover files.
    for output in t.outputs {
        let _ = fs::remove_file(output.path);
    }

    // Let the case prepare fixtures (e.g. a pre-existing file that must not
    // be clobbered) after stale-output removal.
    if let Some(setup) = t.setup {
        setup();
    }

    let binary = env!("CARGO_BIN_EXE_tpack");
    let result = Command::new(binary).args(t.args).output();

    let output = match result {
        Ok(o) => o,
        Err(e) => {
            return fail_test(t.name, &format!("process error: {e}"), &[], &[], failures);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let exit_code = output.status.code().unwrap_or(-1);

    if t.expect_exit < 0 {
        if exit_code == 0 {
            return fail_test(
                t.name,
                "expected non-zero exit, got 0",
                stdout.as_bytes(),
                stderr.as_bytes(),
                failures,
            );
        }
    } else if exit_code != t.expect_exit {
        return fail_test(
            t.name,
            &format!("expected exit {}, got {}", t.expect_exit, exit_code),
            stdout.as_bytes(),
            stderr.as_bytes(),
            failures,
        );
    }

    for expected in t.outputs {
        let data = match fs::read(expected.path) {
            Ok(d) => d,
            Err(_) => {
                return fail_test(
                    t.name,
                    &format!("output missing: {}", expected.path),
                    &[],
                    &[],
                    failures,
                );
            }
        };
        if data.is_empty() {
            return fail_test(
                t.name,
                &format!("output empty: {}", expected.path),
                &[],
                &[],
                failures,
            );
        }
        if !expected.must_contain.is_empty()
            && !data
                .windows(expected.must_contain.len())
                .any(|w| w == expected.must_contain.as_bytes())
        {
            return fail_test(
                t.name,
                &format!(
                    "output {} does not contain {:?}",
                    expected.path, expected.must_contain
                ),
                &[],
                &[],
                failures,
            );
        }
    }

    // Extra per-case assertions (header layout, on-disk invariants, ...).
    if let Some(verify) = t.verify {
        if !verify() {
            return fail_test(t.name, "custom verification failed", &[], &[], failures);
        }
    }

    println!("PASS");
    true
}

fn fail_test(
    name: &str,
    reason: &str,
    stdout: &[u8],
    stderr: &[u8],
    failures: &mut Vec<String>,
) -> bool {
    println!("FAIL");
    println!("        {reason}");
    if !stdout.is_empty() {
        println!("      stdout:\n{}", String::from_utf8_lossy(stdout));
    }
    if !stderr.is_empty() {
        println!("      stderr:\n{}", String::from_utf8_lossy(stderr));
    }
    failures.push(format!("{name}: {reason}"));
    false
}
