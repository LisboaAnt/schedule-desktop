# Calendário (desktop Windows)

Aplicação **Tauri 2** + **HTML/CSS/JS**: modo **widget** (foco) e modo **aplicação**, com personalização de tema e opacidade. Integração com **Google Calendar** está planeada na Fase 2.

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
| [claude.md](claude.md) | Contexto para IA e contribuidores |

## Estado atual (v0.1.0)

- Janela sem decoração nativa, com região de arrastar (`data-tauri-drag-region`).
- **Persistência de posição/tamanho** via `tauri-plugin-window-state`.
- **Config** (`viewMode`, `theme`, `widgetOpacity`) em JSON na pasta de config da app.
- Grelha de **mês** (navegação, hoje realçado); sem sync Google ainda.

## Licença

MIT — ver [LICENSE](LICENSE).
