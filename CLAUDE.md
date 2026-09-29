# dkmarkkort
Kort over alle indberettede danske marker. Rust-workspace med server (tokio, topcoat, Tailwind), pipeline og core; data i SQLite som GeoPackage og MBTiles. Se README.md.

Kode, kommentarer, dokumentation, brugerflade, commits og pull requests er altid på dansk. Identifikatorer og branchnavne staves uden æøå (afgroede, vaelg, aabn).

## Workflow
- Kør altid projektet med `sh run.sh`. Det genstarter udviklingsserveren, hvis den allerede kører, og lytter på 0.0.0.0:3000.
- Never commit to main. Always work on a branch (feat/…, fix/…, chore/…).
- Before pushing: cargo fmt, cargo clippy -- -D warnings and cargo test must pass.
- GitHub-adgang går gennem `gh`, som er logget ind. Git bruger ikke selv det login, så push med `git -c credential.helper= -c 'credential.helper=!gh auth git-credential' push -u origin <branch>`.
- Open a PR with `gh pr create --reviewer Photic` describing what changed and how to test it.
- Never merge. Steen reviews and approves; merge only when explicitly told to.

## Pull requests
Sådan er PR #1–#4 kørt, og sådan køres de fremover. Alt i en PR er på dansk: branch, commits, titel og beskrivelse. Se `gh pr view 4` for et eksempel.

1. Hver opgave får sin egen branch fra en opdateret `main`: typen og et kort emne uden æøå, fx `feat/soegning-og-markvalg`, `fix/…`, `chore/run-sh`. En branch hvis PR er merget, bruges aldrig igen; følgearbejde får en ny.
2. Commits har typen som præfiks og en dansk overskrift (`fix: marker får deres id med i tiles'ene`), en krop der forklarer hvorfor, og `Co-Authored-By`-linjen til sidst. Filer stages eksplicit, aldrig med `git add -A` eller `.`.
3. Før push køres `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings` og `cargo test --workspace`. Push med `gh`-login som beskrevet ovenfor.
4. PR'en oprettes med `gh pr create --base main --reviewer Photic`. Titlen er en dansk sætning uden præfiks.
5. Beskrivelsen har disse afsnit:
   - `## Hvad er ændret`: punkter, grupperet under fede overskrifter (**Pipeline**, **Server** …) når flere dele er rørt.
   - `## Sådan testes det`: kommandoerne, også når data skal bygges igen, nummererede trin at prøve i browseren, og at fmt, clippy og test er grønne.
   - `## Bemærkninger`, når der er noget: hvad der ikke er testet, og kendte fejl der ikke rettes i PR'en.

   Den slutter med `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
6. Steen godkender og merger med en merge commit.

## Security
- This repository is public. Never add secrets, tokens, internal hostnames or IP addresses.
