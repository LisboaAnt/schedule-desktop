# Publicar versão no GitHub (instaladores Windows)

O workflow [`.github/workflows/release-windows.yml`](../.github/workflows/release-windows.yml) usa [tauri-action](https://github.com/tauri-apps/tauri-action) para correr `tauri build` e **anexar** os pacotes à secção **Releases** do repositório.

## O que é gerado

Com `bundle.targets: "all"` no `tauri.conf.json`, no Windows costuma sair:

- **`.msi`** (WiX)
- **`.exe`** (instalador NSIS)

Os ficheiros aparecem como *assets* da release. Também ficam como **artefatos do workflow** (aba Actions → execução → *Artifacts*) se o upload para a release falhar.

## Permissões no GitHub

1. Repositório → **Settings** → **Actions** → **General**
2. Em **Workflow permissions**, escolhe **Read and write permissions**
3. Guarda

Sem isto, o `GITHUB_TOKEN` não consegue criar a release e verás erro do tipo *Resource not accessible*.

## Passos para lançar uma versão (sempre manual)

O workflow **não** corre ao fazeres `git push` de tags — só quando inicias tu na UI do GitHub.

1. **Alinhar versões** (devem coincidir):
   - `src-tauri/tauri.conf.json` → campo `"version"`
   - `src-tauri/Cargo.toml` → `version = "…"`

2. **Commit** e **push** para `main` / `master` (o código que queres embalar).

3. No GitHub: **Actions** → **Release (Windows)** → **Run workflow** → escolhe a branch (normalmente `master`) → **Run workflow**.

4. O *tauri-action* usa a versão dos ficheiros acima, cria/atualiza a release **Agenda v…** (ex.: `v0.1.0-beta.1`) e anexa `.msi` / `.exe`.

5. Opcional: no PC, podes criar a mesma tag Git só para marcar o commit (`git tag v0.1.0-beta.1` + `git push origin v0.1.0-beta.1`) — **não dispara** o workflow; é só organização do histórico.

## Porque não é automático

Assim evitas builds de release em cada tag acidental e podes separar: primeiro merges na branch principal, depois decides **quando** publicar instaladores.

## Assinatura de código (opcional)

Binários sem certificado são aceites para download, mas o Windows **SmartScreen** pode avisar. Para assinatura com certificado Authenticode, vê a tarefa em [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) e documentação futura em `docs/` quando existir.

## Referência

- [Tauri — GitHub](https://v2.tauri.app/distribute/pipelines/github/)
- [COMO-RODAR.md](./COMO-RODAR.md) — build local
