# Оценка понятности и качества tool surface

## Короткий вывод

Если разделять **архитектурное качество** и **простоту первого использования**, картина такая:

- как runtime/tool API: **сильно выше среднего**
- как UX для нового разработчика или оператора: **средне-хорошо, но не интуитивно**

В практических баллах:

- качество проектирования: `8/10`
- понятность без погружения в модель `session/run/job/inbox/wakeup`: `6/10`

Главная мысль: tools здесь в целом хорошо дисциплинированы, но система стала достаточно асинхронной и многослойной, поэтому многие сценарии понятны только после того, как человек соберёт в голове execution model.

## Что уже получилось хорошо

### 1. Surface в целом канонический и typed

Основной плюс — runtime не сваливается в shell-магии и не пытается представить side effects как “магический текст модели”.

Это видно в каталоге tools:

- есть явные семьи (`Filesystem`, `Exec`, `Planning`, `Offload`, `Memory`, `Mcp`, `Agent`);
- у каждого tool есть `description` и policy;
- входы typed и bounded;
- крупные результаты не обязаны раздувать prompt.

См. [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

### 2. Названия файловых и exec tools в основном удачные

Наиболее удачная часть surface:

- `fs_read_text`
- `fs_read_lines`
- `fs_find_in_files`
- `fs_patch_text`
- `fs_replace_lines`
- `exec_start`
- `exec_read_output`
- `exec_wait`

Эти имена хорошо объясняют intent и достаточно явно разделяют:

- чтение файла целиком;
- чтение диапазона строк;
- поиск;
- точечный patch;
- работу с процессом.

Это лучше, чем один перегруженный `fs_edit` или один “универсальный” shell tool.

### 3. У важных agent tools хорошие descriptions

Особенно удачно описаны:

- `message_agent`
- `session_read`
- `session_wait`
- `grant_agent_chain_continuation`

Например, `session_wait` прямо объясняет, что его надо использовать после `message_agent`, если нужен ответ другого агента до завершения хода. Это правильный уровень подсказки в самой схеме tool definition, а не только в README.

См. [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

### 4. Реально экспонируемый модели surface уже чище, чем полный runtime catalog

Это важный сильный момент: provider loop не вываливает модели всё подряд. Он строит automatic surface через `automatic_model_definitions()`.

Это значит, что в обычном model-driven tool loop модель видит именно канонические ids, а не весь исторический/compat слой.

См. [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs), [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

## Что остаётся самым запутанным

### 1. Сложные сценарии размазаны по нескольким tools

Самый заметный пример — межагентное общение:

- `message_agent` отправляет задачу;
- `session_wait` ждёт дочернюю session;
- `session_read` смотрит без ожидания;
- ответ может вернуться обратно ещё и через `inbox`/`wakeup`.

Архитектурно это нормально. UX-wise это тяжело, потому что оператор или новый разработчик ожидает один “tool ask_judge”, а получает цепочку из нескольких механизмов.

См. [`docs/current/05-interagent-background-and-schedules.md`](05-interagent-background-and-schedules.md).

### 2. Сущности `session`, `run`, `job`, `inbox event` и `wakeup` слишком связаны

Это не дефект конкретного названия тула, а дефект порога входа.

Пока человек не понял:

- что такое `session`;
- чем `run` отличается от `job`;
- почему background worker что-то делает позже;
- как inbox events будят session,

часть tool surface выглядит “нелогично”, хотя внутри runtime это последовательно.

Именно поэтому инструменты кажутся сложнее, чем они есть как API.

### 3. Legacy tool names всё ещё создают лишний шум

В коде всё ещё живут legacy ids:

- `fs_read`
- `fs_write`
- `fs_patch`
- `fs_search`

Документация честно говорит, что канонический surface — это typed variants, а legacy-имена остаются как совместимость.

Это правильно для обратной совместимости, но плохо для ясности: пока такие имена видны в коде и документации рядом с canonical tools, читатель тратит внимание на вопрос “а что из этого настоящее?”.

См. [`docs/current/04-tools-and-approvals.md`](04-tools-and-approvals.md), [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs).

### 4. Есть рассинхрон между “полным catalog” и “обычно видимым модели surface”

Это тонкий, но важный момент.

- `ToolCatalog::all_definitions()` содержит и compatibility layer;
- `automatic_model_definitions()` — уже отфильтрованный model-facing surface.

Архитектурно это разумно. Но без явного объяснения разработчик может решить, что все определения одинаково важны для модельного пути.

См. [`crates/agent-runtime/src/tool.rs`](../../crates/agent-runtime/src/tool.rs), [`cmd/agentd/src/agents.rs`](../../cmd/agentd/src/agents.rs), [`cmd/agentd/src/execution/provider_loop.rs`](../../cmd/agentd/src/execution/provider_loop.rs).

## Какие tools сейчас получились лучше всего

Лучше всего спроектированы tools, где:

- имя почти полностью описывает intent;
- у результата понятная форма;
- semantics короткие и локальные.

К этой группе относятся:

- файловые typed tools;
- exec quartet (`exec_start`, `exec_read_output`, `exec_wait`, `exec_kill`);
- plan tools;
- offload retrieval (`artifact_read`, `artifact_search`).

В этих местах surface выглядит зрелым и достаточно “скучным” в хорошем смысле: понятно, что tool делает, и мало места для фантазий модели.

## Какие tools сейчас самые трудные для освоения

Наиболее тяжёлые для первого использования:

- `message_agent`
- `session_wait`
- `grant_agent_chain_continuation`
- schedule tools
- memory/session tools как отдельный класс (`session_search`, `session_read`, `session_wait`)

Причина одна и та же: они опираются не только на локальную операцию, но и на общую execution model.

## Что я бы поменял без ломки архитектуры

### 1. Не менять core ids, а усилить recipes

Я бы не переименовывал канонические ids вроде `message_agent` или `session_wait`.

Они уже встроены в runtime и достаточно точны. Переименование сейчас даст больше churn, чем пользы.

Вместо этого я бы добавил в docs и help короткие recipes:

- “спросить judge и дождаться ответа”;
- “отправить задачу агенту и просто открыть дочернюю session”;
- “прочитать файл, изменить, проверить”;
- “запустить процесс и мониторить вывод”.

То есть объяснять не только tool-by-tool, а task-by-task.

### 2. Явно отделить canonical tools от compatibility layer

Сейчас это уже частично сделано текстом, но я бы сделал различие ещё жёстче:

- в docs пометить legacy tools как compatibility-only;
- не включать их в обзорные списки рядом с canonical variants без отдельной пометки;
- где возможно, не использовать их в примерах вообще.

Идея простая: compatibility должна существовать, но не должна конкурировать за внимание с основным surface.

### 3. Добавить один документ “mental model execution”

Сейчас информация разложена по нескольким текущим документам, и это уже полезно.

Но для tool usability особенно нужен очень короткий документ уровня:

- `session` — контейнер диалога;
- `run` — одно выполнение модели;
- `job` — фоновая работа вокруг run;
- `inbox` — отложенная доставка событий;
- `wakeup` — способ продолжить session после события.

Пока этой сжатой схемы нет в одном месте, сложные tools неизбежно будут казаться менее понятными, чем они на самом деле.

### 4. В operator help показывать составные сценарии, а не только команды

Сейчас help уже честно описывает `judge` и inter-agent path.

Следующий шаг — сделать в help именно микро-сценарии:

1. отправил `\судья ...`
2. получил queued child session
3. либо открыл child session
4. либо дождался через `session_wait`

Такие сценарии резко снижают порог входа без изменения runtime semantics.

## Итоговая оценка

Если смотреть как на инженерный runtime, tool surface уже выглядит крепко:

- он канонический;
- typed;
- bounded;
- policy-aware;
- без второго скрытого execution path.

Если смотреть как на UX для нового человека, проблема не в том, что инструменты “плохо придуманы”, а в том, что они требуют понимания общей execution model.

Поэтому честная формулировка такая:

- **спроектировано хорошо**
- **объяснено уже неплохо**
- **интуитивность всё ещё отстаёт от архитектурной строгости**

Это нормальная стадия для системы, которая уже вышла за пределы “простого чата с тулзами” и стала полноценным локальным agent runtime.
