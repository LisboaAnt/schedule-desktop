# Google Calendar — Fase 2 (guia de implementação)

Não coloques **client secret** nem refresh tokens no repositório. Usa variáveis de ambiente ou ficheiros locais ignorados pelo `.gitignore`.

O **refresh token** da sessão Google é guardado em ficheiro em `app_local_data_dir` (nome `google_oauth_refresh_token`, ao lado do SQLite), com espelho opcional no Credential Manager do Windows. Isto evita falhas em que o keyring em modo dev não persistia a leitura.

## Google Cloud Console

1. Cria um projeto (ou usa um existente).
2. Ativa a **Google Calendar API**.
3. **Credenciais** → criar ID de cliente **OAuth** → tipo **Aplicativo para computador** (ou “Desktop”) conforme a consola atual.
4. **URIs de redirecionamento** autorizados: `http://127.0.0.1:17892/callback` (porta fixa definida em `src-tauri/src/google_calendar.rs`).
5. Escopos na app: `https://www.googleapis.com/auth/calendar.events` (ler e criar/editar eventos). Se ligaste a conta com um escopo antigo (`…readonly`), volta a **Ligar conta Google** para consentir o novo.

## Fluxo recomendado (desktop)

- **OAuth 2.0 com PKCE** + **authorization code** com redirect para **localhost**.
- Guardar **refresh token** de forma segura (ex.: Windows Credential Manager via crate/plugin Tauri).
- Renovar **access token** antes de expirar; nunca logar tokens em builds de release.

## API Calendar v3 (referência)

- Listar eventos: `events.list` com `calendarId`, `timeMin`, `timeMax`, e quando possível `syncToken` ou `updatedMin` para sync incremental.
- Criar / atualizar / apagar: `events.insert`, `events.patch`, `events.delete`.

## Modelo local

- Tabela de cache com `id` de evento Google + `calendarId` + campos normalizados + `raw_json` opcional para campos extra.
- Tabela `sync_state` para chaves por calendário (`sync_token`, `last_sync_ms`). A app grava o `nextSyncToken` devolvido pela API após a última página de cada sync; nas sincronizações seguintes usa **sync incremental** (`syncToken`). Se a API responder **410 Gone**, o token é invalidado e corre-se de novo uma **sync completa** na janela de tempo (ver `google_calendar.rs`).

## Quando esta fase estiver pronta

- Atualizar [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) com `[x]` nas linhas correspondentes.
- Atualizar [claude.md](../claude.md) em “O que já existe”.
