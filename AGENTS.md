# AGENTS.md

Guidance for contributors and AI agents working on this repository.

## Project

`foundry-price-checker` is a small Rust/ratatui terminal UI that search, sorts and
compares Azure AI Foundry model prices. It is a single binary named
`foundry-price-tui`.

- **Crate:** `foundry-price-checker`
- **Binary:** `foundry-price-tui` (see `[[bin]]` in `Cargo.toml`)
- **Edition:** Rust 2021, MSRV 1.88
- **Module layout:** one concern per file in `src/`
  - `main.rs` — CLI, terminal setup/teardown, event loop
  - `app.rs` — application state, filtering, sorting, key/scroll handling
  - `ui.rs` — all rendering (table, detail pane, panels, help)
  - `api.rs` — Azure Retail Prices fetch
  - `model.rs` — meter parsing (`parse_meter`, `tokens_per_unit`) and `Row`
  - `arena.rs` — LMArena Elo fetch (parquet)
  - `openrouter.rs` — context window / capabilities fetch
  - `matching.rs` — shared conservative name matching
  - `options.rs` — curated region/currency/service lists
  - `export.rs` — CSV export helpers

## Commands

Run these before opening a PR; CI enforces all of them.

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

Useful during development:

```bash
cargo run --release                      # launch the TUI
cargo run --release -- --dump            # print the table as CSV, no TUI
cargo run --release -- --dump --no-arena --no-caps   # offline (no external metadata)
```

### Screenshots

The README demo is generated with [VHS](https://github.com/charmbracelet/vhs).

```bash
cargo build --release
vhs docs/demo.tape     # writes docs/demo.gif, docs/screenshot.png, docs/help.png
```

**VHS must be v0.11.0.** v0.12.0 has a regression that cancels the context before
invoking ffmpeg and silently produces no output. `ttyd` and `ffmpeg` must be on `PATH`.

## Commit conventions

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <subject>
```

- **Imperative mood**, lowercase subject, no trailing period, ≤ 72 chars.
- Keep each commit focused on one logical change.
- Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`,
  `ci`, `chore`.
- Scopes are optional and match a module or area, e.g. `feat(arena):`,
  `fix(ui):`, `ci:`.
- Reference issues in the body when relevant (`Closes #12`).

Examples:

```
feat(openrouter): add context window and capabilities columns
fix(matching): reject bare version bumps like grok-4 -> grok-4.20
ci: add tag-triggered release workflow
docs: add VHS demo gif and usage guide
```

### Commit identity

The repository-local git identity is intentional; do not change it. Commits are
authored as configured in `.git/config`, not globally.

## Versioning

Follow [Semantic Versioning](https://semver.org): `MAJOR.MINOR.PATCH`.

- Bump `version` in `Cargo.toml` **before** tagging.
- Tags are prefixed with `v`: `v0.1.0`, `v1.2.3`.
- While pre-1.0 (`0.x`), a breaking change bumps `MINOR`.
- Pushing a `v*` tag triggers `.github/workflows/release.yml`, which builds
  binaries for Linux/macOS/Windows and attaches them to a GitHub Release.

```bash
# release flow
$EDITOR Cargo.toml          # bump version
cargo build --release       # sanity check
git add Cargo.toml Cargo.lock
git commit -m "chore: release v0.2.0"
git tag v0.2.0
git push origin main v0.2.0
```

Publishing to crates.io (manual) is `cargo publish`. Ensure `repository` and
`homepage` metadata are filled in `Cargo.toml` first.

## Code conventions

- `cargo fmt` defaults; no manual formatting debates.
- Clippy must pass with `-D warnings`.
- Prefer self-documenting names; comments explain **why**, not **what**.
- Add unit tests in a colocated `#[cfg(test)] mod tests` for any new parsing or
  matching logic. Do not add network-dependent tests.
- Keep the tool "tiny": prefer standard library and already-present crates over
  new dependencies. Any new dependency must be justified in the PR.
- Never log or commit secrets; the app only uses keyless public APIs.

### Matching rules

Name matching between Azure rows and external catalogues (LMArena, OpenRouter)
lives in `src/matching.rs`. It is deliberately **conservative**:

- Accept exact matches, date/effort suffixes and benign variant suffixes.
- Reject ambiguous or distinct variants (e.g. `gpt-5.1-codex` ≠ `gpt-5.1`).
- When in doubt, return no match rather than a wrong one, and show the matched
  name in the UI so users can verify.

Any change here must keep `cargo test` green and preserve the reject cases.

## Data sources

| Data | Source |
| --- | --- |
| Prices | Azure Retail Prices API (`Consumption` meters only) |
| Elo | LMArena `lmarena-ai/leaderboard-dataset` (`text` / `overall`) |
| Context / capabilities | OpenRouter `/api/v1/models` |

External lookups are best-effort: a failure must never block prices from
loading. Update the README when columns, flags or sources change.

## Repository hygiene

- Do not commit build output or generated files: `/target`, `model_prices_*.csv`,
  `azure_model_prices.csv`.
- Keep the README and `docs/` assets in sync with the UI.
- Prefer small, reviewable PRs. CI (`ci.yml`) runs on every PR.