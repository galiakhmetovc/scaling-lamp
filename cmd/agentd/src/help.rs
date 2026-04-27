#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    Overview,
    Commands,
    Settings,
    Judge,
}

pub(crate) const QUICK_HELP_LINE: &str = "команды: \\агенты | \\агент показать | \\агент выбрать <id> | \\агент создать <имя> [из <шаблона>] | \\агент открыть | \\агент написать <id> <сообщение> | CLI: agent list|show|select|create|open | \\судья <сообщение> | \\цепочка продолжить <chain_id> <причина> | \\расписания | \\расписание создать <id> <секунды> [agent=<id>] :: <промпт> | \\расписание изменить <id> ... | \\расписание показать <id> | \\расписание удалить <id> | \\mcp | \\mcp показать <id> | \\mcp создать <id> command=<cmd> ... | \\mcp изменить <id> ... | \\mcp включить <id> | \\mcp выключить <id> | \\mcp перезапустить <id> | \\mcp удалить <id> | \\память сессии <запрос> | \\память сессия <id> [summary|timeline|transcript|artifacts] | \\память знания <запрос> | \\память файл <path> [excerpt|full] | \\версия | \\логи [N] | \\обновить [tag] | \\система | \\артефакты | \\артефакт <id> | \\дебаг | \\помощь [команды|настройки|судья] | \\настройки | \\контекст | \\план | \\статус | \\процессы | \\пауза | \\стоп | \\отмена | \\задачи | \\скиллы | \\включить <скилл> | \\выключить <скилл> | \\доводка <N|выкл> | \\автоапрув <вкл|выкл> | \\апрув [id] | \\выход";

pub(crate) fn parse_help_topic(raw: Option<&str>) -> Result<HelpTopic, String> {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(HelpTopic::Overview);
    };

    match raw {
        "команды" | "команда" | "commands" | "command" | "cmd" => {
            Ok(HelpTopic::Commands)
        }
        "настройки" | "настройка" | "settings" | "setting" | "config" => {
            Ok(HelpTopic::Settings)
        }
        "судья" | "judge" => Ok(HelpTopic::Judge),
        value => Err(format!(
            "неизвестная тема справки {value}; доступны: команды, настройки, судья"
        )),
    }
}

pub(crate) fn render_help(topic: HelpTopic) -> String {
    match topic {
        HelpTopic::Overview => render_overview_help(),
        HelpTopic::Commands => render_commands_help(),
        HelpTopic::Settings => render_settings_help(),
        HelpTopic::Judge => render_judge_help(),
    }
}

