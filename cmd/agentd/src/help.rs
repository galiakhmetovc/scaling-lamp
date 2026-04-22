#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HelpTopic {
    Overview,
    Commands,
    Settings,
    Judge,
}

pub(crate) const QUICK_HELP_LINE: &str = "команды: \\агенты | \\агент показать | \\агент выбрать <id> | \\агент создать <имя> [из <шаблона>] | \\агент открыть | \\расписания | \\расписание создать <id> <секунды> [agent=<id>] :: <промпт> | \\расписание показать <id> | \\расписание удалить <id> | \\версия | \\обновить | \\система | \\артефакты | \\артефакт <id> | \\помощь [команды|настройки|судья] | \\настройки | \\контекст | \\план | \\статус | \\процессы | \\пауза | \\стоп | \\задачи | \\скиллы | \\включить <скилл> | \\выключить <скилл> | \\доводка <N|выкл> | \\автоапрув <вкл|выкл> | \\апрув [id] | \\выход";

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
            "\\агент <показать|выбрать|создать|открыть> ...",
            &[
                "\\агент показать",
                "\\агент выбрать judge",
                "\\агент создать ревьюер из judge",
                "\\агент открыть",
            ],
        ),
        "/schedule" => (
            "\\расписание <показать|создать|удалить> ...",
            &[
                "\\расписание показать judge-pulse",
                "\\расписание создать judge-pulse 300 :: проверь свежие изменения и дай краткую сводку",
                "\\расписание создать judge-pulse 300 agent=judge :: проверь свежие изменения и дай краткую сводку",
                "\\расписание удалить judge-pulse",
            ],
        ),
        "/artifact" => (
            "\\артефакт <artifact-id>",
            &["\\артефакт artifact-tool-offload-session-123.bin"],
        ),
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
        "- \\агент создать <имя> [из <template>] — создать агента из шаблона default/judge",
        "- \\агент открыть [id|name] — показать путь к agent_home для ручного редактирования",
        "- \\расписания — показать расписания для текущего workspace",
        "- \\расписание показать <id> — показать одно расписание",
        "- \\расписание создать <id> <секунды> [agent=<id>] :: <промпт> — каждые N секунд запускать свежую сессию агента",
        "- \\расписание удалить <id> — удалить расписание",
        "- \\переименовать — переименовать текущую сессию",
        "- \\очистить — очистить текущую сессию и начать заново",
        "- \\выход — выйти из TUI/REPL",
        "",
        "Диагностика и контекст:",
        "- \\версия — показать текущую версию, путь к бинарю и статус обновления",
        "- \\обновить — заменить текущий бинарь release-сборкой из workspace",
        "- \\помощь [команды|настройки|судья] — открыть русскую справку",
        "- \\настройки — краткая справка по настройкам сессии",
        "- \\система — показать SessionHead, SYSTEM.md, AGENTS.md и собранные системные блоки",
        "- \\контекст — показать состояние контекста и compaction",
        "- \\план — показать текущий план",
        "- \\статус — показать активный run, его статус и активные процессы",
        "- \\процессы — показать активные exec-процессы с полной командой и cwd",
        "- \\пауза — мягкая пауза; сейчас честно работает как операторская остановка текущего хода",
        "- \\стоп — остановить активный run оператора и завершить висящий exec_wait",
        "- \\задачи — показать активные фоновые задачи текущей сессии",
        "- \\артефакты — показать offload-артефакты текущей сессии",
        "- \\артефакт <id> — открыть содержимое одного артефакта",
        "- \\отладка — сохранить debug bundle в файл",
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
        "- \\компакт — вручную выполнить compaction контекста",
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
        "\\пауза / \\стоп",
        "- \\пауза сейчас работает как мягкая остановка и пока эквивалентна операторской остановке",
        "- \\стоп немедленно отправляет операторскую остановку активного хода",
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
        "- сейчас compaction не автоматическая; trigger только явной командой",
        "",
        "\\обновить",
        "- копирует `target/release/agentd` поверх текущего бинаря",
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
        "- межагентный вызов идёт через tool `message_agent`",
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
        assert!(rendered.contains("allowlist"));
    }

    #[test]
    fn command_usage_renders_examples_for_argument_commands() {
        let usage = render_command_usage("/completion").expect("usage");
        assert!(usage.contains("Формат: \\доводка <N|выкл>"));
        assert!(usage.contains("\\доводка 3"));
    }

    #[test]
    fn command_usage_error_appends_format_and_examples() {
        let error = render_command_usage_error("/autoapprove", "не хватает аргументов");
        assert!(error.contains("не хватает аргументов"));
        assert!(error.contains("Формат: \\автоапрув <вкл|выкл>"));
        assert!(error.contains("\\автоапрув вкл"));
    }
}
