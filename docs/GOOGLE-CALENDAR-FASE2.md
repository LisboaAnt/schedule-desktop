# Google Calendar — Fase 2 (guia de implementação)

Não coloques **client secret** nem refresh tokens no repositório (exceto se aceitares embutir o secret na build via CI com segredos). O **Client ID** é público: podes colá-lo em `EMBEDDED_GOOGLE_OAUTH_CLIENT_ID` em `google_calendar.rs` para não depender de `.env` (o Tauri nem sempre carrega `.env` como esperas).

O **refresh token** da sessão Google é guardado em ficheiro em `app_local_data_dir` (nome `google_oauth_refresh_token`, ao lado do SQLite), com espelho opcional no Credential Manager do Windows. Isto evita falhas em que o keyring em modo dev não persistia a leitura.

## Google Cloud Console

1. Cria um projeto (ou usa um existente).
2. Ativa a **Google Calendar API**.
3. **Credenciais** → criar ID de cliente **OAuth** tipo **Desktop app**.
4. O callback da app é local (`http://127.0.0.1:<porta>/oauth2callback`) e é gerado automaticamente no login; não precisa de URI pública fixa.
5. Escopos na app: `https://www.googleapis.com/auth/calendar.events` (ler e criar/editar eventos). Se ligaste a conta com um escopo antigo (`…readonly`), volta a **Ligar conta Google** para consentir o novo.

## Escopos e quotas (para utilizadores e contribuidores)

- **Escopo em código**: constante `SCOPE` em `src-tauri/src/google_calendar.rs` — hoje é `https://www.googleapis.com/auth/calendar.events`. Permite listar, criar, alterar e apagar **eventos** do calendário; não cobre outros dados Google fora da Calendar API.
- **Menor privilégio**: só pedir escopos estritamente necessários. Uma variante só leitura seria `https://www.googleapis.com/auth/calendar.events.readonly` (obrigaria novo consentimento se mudasses o produto).
- **Quotas oficiais**: limites e custos em unidades de quota variam por método (`events.list`, `insert`, etc.). Consulta a documentação atual da Google: [Usage limits](https://developers.google.com/workspace/calendar/api/guides/quota) (URL sujeita a mudanças pelo Google).
- **Como esta app reduz uso**: cache SQLite + sincronização incremental com `nextSyncToken`; sync a pedido e intervalo de auto-sync configurável; em respostas **429** (rate limit) a fila offline trata o erro como transitório e re-tenta após sincronizar.

## Fluxo recomendado (desktop)

- **OAuth 2.0 com PKCE** + **authorization code** com callback local loopback (`127.0.0.1`) na própria app.
- **Sem client secret**: para distribuição desktop, o `client_id` público é suficiente.
- A app continua com suporte a **single-instance** + **deep-link** (Tauri), mas o login Google não depende disso.
- Guardar **refresh token** de forma segura (ficheiro em `app_local_data_dir` + keyring quando disponível).
- Renovar **access token** antes de expirar; nunca logar tokens em builds de release.

## API Calendar v3 (referência)

- Listar eventos: `events.list` com `calendarId`, `timeMin`, `timeMax`, e quando possível `syncToken` ou `updatedMin` para sync incremental.
- Criar / atualizar / apagar: `events.insert`, `events.patch`, `events.delete` (ids na URL com percent-encoding via `urlencoding`).

## Modelo local

- Tabela de cache com `id` de evento Google + `calendarId` + campos normalizados + `raw_json` opcional para campos extra.
- Tabela `sync_state` para chaves por calendário (`sync_token`, `last_sync_ms`). A app grava o `nextSyncToken` devolvido pela API após a última página de cada sync; nas sincronizações seguintes usa **sync incremental** (`syncToken`). Se a API responder **410 Gone**, o token é invalidado e corre-se de novo uma **sync completa** na janela de tempo (ver `google_calendar.rs`).

## Quando esta fase estiver pronta

- Atualizar [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) com `[x]` nas linhas correspondentes.
- Atualizar [claude.md](../claude.md) em “O que já existe”.