pub(crate) fn render_command_usage(command: &str) -> Option<String> {
    let (format_line, example_lines): (&str, &[&str]) = match command {
        "/help" => (
            "\\помощь [команды|настройки|судья]",
            &["\\помощь", "\\помощь команды"],
        ),
        "/agents" => ("\\агенты", &["\\агенты"]),
        "/agent" => (
            "\\агент <показать|выбрать|создать|открыть|написать> ...",
            &[
                "\\агент показать",
                "\\агент выбрать judge",
                "\\агент создать ревьюер из judge",
                "\\агент открыть",
                "\\агент написать judge проверь последний вывод и дай вердикт",
            ],
        ),
        "/judge" => (
            "\\судья <сообщение>",
            &["\\судья проверь последний вывод и дай вердикт"],
        ),
        "/chain" => (
            "\\цепочка продолжить <chain-id> <причина>",
            &["\\цепочка продолжить chain-123 нужен ещё один hop для финального ответа"],
        ),
        "/schedule" => (
            "\\расписание <показать|создать|изменить|включить|выключить|удалить> ...",
            &[
                "\\расписание показать judge-pulse",
                "\\расписание создать judge-pulse 300 :: проверь свежие изменения и дай краткую сводку",
                "\\расписание создать judge-pulse 300 agent=judge :: проверь свежие изменения и дай краткую сводку",
                "\\расписание изменить judge-pulse interval=600 enabled=true :: проверь свежие изменения и дай краткую сводку",
                "\\расписание включить judge-pulse",
                "\\расписание выключить judge-pulse",
                "\\расписание удалить judge-pulse",
            ],
        ),
        "/mcp" => (
            "\\mcp <показать|создать|изменить|включить|выключить|перезапустить|удалить> ...",
            &[
                "\\mcp",
                "\\mcp показать docs",
                "\\mcp создать docs command=npx args=-y,@modelcontextprotocol/server-filesystem,/workspace cwd=/srv/mcp env=DEBUG=1;TRACE=yes enabled=true",
                "\\mcp изменить docs command=uvx args=mcp-server-git cwd= env=TRACE=1 enabled=true",
                "\\mcp включить docs",
                "\\mcp выключить docs",
                "\\mcp перезапустить docs",
                "\\mcp удалить docs",
            ],
        ),
        "/memory" => (
            "\\память <сессии|сессия|знания|файл> ...",
            &[
                "\\память сессии offline adqm",
                "\\память сессия session-123 summary",
                "\\память сессия session-123 transcript",
                "\\память знания memory foundation",
                "\\память файл docs/architecture.md excerpt",
            ],
        ),
        "/artifact" => (
            "\\артефакт <artifact-id>",
            &["\\артефакт artifact-tool-offload-session-123.bin"],
        ),
        "/logs" => ("\\логи [N]", &["\\логи", "\\логи 120"]),
        "/completion" => ("\\доводка <N|выкл>", &["\\доводка 3", "\\доводка выкл"]),
        "/autoapprove" => (
            "\\автоапрув <вкл|выкл>",
            &["\\автоапрув вкл", "\\автоапрув выкл"],
        ),
        "/enable" => (
            "\\включить <скилл>",
            &["\\включить timeweb", "\\включить vsphere-govc"],
        ),
        "/disable" => (
            "\\выключить <скилл>",
            &["\\выключить timeweb", "\\выключить vsphere-govc"],
        ),
        "/approve" => (
            "\\апрув [approval-id]",
            &["\\апрув", "\\апрув approval-run-chat-session-123"],
        ),
        "/cancel" => ("\\отмена", &["\\отмена"]),
        "/model" => (
            "\\модель <id>",
            &["\\модель glm-5-turbo", "\\модель gpt-5.4"],
        ),
        "/reasoning" => (
            "\\размышления <вкл|выкл>",
            &["\\размышления вкл", "\\размышления выкл"],
        ),
        "/think" => ("\\думай <уровень>", &["\\думай low", "\\думай high"]),
        _ => return None,
    };

    let mut lines = vec![format!("Формат: {format_line}")];
    if !example_lines.is_empty() {
        lines.push("Примеры:".to_string());
        lines.extend(example_lines.iter().map(|value| format!("- {value}")));
    }
    Some(lines.join("\n"))
}

pub(crate) fn render_command_usage_error(command: &str, detail: &str) -> String {
    match render_command_usage(command) {
        Some(usage) => format!("{detail}\n{usage}"),
        None => detail.to_string(),
    }
}

fn render_overview_help() -> String {
    [
        "Справка",
        "",
        "Темы:",
        "- \\помощь команды — список команд TUI/REPL",
        "- \\помощь настройки — что меняют настройки сессии",
        "- \\помощь судья — как сейчас готовится judge и чего ещё нет",
        "",
        "Коротко:",
        "- основные команды на русском; английские /... алиасы пока сохранены",
        "- Shift+Tab перебирает команды и подставляет их в поле ввода",
        "- судья теперь встроенный агентный шаблон `judge`; выберите его через \\агент выбрать judge и создайте новую сессию",
        "- операторские межагентные действия идут напрямую: \\судья <сообщение>, \\агент написать <id> <сообщение>, \\цепочка продолжить <chain-id> <причина>",
    ]
    .join("\n")
}

