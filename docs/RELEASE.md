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

## Passos para lançar uma versão

1. **Alinhar versões** (devem coincidir):
   - `src-tauri/tauri.conf.json` → campo `"version"`
   - `src-tauri/Cargo.toml` → `version = "…"`

2. **Commit** dessas alterações na branch principal (`main` / `master`).

3. **Criar e enviar a tag** (o nome deve bater com a versão, com prefixo `v`):

   ```powershell
   git tag v0.1.0
   git push origin v0.1.0
   ```

   O workflow corre ao receber o `push` da tag `v*`.

4. Em **Releases** no GitHub, confirma a release **Agenda v…** e os ficheiros `.msi` / `.exe`.

## Disparo manual (sem nova tag)

Na aba **Actions** → **Release (Windows)** → **Run workflow**: útil para testar o pipeline; o *tauri-action* pode criar/atualizar release conforme a versão no `tauri.conf.json`. Para releases estáveis, prefere o fluxo com **tag** acima.

## Assinatura de código (opcional)

Binários sem certificado são aceites para download, mas o Windows **SmartScreen** pode avisar. Para assinatura com certificado Authenticode, vê a tarefa em [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) e documentação futura em `docs/` quando existir.

## Referência

- [Tauri — GitHub](https://v2.tauri.app/distribute/pipelines/github/)
- [COMO-RODAR.md](./COMO-RODAR.md) — build local
