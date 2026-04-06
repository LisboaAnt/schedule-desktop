# Vigia mínimo (`agenda-watchdog`)

Binário **separado** do pacote Tauri (crate `agenda-watchdog` no [workspace](../Cargo.toml)) que:

1. Lança o executável principal (`Agenda.exe` ou `calendario-app.exe` na mesma pasta).
2. Espera que o processo termine.
3. Se o código de saída for **sucesso** (normalmente `0`), o vigia termina — **não** há loop.
4. Se **não** for sucesso, espera (backoff exponencial, com tecto) e volta a lançar, até ao máximo de tentativas por sessão.

## Onde fica o ficheiro

- Em desenvolvimento, após `npm run prepare-watchdog`, o Tauri espera `src-tauri/binaries/agenda-watchdog-x86_64-pc-windows-msvc.exe` (nome exigido por `externalBin`).
- No instalador, o sidecar fica **ao lado** do `.exe` principal.

## «127.0.0.1 recusou a ligação» ao usar o vigia com `target\debug\calendario-app.exe`

O binário **debug** é compilado com `cfg(dev)`: o WebView tenta carregar a UI a partir de um **servidor local** que **só existe** quando corres **`npm run tauri dev`**. Se abrires só `agenda-watchdog.exe` → `target\debug\calendario-app.exe` **sem** o `tauri dev`, não há servidor → **ERR_CONNECTION_REFUSED**.

**O que fazer:**

- **Desenvolvimento:** usa **`npm run tauri dev`** (não dependas do vigia para ver a UI em debug).
- **Testar vigia + app:** usa o **release**, que embute o `frontendDist` e **não** usa esse localhost, por exemplo:

  ```powershell
  cd E:\github\Calendario-app
  cargo build -p calendario-app --release
  .\target\debug\agenda-watchdog.exe --child .\target\release\calendario-app.exe
  ```

  (Ou coloca `calendario-app.exe` de `target\release` na mesma pasta que o vigia e renomeia cópias conforme necessário; o vigia procura `Agenda.exe` / `calendario-app.exe` **ao lado** dele.)

- Alternativa: `npm run build:ci` e depois vigia a apontar para `target\release\calendario-app.exe`.

## Definições na app

Em **Definições → Arranque e bandeja**, a opção **«Vigia: reinício após falha»** grava em `config.json` o campo `autostartUseWatchdog`. Com **«Iniciar com o Windows»** activo, o registo `Run` passa a apontar para `agenda-watchdog.exe` em vez do executável principal (só se o ficheiro existir).

## Variáveis de ambiente (vigia)


| Variável                              | Significado                                                                 |
| ------------------------------------- | --------------------------------------------------------------------------- |
| `AGENDA_CHILD_EXE`                    | Caminho absoluto do `.exe` principal.                                       |
| `AGENDA_WATCHDOG_MAX_ATTEMPTS`        | Tentativas por sessão (defeito 5).                                          |
| `AGENDA_WATCHDOG_BACKOFF_MS`          | Backoff inicial em ms (defeito 2000).                                      |
| `AGENDA_WATCHDOG_BACKOFF_CAP_MS`      | Tecto do backoff (defeito 60000).                                         |
| `AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS`  | Atraso em ms (0–10000, defeito 0) após o filho terminar **com falha** (ou com 0 se `RELUNCH_ON_ZERO`), **antes** do backoff — ajuda a libertar mutex do *single-instance* antes do próximo `spawn`. |
| `AGENDA_WATCHDOG_RELUNCH_ON_ZERO`     | Se `1` ou `true`, trata **saída com código 0** como falha e relança até ao máximo de tentativas. **Perigoso:** também relança após **«Sair»** na bandeja; usar só para testes. |
| `AGENDA_WATCHDOG_LOG=0`               | Desliga `%LOCALAPPDATA%\com.calendario.widget\logs\watchdog.log`.           |

## Limitações

- Se o crash do processo principal **sempre** terminar com código `0`, o vigia **não** relança — nesse caso o plano é a arquitectura em **duas superfícies** ([ROADMAP-WORKERW-AB.md](./ROADMAP-WORKERW-AB.md) fase B).
- Uma **segunda instância** do Tauri pode sair com **0** muito rápido (plugin *single-instance*); o vigia interpreta isso como fecho limpo e **termina**. Um `AGENDA_WATCHDOG_PRE_RETRY_DELAY_MS` (ex.: 500–1000) **antes** do próximo arranque após **falha** pode ajudar quando a corrida envolve saídas **não zero**; para saídas **0**, só `AGENDA_WATCHDOG_RELUNCH_ON_ZERO` (experimental) ou evolução do produto (fase B).

### Mudança de wallpaper / modo WorkerW

Não contes com o vigia para **corrigir** o fecho da app ao mudar wallpaper com a janela ancorada ao WorkerW: o problema é a camada Explorer/WebView2, não a ausência de relançamento. Para **perceber** o que o vigia viu, usa `watchdog.log` (ver [COMO-RODAR.md](./COMO-RODAR.md) — secção «Diagnosticar»).

### Posição da janela após o vigia relançar

Quando o processo principal **morre sem saída limpa**, o `tauri-plugin-window-state` **não** grava no `RunEvent::Exit` — o ficheiro em disco pode ficar desatualizado. A app grava também **após** a estabilização do layout no arranque e **a cada 5 minutos** (ver [MULTI-MONITOR.md](./MULTI-MONITOR.md)) para aproximar o guardado da realidade. Se ainda notares **desvio** (ex.: um pouco para baixo ou para a direita), usa **«Repor layout da janela»** nas definições.

## Build / CI

Após clonar o repositório, antes do primeiro `cargo clippy` / `cargo build` do pacote Tauri com `externalBin`, executar na raiz:

```powershell
npm run prepare-watchdog
```

O `beforeBuildCommand` do Tauri inclui `prepare-watchdog-release` para `npm run tauri build`.