fn render_commands_help() -> String {
    [
        "Команды",
        "",
        "Навигация и сессии:",
        "- \\сессии — открыть список сессий",
        "- \\новая — создать новую сессию",
        "- \\агенты — показать список агентов и текущий выбранный профиль",
        "- \\агент показать [id|name] — показать подробности агента; без аргумента берётся текущий",
        "- \\агент выбрать <id|name> — выбрать глобально текущего агента для новых сессий",
        "- \\агент создать <имя> [из <template>] — создать durable agent profile из шаблона default/judge",
        "- \\агент открыть [id|name] — показать agent_home, prompt files, skills_dir и default workspace",
        "- one-shot CLI: teamdctl agent list|show|select|create|open",
        "- \\агент написать <id> <сообщение> — отправить сообщение другому агенту из текущей сессии",
        "- \\судья <сообщение> — быстрый алиас для отправки сообщения агенту judge",
        "- \\цепочка продолжить <chain-id> <причина> — выдать одно продолжение blocked inter-agent chain",
        "- \\расписания — показать расписания для текущего workspace",
        "- \\расписание показать <id> — показать одно расписание",
        "- \\расписание создать <id> <секунды> [agent=<id>] :: <промпт> — каждые N секунд запускать свежую сессию агента",
        "- \\расписание изменить <id> [field=value ...] [:: новый промпт] — обновить расписание",
        "- \\расписание включить <id> / \\расписание выключить <id> — быстро переключить enabled",
        "- \\расписание удалить <id> — удалить расписание",
        "- \\mcp — показать MCP-коннекторы и их runtime status",
        "- \\mcp показать <id> — показать один MCP-коннектор",
        "- \\mcp создать <id> command=<cmd> [args=a,b] [cwd=/path] [env=K=V;K2=V2] [enabled=true|false] — создать stdio MCP-коннектор",
        "- \\mcp изменить <id> [command=<cmd>] [args=a,b] [cwd=/path|cwd=] [env=K=V;K2=V2|env=] [enabled=true|false] — обновить MCP-коннектор",
        "- \\mcp включить <id> / \\mcp выключить <id> — быстро переключить enabled",
        "- \\mcp перезапустить <id> — вручную перезапустить MCP-коннектор",
        "- \\mcp удалить <id> — удалить MCP-коннектор",
        "- \\память сессии <запрос> — найти исторические сессии по title/summary/transcript/artifacts",
        "- \\память сессия <id> [summary|timeline|transcript|artifacts] — прочитать bounded history одной сессии",
        "- \\память знания <запрос> — искать по README/SYSTEM/AGENTS/docs/projects/notes",
        "- \\память файл <path> [excerpt|full] — прочитать knowledge source bounded way",
        "- \\переименовать — переименовать текущую сессию",
        "- \\очистить — очистить текущую сессию и начать заново",
        "- \\выход — выйти из TUI/REPL",
        "",
        "Диагностика и контекст:",
        "- \\версия — показать текущую версию, путь к бинарю и статус latest GitHub release",
        "- \\логи [N] — показать хвост structured diagnostic log из data_dir/audit/runtime.jsonl",
        "- \\обновить [tag] — скачать latest release или указанный tag и заменить текущий бинарь",
        "- \\помощь [команды|настройки|судья] — открыть русскую справку",
        "- \\настройки — краткая справка по настройкам сессии",
        "- \\система — показать SessionHead, SYSTEM.md, AGENTS.md и собранные системные блоки",
        "- \\контекст — показать состояние контекста и compaction",
        "- \\план — показать текущий план",
        "- \\статус — показать активный run, его статус и активные процессы",
        "- \\процессы — показать активные exec-процессы с полной командой и cwd",
        "- \\пауза — мягкая пауза; сейчас честно работает как операторская остановка текущего хода",
        "- \\стоп — остановить активный run оператора и завершить висящий exec_wait",
        "- \\отмена — отменить вообще всю работу текущей сессии: runs, jobs, missions и queued wakeups",
        "- \\задачи — показать активные фоновые задачи текущей сессии",
        "- \\артефакты — показать offload-артефакты текущей сессии",
        "- \\артефакт <id> — открыть содержимое одного артефакта",
        "- \\дебаг — открыть interactive debug-view: сообщения, tool calls и artifacts",
        "- \\отладка — сохранить debug bundle в DATA_DIR/audit/debug-bundles",
        "- \\скиллы — показать список скиллов и их статус",
        "",
        "Управление скиллами:",
        "- \\включить <скилл> — принудительно включить скилл в сессии",
        "- \\выключить <скилл> — принудительно выключить скилл в сессии",
        "",
        "Исполнение:",
        "- \\апрув [approval-id] — продолжить ожидающий run",
        "- \\автоапрув <вкл|выкл> — автоматически подтверждать ожидающие approvals",
        "- \\доводка <N|выкл> — сколько раз автоматически пинать модель, если она остановилась раньше времени",
        "",
        "Настройки сессии:",
        "- \\модель <id> — выбрать модель для текущей сессии",
        "- \\размышления <вкл|выкл> — показывать или скрывать reasoning в чате",
        "- \\думай <уровень> — задать think level",
        "- \\компакт — вручную форсировать compaction контекста",
        "",
        "Алиасы:",
        "- английские /help, /context, /jobs, /completion, /autoapprove и другие пока работают",
    ]
    .join("\n")
}

