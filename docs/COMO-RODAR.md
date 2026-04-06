# Como rodar no seu PC (Windows 11)

Este projeto é um **aplicativo desktop** (Tauri + WebView2). A forma recomendada de desenvolver e usar no dia a dia é **instalar as ferramentas diretamente no Windows**, não dentro do Docker.

---

## Por que não “rodar o app” só no Docker?

- O Tauri abre uma **janela nativa** usando o **WebView2** do Windows. Isso exige integração com o sistema gráfico do Windows.
- Contêineres Docker no Windows costumam ser **Linux** (via WSL2): lá não existe WebView2 do Windows nem o binário final como você vai distribuir para utilizadores.
- Resultado: **Docker não substitui** o ambiente nativo para `npm run tauri dev` e para testar o widget real.

O Docker continua **útil** para:

- **CI** (GitHub Actions com jobs em Linux) para testar crates Rust sem GUI, ou builds automatizados com matriz de SO.
- **Serviços auxiliares no futuro** (ex.: mock da API Calendar em HTTP) enquanto o app roda **no host** — opcional, não obrigatório para o MVP.

Se no futuro o repositório ganhar um `docker-compose` para mocks ou ferramentas, isso será documentado aqui.

---

## Caminho recomendado: desenvolvimento nativo no Windows 11

### 1. Pré-requisitos

| Ferramenta | Função |
|------------|--------|
| **Node.js** (LTS) | Build do frontend, scripts npm |
| **Rust** (`rustup`) | Backend Tauri e compilação |
| **Visual Studio Build Tools** | C/C++ para linkar o Tauri no Windows ( workload “Desktop development with C++” ou componentes MSVC) |
| **WebView2** | Runtime de renderização (no Windows 11 atualizado costuma já estar instalado) |
| **Git** | Clonar o repositório |

Instalação típica:

1. [Rust](https://rustup.rs/) — `rustup default stable`
2. [Node.js LTS](https://nodejs.org/)
3. [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) — marcar **MSVC** e **Windows SDK**

### 2. Clonar e instalar dependências

Quando o scaffold Tauri existir no repositório (Fase 0):

```powershell
cd E:\github\Calendario-app
npm install
npm run prepare-watchdog
```

O último comando compila o crate `agenda-watchdog` e copia o `.exe` para `src-tauri/binaries/` — necessário para o `tauri-build` validar o `externalBin` antes do primeiro `cargo build` do pacote principal.

### 3. Modo desenvolvimento (hot reload + janela)

```powershell
npm run tauri dev
```

(Se o `package.json` usar outro script, seguir o que estiver documentado no README.)

### 4. Build de produção (instalável)

```powershell
npm run tauri build
```

Saída em `target/release/` (workspace na raiz do repo), artefatos de instalador conforme o Tauri, e o vigia empacotado como sidecar — ver [WATCHDOG.md](./WATCHDOG.md). O instalador **NSIS** usa um [template personalizado](../src-tauri/windows/installer.nsi) para que **atalhos** e **«Executar após instalar»** lancem o **`agenda-watchdog.exe`** (o `.msi` WiX segue o modelo por defeito; ver [INTENCAO-VIGIA-WALLPAPER.md](./INTENCAO-VIGIA-WALLPAPER.md)).

---

## Variáveis e OAuth (Google)

- Credenciais OAuth (**Client ID** e, se aplicável ao tipo de app, segredos) **não** devem ser commitadas.
- Uso típico: ficheiro `.env` local (listado no `.gitignore`) ou configuração só na máquina; o [README](../README.md) e a Fase 2 descrevem o fluxo exato quando implementado.

---

## Docker instalado — como encaixa no teu fluxo

| Cenário | Usar Docker? |
|---------|----------------|
| Desenvolver e ver o widget no ecrã | **Não** — usar toolchain nativa acima |
| Correr testes Rust sem GUI num pipeline | **Sim** — em CI ou `docker run` com imagem `rust` (opcional) |
| Simular API Google offline | **Sim, no futuro** — serviço HTTP em container; app continua no Windows |

Ou seja: **mantém o Docker** para o que já usas (outros projetos, CI); para **este** calendário desktop, o fluxo diário é **PowerShell + `tauri dev` no Windows**.

---

## Problemas comuns

- **Erro de linker / MSVC**: instalar ou reparar Build Tools; reiniciar o terminal.
- **WebView2 em falta**: instalar [Evergreen WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
- **Antivírus a bloquear o `target/`**: exceção na pasta do projeto (caso pontual).

---

## Vigia (`agenda-watchdog`), wallpaper e modo «atrás dos ícones» (WorkerW)

O vigia **só** relança o executável principal quando o processo filho termina com **código de saída diferente de 0** (falha). Uma saída com **0** é tratada como fecho normal — o vigia **termina** e não volta a abrir a app.

Isto significa que, se ao **mudar o wallpaper** (ou noutro cenário Explorer/WebView2) o processo morrer com código **0**, ou se uma **segunda instância** sair logo com **0** (ex.: corrida com *single-instance*), o vigia **não** cumpre a expectativa de «reabrir sempre». Não substitui o endurecimento do WorkerW nem a **fase B** (duas superfícies) descrita em [ROADMAP-WORKERW-AB.md](./ROADMAP-WORKERW-AB.md).

**Variáveis opcionais** para testes (atraso antes de relançar após falha; modo experimental que relança também após saída 0) estão em [WATCHDOG.md](./WATCHDOG.md).

### Diagnosticar: ficheiro `watchdog.log`

1. Com o vigia a correr, reproduz o problema (ex.: mudar wallpaper com a app atrás dos ícones).
2. Abre `%LOCALAPPDATA%\com.calendario.widget\logs\watchdog.log` (ou cola no Explorador: `shell:Local AppData\com.calendario.widget\logs\watchdog.log`).
3. Procura linhas `child_exit` e `session_end`:
   - `child_exit ... success=true code=Some(0)` seguido de `session_end clean_exit` — o vigia saiu porque interpretou **fecho limpo**; não há relançamento com a política por defeito.
   - `child_exit ... success=false code=Some(...)` — houve falha; o vigia deve aplicar backoff e voltar a lançar até ao máximo de tentativas.

---

## Documentação relacionada

- [PLANEJAMENTO.md](./PLANEJAMENTO.md) — fases do produto  
- [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) — checklist por fase  
- [ARQUITETURA-E-STACK.md](./ARQUITETURA-E-STACK.md) — stack técnica  
- [WATCHDOG.md](./WATCHDOG.md) — vigia, variáveis de ambiente, limitações  
- [ROADMAP-WORKERW-AB.md](./ROADMAP-WORKERW-AB.md) — WorkerW e fase B  
