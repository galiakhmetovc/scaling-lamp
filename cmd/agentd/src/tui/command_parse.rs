use crate::bootstrap::BootstrapError;
use crate::help::render_command_usage_error;

pub(super) fn require_arg(raw: &str, command: &str) -> Result<String, BootstrapError> {
    if raw.trim().is_empty() {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(command, "не хватает аргументов"),
        });
    }
    Ok(raw.trim().to_string())
}

pub(super) fn option_arg(raw: &str) -> Option<String> {
    (!raw.trim().is_empty()).then(|| raw.trim().to_string())
}

pub(super) fn parse_optional_positive_usize(
    raw: Option<&str>,
    command: &str,
) -> Result<Option<usize>, BootstrapError> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    let value = raw.parse::<usize>().map_err(|_| BootstrapError::Usage {
        reason: render_command_usage_error(command, "ожидается положительное целое число"),
    })?;
    if value == 0 {
        return Err(BootstrapError::Usage {
            reason: render_command_usage_error(command, "значение должно быть больше нуля"),
        });
    }
    Ok(Some(value))
}

pub(super) fn parse_completion_nudges(raw: &str) -> Result<Option<u32>, BootstrapError> {
    let trimmed = raw.trim();
    if matches!(trimmed, "off" | "выкл" | "disable") {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| BootstrapError::Usage {
            reason: render_command_usage_error(
                "/completion",
                &format!(
                    "неподдерживаемый режим доводки {trimmed}; ожидается выкл или неотрицательное число"
                ),
            ),
        })
}

pub(super) fn describe_completion_mode(completion_nudges: Option<u32>) -> String {
    match completion_nudges {
        None => "выключен".to_string(),
        Some(0) => "включён: после первой ранней остановки сразу нужен апрув оператора".to_string(),
        Some(value) => format!("включён: {value} автоматических пинка перед апрувом"),
    }
}

pub(super) fn parse_auto_approve(raw: &str) -> Result<bool, BootstrapError> {
    match raw.trim() {
        "on" | "1" | "yes" | "да" | "вкл" | "enable" => Ok(true),
        "off" | "0" | "no" | "нет" | "выкл" | "disable" => Ok(false),
        value => Err(BootstrapError::Usage {
            reason: render_command_usage_error(
                "/autoapprove",
                &format!("неподдерживаемый режим автоапрува {value}; ожидается вкл|выкл"),
            ),
        }),
    }
}

pub(super) fn is_command_input(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('/') || trimmed.starts_with('\\')
}

pub(super) fn canonical_command(command: &str) -> Option<&'static str> {
    let normalized = command.trim_end_matches(['\\', '/']);
    match normalized {
        "/session" | "\\сессии" => Some("/session"),
        "/new" | "\\новая" => Some("/new"),
        "/agents" | "\\агенты" => Some("/agents"),
        "/agent" | "\\агент" => Some("/agent"),
        "/judge" | "/судья" | "\\судья" => Some("/judge"),
        "/schedules" | "\\расписания" => Some("/schedules"),
        "/schedule" | "\\расписание" => Some("/schedule"),
        "/mcp" | "\\mcp" => Some("/mcp"),
        "/memory" | "/память" | "\\память" => Some("/memory"),
        "/chain" | "/цепочка" | "\\цепочка" => Some("/chain"),
        "/rename" | "\\переименовать" => Some("/rename"),
        "/clear" | "\\очистить" => Some("/clear"),
        "/help" | "\\помощь" => Some("/help"),
        "/version" | "/версия" | "\\версия" => Some("/version"),
        "/logs" | "/логи" | "\\логи" => Some("/logs"),
        "/update" | "/обновить" | "\\обновить" => Some("/update"),
        "/settings" | "\\настройки" => Some("/settings"),
        "/debug" | "\\отладка" => Some("/debug"),
        "/debug-view" | "\\дебаг" | "\\отладчик" => Some("/debug-view"),
        "/system" | "/система" | "\\система" => Some("/system"),
        "/plan" | "\\план" => Some("/plan"),
        "/status" | "\\статус" => Some("/status"),
        "/processes" | "\\процессы" => Some("/processes"),
        "/pause" | "\\пауза" => Some("/pause"),
        "/stop" | "\\стоп" => Some("/stop"),
        "/cancel" | "\\отмена" => Some("/cancel"),
        "/jobs" | "\\фоновые" => Some("/jobs"),
        "/tasks" | "\\задачи" => Some("/tasks"),
        "/task" | "\\задача" => Some("/task"),
        "/artifacts" | "/артефакты" | "\\артефакты" => Some("/artifacts"),
        "/artifact" | "/артефакт" | "\\артефакт" => Some("/artifact"),
        "/context" | "\\контекст" => Some("/context"),
        "/completion" | "\\доводка" => Some("/completion"),
        "/autoapprove" | "\\автоапрув" => Some("/autoapprove"),
        "/skills" | "\\скиллы" => Some("/skills"),
        "/enable" | "\\включить" => Some("/enable"),
        "/disable" | "\\выключить" => Some("/disable"),
        "/approve" | "\\апрув" => Some("/approve"),
        "/model" | "\\модель" => Some("/model"),
        "/reasoning" | "\\размышления" => Some("/reasoning"),
        "/think" | "\\думай" => Some("/think"),
        "/compact" | "\\компакт" => Some("/compact"),
        "/exit" | "\\выход" => Some("/exit"),
        _ => None,
    }
}
