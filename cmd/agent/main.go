package main

import (
	"flag"
	"fmt"
	"os"

	"teamd/internal/runtime"
)

func main() {
	configPath := flag.String("config", "", "path to root agent config")
	flag.Parse()

	if *configPath == "" {
		fmt.Fprintln(os.Stderr, "missing required --config")
		os.Exit(2)
	}

	if _, err := runtime.BuildAgent(*configPath); err != nil {
		fmt.Fprintf(os.Stderr, "build agent: %v\n", err)
		os.Exit(1)
	}
}
