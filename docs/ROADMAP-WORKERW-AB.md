# Roadmap — Modo atrás dos ícones (WorkerW): endurecer (A) e evoluir produto (B)

Este documento descreve um **plano em duas camadas** alinhado a [MULTI-MONITOR.md](./MULTI-MONITOR.md) e ao código em `src-tauri/src/windows_desktop.rs` + `lib.rs` (watchdog, `wallpaper_try_reanchor`).

| Camada | Objetivo | Esforço relativo |
|--------|----------|------------------|
| **A — Endurecer WorkerW** | Menos sumiços, menos flicker, melhor diagnóstico, **sem** reescrever a UI em GPU. | Médio (iterativo) |
| **B — Qualidade de produto** | Separar “camada de fundo” da “UI rica” para não depender de WebView filho do `WorkerW` em tudo. | Alto (arquitetura) |

---

## Fase A — Endurecer o WorkerW (prioridade)

**Meta da fase:** Comportamento **previsível** após mudança de wallpaper, sleep, hot-plug de monitores e reinício do Explorer, com **mínimo** de piscar — **sem** reescrever a UI em GPU.

**Ordem sugerida:** A1 → A2 → A3 → A4 → A5 (A1 primeiro para medir o que A2–A4 alteram).

---

### A1 — Observabilidade (logs e reprodução)

#### Task A1.1 — Logs de ancoragem ao WorkerW

- **Objetivo:** Saber *porque* a janela deixou de estar correctamente parentada, sem adivinhar.
- **Escopo:** Em `windows_desktop.rs` / chamadas desde `lib.rs`, registar (nível `debug` ou atrás de variável de ambiente / build `debug`): resultado de `workerw_behind_icons()` (HWND obtido ou “nenhum”), falhas de `SetParent` / `SetWindowPos`, e `GetParent` da janela principal comparado ao WorkerW esperado.
- **Critérios de aceitação:** Com um build de debug, uma sequência de acções (mudar wallpaper, stress no Explorer) produz linhas de log suficientes para distinguir “WorkerW não encontrado” vs “parent incorrecto” vs “API Win32 falhou com código X”.

- [x] **A1.1** concluída (logs em `windows_desktop` + `workerw.log`; `GetLastError` nas falhas de `SetParent`)

#### Task A1.2 — Origem e contagem de reancoramentos

- **Objetivo:** Evitar loops invisíveis e perceber se o watchdog está a disparar em excesso.
- **Escopo:** Cada chamada a `wallpaper_try_reanchor` (ou equivalente) deve poder ser etiquetada com uma **origem**: `watchdog`, `arranque`, `single_instance`, `comando_ipc`, `outro` (enum ou strings fixas). Opcional: contadores por sessão ou ring buffer dos últimos N eventos só em debug.
- **Critérios de aceitação:** Num relatório de bug, consegue-se dizer “12 reancoramentos em 2 min, todos `watchdog`” ou “só no arranque”.

- [x] **A1.2** concluída (origens `watchdog`, `single_instance`, `resumed`, `setting_change`; contadores por origem em `lib.rs`)

#### Task A1.3 — Documentação de reprodução e logs

- **Objetivo:** Qualquer pessoa (ou tu daqui a 6 meses) reproduz o problema com passos iguais.
- **Escopo:** Secção nova ou subsecção em [MULTI-MONITOR.md](./MULTI-MONITOR.md): passos para testar wallpaper / slideshow / dois ecrãs; como activar logs; exemplo de 3–5 linhas “saudáveis” vs “anómalas”.
- **Critérios de aceitação:** Seguir o doc permite repetir o cenário e saber que strings procurar no output.

- [x] **A1.3** concluída ([MULTI-MONITOR.md](./MULTI-MONITOR.md) — diagnóstico, exemplos de linhas, matriz de testes)

---

### A2 — Reancoragem condicional e menos flicker

#### Task A2.1 — Detecção de pai incorrecto antes de reparentar tudo

- **Objetivo:** Não repetir `set_behind_icons` completo quando já estamos bem ancorados.
- **Escopo:** Guardar o último `HWND` do WorkerW usado com sucesso. Antes de reancorar, obter `GetParent` da janela principal; se for o WorkerW actual **e** a janela cumprir um teste mínimo de consistência (parent + visibilidade), **não** executar o caminho pesado.
- **Critérios de aceitação:** Com modo fundo estável, os logs mostram *skip* explícito na maioria dos ticks do watchdog, não reancoragem completa a cada intervalo.

- [x] **A2.1** concluída

#### Task A2.2 — `show` / `unminimize` só quando necessário

