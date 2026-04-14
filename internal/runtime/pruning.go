package runtime

import (
	"fmt"
	"strings"

	"teamd/internal/compaction"
	"teamd/internal/provider"
)

func PrunePromptResidency(messages []provider.Message, maxToolChars int) []provider.Message {
	if len(messages) == 0 {
		return nil
	}
	out := make([]provider.Message, len(messages))
	copy(out, messages)
	protectedStart := lastUserIndex(messages)
	if protectedStart < 0 {
		protectedStart = len(messages)
	}
	for i := 0; i < protectedStart; i++ {
		out[i] = pruneResidencyMessage(out[i], maxToolChars)
	}
	return out
}

func pruneResidencyMessage(msg provider.Message, maxToolChars int) provider.Message {
	if msg.Role == "tool" {
		pruned := compaction.ReduceForCompaction(msg, maxToolChars)
		if strings.TrimSpace(pruned.Content) != strings.TrimSpace(msg.Content) {
			return pruned
		}
		if strings.TrimSpace(msg.Content) != "" {
			pruned.Content = fmt.Sprintf("%s tool output omitted from prompt residency; inspect transcript or artifacts if needed", toolResidencyName(msg))
			return pruned
		}
	}
	if msg.Role == "assistant" && len(msg.ToolCalls) > 0 && strings.TrimSpace(msg.Content) == "" {
		names := make([]string, 0, len(msg.ToolCalls))
		for _, call := range msg.ToolCalls {
			if strings.TrimSpace(call.Name) == "" {
				continue
			}
			names = append(names, call.Name)
		}
		pruned := msg
		pruned.Content = "assistant requested tools: " + strings.Join(names, ", ")
		pruned.ToolCalls = nil
		return pruned
	}
	return msg
}

func lastUserIndex(messages []provider.Message) int {
	for i := len(messages) - 1; i >= 0; i-- {
		if messages[i].Role == "user" {
			return i
		}
	}
	return -1
}

func toolResidencyName(msg provider.Message) string {
	if strings.TrimSpace(msg.Name) != "" {
		return msg.Name
	}
	return "tool"
}
