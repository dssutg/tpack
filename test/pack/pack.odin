// pack.odin - tpack integration test harness.
//
// Builds the tpack binary once, then runs it across a matrix of output
// formats and packing features. Each case asserts the child's process exit
// code and that every expected output file was produced (text formats are
// additionally checked for expected sprite paths).
//
// Run via `make harness` / `odin run test/pack` from the project root
// (the harness spawns `./tpack` and uses `test/sprites` as its input).

package main

import "core:fmt"
import "core:os"
import "core:strings"

// Expected_File is an output file the tool must have produced.
Expected_File :: struct {
	path:         string,
	must_contain: string, // "" = existence + non-empty is enough
}

// Test_Case describes one invocation of the tpack binary.
Test_Case :: struct {
	name:        string,
	args:        []string,
	outputs:     []Expected_File,
	expect_exit: int, // required process exit code; -1 = any non-zero
	// setup, when non-nil, prepares fixture files after stale-output removal.
	setup:       proc(),
	// verify, when non-nil, runs extra assertions after the output checks and
	// must succeed for the case to pass.
	verify:      proc() -> bool,
}

// failures collects human-readable reasons for the final summary. Messages
// are cloned off the temp allocator so they survive until main ends.
failures: [dynamic]string

main :: proc() {
	defer delete_failures()

	fmt.println("tpack harness")
	fmt.println("-------------")

	if !build_tool() {
		fmt.println("failed to build ./tpack, aborting")
		os.exit(1)
	}

	tests := []Test_Case {
		{
			name = "c map (auto-size)",
			args = {
				"--map-format",
				"c",
				"test/sprites",
				"test/textures/harness_c.png",
				"test/textures/harness_c.c",
			},
			outputs = {
				{path = "test/textures/harness_c.png"},
				{path = "test/textures/harness_c.c", must_contain = "test/sprites/wieldhand.png"},
				{
					path = "test/textures/harness_c.c",
					must_contain = "test/sprites/zuma/green-ball.png",
				},
			},
		},
		{
			name = "json map (auto-size)",
			args = {
				"--map-format",
				"json",
				"test/sprites",
				"test/textures/harness_json.png",
				"test/textures/harness_json.json",
			},
			outputs = {
				{path = "test/textures/harness_json.png"},
				{
					path = "test/textures/harness_json.json",
					must_contain = `"test/sprites/wieldhand.png"`,
				},
				{
					path = "test/textures/harness_json.json",
					must_contain = `"test/sprites/zuma/zuma.png"`,
				},
			},
		},
		{
			name = "ssm map (auto-size)",
			args = {
				"--map-format",
				"ssm",
				"test/sprites",
				"test/textures/harness_ssm.png",
				"test/textures/harness_ssm.ssm",
			},
			outputs = {
				{path = "test/textures/harness_ssm.png"},
				{
					path = "test/textures/harness_ssm.ssm",
					must_contain = "test/sprites/wieldhand.png",
				},
			},
		},
		{
			name = "spr map (spr-root)",
			args = {
				"--map-format",
				"spr",
				"--spr-root",
				"test/sprites",
				"test/sprites",
				"test/textures/harness_spr.png",
				"test/textures/harness_spr.spr",
			},
			outputs = {
				{path = "test/textures/harness_spr.png"},
				{path = "test/textures/harness_spr.spr", must_contain = "SPRS"},
			},
			// Beyond a bare "SPRS" exists-check, validate the header layout
			// against docs/spr-format-spec.md: the implicit padding after
			// scale_mode (bytes 9..11) must be zero, or the file is
			// byte-nondeterministic across runs (uninitialized struct
			// padding).
			verify = proc() -> bool {
				data, rerr := os.read_entire_file(
					"test/textures/harness_spr.spr",
					context.temp_allocator,
				)
				if rerr != nil {
					return false
				}
				if len(data) < 16 {
					return false
				}
				if string(data[0:4]) != "SPRS" {
					return false
				}
				version :=
					u32(data[4]) | u32(data[5]) << 8 | u32(data[6]) << 16 | u32(data[7]) << 24
				if version != 1 {
					return false
				}
				if data[8] != 1 { 	// default --scale-mode is linear
					return false
				}
				if data[9] != 0 || data[10] != 0 || data[11] != 0 {
					return false
				}
				count :=
					u32(data[12]) | u32(data[13]) << 8 | u32(data[14]) << 16 | u32(data[15]) << 24
				if count == 0 {
					return false
				}
				if len(data) != 16 + int(count) * 12 {
					return false
				}
				return true
			},
		},
		{
			name = "structured-c map",
			args = {
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
			},
			outputs = {
				{path = "test/textures/harness_struct.png"},
				{path = "test/textures/harness_struct.c", must_contain = "HARNESS_Wieldhand"},
				{path = "test/textures/harness_struct.c", must_contain = "HARNESS_Green_Ball"},
				{
					path = "test/textures/harness_struct.c",
					must_contain = "Harness_Rect harness_sprites[]",
				},
			},
		},
		{
			name = "explicit size + bf + padding",
			args = {
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
			},
			outputs = {
				{path = "test/textures/harness_bf.png"},
				{path = "test/textures/harness_bf.c", must_contain = "test/sprites/air.png"},
			},
		},
		{
			name = "negative: --no-overwrite refuses existing output",
			args = {
				"--no-overwrite",
				"--map-format",
				"c",
				"test/sprites",
				"test/textures/harness_noovw.png",
				"test/textures/harness_noovw.c",
			},
			expect_exit = 1,
		},
		{
			name = "negative: invalid map format",
			args = {
				"--map-format",
				"bogus",
				"test/sprites",
				"test/textures/harness_bogus.png",
				"test/textures/harness_bogus.c",
			},
			expect_exit = 2,
		},
		{
			name = "negative: invalid heuristic",
			args = {
				"--heuristic",
				"nope",
				"test/sprites",
				"test/textures/harness_heur.png",
				"test/textures/harness_heur.c",
			},
			expect_exit = 2,
		},
		{
			name = "negative: auto format with unknown extension",
			// No --map-format: auto is the default, and .dat is not a known
			// sprite-map extension. The failure must happen BEFORE any file
			// is touched: the pre-existing .dat may not be truncated, and
			// the atlas PNG must not be written at all.
			args = {
				"test/sprites",
				"test/textures/harness_auto.png",
				"test/textures/harness_auto.dat",
			},
			expect_exit = 1,
			setup = proc() {
				os.remove("test/textures/harness_auto.png")
				os.remove("test/textures/harness_auto.dat")
				_ = os.write_entire_file("test/textures/harness_auto.dat", "PRECIOUS\n")
			},
			verify = proc() -> bool {
				data, rerr := os.read_entire_file(
					"test/textures/harness_auto.dat",
					context.temp_allocator,
				)
				if rerr != nil {
					return false
				}
				if string(data) != "PRECIOUS\n" {
					return false
				}
				if os.exists("test/textures/harness_auto.png") {
					return false
				}
				return true
			},
		},
	}

	// The --no-overwrite case needs its outputs to already exist. Ignore
	// (but consume) the write result: a failure surfaces as a failed case.
	noovw_w1 := os.write_entire_file("test/textures/harness_noovw.png", []byte{0})
	noovw_w2 := os.write_entire_file("test/textures/harness_noovw.c", []byte{0})
	if noovw_w1 != nil || noovw_w2 != nil {
		fmt.eprintln("harness: could not prepare --no-overwrite fixture outputs")
		os.exit(1)
	}

	passed, failed := 0, 0
	for t in tests {
		if run_test(t) {
			passed += 1
		} else {
			failed += 1
		}
	}

	fmt.printf("\n%d/%d tests passed\n", passed, len(tests))
	if failed > 0 {
		fmt.println("failed:")
		for f in failures {
			fmt.printf("  - %s\n", f)
		}
		os.exit(1)
	}
}

