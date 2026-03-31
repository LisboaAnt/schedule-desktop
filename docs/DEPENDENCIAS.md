# Dependências — auditoria

Última revisão: inventário manual dos manifestos e uso no código (`src-tauri/src/`).

## npm (`package.json`)

| Pacote | Tipo | Notas |
|--------|------|--------|
| `@tauri-apps/cli` | dev | Única dependência; `npm run dev` / `npm run build`. Nada é empacotado no runtime do frontend (vanilla, sem bundler pesado). |

**Conclusão:** não há dependências de produção JS para cortar.

## Rust (`src-tauri/Cargo.toml`)

| Crate | Uso |
|-------|-----|
| `tauri` | Shell, IPC, janelas, bandeja |
| `tauri-build` | Build |
| `tauri-plugin-autostart` | Iniciar com o Windows |
| `tauri-plugin-opener` | Abrir pastas no explorador |
| `tauri-plugin-window-state` | Posição/tamanho da janela |
| `serde` / `serde_json` | Config e API JSON |
| `rusqlite` | Cache local + fila offline |
| `reqwest` | Google Calendar API (TLS `rustls`, `blocking`) |
| `tokio` | Runtime async usado pelo stack Tauri / tarefas |
| `keyring` | OAuth refresh token (Windows Credential Manager) |
| `rand` | PKCE / nonces OAuth, IDs Meet |
| `sha2` | PKCE (S256) |
| `base64` | PKCE |
| `url` | Parse de URLs OAuth e REST |
| `urlencoding` | Query OAuth |
| `open` | Abrir o browser no fluxo OAuth |
| `chrono` | Datas de eventos / intervalos de sync |
| `windows` (target Windows) | WorkerW / DWM / posição da pílula |

**Conclusão:** não há crate óbvio para remover sem substituir funcionalidade (ex.: tirar `open` obrigaria outra forma de lançar o browser no OAuth).

## Boas práticas futuras (opcional)

- Correr `cargo audit` localmente ou no CI (pode exigir ajustes se houver avisos transitórios).
- Manter `reqwest` com `default-features = false` + `rustls` (já assim) para evitar `openssl` nativo no Windows.

## Referência

- [ARQUITETURA-E-STACK.md](./ARQUITETURA-E-STACK.md) — visão geral da stack.
