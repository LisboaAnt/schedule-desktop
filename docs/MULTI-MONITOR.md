# Multi-monitor (Windows)

## Posição e tamanho guardados

O plugin **`tauri-plugin-window-state`** grava a janela principal em **pixels físicos** (coordenadas do ambiente virtual). Ao reabrir:

- Se **algum canto** do retângulo guardado cair dentro de um ecrã conhecido, a posição é restaurada (igual à lógica interna do plugin).
- Se **nenhum** ecrã intersectar esse retângulo (ex.: desligaste o monitor onde a janela estava), o plugin **não** força uma posição; a app pode ficar fora do espaço visível.

## O que esta app faz a mais

1. **~150 ms após o arranque** — chamada a `clamp_main_window_to_visible_workspace`: se a janela principal **não** intersecta nenhum monitor, faz-se **`center()`** (evita janela “perdida” após mudar a configuração de ecrãs).
2. **`RunEvent::Resumed`** (Tauri) — o mesmo clamp ao retomar a app (útil ao voltar do sleep ou quando o SO notifica retoma).
3. **Foco da janela** — o frontend chama `ensure_main_window_on_screen` ao receber `window` `focus`, para recuperar após hot-plug de monitores com a app já aberta.
4. **Comando manual** — `ensure_main_window_on_screen` (IPC) para testes ou futuras opções em definições.
5. **Gravação do estado da janela após o layout estabilizar** — `~500 ms` após o arranque e após `apply_desktop_wallpaper_state_on_launch` (WorkerW ou clamp), a app chama `save_window_state` no plugin (`tauri-plugin-window-state`).
   - O plugin restaura a janela **antes** de `set_behind_icons` / `move_as_child_at_screen_rect`; o HWND move-se depois. Gravar cedo **alinha** o ficheiro `.window-state.json` à geometria **final** e reduz o desvio “um pouco para baixo / direita” no próximo arranque ou relançamento pelo vigia.
6. **Gravação periódica** — a cada **5 minutos** (em segundo plano), outra `save_window_state`. Assim, se o processo **terminar sem** `RunEvent::Exit` (crash, `taskkill`, etc.), o ficheiro em disco fica mais próximo da última posição visível.

Não corre este clamp enquanto o modo **atrás dos ícones** (WorkerW) está ativo (`DESKTOP_WALLPAPER_ACTIVE`), para não desfazer o ancoramento.

### Desvio de posição ao relançar (vigia / crash)

Se o rectângulo guardado ficar desfasado da posição real (WorkerW + plugin que mistura posição externa com tamanho interno), a janela pode aparecer **ligeiramente** deslocada em relançamentos. Usa **«Repor layout da janela»** nas definições para apagar o estado guardado e centrar; ver também [WATCHDOG.md](./WATCHDOG.md) — vigia e saída sem `Exit`.

## Modo «atrás dos ícones»

O código comenta que, após `SetParent` ao **WorkerW**, o Windows pode recolocar a janela. A descoberta do WorkerW segue o shell clássico (Progman → janelas filhas). Em **vários monitores**, o comportamento depende de como o Windows expõe essa camada (por vezes um único fundo virtual, por vezes casos limite no ecrã secundário).

Ao **arrancar** com esta opção activa (ou ao **activar** o modo nas definições), a app alinha o HWND à **área de trabalho** (`rcWork`) do monitor — o mesmo rectângulo que o Windows usa para uma janela maximizada na área útil (sem barra de tarefas, etc.). Isto evita **margens grandes** à volta do calendário quando o estado guardado restaurava um tamanho intermédio sem maximizar (comum após relançar com o vigia). Para desactivar este alinhamento e manter só o rectângulo imediatamente após `set_behind_icons`, define **`AGENDA_WALLPAPER_SKIP_WORK_AREA_SNAP=1`** antes de arrancar.

Se algo falhar nesse modo num arranjo específico, usa **«Repor layout da janela»** nas definições ou desativa o modo atrás dos ícones.

### Diagnóstico (logs WorkerW)

Para investigar sumiços, flicker ou reancoragens em excesso:

1. Define a variável de ambiente **`AGENDA_WORKERW_DEBUG=1`** antes de arrancar a app (ou usa um build **debug**, que também activa estes logs).
2. No terminal / consola onde corre o executável, procura linhas com prefixo **`[agenda] workerw`**, por exemplo:
   - `workerw reanchor origin=watchdog|single_instance|resumed` — origem da reancoragem e contador `n` por origem;
   - `set_behind_icons` — `parent_before` / `parent_after` e se coincidem com o WorkerW esperado;
   - `WorkerW encontrado` vs `WorkerW não encontrado`.
