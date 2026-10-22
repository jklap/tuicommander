# Riepilogo per Boss: un solo percorso GitHub in TUIC, via HTTP

Piano: `plans/github-http-only.md` (story 1708-a7a5). Decisione di Boss del 2026-10-11: tutto l'accesso GitHub passa dal layer HTTP/GraphQL esistente; zero spawn di `gh` nel codice di produzione; `token_from_gh_cli` rimosso.

## Cosa ho trovato (25 righe di inventario)

I siti noti erano 4. In realtà i punti di produzione che usano `gh` sono **6**:

1. **Scritture** (crea PR, crea issue, review): `gh api` con il token dell'account in `GH_TOKEN`. Nato il 2026-07-10 solo perché "c'era già il runner gh"; `RepoApi::rest("POST")` fa già tutto.
2. **Log CI per l'auto-heal**: `gh run list`, `gh run view`, `gh api …/logs`. Usa **il login di gh**, non l'account di TUIC; rifiuta GHE.
3. **Check falliti (anche CI esterne)**: `gh pr list --limit 100` + `gh pr checks`. Non era nella lista. Bug latente: se la PR non è tra le prime 100 aperte, nessun check.
4. **Prova "PR mergiata"** per rimozione worktree/branch e sidebar: `gh api graphql --paginate`. Login di gh, per host.
5. **`token_from_gh_cli`**: `gh auth token` nella catena dei token.
6. **Crate `gh-token`**: non era nella lista. Legge `hosts.yml` e, quando gh tiene il token nel keyring (il caso normale), lancia anch'esso `gh auth token`.

Il resto dell'inventario: tipi (`TokenSource::GhCli`, `AccountKind::GithubComGhCli`, catalogo settings generato), UI (tab GitHub), commenti, 5 gruppi di test, 7 documenti più `to-test.md`. Fuori scope: Makefile, workflow CI, hook pre-push, fixture registrate con `gh` e i prompt che dicono all'agente di usare `gh` (Q1).

## Incoerenze del layer HTTP

- Tre identità diverse: token dell'account, login di gh, e la catena ambient che può venire da gh.
- I percorsi gh saltano il circuit breaker e il conteggio dei rate limit.
- Gli header REST sono copiati 10 volte in `github.rs` più altre 3. `User-Agent` è scritto in due modi diversi e l'API version è inviata solo in 2 siti.
- Formati di errore diversi da un sito all'altro. Il piano mantiene il testo di ogni sito (è voluto) e unifica solo la costruzione della richiesta.

## Piano (6 step, una story ciascuno)

1. Un solo builder per le richieste REST, più il test di guardia `production_code_spawns_no_gh` con una allowlist che ogni step abbassa (RED prima del codice).
2. Scritture via `PrApi::rest`.
3. Prova PR mergiata via GraphQL paginato (`graphql_pages`); un adapter sync→async con `Handle::block_on` sui thread bloccanti.
4. Check falliti via `statusCheckRollup` del commit (riusa dedup e classificatore della sidebar).
5. Log CI via REST: run per `head_sha`, job paginati, log con redirect senza token verso lo storage, solo la coda del log, una deadline unica (chiude il `DEFERRED 2026-09-25`). GHE ora funziona.
6. Rimozione di `token_from_gh_cli`, `gh-token`, delle varianti enum, della UI e dei documenti. Nessun codice di compatibilità serve: un record salvato con `gh_cli` si legge già come OAuth.

## Conseguenza visibile per l'utente

Chi ha fatto solo `gh auth login` vede "Not signed in" nel tab GitHub. Stato PR, anelli CI, auto-heal e scritture si fermano finché non entra con OAuth (Settings → GitHub) o imposta `GH_TOKEN`/`GITHUB_TOKEN`. Su macOS l'app GUI non vede le variabili del profilo shell, quindi la strada pratica è OAuth.

## Domande aperte (con default)

- Q1 I prompt predefiniti dicono all'agente di usare `gh pr create`/`gh pr edit`. È lo strumento dell'agente, non di TUIC. Default: lasciarli.
- Q2 Il test di parità ignorato usa `gh` come oracolo. Default: tenerlo (è codice di test).
- Q3 Avviso di migrazione per chi usava solo gh? Default: nessuno, bastano il tab GitHub e la documentazione.

Dipendenza: lo step 6 va dopo il merge di `chore/remove-legacy-github-accounts` e va coordinato con la story 1679-1419.
