# Changelog

O formato baseia-se em [Keep a Changelog](https://keepachangelog.com/pt-PT/1.1.0/).

## [Unreleased]

### Adicionado

- **GitHub Releases**: workflow `release-windows.yml` só com **disparo manual** (`workflow_dispatch`); CI em PR + manual (sem CI em cada push a `master`). [docs/RELEASE.md](docs/RELEASE.md).

### Alterado

- **Dependências**: documento [docs/DEPENDENCIAS.md](docs/DEPENDENCIAS.md) com inventário; sem remoções (todas as crates em uso).
- **Listas Semana / Dia**: limite 60/120 cartões com «+N mais»; «Mostrar todos» / «Mostrar só os primeiros N»; ordenação com `compareAgendaTasks`; expansão reposta ao mudar semana ou dia.
- **Comunidade**: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) (Contributor Covenant 2.1, PT).
- **Multi-monitor**: se a janela principal não intersecta nenhum ecrã após restauro do estado, centra automaticamente (arranque com pequeno atraso, `RunEvent::Resumed`, foco da janela); comando `ensure_main_window_on_screen`. Ver [docs/MULTI-MONITOR.md](docs/MULTI-MONITOR.md).
- **Transparência**: janela principal `transparent: true` (Tauri); slider em definições ajusta `--fill-a` nos fundos (CSS `rgb` com alpha) para ver o ambiente de trabalho; deixa de se usar só `opacity` no `body`.
- **Definições**: transparência 0–65%; removida «densidade da vista»; `viewMode` gravado como `widget`; fundo da área rolável sem camada opaca extra.
- **Vista mês**: linhas de tarefas por célula conforme altura (`ResizeObserver`); células sem barra de scroll.

### Notas

- Reiniciar a app após alterar `tauri.conf.json` (transparência nativa).

## [0.1.2] — 2026-04-06

### Alterado

- Modo «atrás dos ícones»: após ancorar ao WorkerW, a janela pode alinhar-se à área útil do monitor (`snap`); opt-out com `AGENDA_WALLPAPER_SKIP_WORK_AREA_SNAP=1`. Ver [MULTI-MONITOR.md](docs/MULTI-MONITOR.md).
- Persistência da geometria da janela: gravação do estado após estabilizar o layout no arranque e gravação periódica (5 min) para reduzir desvio após crash/vigia.
- Vigia (`agenda-watchdog`): variáveis `AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS` e `AGENDA_WATCHDOG_RELUNCH_ON_ZERO`; documentação em [WATCHDOG.md](docs/WATCHDOG.md) e [COMO-RODAR.md](docs/COMO-RODAR.md).
- [RELEASE.md](docs/RELEASE.md): endpoint do updater, `latest.json`, checklist de versão e branch `master`.
