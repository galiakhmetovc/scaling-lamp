# Web Operator Console Design

## Goal

Перевести web UI из состояния "TUI в браузере" в нормальную операторскую консоль: сильная иерархия, один главный рабочий фокус, чище `Sessions/Chat` UX, и единый визуальный язык для `Plan`, `Tools`, `Settings`.

## Problems In Current UI

- Все основные поверхности выглядят почти одинаково, поэтому главный рабочий контекст не читается.
- Верхняя часть экрана работает как utility shell, а не как control header.
- `Chat` уже функционален, но визуально не закреплён как основное рабочее полотно.
- `Sessions` и `Chat` конкурируют между собой вместо явного разделения ролей.
- Мобильная адаптация остаётся побочным эффектом desktop-grid, а не осмысленным layout mode.

## Recommended Direction

`Operator console` с лёгким `mission control` слоем.

Это означает:

- не делать маркетинговый control center с лишней декоративностью;
- не оставлять старый TUI-like look;
- строить интерфейс как рабочую консоль, где композиция и контраст помогают работать, а не просто выглядят "темно и технично".

## Visual Thesis

Спокойная тёмная операторская консоль с холодным акцентом, жёсткой типографической иерархией, одним доминирующим рабочим полотном на экран и вторичными сервисными панелями, которые не спорят с главным потоком работы.

## Information Hierarchy

### Control Header

Header становится главным структурным слоем, а не просто полосой с заголовком:

- слева: продукт, active session, краткий session context;
- справа: daemon health, provider, model, connectivity;
- tabs интегрированы в тот же слой, а не висят отдельной полосой без роли.

### Primary Workspace

Каждая вкладка должна иметь один главный surface:

- `Chat`: timeline/composer;
- `Sessions`: session catalog;
- `Plan`: task tree/details;
- `Tools`: live operator actions/details;
- `Settings`: form/raw editing.

Secondary content не должен визуально повторять главный surface.

### Surface System

Нужны три уровня поверхностей:

1. `Primary surface` — главный рабочий холст.
2. `Secondary surface` — соседний contextual pane.
3. `Utility surface` — metadata, chips, small controls.

Они должны различаться по:

- плотности;
- контрасту;
- radius;
- padding;
- border prominence;
- shadow depth.

## Layout Rules

### Chat

- В `Chat` вкладке нет session rail.
- Основа: широкая timeline колонка + узкая sidebar для queue, `/btw`, run metadata.
- Composer закреплён внизу как primary action surface.
- Status bar принадлежит composer region, а не живёт как отдельный декоративный footer.

### Sessions

- `Sessions` это самостоятельный каталог сессий, а не постоянный rail для всего приложения.
- Нужны: clear active marker, search/filter-ready layout seam, last activity/status badges.

### Plan / Tools / Settings

- Используют тот же визуальный язык, но не копируют `Chat`.
- `Plan`: emphasis на selected task и detail reading.
- `Tools`: emphasis на approvals/running state и деталях действия.
- `Settings`: emphasis на revision, dirty state, apply/reload semantics.

## Typography

- Сильнее различить display/title/section/meta/body.
- Уменьшить количество визуально равных labels.
- Metadata должна быть тише, но не бледной до потери читаемости.
- Transcript и markdown blocks должны читаться как контент, а не как box metadata.

## Color System

- Базовая тема остаётся тёмной.
- Один основной холодный accent для active/focus/navigation.
- Один status accent для positive/running.
- Danger и warning используются только по делу.
- Не использовать равномерный accent noise по всему экрану.

## Motion

Motion должен помогать состоянию, а не украшать layout:

- мягкое переключение tabs;
- subtle stream reveal в chat timeline;
- run-active state должен ощущаться не только таймером, а поведением статуса и поверхности.

## Mobile Behavior

- Header остаётся компактным и читаемым.
- Tabs допускают горизонтальный скролл.
- В `Chat` sidebar уходит под timeline.
- Composer остаётся главной нижней областью.
- Dense desktop split layouts должны складываться предсказуемо, без ломки иерархии.

## Architecture Constraints

- Никакого отдельного web-only source of truth.
- Всё продолжает жить на текущем daemon API, bootstrap snapshot и websocket event stream.
- UI state разделяется на tab-specific modules и view-model seams, а не разрастается в `App.tsx`.
- Visual redesign не должен ломать multi-client daemon model.

## Testing Strategy

- `vitest` на view-model seams и ключевые layout decisions.
- build verification через `npm run build`.
- daemon/runtime regression tests только если нужны новые snapshot поля или event semantics.
- browser-smoke against live daemon после сборки.

## Delivery Order

1. Redesign shell: control header, tabs, base surface hierarchy.
2. Redesign `Sessions` and `Chat` layout/presentation under the new shell.
3. Re-style `Plan`, `Tools`, `Settings` to the same visual language.
4. Mobile/adaptive cleanup and live smoke verification.
