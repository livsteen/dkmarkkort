# dkmarkkort
Kort over alle indberettede danske marker. Rust-workspace med server (tokio, topcoat, Tailwind), pipeline og core; data i SQLite som GeoPackage og MBTiles. Se README.md.

Kode, kommentarer, dokumentation og brugerflade er på dansk. Identifikatorer staves uden æøå (afgroede, vaelg, aabn).

## Workflow
- Never commit to main. Always work on a branch (feat/…, fix/…, chore/…).
- Before pushing: cargo fmt, cargo clippy -- -D warnings and cargo test must pass.
- GitHub-adgang går gennem `gh`, som er logget ind. Git bruger ikke selv det login, så push med `git -c credential.helper= -c 'credential.helper=!gh auth git-credential' push -u origin <branch>`.
- Open a PR with `gh pr create --reviewer Photic` describing what changed and how to test it.
- Never merge. Steen reviews and approves; merge only when explicitly told to.

## Security
- This repository is public. Never add secrets, tokens, internal hostnames or IP addresses.
