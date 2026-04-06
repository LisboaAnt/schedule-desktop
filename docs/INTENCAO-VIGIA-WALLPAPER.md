# Intenção de produto — vigia externo e modo fundo (WorkerW)

**Data (última actualização):** 2026-04-06

Este documento regista o **objectivo do utilizador** e a **realidade técnica** actual, para não confundir o binário **`agenda-watchdog.exe`** com o temporizador interno **`start_wallpaper_layer_watchdog`** em [`src-tauri/src/lib.rs`](../src-tauri/src/lib.rs).

## Objectivo

- Com a app no **modo atrás dos ícones** (WorkerW / «mundo do fundo»), quando o processo **fecha** (por exemplo após **mudança de wallpaper** ou instabilidade do Explorer/WebView2), o utilizador pretende que a app **volte a abrir** de forma fiável.
- O **`agenda-watchdog.exe`** é o candidato natural a **relançar** o executável principal após **falhas** (saída com código ≠ 0), porque é um processo **pai** que faz `spawn` + `wait` + backoff (ver [`WATCHDOG.md`](./WATCHDOG.md)).

## Dois «vigias» no projecto

| Nome no código / binário | Função |
|--------------------------|--------|
| **`agenda-watchdog.exe`** | Processo **externo**: lança `Agenda.exe` / `calendario-app.exe`, relança após **falha** (não após saída limpa com código 0, salvo `AGENDA_WATCHDOG_RELUNCH_ON_ZERO`). |
| **Timer `start_wallpaper_layer_watchdog`** | **Dentro** da app: pede **reancoragem** ao WorkerW (`schedule_wallpaper_try_reanchor`, origem `"watchdog"`). **Não** reinicia o processo. |

## O que foi feito no instalador (NSIS)

- O template em [`src-tauri/windows/installer.nsi`](../src-tauri/windows/installer.nsi) faz com que **atalhos do Menu Iniciar**, **atalho do ambiente de trabalho** (quando criado) e **«Executar após instalar»** apontem para **`agenda-watchdog.exe`**, desde que o sidecar exista em `$INSTDIR`.
- Assim, o arranque «normal» pelo atalho passa a **incluir o vigia** na cadeia de processos.

## Instalador `.msi` (WiX)

- O pacote **WiX** gerado pelo Tauri **não** foi alterado neste passo: os atalhos continuam a seguir o template MSI por defeito (executável principal).
- Se no futuro quiseres **paridade** com o NSIS, será preciso um **fragmento WiX** ou template alinhado à versão do `tauri-bundler` (esforço separado).

## Relançamento após wallpaper / WebView2

- O vigia **só** trata saídas **não** bem-sucedidas como «falha» a relançar. Se o processo morrer com **código 0** (ou comportamento que o vigia interpreta como sucesso), **não** há relançamento — ver limitações em [`WATCHDOG.md`](./WATCHDOG.md) e a **fase B** em [`ROADMAP-WORKERW-AB.md`](./ROADMAP-WORKERW-AB.md) (duas superfícies).
- **Não** existe ainda uma regra segura na app para distinguir **«Sair»** do utilizador de **morte anómala** no modo wallpaper; por isso **não** se força `exit(≠0)` só com base em `desktop_behind_icons`.
- Em arranque, se existir **`AGENDA_WATCHDOG_SESSION`** (injectada pelo vigia), regista-se uma linha em `workerw.log` para diagnóstico (ver [`COMO-RODAR.md`](./COMO-RODAR.md)).

## Manutenção do `installer.nsi`

- O ficheiro é um **fork** do `installer.nsi` do **tauri-bundler**. Ao **subir** a versão do `@tauri-apps/cli` / Tauri, convém **diff** com o template oficial e reaplicar os trechos que apontam para `agenda-watchdog.exe`.
