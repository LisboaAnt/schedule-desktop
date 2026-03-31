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