- **Objetivo:** Reduzir o “flash” de uma frame em que a janela salta de estado.
- **Escopo:** Rever `wallpaper_try_reanchor` (e chamadas similares): chamar `show()` / `unminimize()` apenas se a janela estiver minimizada ou invisível **e** for mesmo preciso para recuperar o modo fundo.
- **Critérios de aceitação:** Teste visual: activar modo atrás dos ícones várias vezes seguidas; comparar gravação de ecrã antes/depois — flicker claramente menor quando “já estava bem parentada”.

- [x] **A2.2** concluída

#### Task A2.3 — Debounce de reancoragens na thread principal

- **Objetivo:** Vários eventos do SO em rajada não geram múltiplos `set_behind_icons` em poucos milissegundos.
- **Escopo:** Fila única na thread principal: pedidos de reancoragem dentro de uma janela (ex.: 300–500 ms) fundem-se numa só execução; o último pedido prevalece se houver conflito.
- **Critérios de aceitação:** Simular rajada (ou inspeccionar logs) mostra uma única reancoragem pesada por debounce.

- [x] **A2.3** concluída

---

### A3 — Política do watchdog (intervalo e custo)

#### Task A3.1 — Intervalo configurável ou backoff

- **Objetivo:** Menos trabalho e menos oportunidades de flicker quando o sistema está estável.
- **Escopo:** Tornar o intervalo do loop (hoje ~8 s) configurável em dev (`env` ou constante de build). Opcional: se K ciclos seguidos não mudarem o HWND do WorkerW nem o parent, aumentar o intervalo até um tecto (backoff), ou manter intervalo longo até um “evento de suspeita”.
- **Critérios de aceitação:** Valores por defeito documentados; em máquina estável, o timer não piora o uso de CPU face ao estado actual (ideal: melhora após backoff).

- [x] **A3.1** concluída (env + backoff; ver [MULTI-MONITOR.md](./MULTI-MONITOR.md))

#### Task A3.2 — (Opcional) Health check leve vs reancoragem pesada

- **Objetivo:** Separar “perguntar se ainda estamos bem” de “refazer toda a operação”.
- **Escopo:** Num ciclo lento, apenas `GetParent` + comparação; só se falhar, disparar caminho completo (`set_behind_icons`). Pode fundir-se com A2.1.
- **Critérios de aceitação:** Em situação estável, a maioria dos ciclos só executa o check leve (visível nos logs da A1).

- [x] **A3.2** concluída (`classify_wallpaper_heal` em `wallpaper_try_reanchor_impl` antes de `FullReparent`; maioria dos ticks com `skip=anchored_ok` / `heal=light_visibility`)

---

### A4 — Alinhamento com eventos do Windows

#### Task A4.1 — Gatilho após mudanças de ambiente (ex.: wallpaper)

- **Objetivo:** Recuperar mais rápido após o Explorer refrescar o ambiente de trabalho, sem depender só do próximo tick do watchdog.
- **Escopo:** Investigar `WM_SETTINGCHANGE` (e outras mensagens úteis na versão Windows alvo); encaminhar para a thread principal um pedido **debounced** de reancoragem (reutilizar A2.3). Não retirar o watchdog até validação em campo.
- **Critérios de aceitação:** Após mudar wallpaper, o tempo até a janela voltar a ser visível é igual ou melhor que só com watchdog; sem tempestade de reancoragens nos logs.

- [x] **A4.1** concluída (listener `WM_SETTINGCHANGE` / `SPI_SETDESKWALLPAPER` + debounce; watchdog mantido)

#### Task A4.2 — Retoma de sessão e coerência com `DESKTOP_WALLPAPER_ACTIVE`

- **Objetivo:** Sleep/resume e mudanças de sessão não deixam o estado Rust/UI dessincronizado do HWND real.
- **Escopo:** Onde já existe `RunEvent::Resumed` ou clamp de janela, garantir que, se `DESKTOP_WALLPAPER_ACTIVE` e `desktop_behind_icons`, ocorre **uma** revalidação ordenada (sem conflito com o clamp de monitores — ver [MULTI-MONITOR.md](./MULTI-MONITOR.md)).
- **Critérios de aceitação:** Teste manual: modo fundo activo → sleep → acordar → janela ainda no modo fundo sem precisar togglear a opção nas definições.

- [x] **A4.2** concluída (reancoragem imediata + segunda após ~650 ms; clamp continua a ignorar modo fundo)

---

### A5 — Testes manuais, limites e fallback

#### Task A5.1 — Matriz de testes documentada

- **Objetivo:** Cobrir os cenários que mais partem o WorkerW.
- **Escopo:** Tabela ou checklist: 1 monitor / 2 monitores; wallpaper fixo / slideshow; mudança de resolução; sleep-resume; reinício do Explorer (se aplicável). Para cada linha: resultado esperado + campo para assinar em releases.
- **Critérios de aceitação:** Lista revista pelo menos uma vez antes de fechar a fase A.

