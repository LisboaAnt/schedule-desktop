# claude.md — Calendário Desktop (Widget + App)

Este arquivo documenta o projeto para **desenvolvedores humanos** e para **assistentes de IA** (Claude, Cursor, etc.). Mantê-lo alinhado com `docs/` quando o produto evoluir.

---

## O que é o projeto

Agenda / calendário para **Windows 11** com:

- **Modo widget** (prioridade): janela leve, posição/tamanho personalizáveis; no **Windows** há modo **atrás dos ícones** (WorkerW) com pílula para voltar.
- **Modo aplicativo**: mesma base, UI expandida.
- **Vistas** mês, semana e dia; tarefas por dia (demo em `src/agenda.js` até haver Google Calendar).
- **Sincronização bidirecional com Google Calendar** (planeada).
- **Visual personalizável** (tema, transparência da janela via `--fill-a` + janela Tauri transparente; mais tokens depois).
- Foco em **baixo uso de RAM, CPU e rede**; adequado a **iniciar com o Windows**.
- **Open source** no GitHub.

---

## Documentação oficial do planejamento

| Documento | Conteúdo |
|-----------|-----------|
| [docs/PLANEJAMENTO.md](./docs/PLANEJAMENTO.md) | Visão, requisitos, fases, riscos, critérios de sucesso |
| [docs/TAREFAS-POR-CICLO.md](./docs/TAREFAS-POR-CICLO.md) | Tarefas/checklist por fase (Fase 0–4 + backlog) |
| [docs/CICLOS-PROXIMOS.md](./docs/CICLOS-PROXIMOS.md) | Próximos ciclos após UI (Fase 2 em diante) |
| [docs/GOOGLE-CALENDAR-FASE2.md](./docs/GOOGLE-CALENDAR-FASE2.md) | OAuth + Calendar API (implementação) |
| [docs/COMO-RODAR.md](./docs/COMO-RODAR.md) | Rodar no PC (Windows 11), toolchain nativa vs Docker |
| [docs/ARQUITETURA-E-STACK.md](./docs/ARQUITETURA-E-STACK.md) | Stack recomendada (Tauri + WebView2), módulos, sync, otimizações, segurança |
| [CHANGELOG.md](./CHANGELOG.md) | Alterações por versão |
| [docs/MULTI-MONITOR.md](./docs/MULTI-MONITOR.md) | Vários ecrãs, estado da janela, modo atrás dos ícones |
| [docs/DEPENDENCIAS.md](./docs/DEPENDENCIAS.md) | Crates Rust e npm — auditoria |
| [docs/RELEASE.md](./docs/RELEASE.md) | Instaladores no GitHub Releases |
| [CODE_OF_CONDUCT.md](./CODE_OF_CONDUCT.md) | Pacto de Contribuinte (comunidade) |

Leia estes dois antes de implementar funcionalidades grandes.

---

## Decisões técnicas (resumo)

- **Shell**: Tauri 2 + WebView2 (Windows).
- **Lógica sensível e rede**: Rust (comandos Tauri / IPC), tokens no armazenamento seguro do SO.
- **UI**: web enxuta; preferir stack frontend leve (vanilla, Svelte ou Solid).
- **Dados locais**: SQLite para cache e fila offline; JSON para prefs simples no início.
- **API**: Google Calendar API v3 + OAuth2 com práticas para app desktop (PKCE, etc.).

---

## Convenções para quem contribui

1. **Não** commitar secrets, refresh tokens ou `.env` com credenciais reais.
2. **Manter leveza**: questionar cada dependência npm/cargo nova.
3. **Sync**: mudanças que afetem modelo de eventos devem considerar conflito offline/online e quotas da API.
4. **Windows**: testar widget (posição multi-monitor), iniciar com Windows e tray quando existirem.
5. Idioma dos docs de produto: **português** (alinhado ao autor); código e nomes de API em inglês são aceitáveis.

---

## O que já existe no repositório

- **Tauri 2** + frontend **vanilla** em `src/` (`index.html`, `styles.css`, `main.js`).
- **Plugin window-state**: posição e tamanho da janela persistidos.
- **Comandos Rust** `get_app_config` / `save_app_config` (JSON em `app_config_dir`).
- UI: agenda (mês/semana/dia), sync Google, editor de eventos, arrastar na barra superior (`-webkit-app-region: drag`), definições com transparência; vista mês com número de linhas por célula conforme altura da janela.
- **LICENSE** (MIT), **README.md**, **CI** (`.github/workflows/ci.yml` — build Windows).
- Ícones em `src-tauri/icons/` (gerados a partir de fonte removida do repo).

Próximos passos: **lazy loading** na UI (muitos eventos), **revisão de dependências**, **CHANGELOG/releases**, validação **multi-monitor** — ver [docs/CICLOS-PROXIMOS.md](./docs/CICLOS-PROXIMOS.md), [TAREFAS-POR-CICLO.md](./docs/TAREFAS-POR-CICLO.md) e [CHANGELOG.md](./CHANGELOG.md).

---

## Perguntas frequentes para IA ao trabalhar neste repo

**Onde está o “widget”?**  
Será uma janela Tauri com configuração de tamanho/posição/tema — não um gadget legado do Windows.

**Onde fica a sincronização?**  
Preferencialmente no **Rust**, exposta ao frontend via `invoke`; tokens não devem vazar para logs.

**Prioridade absoluta?**  
Leveza e modo widget; app completo é secundário mas mesma codebase.

---

## Atualizar este arquivo

Ao fechar uma fase grande ou mudar stack, atualizar:

- A tabela de links em “Documentação oficial”.
- “Decisões técnicas” se algo divergir de Tauri/WebView2/SQLite.
- “O que ainda não existe” conforme o repositório ganhar código.
