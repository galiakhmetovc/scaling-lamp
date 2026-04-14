# Launcher And Ops

## Как бот запускается

Сейчас нормальный launcher — это:

- [scripts/teamd-agentctl](/home/admin/AI-AGENT/data/projects/teamD/scripts/teamd-agentctl)

Он ставит `systemd --user` units для:

- `teamd-main.service`
- `teamd-helper.service`

## Почему не обычный background shell

Потому что detached shell path уже ломался:

- stale `agent.pid`
- бот умирал после выхода shell
- оставались stray pollers
- Telegram давал `409 getUpdates conflict`

`systemd --user` решает это лучше:

- `Restart=always`
- `MAINPID` известен
- есть `journalctl`
- cleanup можно делать через `ExecStartPre`

## Основные команды

```bash
./scripts/teamd-agentctl status teamd-main
./scripts/teamd-agentctl restart teamd-main
./scripts/teamd-agentctl logs teamd-main --lines 100
```

И то же самое для `teamd-helper`.

## Что проверять, если бот не отвечает

1. `./scripts/teamd-agentctl status teamd-main`
2. `./scripts/teamd-agentctl logs teamd-main --lines 100`
3. `ss -ltnp | grep 18081`
4. trace directory
5. Postgres доступность

## Live workspace contract

Главные live paths:

- `/home/administrator/teamD`
- `/home/administrator/teamD-helper`

Там лежат:

- `teamd-agent`
- `.env`
- `agent.pid`
- `var/`

`agent.pid` теперь пишет systemd, а не shell wrapper.
