package main

import (
	"os"
	"os/exec"
)

func main() {
	cmd := exec.Command("go", "run", "./main.go",
		"--width", "2048",
		"--height", "2048",
		"test/sprites",
		"test/textures/sprites.png",
		"test/textures/sprites.c",
	)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	if cmd.Run() != nil {
		os.Exit(1)
	}
}
