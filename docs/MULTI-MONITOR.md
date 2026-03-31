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

Não corre este clamp enquanto o modo **atrás dos ícones** (WorkerW) está ativo (`DESKTOP_WALLPAPER_ACTIVE`), para não desfazer o ancoramento.

## Modo «atrás dos ícones»

O código comenta que, após `SetParent` ao **WorkerW**, o Windows pode recolocar a janela. A descoberta do WorkerW segue o shell clássico (Progman → janelas filhas). Em **vários monitores**, o comportamento depende de como o Windows expõe essa camada (por vezes um único fundo virtual, por vezes casos limite no ecrã secundário).

Se algo falhar nesse modo num arranjo específico, usa **«Repor layout da janela»** nas definições ou desativa o modo atrás dos ícones.

## Pílula de restauro

`physical_position_for_pill_beside_main` já limita a posição ao **retângulo virtual** (`SM_XVIRTUALSCREEN`, etc.) para a pílula não sair do espaço dos ecrãs.

## Referências

- `src-tauri/src/lib.rs` — `clamp_main_window_to_visible_workspace`, `ensure_main_window_on_screen`
- `src-tauri/src/windows_desktop.rs` — WorkerW, pílula
- Plugin: `tauri-plugin-window-state` (restauro condicionado a `intersects`)
