# Planejamento — Calendário Desktop (Widget + App)

## Visão do produto

Aplicativo de calendário para **Windows 11** que funciona em **dois modos**:

1. **Widget** (foco principal): janela sempre visível (ou quase), posição e tamanho personalizáveis, pensado para ficar “no fundo” da área de trabalho ou em segundo plano visual, com consumo mínimo de recursos.
2. **Aplicativo completo**: mesma base, interface expandida para gestão detalhada de eventos, configurações e sincronização.

Objetivo: ser **o mais leve possível** em RAM, CPU e tráfego de rede, adequado para **iniciar com o Windows** sem degradar a inicialização do sistema.

---

## Requisitos funcionais

| ID | Requisito |
|----|-----------|
| RF1 | Modo widget: janela com tamanho e posição persistidos (por monitor, quando aplicável). |
| RF2 | Modo aplicativo: visualização e edição ampliadas (lista, semana, mês, detalhe de evento). |
| RF3 | Integração com **Google Calendar** (leitura e escrita): criar, editar e apagar eventos refletem na agenda Google. |
| RF4 | Sincronização incremental quando possível (evitar baixar calendário inteiro a cada abertura). |
| RF5 | **Visual totalmente personalizável**: tema (claro/escuro), cores, tipografia, densidade, opacidade do widget, bordas, etc. |
| RF6 | Opção de **iniciar com o Windows** (atalho na pasta Inicializar ou registro/Tarefa — a definir na implementação). |
| RF7 | Projeto **open source** no GitHub: licença clara, README, contribuição e builds reproduzíveis. |

---

## Requisitos não funcionais

| ID | Requisito |
|----|-----------|
| RNF1 | **Leveza**: footprint de RAM e CPU baixo em idle; WebView/nativo enxuto em vez de runtime pesado. |
| RNF2 | **Rede**: sincronização sob demanda + intervalo configurável; uso de APIs eficientes (sync token / `updatedMin` onde couber). |
| RNF3 | **Segurança**: credenciais OAuth2 armazenadas de forma segura no SO (ex.: Windows Credential Manager ou equivalente via framework). |
| RNF4 | **Confiabilidade**: tratamento offline (fila local de alterações + retry). |
| RNF5 | **Manutenibilidade**: código modular (UI / sync / armazenamento local / config). |

---

## Público-alvo e premissas

- Usuários no **Windows 11** que querem um calendário sempre à mão, integrado ao Google.
- Aceita dependência de **conta Google** e de **internet** para sincronização (com comportamento degradado offline).
- Widget não substitui “gadgets” nativos antigos; é uma **janela própria** (tipicamente sem borda ou com borda mínima), com opções de “sempre no topo” e click-through se fizer sentido em fases futuras.

---

## Fora de escopo (inicial)

- Sincronização com Outlook/iCloud/outros provedores (pode vir depois).
- Colaboração em tempo real além do que o Google Calendar já oferece.
- Versões macOS/Linux (arquitetura pode permitir porte futuro se a stack for multiplataforma).

---

## Fases sugeridas

### Fase 0 — Fundação
- Repositório, licença, CI básico, estrutura de pastas, build debug/release.

### Fase 1 — Shell do app + configuração
- Janela principal, persistência de preferências (posição, tamanho, tema).
- Modo “widget” vs “app” (dois layouts ou transição entre estados).

### Fase 2 — Google Calendar
- OAuth2 device flow ou loopback localhost (conforme política Google e tipo de app).
- CRUD de eventos + listagem por intervalo de datas.
- Armazenamento local de cache e fila offline.

### Fase 3 — Polimento e leveza
- Perfis de sincronização, limites de frequência, lazy loading da UI.
- Iniciar com Windows, ícone na bandeja, fechar = minimizar (opcional).

### Fase 4 — Comunidade
- Documentação para contribuidores, issues templates, releases com binários assinados (quando possível).

---

## Riscos e mitigações

| Risco | Mitigação |
|-------|-----------|
| Revisão do app OAuth na Google Cloud | Documentar tipo de app (desktop), URIs e escopos mínimos. |
| Quotas da API | Cache local, sync incremental, intervalos configuráveis. |
| WebView2 não instalado no Windows 11 | Detectar e orientar instalação (raro no 11 atualizado). |
| Complexidade “widget sobre ícones” | Começar com janela normal personalizada; recursos avançados de desktop layer depois. |

---

## Critérios de sucesso

- Widget usa **poucos MB** em idle comparado a stacks Electron típicos.
- Alterações no app aparecem no Google Calendar e vice-versa após sync.
- Usuário consegue **tematizar** o visual sem recompilar.
- Instalação e “iniciar com Windows” documentadas e testadas em Windows 11 limpo.

---

## Documentação relacionada

- [ARQUITETURA-E-STACK.md](./ARQUITETURA-E-STACK.md) — stack recomendada, módulos e otimizações técnicas.
- [TAREFAS-POR-CICLO.md](./TAREFAS-POR-CICLO.md) — checklist detalhada por fase (ciclos).
- [COMO-RODAR.md](./COMO-RODAR.md) — como executar no Windows 11 e papel do Docker.
- [../claude.md](../claude.md) — índice e orientação para desenvolvimento e para assistentes de IA.
