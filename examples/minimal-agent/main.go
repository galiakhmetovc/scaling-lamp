package main

import (
	"bufio"
	"context"
	"fmt"
	"os"
	"strings"
)

func main() {
	ctx := context.Background()
	provider := EchoProvider{}
	tools := map[string]Tool{
		"time.now": TimeTool{},
	}
	memory := NewMemoryStore()
	engine := Engine{
		Provider: provider,
		Tools:    tools,
		Memory:   memory,
	}

	fmt.Println("minimal agent ready; type a message or Ctrl+D to exit")
	scanner := bufio.NewScanner(os.Stdin)
	for scanner.Scan() {
		text := strings.TrimSpace(scanner.Text())
		if text == "" {
			continue
		}
		reply, err := engine.Handle(ctx, text)
		if err != nil {
			fmt.Println("error:", err)
			continue
		}
		fmt.Println(reply)
	}
}
