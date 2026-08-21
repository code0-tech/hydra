# hydra

`codezero` — a CLI setup wizard for running [CodeZero](https://codezero.build) locally. Answer a few
prompts and it renders your configuration, brings the runtime up, and gives you commands to check
on it, add/remove actions, and upgrade it going forward.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) with the Compose v2 plugin (`docker compose version`
  should work). `codezero` checks for both before running any command and tells you what's missing.

## Install

**Homebrew** (macOS/Linux):

```sh
brew install code0-tech/tap/codezero
```

**Cargo:**

```sh
cargo install codezero
```

**Prebuilt binary:** grab the archive for your OS/arch from the
[latest release](https://github.com/code0-tech/hydra/releases/latest) and put the extracted
`codezero` on your `PATH`.

**From source:**

```sh
git clone https://github.com/code0-tech/hydra.git
cd hydra
cargo build --release
./target/release/codezero --help
```

Every method produces a standalone binary — neither the setup wizard nor the action catalog is
baked in. Both are fetched live at runtime (the wizard bundle from `reticulum`, the action catalog
from `centaurus`), so it works from any directory without those repos checked out nearby, and
updating either never requires a new `codezero` release. `--bundle`/`--index` still let you point
at a directory on disk instead, useful when developing against an edited bundle.

## Quick start

```sh
codezero setup
```

Walks you through a short wizard (admin account, runtime profiles, image registry/version), writes
`.codezero/.env` and `.codezero/docker-compose.yml`, then pulls and starts everything. When it's
done it prints the URL CodeZero is running at.

## Commands

| Command | What it does |
|---|---|
| `codezero setup [--dev]` | Interactive first-time setup. `--dev` skips every prompt, using manifest defaults and the latest in-progress reticulum build — for CodeZero developers, not a typical install. |
| `codezero start` | Start a previously configured stack. |
| `codezero stop` | Stop the stack and free its resources. |
| `codezero status` | Show each service's current state. |
| `codezero logs [service] [-f] [--tail N]` | Stream logs for one service, or everything. |
| `codezero configure` | Re-run the setup wizard against your existing install — prompts are pre-filled with current values, existing secrets are kept as-is. |
| `codezero upgrade [--tag] [--registry] [--edition] [--dev]` | Bump the running image version in place — pulls and recreates only what changed, without touching your existing secrets/passwords. Prompts for a version if no flag is given. |
| `codezero plugin ls` | List every action in the catalog (fetched live from centaurus) and which ones you've installed. |
| `codezero plugin install <name>` | Add an action (e.g. `codezero plugin install gls-action`, or `gls-action@1.2.3` for a specific version). Fails with a clear message if the action needs a capability (like `rest-action`) your setup doesn't have enabled. |
| `codezero plugin uninstall <name>` | Remove a previously installed action. |
| `codezero reset` | Stop everything, wipe `.codezero/`, and run `setup` again from scratch. Destructive — generates fresh secrets and passwords. Prefer `upgrade`/`configure` if you just want to change something. |

## How it's put together

- The whole bundle (`manifest.json`, `docker-compose.yml`, `.env`, `service.configuration.json.tera`)
  is fetched live at runtime from a single place: `reticulum`'s `docker-compose/` folder. Reticulum
  is the canonical source for the actual compose stack, so neither `docker-compose.yml` nor `.env`
  is hand-copied here; `manifest.json` lives alongside them there too, so the whole bundle updates in
  one place. `manifest.json` declares the setup wizard: each prompt, its section/type/default, which
  values are auto-generated secrets, and which templates to render.
- `.env` isn't a Tera template — it's reticulum's own vendored file, patched in place: only the keys
  the wizard actually collects (steps + generated secrets) get overwritten; every other setting
  (ports, hosts, log levels, ...) keeps whatever default reticulum already ships, so `manifest.json`
  never has to duplicate them. Every value `docker-compose.yml` needs that varies per install is a
  `${VAR}` reference Docker Compose resolves from the resulting `.codezero/.env` at runtime.
- `bundle/` in this repo is a local fixture, not what ships — use `--bundle bundle/` to iterate on
  template changes offline before copying them over to reticulum.
- Installed actions each get their own compose fragment under `.codezero/actions/`, merged on top of
  the base stack via `docker compose -f base.yml -f actions/*.yml`.

