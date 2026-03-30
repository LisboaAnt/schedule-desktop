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
```

### 3. Modo desenvolvimento (hot reload + janela)

```powershell
npm run tauri dev
```

(Se o `package.json` usar outro script, seguir o que estiver documentado no README.)

### 4. Build de produção (instalável)

```powershell
npm run tauri build
```

Saída em `src-tauri/target/release/` e artefatos de instalador conforme configuração do Tauri.

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

## Documentação relacionada

- [PLANEJAMENTO.md](./PLANEJAMENTO.md) — fases do produto  
- [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) — checklist por fase  
- [ARQUITETURA-E-STACK.md](./ARQUITETURA-E-STACK.md) — stack técnica  