3. Reprodução típica: activa **atrás dos ícones**, muda o **wallpaper** ou activa **slideshow**, observa se a janela desaparece ou pisca; copia o trecho de log correspondente para o relatório.
4. **Watchdog (intervalo)** — `AGENDA_WORKERW_WATCHDOG_SEC` (defeito **8**, intervalo entre ticks em segundos, 2–120). **Backoff opcional:** se definires `AGENDA_WORKERW_WATCHDOG_BACKOFF_MAX_SEC` **maior** que o intervalo base (ex.: 60), o tempo entre ticks aumenta em passos de 4 s depois de ciclos estáveis consecutivos (`anchored_ok` / `light_visibility`) — o mínimo de ciclos estáveis antes de subir está em `AGENDA_WORKERW_WATCHDOG_STABLE_TICKS` (defeito **4**). Com `AGENDA_WORKERW_DEBUG=1`, aparecem linhas no terminal quando o intervalo muda.
5. **Retoma de sessão** — com modo fundo activo, ao `RunEvent::Resumed` agenda-se uma reancoragem imediata e outra **~650 ms** depois (para quando monitores e o shell ainda estão a estabilizar após sleep).
6. **Ficheiro em disco** — `%LOCALAPPDATA%\com.calendario.widget\logs\workerw.log` (append + flush por linha). Variáveis: `AGENDA_WORKERW_LOG` (caminho alternativo), `AGENDA_WORKERW_FILE_LOG=0` para desligar. Útil quando o processo termina sem saída no terminal.

#### Exemplos de linhas (saudáveis vs anómalas)

| Situação | O que procurar no `workerw.log` ou no terminal (`AGENDA_WORKERW_DEBUG=1`) |
|----------|--------------------------------------------------------------------------|
| **Estável** | Muitas entradas `skip=anchored_ok` ou `reanchor_impl heal=None` nos ticks do watchdog; `watchdog_outcome stable=true` com `next_sleep_sec` a subir só se usares backoff. |
| **Recuperação leve** | `heal=light_visibility` — pai correcto, só visibilidade/pílula. |
| **Reancoragem completa** | `reanchor_impl FullReparent path` seguido de `set_behind_icons ok` ou `post_SetParent parent_after=… match=true`. |
| **WorkerW em falha** | `WorkerW não encontrado` ou erro `Camada WorkerW não encontrada`; no terminal, `GetLastError=` numa falha de `SetParent`. |
| **Pai incorrecto** | `SetParent não fixou o pai esperado` ou `parent_after` ≠ WorkerW esperado nos logs de `set_behind_icons`. |

### Matriz de testes manuais (WorkerW)

Usar antes de releases Windows ou após alterações em `windows_desktop.rs` / watchdog. Coluna **Assinatura**: data + versão ou «n/a».

| # | Cenário | Passos resumidos | Resultado esperado | Assinatura |
|---|---------|------------------|--------------------|------------|
| 1 | 1 monitor, wallpaper fixo | Modo atrás dos ícones; mudar wallpaper uma vez | Janela visível atrás dos ícones em menos de ~3 s; sem precisar desligar o modo | |
| 2 | 1 monitor, slideshow | Slideshow activo; deixar 2–3 transições | Sem ficar invisível de forma persistente; logs com `skip=anchored_ok` predominante se estável | |
| 3 | 2 monitores | Modo fundo; mover janela (se aplicável); mudar wallpaper | Sem perder ancoramento de forma permanente no monitor principal | |
| 4 | Mudança de resolução / escala | Alterar escala DPI ou resolução do ecrã com a app em modo fundo | Recuperação automática ou pílula utilizável; fallback A5.2 só após falhas repetidas | |
| 5 | Sleep / resume | Modo fundo activo; suspender e acordar o PC | Continua em modo fundo sem togglear a opção; segunda reancoragem ~650 ms após resume | |
| 6 | Explorer / shell | (Opcional) Reiniciar Explorer com a app em modo fundo | Recuperação ao relançar o shell ou após watchdog; se impossível, fallback com alerta | |
| 7 | Vigia (`agenda-watchdog`) | Autostart com vigia + `taskkill /F /IM calendario-app.exe` (simula falha) | Processo volta a subir até ao limite; saída normal (bandeja → Sair) não entra em loop — [WATCHDOG.md](./WATCHDOG.md) | |

## Pílula de restauro

`physical_position_for_pill_beside_main` já limita a posição ao **retângulo virtual** (`SM_XVIRTUALSCREEN`, etc.) para a pílula não sair do espaço dos ecrãs.

## Referências

- `src-tauri/src/lib.rs` — `clamp_main_window_to_visible_workspace`, `ensure_main_window_on_screen`, watchdog WorkerW
- `src-tauri/src/windows_desktop.rs` — WorkerW, pílula
- `src-tauri/src/workerw_log.rs` — `workerw.log` em disco (opcional)
- [WATCHDOG.md](./WATCHDOG.md) — vigia mínimo (`agenda-watchdog.exe`)
- Plugin: `tauri-plugin-window-state` (restauro condicionado a `intersects`)
