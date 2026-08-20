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

Every method produces a standalone binary — the setup wizard and action catalog are baked in, so
it works from any directory without this repo checked out nearby. `--bundle`/`--index` still let
you point at a directory on disk instead, useful when developing against an edited bundle.

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
| `codezero upgrade [--tag] [--registry] [--edition] [--dev]` | Bump the running image version in place — pulls and recreates only what changed, without touching your existing secrets/passwords. Prompts for a version if no flag is given. |
| `codezero install <name>` | Add an action (e.g. `codezero install gls`, or `gls@1.2.3` for a specific version). See available actions in `actions/index.json`. |
| `codezero uninstall <name>` | Remove a previously installed action. |
| `codezero reset` | Stop everything, wipe `.codezero/`, and run `setup` again from scratch. Destructive — generates fresh secrets and passwords. Prefer `upgrade` if you just want a newer version. |

## How it's put together

- `bundle/manifest.json` declares the setup wizard: each prompt, its section/type/default, which
  values are auto-generated secrets, and which templates to render. Adding a new setting is usually
  a manifest + template change, not a Rust change.
- `bundle/docker-compose.yml` is vendored as-is from upstream — `codezero` never rewrites it. Every
  value that needs to vary per install (ports, image tag, secrets, ...) is a `${VAR}` reference that
  Docker Compose resolves from `.codezero/.env` at runtime, so upstream compose changes are just a
  file drop-in, not a merge.
- `bundle/env.tera` is the one file `codezero` actually renders, combining your wizard answers with
  generated secrets into `.codezero/.env`.
- Installed actions each get their own compose fragment under `.codezero/actions/`, merged on top of
  the base stack via `docker compose -f base.yml -f actions/*.yml`.

