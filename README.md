# Agenda (desktop Windows)

Aplicação **Tauri 2** + **HTML/CSS/JS**: **agenda** com vistas **mês / semana / dia**, layout tipo **widget**, **tema** e **transparência** (janela nativa + fundos sem opacidade sólida a tapar o ambiente de trabalho). No **Windows**, modo opcional **atrás dos ícones** (com pílula para voltar). Integração **Google Calendar** — ver [docs/CICLOS-PROXIMOS.md](docs/CICLOS-PROXIMOS.md).

## Requisitos

- **Windows 11** (ou 10 com WebView2)
- [Node.js](https://nodejs.org/) (LTS)
- [Rust](https://rustup.rs/) + **Visual Studio Build Tools** (C++ / MSVC) — ver [pré-requisitos Tauri](https://v2.tauri.app/start/prerequisites/)

> Neste ambiente de desenvolvimento, confirme com `rustc --version` e `node --version`.

## Como executar

```powershell
cd E:\github\Calendario-app
npm install
npm run dev
```

Build de release:

```powershell
npm run build
```

Os instaladores (`.msi` e `.exe` NSIS) saem em `src-tauri/target/release/bundle/`.

## Descarregar do GitHub (releases)

Quando o repositório está no GitHub e as **Actions** têm permissão de escrita, ao publicares uma **tag** `v0.1.0` (etc.) o workflow **Release (Windows)** gera os instaladores e anexa-os à página **Releases**.

Passo a passo: **[docs/RELEASE.md](docs/RELEASE.md)** (permissões, alinhar `tauri.conf.json` / `Cargo.toml`, `git tag` + `git push`).

## Documentação

| Ficheiro | Conteúdo |
|----------|-----------|
| [docs/COMO-RODAR.md](docs/COMO-RODAR.md) | Detalhes, Docker vs nativo |
| [docs/PLANEJAMENTO.md](docs/PLANEJAMENTO.md) | Visão e fases |
| [docs/TAREFAS-POR-CICLO.md](docs/TAREFAS-POR-CICLO.md) | Checklist |
| [docs/CICLOS-PROXIMOS.md](docs/CICLOS-PROXIMOS.md) | Próximas fases (após UI) |
| [docs/MULTI-MONITOR.md](docs/MULTI-MONITOR.md) | Vários ecrãs e estado da janela |
| [docs/DEPENDENCIAS.md](docs/DEPENDENCIAS.md) | Auditoria de crates / npm |
| [CHANGELOG.md](CHANGELOG.md) | Alterações por versão |
| [docs/RELEASE.md](docs/RELEASE.md) | Publicar instaladores no GitHub Releases |
| [docs/GOOGLE-CALENDAR-FASE2.md](docs/GOOGLE-CALENDAR-FASE2.md) | Guia OAuth / API (Fase 2) |
| [claude.md](claude.md) | Contexto para IA e contribuidores |
| [SECURITY.md](SECURITY.md) | Reporte responsável de vulnerabilidades |

## Estado atual (v0.1.0)

- Janela sem decoração: **barra de arrastar** + vistas **Mês / Semana / Dia**; integração **Google Calendar** com cache local e editor de eventos.
- **Transparência**: WebView/janela transparentes; slider em definições controla alpha dos fundos (não só escurecer o `body`).
- **Persistência**: `window-state` + `config.json` (`theme`, `widgetOpacity`, `agendaView`, `autoSyncMinutes`, `closeToTray`, …); `viewMode` fixo `widget` na gravação.
- **SQLite** local: `agenda_cache.sqlite3` (cache de eventos, `sync_state`, fila offline de mutações).
- **Windows**: modo fundo do ambiente de trabalho (WorkerW), pílula de restauro, bandeja, autostart opcional.

Ver também **[CHANGELOG.md](CHANGELOG.md)** (secção *Unreleased* para o que ainda não está etiquetado em release).

## Código de conduta

[CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) (Contributor Covenant 2.1, português).

## Licença

MIT — ver [LICENSE](LICENSE).