- [x] **A5.1** concluída (tabela em [MULTI-MONITOR.md](./MULTI-MONITOR.md#matriz-de-testes-manuais-workerw))

#### Task A5.2 — Fallback quando o ancoramento falha de forma persistente

- **Objetivo:** O utilizador não fica com o calendário “perdido” sem perceber o que fazer.
- **Escopo:** Após N falhas consecutidas de reancoragem ou WorkerW indisponível durante M segundos: desactivar modo atrás dos ícones com persistência em config; opcionalmente notificação ou banner na UI; link para troubleshooting no doc.
- **Critérios de aceitação:** Simular falha prolongada (ex.: VM) leva a fallback controlado, não silêncio nem crash.

- [x] **A5.2** concluída

---

### Critérios globais de conclusão da fase A

- Mudança de wallpaper **não** deixa o calendário invisível &gt; 2–3 s sem recuperação automática (na matriz A5.1).
- Flicker perceptível **reduzido** face ao baseline (gravação de ecrã + feedback subjectivo).
- Logs (A1) permitem diagnosticar relatórios de utilizadores sem pedir builds especiais em todos os casos.

---

## Fase B — Qualidade de produto sem reescrever tudo em GPU

**Escalonamento (tracking):** Se, após a fase A (incl. validação `IsWindow`, listener de wallpaper, fallback A5.2, `NotifyParentWindowPositionChanged` no WebView2), o modo fundo continuar **instável** nos builds Windows alvo (mudança de wallpaper, 24H2, etc.), priorizar **B0/B1** (duas superfícies) em vez de acumular mais heurísticas só no WorkerW + mesma WebView.

**Meta:** O utilizador continua com **Tauri + UI web** para o núcleo; a parte **instável** (fundo atrás dos ícones) deixa de ser “a mesma janela WebView reparentada” para tudo.

### B0 — Decisão de produto (pré-requisito)

- [ ] **B0.1** — Definir o que o **modo fundo** mostra: só **espelho** (mês compacto + dia) vs **UI completa** (aceitar limitações).
- [ ] **B0.2** — Definir o que acontece ao **clicar** “editar”: janela normal / primeiro plano (sempre fora do WorkerW).

### B1 — Arquitectura em duas superfícies

- [ ] **B1.1** — Especificar **Surface A** (fundo): janela nativa mínima ou WebView **reduzido** só para vista leve, ancorada ao WorkerW.
- [ ] **B1.2** — Especificar **Surface B** (app): janela principal Tauri existente para interacção completa, definições, OAuth, etc.
- [ ] **B1.3** — Canal de dados entre A e B: eventos Tauri (`emit`/`listen`), estado partilhado (Rust) ou ficheiro/cache já usado pelo calendário.

### B2 — Implementação incremental (sugestão de ordem)

- [ ] **B2.1** — Modo “**só espelho no fundo**”: Surface A mostra dados já no cache; Surface B continua a ser a fonte de verdade para edição.
- [ ] **B2.2** — Sincronizar tema, transparência e posição entre A e B sem duplicar lógica de layout em dois frontends (preferir tokens Rust → ambas).
- [ ] **B2.3** — Fallback: se Surface A falhar ao ancorar, usar apenas Surface B em modo janela normal + mensagem clara.

### B3 — Documentação e UX

- [ ] **B3.1** — Actualizar [MULTI-MONITOR.md](./MULTI-MONITOR.md) e README com o novo modelo mental (duas superfícies).
- [ ] **B3.2** — Textos de definições: explicar consumo de recursos e que o modo fundo é “melhor esforço” no Windows.

### Critérios de conclusão da fase B

- Falhas na camada WorkerW **afectam só** o espelho de fundo, não bloqueiam a app principal.
- Utilizador percebe **uma** experiência coerente (dados iguais no fundo e na janela).

---

## Relação com outras fases do projecto

- **Fase 2 (Google Calendar):** cache local e modelo de eventos alimentam bem a **Surface A** espelhada (B) e reduzem dependência de rede no fundo.
- **Fase 3 (polimento):** A e B encaixam em “leveza + fiabilidade do widget”.

## Referências de código

- `src-tauri/src/windows_desktop.rs` — descoberta de `WorkerW`, `SetParent`, DWM, região.
- `src-tauri/src/lib.rs` — `wallpaper_try_reanchor`, `start_wallpaper_layer_watchdog`, `spawn_wallpaper_setting_listener`, fallback A5.2, notificação WebView2 após reparent; estado `DESKTOP_WALLPAPER_ACTIVE`.
- `src-tauri/src/workerw_log.rs` — log persistente para diagnóstico.

---

*Documento vivo: actualizar checkboxes e datas à medida que as tarefas forem concluídas.*
