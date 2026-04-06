# Publicar versão no GitHub (instaladores Windows)

O workflow [`.github/workflows/release-windows.yml`](../.github/workflows/release-windows.yml) usa [tauri-action](https://github.com/tauri-apps/tauri-action) para correr `tauri build` e **anexar** os pacotes à secção **Releases** do repositório.

## O que é gerado

Com `bundle.targets: "all"` no `tauri.conf.json`, no Windows costuma sair:

- **`.msi`** (WiX)
- **`.exe`** (instalador NSIS)

Os ficheiros aparecem como *assets* da release. Também ficam como **artefatos do workflow** (aba Actions → execução → *Artifacts*) se o upload para a release falhar.

O workflow tem `uploadUpdaterJson: true`, pelo que o **`latest.json`** (metadados do Tauri updater) também é publicado na release — necessário para o cliente **procurar actualizações** e instalar com um clique.

### Instalador «um ficheiro» vs ficheiros na pasta de instalação

- **Download no GitHub:** normalmente **um** `.exe` (NSIS) e/ou **um** `.msi` por release — é o que o utilizador descarrega.
- **Depois de instalar:** com `externalBin` (ex.: `agenda-watchdog`), o instalador coloca o `.exe` principal **e** o sidecar **na mesma pasta** — não é tudo fundido num único `.exe` no disco.

## Endpoint do updater (`latest.json`)

Em [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json), `plugins.updater.endpoints` deve apontar para:

`https://github.com/OWNER/REPO/releases/latest/download/latest.json`

onde **OWNER/REPO** é exactamente o repositório onde corres [`.github/workflows/release-windows.yml`](../.github/workflows/release-windows.yml) e publicas Releases.

**Verificação local:**

```powershell
git remote get-url origin
```

O caminho `…/OWNER/REPO/…` no URL tem de coincidir com o do `endpoints`. Neste repositório, `origin` é `LisboaAnt/schedule-desktop`, alinhado com a configuração actual. Se forkares ou mudares o nome do repo no GitHub, **actualiza** o `endpoints`; caso contrário o botão «Atualizar» na app procura updates no sítio errado.

## Permissões no GitHub

1. Repositório → **Settings** → **Actions** → **General**
2. Em **Workflow permissions**, escolhe **Read and write permissions**
3. Guarda

Sem isto, o `GITHUB_TOKEN` não consegue criar a release e verás erro do tipo *Resource not accessible*.

## Passos para lançar uma versão (sempre manual)

O workflow **não** corre ao fazeres `git push` de tags — só quando inicias tu na UI do GitHub.

1. **Alinhar versões** (as duas primeiras devem coincidir com o número da release):
   - `src-tauri/tauri.conf.json` → campo `"version"`
   - `src-tauri/Cargo.toml` → `version = "…"`
   - (opcional) `package.json` → `"version"` — só para consistência com scripts npm

2. **Commit** e **push** para `main` / `master` (o código que queres embalar). **Só isto não gera release** — o workflow é manual.

3. No GitHub: **Actions** → **Release (Windows)** → **Run workflow** → escolhe a branch (neste repo: **`master`**) → **Run workflow**.

4. O *tauri-action* usa a versão dos ficheiros acima, cria/atualiza a release **Agenda v…** (ex.: `v0.1.0-beta.1`) e anexa `.msi` / `.exe` e o **`latest.json`** do updater (para os clientes detectarem a nova versão).

5. Opcional: no PC, podes criar a mesma tag Git só para marcar o commit (`git tag v0.1.0-beta.1` + `git push origin v0.1.0-beta.1`) — **não dispara** o workflow; é só organização do histórico.

## Porque não é automático

Assim evitas builds de release em cada tag acidental e podes separar: primeiro merges na branch principal, depois decides **quando** publicar instaladores.

## Assinatura de código (opcional)

Binários sem certificado são aceites para download, mas o Windows **SmartScreen** pode avisar. Para assinatura com certificado Authenticode, vê a tarefa em [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) e documentação futura em `docs/` quando existir.

## Referência

- [Tauri — GitHub](https://v2.tauri.app/distribute/pipelines/github/)
- [COMO-RODAR.md](./COMO-RODAR.md) — build local
