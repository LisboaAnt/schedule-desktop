# Agenda (desktop Windows)

Aplicação **Tauri 2** + **HTML/CSS/JS**: **agenda** com tarefas por dia (dados de demonstração), vistas **mês / semana / dia**, modo **widget** e modo **aplicação**, tema e opacidade. No **Windows**, modo opcional **atrás dos ícones** do ambiente de trabalho (com pílula para voltar). Integração **Google Calendar** — ver [docs/CICLOS-PROXIMOS.md](docs/CICLOS-PROXIMOS.md).

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

## Documentação

| Ficheiro | Conteúdo |
|----------|-----------|
| [docs/COMO-RODAR.md](docs/COMO-RODAR.md) | Detalhes, Docker vs nativo |
| [docs/PLANEJAMENTO.md](docs/PLANEJAMENTO.md) | Visão e fases |
| [docs/TAREFAS-POR-CICLO.md](docs/TAREFAS-POR-CICLO.md) | Checklist |
| [docs/CICLOS-PROXIMOS.md](docs/CICLOS-PROXIMOS.md) | Próximas fases (após UI) |
| [docs/GOOGLE-CALENDAR-FASE2.md](docs/GOOGLE-CALENDAR-FASE2.md) | Guia OAuth / API (Fase 2) |
| [claude.md](claude.md) | Contexto para IA e contribuidores |
| [SECURITY.md](SECURITY.md) | Reporte responsável de vulnerabilidades |

## Estado atual (v0.1.0)

- Janela sem decoração: **barra de arrastar** + vistas **Mês / Semana / Dia**; dados **demo** em JS até à Fase 2.
- **Persistência**: `window-state` + `config.json` (`viewMode`, `theme`, `widgetOpacity`, `agendaView`, …).
- **SQLite** local: `agenda_cache.sqlite3` (cache de eventos, `sync_state`, fila offline de mutações).
- **Windows**: modo fundo do ambiente de trabalho (WorkerW), pílula de restauro, bandeja.

## Licença

MIT — ver [LICENSE](LICENSE).
