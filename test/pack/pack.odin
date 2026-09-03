package main

import "core:os"

main :: proc() {
	xclip_proc, start_err := os.process_start({
		command = {
      "odin", "run", ".",
      "--width", "2048",
      "--height", "2048",
      "test/sprites",
      "test/textures/sprites.png",
      "test/textures/sprites.c",
    },
		stdin   = os.stdin,
		stdout  = os.stdout,
		stderr  = os.stderr,
	})
	if start_err != nil {
    os.exit(1)
	}

	if _, wait_err := os.process_wait(xclip_proc); wait_err != nil {
    os.exit(1)
  }
}
