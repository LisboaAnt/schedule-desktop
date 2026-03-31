# Política de segurança

## Versões suportadas

Correções de segurança são consideradas para a **linha principal** do repositório (`main` / `master`) e para a **última versão publicada** em releases, quando aplicável.

## Reportar uma vulnerabilidade

**Não** abras um issue público para vulnerabilidades sensíveis (tokens, execução remota, fugas de dados).

1. Envia um email ou abre um **security advisory** privado no GitHub (se o repositório tiver a funcionalidade ativada), com:
   - descrição do problema e impacto;
   - passos para reproduzir (se possível);
   - versão da app / commit e SO (ex.: Windows 11).

2. Aguarda confirmação de receção antes de divulgar publicamente.

## Boas práticas para quem usa a app

- Não partilhes **refresh tokens**, ficheiros em `app_local_data_dir` nem conteúdo de `config.json` com credenciais.
- Mantém o **WebView2** e o **Windows** atualizados.
- Para desenvolvimento: não commits com `GOOGLE_OAUTH_CLIENT_SECRET` ou tokens; usa `.env` local ignorado pelo Git (ver `.gitignore`).

## Divulgação responsável

Agradecemos a divulgação coordenada para podermes corrigir antes de tornar o detalhe público.
