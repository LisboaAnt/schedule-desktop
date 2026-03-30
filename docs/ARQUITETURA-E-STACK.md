# Arquitetura, stack e otimizações

## Recomendação de stack (leveza no Windows 11)

### Shell da aplicação: **Tauri 2**

- **Por quê**: processo nativo pequeno em **Rust** + **WebView2** (já presente no Windows 11). Evita empacotar Chromium inteiro como no Electron.
- **Trade-off**: UI em HTML/CSS/JS (ou framework frontend leve). O peso depende do que for carregado na página — manter dependências mínimas.

### Frontend (sugerido)

- **Vanilla** ou **Svelte** (compila para JS enxuto) ou **Solid** — evitar frameworks pesados se o objetivo for mínimo RAM.
- CSS com variáveis CSS para **temas** e personalização sem engine extra.

### Sincronização Google Calendar

- **Google Calendar API v3** (REST).
- Fluxo **OAuth 2.0** para aplicativo desktop (PKCE + localhost redirect ou fluxo adequado ao tipo de cliente registrado no Google Cloud).
- **Tokens**: refresh token armazenado via **plugin Tauri** ou crate que use **Credential Manager** do Windows.

### Armazenamento local

- **SQLite** (via `sqlx` ou `rusqlite` no Rust, ou exposto ao frontend conforme design) para:
  - cache de eventos;
  - fila de mutações offline;
  - preferências que não couberem só em JSON.

JSON simples (`app_config.json` na pasta do app) pode bastar na Fase 1 para layout e tema.

---

## Arquitetura lógica (módulos)

```
┌─────────────────────────────────────────┐
│  UI (WebView) — widget / app            │
│  tema, layout, formulários de evento    │
└─────────────────┬───────────────────────┘
                  │ comandos IPC
┌─────────────────▼───────────────────────┐
│  Core (Rust)                             │
│  • config / persistência                 │
│  • Google API client + OAuth             │
│  • sync engine (pull/push, conflitos)    │
│  • modelo de eventos unificado           │
└─────────────────┬───────────────────────┘
                  │
┌─────────────────▼───────────────────────┐
│  SQLite + credenciais (OS)               │
└─────────────────────────────────────────┘
```

- **IPC Tauri**: frontend chama comandos Rust (`invoke`) para operações de rede e segredos; evita expor tokens ao JS desnecessariamente.

---

## Otimizações de desempenho e rede

1. **Sync incremental**: usar `syncToken` em `events.list` quando disponível; senão `updatedMin` com último timestamp conhecido.
2. **Janelas de tempo**: só buscar eventos para intervalo visível + margem (ex.: ±3 meses), expandindo sob demanda.
3. **Debounce** de edições rápidas antes de `PATCH` na API.
4. **Timer de sync**: padrão conservador (ex.: 5–15 min) + sync manual + sync ao focar a janela.
5. **Inicialização**: carregar UI do widget primeiro com cache local; sync em background.
6. **Bundle frontend**: tree-shaking, code splitting mínimo, poucas fontes (ou sistema).
7. **Imagens**: nenhum asset pesado no widget; ícones SVG inline ou fonte de ícones reduzida.

---

## Widget: comportamento no Windows

- Janela Tauri com:
  - tamanho e posição salvos;
  - opcional `always_on_top`;
  - decorações desativadas ou customizadas para aparência de widget;
  - transparência/opacity se suportado e desejado.
- **Iniciar com o Windows**:
  - atalho em `%APPDATA%\Microsoft\Windows\Start Menu\Programs\Startup`, ou
  - entrada `HKCU\...\Run` (documentar implicações de permissão e desinstalação).

---

## Segurança (resumo)

- Client ID / secrets: seguir [OAuth para apps nativos](https://developers.google.com/identity/protocols/oauth2/native-app); preferir **PKCE** e não embutir client secret em app público se a política Google exigir tipo “Desktop”.
- Escopos mínimos: `https://www.googleapis.com/auth/calendar` (ou escopos mais restritos se surgirem).
- Nunca logar tokens em release; logs redigidos.

---

## Testes sugeridos

- Testes unitários Rust para parsing de respostas da API e merge cache ↔ servidor.
- Testes manuais: primeiro login, refresh token expirado, offline → online, conflito simples (edição no telefone + no app).

---

## Alternativas consideradas (referência)

| Stack | Prós | Contras |
|-------|------|---------|
| Electron | Ecossistema enorme | RAM alta, bundle grande |
| .NET WPF/WinUI | Nativo Windows | Menos “web custom”, outro runtime |
| Flutter desktop | UI rica | Engine maior que Tauri para app pequeno |

Para o objetivo **máxima leveza + UI personalizável + GitHub OSS**, Tauri costuma ser o melhor equilíbrio atual.