// build_tool compiles the tpack binary once so each case can invoke it
// directly instead of paying an odin build per test.
build_tool :: proc() -> bool {
	fmt.println("building ./tpack ...")
	state, stdout, stderr, err := os.process_exec(
		{command = {"odin", "build", "."}},
		context.temp_allocator,
	)
	defer delete(stdout, context.temp_allocator)
	defer delete(stderr, context.temp_allocator)
	if err != nil {
		fmt.eprintfln("  build error: %v", err)
		return false
	}
	if state.exit_code != 0 {
		fmt.print(string(stdout))
		fmt.print(string(stderr))
		return false
	}
	return true
}

run_test :: proc(t: Test_Case) -> bool {
	fmt.printf("  %-38s ", t.name)

	// Remove stale outputs from earlier runs so a stopped writing-but-exiting-0
	// tool cannot be masked by leftover files.
	for output in t.outputs {
		os.remove(output.path)
	}

	// Let the case prepare fixtures (e.g. a pre-existing file that must not
	// be clobbered) after stale-output removal.
	if t.setup != nil {
		t.setup()
	}

	cmd := make([dynamic]string, 0, len(t.args) + 1, context.temp_allocator)
	append(&cmd, "./tpack")
	append(&cmd, ..t.args)

	state, stdout, stderr, err := os.process_exec({command = cmd[:]}, context.temp_allocator)
	defer delete(stdout, context.temp_allocator)
	defer delete(stderr, context.temp_allocator)
	if err != nil {
		return fail_test(t.name, fmt.tprintf("process error: %v", err), stdout, stderr)
	}

	if t.expect_exit < 0 {
		if state.exit_code == 0 {
			return fail_test(t.name, "expected non-zero exit, got 0", stdout, stderr)
		}
	} else if state.exit_code != t.expect_exit {
		return fail_test(
			t.name,
			fmt.tprintf("expected exit %d, got %d", t.expect_exit, state.exit_code),
			stdout,
			stderr,
		)
	}

	for expected in t.outputs {
		data, rerr := os.read_entire_file(expected.path, context.temp_allocator)
		if rerr != nil {
			return fail_test(t.name, fmt.tprintf("output missing: %s", expected.path), nil, nil)
		}
		if len(data) == 0 {
			return fail_test(t.name, fmt.tprintf("output empty: %s", expected.path), nil, nil)
		}
		if expected.must_contain != "" && !strings.contains(string(data), expected.must_contain) {
			return fail_test(
				t.name,
				fmt.tprintf("output %s does not contain %q", expected.path, expected.must_contain),
				nil,
				nil,
			)
		}
	}

	// Extra per-case assertions (header layout, on-disk invariants, ...).
	if t.verify != nil && !t.verify() {
		return fail_test(t.name, "custom verification failed", nil, nil)
	}

	fmt.println("PASS")
	return true
}

fail_test :: proc(name, reason: string, stdout, stderr: []byte) -> bool {
	fmt.println("FAIL")
	fmt.printf("        %s\n", reason)
	if len(stdout) > 0 {
		fmt.printf("      stdout:\n%s\n", string(stdout))
	}
	if len(stderr) > 0 {
		fmt.printf("      stderr:\n%s\n", string(stderr))
	}
	msg := fmt.tprintf("%s: %s", name, reason)
	append(&failures, strings.clone(msg))
	return false
}

delete_failures :: proc() {
	for f in failures {
		delete(f)
	}
	delete(failures)
}
