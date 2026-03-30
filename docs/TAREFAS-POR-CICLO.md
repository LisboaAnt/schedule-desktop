# Tarefas por ciclo (fases)

Checklist operacional alinhada ao [PLANEJAMENTO.md](./PLANEJAMENTO.md). Marque `[x]` conforme for concluindo.

---

## Fase 0 — Fundação

- [x] Escolher e adicionar **licença** no repositório (ex.: MIT).
- [x] **README.md** na raiz: descrição curta, link para `docs/`, requisitos de sistema.
- [x] Scaffold **Tauri 2** + frontend (HTML/CSS/JS ou template escolhido).
- [x] Estrutura de pastas acordada (`src-tauri/`, `src/` ou `ui/`, `docs/`).
- [x] **`.gitignore`**: `node_modules/`, `target/`, `.env`, credenciais, builds locais.
- [x] **CI** (GitHub Actions ou similar): build Windows em release (`.github/workflows/ci.yml`).
- [ ] `cargo clippy` / testes Rust no CI (opcional além do build).
- [x] Documentar versões mínimas: **Rust**, **Node**, **WebView2** — ver [COMO-RODAR.md](./COMO-RODAR.md).
- [x] Primeiro **`tauri build`** gerando instalador/portable no Windows (MSI + NSIS em `src-tauri/target/release/bundle/`).

---

## Fase 1 — Shell do app + configuração

- [x] Janela principal com título, ícone e tamanho mínimo adequado ao widget.
- [x] Persistir **posição e tamanho** da janela ao fechar; restaurar ao abrir (`tauri-plugin-window-state`).
- [ ] Suporte básico a **múltiplos monitores** (validar e ajustar se necessário).
- [x] Alternar **modo widget** × **modo app** (layout distinto).
- [x] Opções de janela para widget: **sem decoração**; redimensionável; região de arrastar.
- [x] Persistência de preferências em arquivo local (JSON via `app_config_dir`).
- [x] Esqueleto de **tema**: variáveis CSS + claro / escuro / sistema.
- [x] Painel de **definições** (tema, opacidade); falta: abrir pasta de dados, reset layout.

---

## Fase 2 — Google Calendar

- [x] Esboço **SQLite** (`agenda_cache.sqlite3`): tabelas `cached_events` + `sync_state` (ver `src-tauri/src/local_store.rs`).
- [x] Tipo **CalendarEvent** unificado em Rust (`calendar_model.rs`) e comando **`get_calendar_state`** (UI ainda em demo).
- [x] Documentação: [GOOGLE-CALENDAR-FASE2.md](./GOOGLE-CALENDAR-FASE2.md), [CICLOS-PROXIMOS.md](./CICLOS-PROXIMOS.md).
- [ ] Projeto no **Google Cloud Console**: tipo de cliente OAuth adequado a app desktop, URIs de redirecionamento documentados.
- [x] Fluxo **OAuth2** (PKCE / localhost): login, troca de código, **refresh token** seguro.
- [x] Armazenar tokens com **Credential Manager** do Windows (ou abstração Tauri equivalente).
- [x] Cliente Calendar API v3 no **Rust**: `events.list` por intervalo + **sync incremental** com `syncToken` (calendário `primary`).
- [x] **Criar** evento no calendário `primary` (`events.insert`) + entrada na cache local.
- [x] **Atualizar / apagar** evento (`events.patch`, `events.delete`) + cache; UI em folha ao clicar na vista Semana/Dia.
- [x] **Modelo unificado** de evento (UI ↔ Rust ↔ JSON API) — leitura/listagem; escrita em falta.
- [x] **SQLite**: cache de eventos + `sync_state` com `nextSyncToken` / incremental; `updatedMin` não usado.
- [ ] **Fila offline**: mutações enfileiradas quando sem rede; envio com retry e tratamento de erro.
- [ ] UI: lista/semana/mês mínimo viável + formulário de evento (título, início/fim, calendário).
- [ ] Documentar **escopos** e limites de quota para utilizadores/contribuidores.

---

## Fase 3 — Polimento e leveza

- [x] **Ícone na bandeja** (básico): menu / clique para trazer a janela à frente (completar: ocultar, sair, etc.).
- [ ] Intervalo de **sync automático** configurável + botão “sincronizar agora”.
- [ ] Sync ao **focar** a janela (opcional, com throttle).
- [ ] **Lazy loading** de meses/dias na UI; evitar renderizar milhares de nós de uma vez.
- [ ] Revisão de **dependências** (remover o que não for essencial).
- [ ] **Iniciar com o Windows**: implementar e documentar (Startup ou `Run`).
- [ ] **Ícone na bandeja**: mostrar/ocultar widget, sair de verdade (além do “trazer à frente”).
- [ ] Comportamento opcional: **fechar = minimizar** para bandeja.
- [ ] Opções avançadas de personalização: **opacidade**, densidade, fonte, cores por token de tema.
- [ ] Medição informal de **RAM em idle** (notas no README ou doc de release).

---

## Fase 4 — Comunidade

- [x] **CONTRIBUTING.md** (base): como rodar, PRs, sem secrets — expandir com branches/templates depois.
- [ ] **CODE_OF_CONDUCT.md** (opcional mas recomendado para OSS).
- [ ] Templates de **issue** (bug, feature) e **pull request**.
- [ ] **CHANGELOG.md** ou releases com notas por versão.
- [ ] Pipeline de **release**: artefatos `.msi`/`.exe` (ou nsis) anexados ao GitHub Releases.
- [ ] Instruções para **assinatura** de binários (quando houver certificado).
- [ ] Política de segurança (**SECURITY.md**) e canal para reportar vulnerabilidades.

---

## Pós-MVP (backlog, fora dos ciclos obrigatórios)

- [ ] “Sempre no topo” e **click-through** (se desejado e viável no Windows).
- [ ] Outros provedores (Outlook, etc.).
- [ ] Builds **macOS/Linux** se a base Tauri permitir sem reescrever metade do app.

---

## Referência rápida

| Fase | Foco principal |
|------|----------------|
| 0 | Repo + Tauri + CI + primeiro build |
| 1 | Janela, prefs, widget vs app, tema base |
| 2 | OAuth + API Google + SQLite + fila offline + UI de eventos |
| 3 | Sync inteligente, tray, startup, leveza, customização visual |
| 4 | OSS maduro: docs, templates, releases |
