# hello-platform
Rust web service used to build and test the CI/CD pipeline.

## Workflow
- Never commit to main. Always work on a branch (feat/…, fix/…, chore/…).
- Before pushing: cargo fmt, cargo clippy -- -D warnings and cargo test must pass.
- Open a PR with `gh pr create` describing what changed and how to test it.
- Never merge. Steen reviews and approves; merge only when explicitly told to.

## Security
- This repository is public. Never add secrets, tokens, internal hostnames or IP addresses.
