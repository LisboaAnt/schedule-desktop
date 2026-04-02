# Próximos ciclos de desenvolvimento

Resumo alinhado a [PLANEJAMENTO.md](./PLANEJAMENTO.md) e [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md). Os **ajustes visuais** do widget/modo fundo encaixam na Fase 1; a partir daqui o foco muda para **dados reais e integração**.

---

## Onde estamos

| Fase | Estado |
|------|--------|
| **0 — Fundação** | Concluída (repo, Tauri, CI, build). |
| **1 — Shell + config** | Fechada para o essencial; **multi-monitor**: clamp + doc [MULTI-MONITOR.md](./MULTI-MONITOR.md) (modo WorkerW ainda a validar em todos os arranjos). |
| **2 — Google Calendar** | **Em curso** — próximo bloco principal. |
| **3 — Polimento / leveza** | Parcial: já existe **bandeja** básica; faltam sync periódico, iniciar com Windows, fechar→bandeja, etc. |
| **4 — Comunidade** | Por iniciar (CONTRIBUTING, templates, CHANGELOG, releases). |

---

## Ciclo imediato (Fase 2 — ordem sugerida)

1. **Armazenamento local** — SQLite: cache de eventos + `sync_state` (`syncToken` / `updatedMin` quando houver API).
2. **Modelo unificado** — tipo Rust (e espelho JSON) para evento: UI ↔ Rust ↔ Google Calendar API v3.
3. **Documentação OAuth** — [GOOGLE-CALENDAR-FASE2.md](./GOOGLE-CALENDAR-FASE2.md): tipo de cliente, redirect localhost, escopos; sem secrets no repo.
4. **OAuth2 no Rust** — PKCE + loopback; tokens no **Credential Manager** (Windows) via plugin/crate adequado.
5. **Cliente HTTP** — `events.list` / `insert` / `patch` / `delete` por intervalo visível.
6. **UI** — ligar mês/semana/dia ao cache + formulário mínimo de evento; fila offline para mutações sem rede.

---

## Ciclo seguinte (Fase 3)

- Sync automático configurável + “sincronizar agora”.
- Sync ao focar (com throttle).
- Lazy loading de meses na grelha.
- **Iniciar com o Windows** + opção fechar = minimizar para bandeja.
- Notas de RAM em idle no README.

---

## Ciclo comunidade (Fase 4)

- CONTRIBUTING.md, templates de issue/PR, CHANGELOG, releases com artefactos, SECURITY.md.

---

## Referência

- Checklist detalhada: [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md)
- Stack e módulos: [ARQUITETURA-E-STACK.md](./ARQUITETURA-E-STACK.md)
- Modo **atrás dos ícones** (WorkerW): roadmap tarefas **A + B** — [ROADMAP-WORKERW-AB.md](./ROADMAP-WORKERW-AB.md)
