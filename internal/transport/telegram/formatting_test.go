package telegram

import (
	"strings"
	"testing"
	"unicode/utf8"
)

func TestFormatTelegramReplyLeavesTablesUnreshaped(t *testing.T) {
	input := "| # | NAME | STATUS | REASON |\n|---|---|---|---|\n| 1 | *api* | failed | missing DB_URL |"
	out := FormatTelegramReply(input)
	if !strings.Contains(out, "| # | NAME | STATUS | REASON |") {
		t.Fatalf("expected original table to remain, got %q", out)
	}
	if !strings.Contains(out, "<b>api</b>") {
		t.Fatalf("expected inline markdown to still render, got %q", out)
	}
}

func TestFormatTelegramReplyPreservesMarkdownWeightWithoutTelegramLists(t *testing.T) {
	input := "## Summary\n1. **api**\n2. `worker`\n"
	out := FormatTelegramReply(input)
	if strings.Contains(out, "1.") || strings.Contains(out, "2.") {
		t.Fatalf("expected numbering to be flattened, got %q", out)
	}
	if !strings.Contains(out, "<b>api</b>") || !strings.Contains(out, "<code>worker</code>") {
		t.Fatalf("expected markdown adapted for Telegram, got %q", out)
	}
}

func TestFormatTelegramReplyKeepsUTF8WhenTableHeadersAreCyrillic(t *testing.T) {
	input := "| Инструмент | Описание |\n|---|---|\n| shell.exec | Выполняет команды |"
	out := FormatTelegramReply(input)
	if !utf8.ValidString(out) {
		t.Fatalf("expected valid utf8 output, got %q", out)
	}
	if !strings.Contains(out, "| Инструмент | Описание |") {
		t.Fatalf("expected cyrillic table to remain, got %q", out)
	}
}

func TestFormatTelegramReplyPreservesProseAroundMultipleTables(t *testing.T) {
	input := "## Контекст\n\nТекст до таблицы.\n\n| Тул | Назначение |\n|---|---|\n| shell_exec | Выполнение shell-команд |\n\n---\n\n## Навыки\n\n| Навык | Описание |\n|---|---|\n| example | Example skill bundle |\n"
	out := FormatTelegramReply(input)
	if !strings.Contains(out, "Контекст") {
		t.Fatalf("expected prose to remain, got %q", out)
	}
	if !strings.Contains(out, "| Тул | Назначение |") || !strings.Contains(out, "shell_exec | Выполнение shell-команд") {
		t.Fatalf("expected first table content to remain, got %q", out)
	}
	if !strings.Contains(out, "| Навык | Описание |") || !strings.Contains(out, "example | Example skill bundle") {
		t.Fatalf("expected second table content to remain, got %q", out)
	}
}