fn render_settings_help() -> String {
    [
        "Настройки сессии",
        "",
        "\\доводка <N|выкл>",
        "- выкл — режим доводки выключен",
        "- 0 — если модель остановилась рано, сразу просим апрув оператора",
        "- N>0 — сначала автоматически пинаем модель ещё N раз, потом просим апрув",
        "",
        "\\автоапрув <вкл|выкл>",
        "- вкл — если run ушёл в waiting_approval, TUI сам делает \\апрув",
        "- выкл — подтверждение остаётся ручным",
        "",
        "\\пауза / \\стоп / \\отмена",
        "- \\пауза сейчас работает как мягкая остановка и пока эквивалентна операторской остановке",
        "- \\стоп немедленно отправляет операторскую остановку активного хода",
        "- \\отмена жёстко гасит всю работу текущей сессии и её локальных дочерних сессий",
        "",
        "\\модель <id>",
        "- меняет модель только для текущей сессии",
        "",
        "\\размышления <вкл|выкл>",
        "- включает или скрывает reasoning-строки в чате",
        "",
        "\\думай <уровень>",
        "- задаёт think level для текущей сессии",
        "",
        "\\компакт",
        "- вручную запускает compaction контекста",
        "- auto-compaction тоже может сработать перед provider turn, если prompt подошёл к порогу context window",
        "- порог задаётся через [context].auto_compaction_trigger_ratio и context_window_tokens_override",
        "",
        "\\обновить [tag]",
        "- без аргумента скачивает latest GitHub release для `agentd`",
        "- с tag скачивает конкретный release, например `\\обновить v1.0.1`",
        "- после обновления нужно перезапустить TUI/daemon",
    ]
    .join("\n")
}

fn render_judge_help() -> String {
    [
        "Судья",
        "",
        "Судья теперь встроенный агентный шаблон `judge`.",
        "Как включить:",
        "- \\агент выбрать judge",
        "- \\новая",
        "",
        "Что это даёт:",
        "- новая сессия возьмёт SYSTEM.md и AGENTS.md из agent_home судьи",
        "- у судьи свой allowlist тулов; file-write и exec_* ему недоступны",
        "- судья может участвовать в межагентных цепочках и выдавать разовый grant для продолжения сверх max_hops",
        "",
        "Операторские команды:",
        "- \\судья <сообщение> — быстро отправить judge задачу из текущей сессии",
        "- \\агент написать judge <сообщение> — то же самое явной командой",
        "- \\цепочка продолжить <chain-id> <причина> — разрешить ещё один hop для blocked chain",
        "",
        "Если нужен удалённый judge-узел через A2A, настройка остаётся такой:",
        "[daemon]",
        "public_base_url = \"https://daemon-a.example\"",
        "",
        "[daemon.a2a_peers.judge]",
        "base_url = \"https://daemon-b.example\"",
        "bearer_token = \"<token>\"",
        "",
        "Важно:",
        "- локальный judge больше не скрытая магия, а обычный агентный профиль",
        "- межагентный вызов идёт через tool `message_agent`, а ожидание ответа дочерней сессии — через `session_wait`",
        "- A2A-настройка нужна только если вы хотите вынести judge на другой daemon",
    ]
    .join("\n")
}

#[cfg(test)]
mod tests {
    use super::{
        HelpTopic, parse_help_topic, render_command_usage, render_command_usage_error, render_help,
    };

    #[test]
    fn parses_russian_help_topics() {
        assert_eq!(
            parse_help_topic(None).expect("overview"),
            HelpTopic::Overview
        );
        assert_eq!(
            parse_help_topic(Some("команды")).expect("commands"),
            HelpTopic::Commands
        );
        assert_eq!(
            parse_help_topic(Some("настройки")).expect("settings"),
            HelpTopic::Settings
        );
        assert_eq!(
            parse_help_topic(Some("судья")).expect("judge"),
            HelpTopic::Judge
        );
    }

    #[test]
    fn judge_help_is_honest_about_current_enable_path() {
        let rendered = render_help(HelpTopic::Judge);
        assert!(rendered.contains("\\агент выбрать judge"));
        assert!(rendered.contains("[daemon.a2a_peers.judge]"));
        assert!(rendered.contains("message_agent"));
        assert!(rendered.contains("session_wait"));
        assert!(rendered.contains("allowlist"));
    }

    #[test]
    fn command_usage_renders_examples_for_argument_commands() {
        let usage = render_command_usage("/completion").expect("usage");
        assert!(usage.contains("Формат: \\доводка <N|выкл>"));
        assert!(usage.contains("\\доводка 3"));
    }

    #[test]
    fn command_usage_covers_interagent_operator_commands() {
        let judge = render_command_usage("/judge").expect("judge usage");
        assert!(judge.contains("Формат: \\судья <сообщение>"));
        assert!(judge.contains("\\судья проверь последний вывод"));

        let chain = render_command_usage("/chain").expect("chain usage");
        assert!(chain.contains("Формат: \\цепочка продолжить <chain-id> <причина>"));
        assert!(chain.contains("chain-123"));
    }

    #[test]
    fn command_usage_error_appends_format_and_examples() {
        let error = render_command_usage_error("/autoapprove", "не хватает аргументов");
        assert!(error.contains("не хватает аргументов"));
        assert!(error.contains("Формат: \\автоапрув <вкл|выкл>"));
        assert!(error.contains("\\автоапрув вкл"));
    }
}
