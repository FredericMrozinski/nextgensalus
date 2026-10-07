# Dataset Workbench — design document

Status: **design agreed; the framework changes F0–F5 (section 10) and the plugins (section 10.2) are implemented and verified in a browser (2026-10-06). Remaining: see 10.2 "Not done / next".**

This document is written so that an agent with **no prior context** can pick it up and continue. It has three parts:

1. A primer on the Salus framework as it exists today (sections 1–2). Read it first; it explains the vocabulary and the pitfalls.
2. The product and data-model design for the **Dataset Workbench** (sections 3–8).
3. The plugin/UI architecture, the **framework changes** it requires, a roadmap and the open questions (sections 9–14).

Everything marked **Decision** was explicitly agreed with the user. Everything marked **Proposal** is my suggestion that the user has not
confirmed. Everything under **Open questions** still needs an answer.

---

## 0. How the user likes to work

- The user (Salus's author, a medical-imaging researcher) wants to **think design through before any implementation**. Do not build until they say so.
  Discuss, challenge ideas (they explicitly ask to be pushed back on when something is a bad idea), then write things down.
- They prefer **as little mechanism as possible in the plugin SDKs** and most behavior **in the framework**.
- Keep answers concise. Medical context: datasets for ML on pathology and clinical data. Everything lives on a server; no anonymization yet.
- User management and filesystem sandboxing will be done **by the framework later** (plugins will be spawned sandboxed and see only files the user may see).
  Plugins must therefore not assume absolute paths and must tolerate files that are not accessible.
- The user said "source cohort" and "source project" interchangeably. This document calls it a **Source** (section 3).

---

## Handoff: where we are and what to do next

**Done.** The design (sections 2–9), all decisions (section 4) and the framework work F0–F5 (section 10; what exists and where it differs from the plan: 10.1). The framework now supports everything the workbench needs:
frontend components (`[[frontend-component]]`, api-version 3 only), `target-panel`, `openComponent`/`openDependency` with params and reuse, browser-local messaging between components, the file-viewer registry with a viewer picker (`salus.openFile`), the shared theme, TypeScript typings. Docs under `docs/` describe all of it.

**The workbench plugins are done too** (section 10.2): Dataset Workbench with its three components, the relational viewer and the image-viewer migration.

**Reference implementation of the framework features:** `examples/plugins/org.salus.componentsdemo` (a minimal Python backend and four components: `main`, `side` and two file viewers for `.demo`). Copy its manifest and component layout. Plugin-development docs: `docs/getting-started/quickstart.md`, `docs/guides/components.md`, `docs/guides/file-viewers.md`, `docs/guides/image-viewer.md` (a real non-trivial plugin).

### Environment state (outside the repo; may differ on another machine)

- Plugins directory (macOS): `~/Library/Application Support/org.fredericmrozinski.salus/plugins/` contains `org.salus.testplugin` (the user's chat test plugin, **manifest already migrated to api-version 3**), `org.salus.imageviewer`, `org.salus.imagecontrols`, `org.salus.componentsdemo`. The installed copies are **copies**: after changing a plugin in `examples/plugins/`, re-copy it
  (`rsync -a --exclude .venv --exclude 'slides/*.svs' examples/plugins/<id>/ "<plugins dir>/<id>/"`) and restart the server.
- `org.salus.imageviewer` (installed copy only, not in git): `backend/.venv` (OpenSlide via `openslide-python` + `openslide-bin`, Pillow) and `backend/slides/` with `slide.svs` (57 MB) and `big.svs` (1.7 GB, TCGA-BLCA, 125,495 x 76,011 px). The viewer serves the first slide alphabetically (`big.svs`).
- Test file for the demo: `echo demo > /tmp/example.demo`. Dev login: open `http://localhost:8080/dev/login` once per browser (sets the session cookie; user 0).
- No servers are running. Docs venv for building `docs/`: see `README.md` (needs `requirements-docs.txt`).

### Git state

Everything since commit `4abdd2e` is **uncommitted** on branch `http-plugin-communication` (modified: `packages/api/**`, `packages/web/**`, `README.md`, `.gitignore`, `Cargo.lock`; untracked: `design/`, `docs/`, `examples/`, `sdk/`, `mkdocs.yml`, `requirements-docs.txt`,
`packages/api/src/{browser_connections.rs,theme.css}`, `packages/web/src/workspace/{theme.rs,viewer_picker.rs}`). The user has not asked for a commit; suggest one (the work is large and tested) before starting plugin work, but do not commit without being asked.

### Suggested next steps, in order

1. **Create the workbench plugin as its own project, outside the Salus repo** (D42): e.g. a new git repo `org.salus.datasetworkbench/` with `manifest.toml`, `backend/` (Python), `frontend/` (one multi-page Vite project, D43) and a small `install.sh` of its own that builds the frontend and copies/symlinks the plugin folder into the Salus plugins directory. Copy the structure of `examples/plugins/org.salus.componentsdemo` and `examples/plugins/org.salus.imageviewer` (own venv, `run.sh`). Salus's SDK files (`sdk/js/salus_sdk.js`, `.d.ts`, `sdk/python/salus_sdk.py`) are copied into the plugin (they are not a published package).
2. **Workbench backend catalog module** (roadmap 2): `backend/workbench/` as plain Python with unit tests (stdlib `unittest`/`pytest`) and **no UI**: workbench root from `SALUS_WORKBENCH_ROOT`, groups/sources/datasets, file scan with ids and move detection, table ingest (type inference, issues), schema + states (section 7.3), upload endpoints (section 6.5). Then the routes/channels that serve the components. Measure chunked upload throughput with a multi-GB file early.
3. **Frontend tooling spike** (D43): React + TypeScript + Vite multi-page, shared `src/shared/`; use the SDK as an ES module plus `salus_sdk.d.ts` and the `--salus-*` CSS variables (`docs/guides/theming.md`). Verify a built `dist/` page loads as a component (relative asset URLs, Vite `base: "./"`) before building real UI. Choose `lifetime` deliberately: `session` fits the workbench (the catalog and caches should survive closing the panels); `panel` suits plugins that should not linger.
4. Components in this order: `browser` (left, entry), `entry-editor` (center), `dataset-viewer` (right) — and the viewers: relational viewer plugin (new), image viewer (migrate to open the file from `salus.params.file`, add `file_viewer_for`, drop the controls plugin and dependency, use OpenSeadragon's own controls; update `docs/guides/image-viewer.md` accordingly).
5. Keep `docs/` in sync with any framework change you make (strict build: `mkdocs build --strict`).

### Working rules that worked well

- Verify in the real browser (`dx serve`, built-in browser pane) after UI/framework changes; unit tests alone missed several real bugs (stale processes, tab unmounting, cached tiles).
- Kill stale processes explicitly (section 1.4) and confirm port 8080 is free before testing.
- Ask the user before building something not decided here; they want to be challenged when an idea is weak.

---

## 1. Salus framework primer (state of the repo)

Repo root: `/Users/frederic/PhD/Coding/next_gen_salus`. Rust workspace with **Dioxus 0.7** fullstack (read `AGENTS.md` for the Dioxus 0.7 API; `cx`, `use_state` etc. no longer exist).

> **Git state:** see the Handoff section (everything since `4abdd2e` is uncommitted on branch `http-plugin-communication`; pre-HTTP state is committed on `main`, tag `checkpoint-before-http`). Do not `git checkout`/`reset` without stashing.

### 1.1 What Salus is

A web framework that hosts **plugins** in an IDE-like workspace (four panels: left, center, right, bottom; tabs per panel). A plugin = **frontend** (static web page in an
iframe) + **backend** (a process, Python SDK provided) + `manifest.toml`. Hosted remotely; medical use.

### 1.2 Code map

| Path | What |
|---|---|
| `packages/api/src/lib.rs` | `run_server`: axum router (`/plugins/*` assets, `/plugin-stream` websocket, `/plugin-api/{fe_pid}/...` HTTP gateway, `/dev/login`). |
| `.../plugin_loader.rs` | Parses `manifest.toml` (incl. optional `[dependencies]`). Manifests are read **at server start**. |
| `.../plugin_library.rs` | Registry of installed plugins; ids from 10000; `get_plugin_by_identifier(folder_name)`. |
| `.../plugin_process_manager.rs` | Spawns backend (ids from 20000) and frontend (ids from 30000) processes; parent links for dependencies; `spawn_dependent_frontend_process`. |
| `.../plugin_message_router.rs` | Backend Unix-socket framing/reassembly, dispatch by channel prefix (`ws://`, `http://`, `salus://`). |
| `.../theme.css` | The theme stylesheet served at `/salus/theme.css` (route in `lib.rs`). |
| `.../theme.css` | Theme stylesheet served at `/salus/theme.css` (route in `lib.rs`). |
| `.../browser_connections.rs` | Websocket registry per browser page/user; routes `peer://` messages between related frontends. |
| `.../http_gateway.rs` | Per-backend route tables (`salus://http/open|close`), forwards `/plugin-api/<fe id>/...` to the backend, timeouts (30 s). |
| `.../salus_control.rs`, `message_frame.rs` | `salus://` requests; wire frame codec and meta container (shared with wasm). |
| `.../framework_web_api.rs` | Dioxus server functions: `get_plugin_from_id`, `spawn_frontend_plugin_process`, `get_available_plugins`, `spawn_dependent_frontend_process`. |
| `.../asset_server.rs`, `session_manager.rs` | Serves plugin files at `/plugins/<abs path>` (session cookie required); dev login sets cookie `session_id=0` (user 0). |
| `packages/web/src/main.rs` | App root; `init_plugin_bridge` + `Workspace`. |
| `.../frontend_communication_relay.rs` | In the browser page: relays iframe `postMessage` frames <-> server websocket; answers `salus://` control requests (`component/open`, `dependency/open`, `component/list`, `file/open`); routes `peer://` messages between components of one plugin locally; frontend registry; theme injection. |
| `.../workspace/` | `state.rs` (`WorkspaceState`, `PanelId`, `TabContent::Component`, `Theme`, viewer picker state), `panel.rs` (all tabs stay mounted), `tab_bar.rs`, `tab_content.rs` (`PluginFrame` shows the iframe of an existing frontend process), `plugin_picker.rs` ("+" lists entry plugins), `viewer_picker.rs` (choose a viewer for a file), `theme.rs` (page theme + localStorage), `mod.rs` (layout, resizable panels, tab opener, theme toggle). |
| `sdk/js/salus_sdk.js` | Frontend SDK (ES module, no deps) + tests (`node --test sdk/js/salus_sdk.test.mjs`) + `salus_sdk.d.ts` (TypeScript declarations) + README. |
| `sdk/python/salus_sdk.py` | Backend SDK (Python 3.10+, no deps). Canonical copy; plugins carry their own copy. |
| `examples/plugins/` | `org.salus.componentsdemo` (components, file viewers, theme demo), `org.salus.imageviewer` (OpenSeadragon + OpenSlide), `org.salus.imagecontrols` (testing only, **to be dropped** with the viewer migration). |
| `docs/`, `mkdocs.yml` | Plugin-developer docs (MkDocs Material). Build: `mkdocs build --strict`. Read these pages for SDK details: `docs/guides/*.md`, `docs/reference/*.md`. |

Installed plugins live in the **plugins directory**: macOS `~/Library/Application Support/org.fredericmrozinski.salus/plugins/`, Linux `~/.local/share/salus/plugins/`.
A plugin's **identifier is its folder name** (e.g. `org.salus.imageviewer`).

### 1.3 Plugin facts you need

- **Manifest** (`manifest.toml`, **api-version 3 only**): `[description]` name/description/developer/contact/version; `[meta] api-version = 3` and optional `entry-component`; `[backend] entrypoint, lifetime` (`lifetime` panel/session/system is honored (D44));
  one or more `[[frontend-component]]` (`component-name`, `entrypoint`, `target-panel`, optional `title` and `file_viewer_for`); optional `[dependencies] plugins = [...]`. See `docs/reference/manifest.md`.
- **Backend:** executable started with the Unix-socket path as `argv[1]`; shared by all components of a plugin instance; its lifetime follows `lifetime` in the manifest (`panel`: ends with its last frontend; `session`/`system`: keeps running and is reused, D44).
  Python SDK: `@app.channel("x")`, `@app.route("GET", "/path/{param}")`, `Payload`, `app.run(main)`.
  To use pip packages, a plugin ships its own venv and an `exec`-ing launcher script (see `examples/plugins/org.salus.imageviewer/backend/run.sh`).
- **Frontend:** `import { Salus } from "./salus_sdk.js"; const salus = Salus.connect();` reads `?fe_process_id=` (and `component`, `parent_fe_process_id`, `params`) from its URL. Channels (`ws://`) to its backend,
  `salus.http.*` to backend routes (`/plugin-api/<fe id>/...`, any method, binary ok), `salus.openComponent(name, ...)` / `salus.openDependency(id, ...)` open components in their `target-panel` and return a `Peer`,
  `salus.components()`, `salus.parent`, `salus.openFile(path)`, `salus.component`, `salus.params`.
- **Frontend ids restart with the server** (30000, 30001, ...). Never persist them and never cache by URLs that contain them without a content id in the URL.
- Plugin frontends are **same-origin with the Salus page and not sandboxed** (trusted code).
- Limits: HTTP request body 32 MiB; request timeout 30 s; frames 64 MiB (SDKs fragment); see `docs/concepts/security-and-limits.md`.

### 1.4 Running and testing

```sh
cd packages/web && dx serve                # server + web on http://localhost:8080 ; then open /dev/login once (sets the dev session cookie)
cargo test -p api --features server        # Rust tests (28)
node --test sdk/js/salus_sdk.test.mjs      # JS SDK tests (46)
mkdocs build --strict                      # docs (needs requirements-docs.txt)
```

**Pitfalls learned the hard way:**
- `pkill -f "dx serve"` is **not enough**: old servers (`target/dx/web/debug/web/server-*`) and plugin backends survive and keep port 8080 with stale manifests.
  Use `pkill -9 -f "dx serve"; pkill -9 -f "target/dx/web/debug/web/server"; pkill -9 -f "plugins/.*/backend/(plugin.py|run.sh)"`, then check `lsof -nP -iTCP:8080`.
- Manifest changes and backend code changes need a **server restart** (backends are long-lived and shared).
- The "dx and dioxus versions are incompatible" warning is harmless. The first click after `/dev/login` may need a moment (page hydration).
- The plugin picker lists plugins in random order. Plugin tab content must be keyed per tab (already done in `panel.rs`).
- The dev login user is user 0; `PluginFrame` spawns frontends as user 0.

---

## 2. Vocabulary

| Term | Meaning |
|---|---|
| **Workbench root** | The top-level directory of the Dataset Workbench on the server's disk. Contains `sources/` and `datasets/`. |
| **Source** (= source project = source cohort) | A directory under `sources/` holding raw data of one homogeneous shape plus metadata. Reusable by any number of datasets. Two types: **file source**, **table source**. |
| **Group** | A plain folder under `sources/` (nestable) used only to organize sources in the UI. |
| **Item** | A unit inside a source: a **file** (file source) or a **row** (table source). Has a stable id. |
| **Dataset** (= dataset project) | A directory under `datasets/` holding a **schema** and **entries** that reference source items and/or hold primitive values. |
| **Schema** | Ordered list of **slots** defined when a dataset is created. |
| **Slot** | One named field of every entry. Kind: **value**, **file** or **row**. |
| **Entry** | The most granular unit of the dataset (e.g. one WSI, or one question-answer pair). One cell per slot. |
| **Cell** | The content of one slot in one entry. |

---

## 3. Product vision

Create, view, edit and (later) export **datasets for medicine**. Assets (WSIs, PDFs, CSV patient tables, ...) exist **once** on disk in **Sources**; a **Dataset**
is a definition that bundles source items (plus a few dataset-local primitive values) into entries according to a schema. Like the TCGA/GDC cohort builder:
the repository holds the data, a cohort is a curated selection.

Example (WSI-VQA): sources = `patients` (table), `slides/skin` (WSIs), `qa` (table of question-answer pairs, or authored in the dataset).
Dataset schema = `patient` (row, columns: pseudonym only), `slide` (file: .svs/.tif), `question` (value: text), `answer` (value: text).
One entry per question-answer pair.

---

## 4. Decision log

| # | Decision | Rationale |
|---|---|---|
| D1 | Two spaces: `sources/` and `datasets/` under one **workbench root** directory; each source and each dataset has its own folder. | Sources are shared; datasets bundle. |
| D2 | Sources can be organized in **nested group folders**. | Easier for users. |
| D3 | A source is either a **file source** or a **table source** (separated by item **shape**, not by file extension). A file source may contain mixed file types. | Clear UI, keys, validation, viewers. |
| D4 | Each source directory holds **source metadata** (SQLite) that assigns every item an **id**; datasets reference `(source id, item id)`, never paths. | Survives moves/renames. |
| D5 | A table source is backed by **exactly one CSV file** for now (later: JSON, SQLite, DB). It declares its columns, types and a unique **key column**. | Simplicity. |
| D6 | Table sources are ingested into the source's SQLite as a **read-only cache** for fast paging/sort/filter; the CSV stays the truth and is re-ingested when it changes. | Data-grid performance, <1M rows. |
| D7 | Datasets are **SQLite** (one `dataset.sqlite` per dataset); sources have one `source.sqlite` each. Readable output comes from exporters later. | Relational data, atomic edits, indexing. |
| D8 | A **schema** must be defined first when creating a dataset. | Entries are validated against it. |
| D9 | **No cardinality** on slots: every slot holds exactly one item/value (required) or zero/one (optional). An entry is the most granular unit (a WSI, a QA pair). One-to-many is expressed by repeating the same referenced item across entries. | Matches ML datasets; flat export. Splits by patient are a group-by on a reference slot later. |
| D10 | Slot kinds: **value** (primitive stored in the entry), **file** (reference to a file item), **row** (reference to a table row, with a **column selection**). | Dates/numbers have no source row to attach to; column selection enables pseudonymization. |
| D11 | Value types for now: text, integer, float, boolean, date, datetime, choice. | Sufficient for now. |
| D12 | A file slot has `sources` (>=1, multiple allowed) and `extensions` (empty = any). A row slot has `sources` (>=1, multiple allowed; chosen columns must exist in all) and `columns` (default all). Value slots have neither. | User's slot model + proposed refinements (confirmed). |
| D13 | **Schema edits are allowed even if existing entries break** the new rules; affected entries and items get states (D32) instead of blocking the edit. Required fields may stay empty while editing. | Users restructure step by step. (Supersedes an earlier "block the edit" choice.) |
| D14 | Versioning of datasets/sources is **out of scope**; only **detect change** via fingerprints stored with each reference. | Complexity. |
| D15 | Export, anonymization, user management: **later**. Exporting is built last. | Scope. |
| D16 | The UI consists of **three plugins**: the **Dataset Workbench** plugin (one backend, **three frontend components**: `browser` on the left, `dataset-viewer` on the right, `entry-editor` in the center), the reusable **relational-data viewer**, and the existing **image viewer** (reused; the image-controls plugin is dropped). | Reuse; one backend avoids sharing code between plugins. (Updated by D24/D25/D29.) |
| D17 | Plugins that can open files declare it in the manifest (`file_viewer_for`); **Salus** lists viewers for a file type and spawns one for a file. No declared dependency, no peer communication needed. | Standard file-opening mechanism; framework does the mechanics. |
| D18 | The framework supplies a **shared theme stylesheet** and applies theme changes itself (no SDK mechanics). | User wants minimal SDK. |
| D20 | Both viewers (relational viewer, image viewer) open in the **center** panel (as tabs next to the `entry-editor`). | User preference. |
| D21 | The workbench root is configured with the environment variable **`SALUS_WORKBENCH_ROOT`** for now (documented default). | Stopgap until the framework provides a root. |
| D22 | `file_viewer_for` lists **lowercase extensions without a dot**, matched case-insensitively. | Simple. |
| D23 | The image viewer uses **OpenSeadragon's own controls** (`showNavigationControl: true`); the `imagecontrols` plugin and the dependency are dropped. | Controls plugin was only a test. |
| D19 | Frontend tech: **React + TypeScript + Vite**, TanStack Table and Query, headless components (Radix/React Aria) styled with framework CSS variables. | Ecosystem for data grids and searchable dropdowns. |
| D24 | A plugin declares **one or more frontend components** in its manifest as `[[frontend-component]]` tables, each with a unique **`component-name`**. All components of a plugin share the plugin's single backend. Each running component instance has its **own frontend process id**; the process manager records its `component_name`. | User's "components" idea (replaces the single `[frontend]`). |
| D25 | The shared-catalog problem is solved by **one plugin with several components**; no shared Python library. | Backend is already shared per plugin; no multi-process SQLite concurrency. |
| D26 | **Components of one plugin communicate through the SDK, and Salus routes those messages only in the browser (the Salus page relay), never through the server or the backend.** Cross-plugin (dependency) messaging stays as built: routed by the server. | User: no direct component-to-component channel, but an SDK/API; frontend-only routing. |
| D27 | The manifest **`api-version` becomes 3**, and **version 2 is dropped entirely**: no backward compatibility, `[frontend]` is no longer valid, all existing plugins must be migrated. | User: the framework needs no backward compatibility. |
| D28 | The **entry component** of a plugin is named in **`[meta]` as a string holding a `component-name`** (proposed key `entry-component`). **Only that component is listed** in the "+" picker; no key means the plugin is not listed (e.g. pure file viewers). | User: entry defined in `[meta]`, not as a flag on the component. |
| D29 | The workbench has **three components**: `browser` (left; tab switcher **Sources / Datasets**), `dataset-viewer` (right; list of the entries of one dataset, a **+** button to add entries, statistics at the bottom), `entry-editor` (center; shows one entry and edits its fields). | User's UI layout. |
| D30 | **One instance of each workbench component.** Opening an already-open component focuses it; the entry editor and dataset viewer **switch datasets** instead of opening a tab per dataset. | User's decision. |
| D31 | **Datasets stay flat** under `datasets/` (no groups/nesting). | User. |
| D32 | **State model.** An **entry** is `valid`, `incomplete` or `corrupted`. An **entry item** (the cell of one slot) is `ok`, `incomplete` (nothing entered yet in a required slot), `invalid` (the content no longer fits the slot's rules after a schema edit), `source missing` (the referenced source item was deleted) or `source changed` (it was edited). An entry is `corrupted` if any item is `source missing` or `source changed`; otherwise `incomplete` if any required item is `incomplete` or any item is `invalid`; otherwise `valid`. For `source changed` the entry editor offers **"Mark as valid"**, which stores the item's current fingerprint and dismisses the warning. For `source missing`, "Mark as valid" **clears the reference** (the item becomes empty, so the entry is `incomplete` if the slot is required) and the editor also lets the user **pick a replacement item**. | User's "corrupted" flag; replaces the earlier issue-code report. Confirmed. |
| D33 | A **row** reference's fingerprint covers **only the slot's selected columns**; changes to other columns do not flag `changed`. | User: other columns do not matter. |
| D34 | **State-change notifications** go through the backend, which broadcasts events to all attached components of the plugin; **navigation commands** between components use the component messaging of D26. | Confirmed. |
| D35 | Plugins only ever ask Salus to **open a file** (`salus.openFile(path)`) and never know which viewers exist. If several viewers match, **Salus shows a picker**; with none, Salus says so. | User: keep it as simple as possible for plugins. |
| D36 | **No export functionality yet**; no export UI or policy is built now. | User. |
| D37 | **No file metadata extractors** for now. Items store only generic file facts (size, mtime, extension); viewers read what they need from the file themselves. | Not needed yet. |
| D38 | The source browser can **upload files** into sources, as **chunked uploads through the existing HTTP routes** (section 6.5). | Works with today's framework limits; no framework change. |
| D39 | **Upload details:** a name collision is **rejected unless the user chooses "overwrite"**; a hash is computed **incrementally at commit** (optional); a **table source's CSV may be replaced by upload** and is treated as a source change (D32). | Confirmed. |
| D40 | If a **source is deleted entirely**, all items referencing it are `source missing`, and the schema editor shows a warning on slots whose `sources` list contains the dangling id. | Confirmed. |
| D41 | Each frontend component has an optional **`title`** (the tab label; default: the plugin name). | Several components of one plugin need distinguishable tabs. Confirmed. |
| D42 | **Plugins live outside the Salus repository**, each in its own git repo and its own folder; they are built separately and installed by copying (or symlinking) the built plugin folder into Salus's plugins directory. `examples/plugins/` in the Salus repo stays as teaching examples only. There is no `plugins/` folder and no install script in the Salus repo for now. | User. |
| D43 | **One multi-page Vite project per plugin** (React + TypeScript): one HTML entry per frontend component and a shared source folder. | Shared code lives and builds once. (Needs a short spike that built pages work under `/plugins/<abs path>/...`, Vite `base: "./"`.) |
| D44 | **Backend lifecycle.** Closing a frontend (tab) ends its frontend process and sends the backend a `detached` event. `lifetime = "panel"`: a **new backend per opened plugin instance**, killed (gracefully, then forced after a few seconds) when its last frontend closes. `lifetime = "session"`: the user's running backend of the plugin is reused by later frontends and keeps running. `lifetime = "system"`: like session but shared by all users. A component opened by another component of the **same plugin** always shares its opener's backend. Frontends are tied to the browser page that opened them: closing or reloading the page closes them. | User asked for it; implemented 2026-10-06. |

---

## 5. Storage layout

```text
<workbench root>/                       configured per installation (see Open questions)
├── sources/
│   ├── clinic-a/                       a GROUP (plain folder; nestable)
│   │   ├── patients/                   a TABLE source
│   │   │   ├── patients.csv            exactly one CSV
│   │   │   └── source.sqlite           metadata + ingested cache
│   │   └── visits/                     another table source
│   └── skin-cancer/
│       └── wsi/                        a FILE source
│           ├── slide-001.svs
│           ├── reports/report-001.pdf  files may sit in subfolders
│           └── source.sqlite           metadata (ids, fingerprints)
└── datasets/
    └── wsi-vqa-skin/
        └── dataset.sqlite              schema, entries, cells, validation
```

- A directory is a **source** iff it contains `source.sqlite`; any other folder under `sources/` is a **group**. Reserved file names inside a source: `source.sqlite`, `source.sqlite-wal`, `source.sqlite-shm`
  (the file scanner ignores them).
- Datasets are flat under `datasets/` (D31).
- SQLite runs in WAL mode (the workbench's single backend process uses worker threads, and external tools can read the files). SQLite is unsafe on network filesystems; the workbench root must be local disk.

---

## 6. Sources

### 6.1 Common

`source.sqlite` table `meta`: `source_id` (UUID, **the identity datasets reference**), `name`, `type` (`file`|`table`), `created_at`, `schema_version`.
Moving or renaming the source folder must not change `source_id`.

**Item status** (computed on rescan or access): `ok`, `changed` (fingerprint differs from the one stored in a reference; for rows only the referenced columns count, D33), `missing`, `not_accessible` (exists but the current user cannot read it; framework sandbox, later). A reference whose source item is `changed`/`missing` gives the entry item the state `source changed`/`source missing` (section 7.3).

### 6.2 File sources

- Items = files under the source directory (recursive), excluding reserved names.
- Table `items`: `id` (generated, short unique string/UUID), `rel_path`, `ext` (lowercased, no dot), `size`, `mtime_ns`, `hash` (nullable, computed lazily/optionally), `metadata` (JSON, reserved; no extractors for now, D37), `status`, `first_seen`, `last_seen`.
- **Scan** (rescan on demand and on open) matches files to existing items by `rel_path` first, then by fingerprint (size + mtime, optionally hash) to detect **moves**, which keep their id.
  New files get new ids; vanished files become `missing` (rows are kept so references stay resolvable).
- A **fingerprint** is `(size, mtime_ns[, hash])`. Hashing multi-GB slides is slow: hash only on demand/in the background.
- Scanning large folders is a **background job** that reports progress (channel messages from the backend).

### 6.3 Table sources

- Backed by **one CSV** (`*.csv` in the source directory). Metadata declares columns: `name`, `type` (text, integer, float, boolean, date, datetime), `is_key`.
  On creation, columns/types are **inferred** from the header and a sample; the user confirms/edits. Exactly one **key column** (unique, non-empty) — the key is the item id.
- Ingest into `source.sqlite`: table `data` with one typed column per declared column (dynamic DDL), `row_hash` for change detection, `status`.
  Rows that fail type checks or have empty/duplicate keys are **reported** in `row_issues` and **not referenceable** until fixed; all other rows are ingested.
- Re-ingest when the CSV fingerprint (size, mtime) changes. Item id = key value; a referenced row whose `row_hash` changed is `changed`; a vanished key is `missing`.
- Read-only for now: edits happen in the CSV (a later feature may edit through the viewer).

### 6.4 Source UI behavior

In the `browser` component (Sources tab): create group, create source (choose type; for a table source pick the CSV and confirm columns/key), rescan/re-ingest, browse a file source's files as a tree (server-side listing/search for large sources), show item status badges
and a table source's ingest issues. Opening a file or a table source's CSV goes through the file-viewer registry (image viewer, relational viewer, ...), not through a separate source-detail screen.

### 6.5 Uploading files (D38)

The `browser` component can upload files into a source: into any folder of a **file source** (e.g. WSIs, PDFs) or as the single CSV of a **table source**. Replacing a table source's CSV (or overwriting a file) is a *change* of the source, so references to affected items become `source changed` (D32).

**Can large files (WSIs) be uploaded with today's framework?** Not in one request, but yes in chunks. Facts from the code: the HTTP gateway rejects request bodies over **32 MiB** (`DefaultBodyLimit` in `lib.rs`, answers 413), buffers each request body in memory, forwards it as one backend frame (max 64 MiB) and times a request out after **30 s**.
Bodies travel as raw bytes (not number arrays), so a **chunked upload works within these limits with no framework change**; a single multi-GB request does not. Expected throughput is limited by memory copies in the gateway and Python backend (several copies of each chunk) and by the network; measure it early (roadmap step 2) before relying on it for huge slides.

**Protocol (proposal)** — backend routes of the workbench plugin, called from the `browser` component with `salus.http`:

| Route | Meaning |
|---|---|
| `POST /upload` `{source, path, size, overwrite?}` | Start. Backend validates the destination (see below), checks free disk space, creates `<dest>.upload-<id>.part`, returns `{upload_id, chunk_size}` (chunk size about 8 MiB, safely below the 32 MiB / 30 s limits). |
| `GET /upload/{id}` | Returns `{received}` bytes so far, so an interrupted upload can **resume**. |
| `PUT /upload/{id}/{offset}` (body = chunk) | Appends the chunk if `offset == received` (else returns `received` so the client resyncs); returns `{received}`. Sequential per file; several files may upload in parallel. |
| `POST /upload/{id}/commit` | Checks `received == size`, optionally computes/stores a hash incrementally, **renames atomically** to the final name, creates or updates the item, returns the item. |
| `DELETE /upload/{id}` | Abort and delete the `.part` file. |

Frontend: `file.slice(offset, offset + chunkSize)` as a `Blob` body with `salus.http.put(...)` (the SDK accepts Blob bodies); progress is shown per chunk (fetch has no byte-level upload progress); failed chunks are retried with backoff; drag-and-drop and a file picker for one or many files.

**Decided (D39):** name collisions are rejected unless `overwrite` is set; the hash is computed incrementally at commit; replacing a table source's CSV is allowed.

**Safety:** the destination is resolved inside the source directory only (reject absolute paths, `..`, reserved names such as `source.sqlite*`, names ending in `.part`); partial files use a reserved suffix that the scanner ignores and a cleanup job removes stale `.part` files; name collisions are rejected unless `overwrite`; refuse when free disk space is below the file size plus a margin.
Quotas and per-user write permissions come later with the framework sandbox.

**Later (not now):** a framework-level upload endpoint that streams the body straight to a path the plugin backend registered (no Python or memory copies in the data path, sandbox-aware) would remove the chunking and the size limits. Tracked as a possible F6.

---

## 7. Datasets

### 7.1 Schema

A dataset's **schema** is an ordered list of **slots**. It is defined first (D8) but remains editable (D13).

```text
Slot
  name          identifier-like, unique in the dataset ("slide", "patient", "question")
  label         display text
  description   optional
  required      bool
  unique        bool (optional; no two entries may have the same value/item in this slot)
  kind          "value" | "file" | "row"

  kind = value:   value_type      text | integer | float | boolean | date | datetime | choice
                  constraints     default; integer/float: min, max, unit label; text: max length / regex; choice: allowed values
  kind = file:    sources         list of source ids (>= 1; file sources only)
                  extensions      list of lowercase extensions without dot; empty = any
  kind = row:     sources         list of source ids (>= 1; table sources only)
                  columns         list of column names this slot contributes (default: all);
                                  every listed column must exist in every selected source
```

The **column selection on row slots** exists so e.g. only a pseudonym column of the patient table is part of the dataset/export, not the real name.

### 7.2 Entries and cells

- An **entry** = one record; it has one **cell** per slot.
- Cell content: **value slot** -> a typed value; **file/row slot** -> `(source_id, item_id)` plus the **fingerprint** captured when the reference was set (D14).
- The same source item may be referenced by many entries (e.g. one patient row in many entries). Entries can be created with empty required cells (see validation).
- Entries are **picked by hand** in the `entry-editor` component (no auto-join and no CSV import for now).
  Reference cells use a **dropdown with a search field at the top** (server-side search over the allowed sources; shows item key/path and source label).

### 7.3 States (D13, D32)

Nothing here ever blocks editing: users can leave required fields empty, edit another entry and come back.

**Entry item (cell) states**

| State | Meaning |
|---|---|
| `ok` | Filled and consistent with the schema and the sources. |
| `incomplete` | Nothing entered yet in a required slot. (An empty *optional* slot is `ok`.) |
| `invalid` | The content no longer fits the slot's rules after a schema edit (type, range, choice, regex, no-longer-allowed extension or source, missing column in a row slot, `unique` violation). |
| `source missing` | A file/row slot references a source item that has been deleted. |
| `source changed` | The referenced source item was edited (its fingerprint, D14/D33, no longer matches the one stored in the cell). |

**Entry states** (derived, in this order of precedence): `corrupted` if any item is `source missing` or `source changed`; otherwise `incomplete` if any required item is `incomplete` or any item is `invalid`; otherwise `valid`.

**"Mark as valid" (D32).** When an entry is opened in the `entry-editor`, each affected item shows what happened (deleted / changed). For `source changed`, **"Mark as valid"** stores the item's current fingerprint and the warning disappears (the new source content is accepted).
For `source missing`, it **clears the reference** (the item becomes empty; the entry is `incomplete` if the slot is required), and the editor lets the user **pick a replacement** item instead.

**When states are computed:** when a cell is edited (that entry); on a source rescan or ingest (all cells referencing changed/missing items; background job); when a dataset is opened (cheap fingerprint check); on demand (full check). The results are stored in the dataset file (`state` columns, section 8) so the lists and statistics are fast.

**`invalid` items** arise from schema edits (D13); they are not caused by sources. The item shows which rule it violates; the user fixes or clears the value.

The `dataset-viewer` filters and counts by entry state; the `entry-editor` marks each item with its state.

### 7.4 Schema editing

Allowed at any time. Adding a slot: existing entries get an empty cell (the entry becomes `incomplete` if the slot is required). Removing a slot: its cells are dropped (confirm first). Changing kind or type: cells that cannot be converted are cleared after confirmation.
Everything else just triggers re-computation of states.

---

## 8. SQLite layouts (proposed; adjust during implementation)

`source.sqlite` (common + type-specific):

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);            -- source_id, name, type, created_at, schema_version
-- file source
CREATE TABLE items (
  id TEXT PRIMARY KEY, rel_path TEXT UNIQUE NOT NULL, ext TEXT, size INTEGER, mtime_ns INTEGER,
  hash TEXT, metadata TEXT /*JSON*/, status TEXT NOT NULL DEFAULT 'ok', first_seen TEXT, last_seen TEXT);
-- table source
CREATE TABLE columns (position INTEGER, name TEXT PRIMARY KEY, type TEXT NOT NULL, is_key INTEGER NOT NULL DEFAULT 0);
CREATE TABLE data (/* key column PRIMARY KEY + one typed column per declared column, plus: */ row_hash TEXT, status TEXT NOT NULL DEFAULT 'ok');
CREATE TABLE row_issues (row_number INTEGER, key TEXT, column_name TEXT, code TEXT, message TEXT);
CREATE TABLE ingest (csv_name TEXT, size INTEGER, mtime_ns INTEGER, ingested_at TEXT);
```

`dataset.sqlite`:

```sql
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);            -- dataset_id (UUID), name, created_at, schema_version
CREATE TABLE slots (
  id INTEGER PRIMARY KEY, position INTEGER NOT NULL, name TEXT UNIQUE NOT NULL, label TEXT, description TEXT,
  required INTEGER NOT NULL, "unique" INTEGER NOT NULL DEFAULT 0, kind TEXT NOT NULL,   -- value | file | row
  value_type TEXT, constraints TEXT /*JSON*/, sources TEXT /*JSON list of source ids*/,
  extensions TEXT /*JSON list*/, columns TEXT /*JSON list*/);
CREATE TABLE entries (id INTEGER PRIMARY KEY, created_at TEXT, updated_at TEXT,
  state TEXT NOT NULL DEFAULT 'incomplete');                      -- valid | incomplete | corrupted (derived, section 7.3)
CREATE TABLE cells (                                              -- EAV: robust against schema edits (D13)
  entry_id INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  slot_id  INTEGER NOT NULL REFERENCES slots(id)   ON DELETE CASCADE,
  value TEXT,                                                     -- value slots: canonical text/JSON encoding
  ref_source_id TEXT, ref_item_id TEXT, ref_fingerprint TEXT,     -- file/row slots; fingerprint of the selected columns for rows (D33)
  state TEXT NOT NULL DEFAULT 'incomplete',                       -- ok | incomplete | source_missing | source_changed | invalid
  state_detail TEXT,
  PRIMARY KEY (entry_id, slot_id));
CREATE INDEX cells_ref ON cells(ref_source_id, ref_item_id);
CREATE INDEX entries_state ON entries(state);
```

An EAV `cells` table (not a wide table) is chosen because schema edits must not require DDL migrations; with <1M entries and ~10 slots it is fast enough. Exports pivot it.

---

## 9. Plugins and UI

Three plugins. The workbench is **one plugin with three frontend components** (D24/D25/D29). A later "embedding" feature may merge more, but is not needed now.

| Plugin (proposed identifier) | Component | Panel | Role |
|---|---|---|---|
| `org.salus.datasetworkbench` | `browser` (**entry component**, D28) | **left** | Tab switcher **Sources / Datasets**. *Sources:* tree of groups and sources; file sources expand to their files (the "source file browser"), table sources show their CSV; status badges (`missing`, `changed`); create group/source, **upload files** (section 6.5), rescan/re-ingest, ingest issues. Clicking a file opens it in a viewer via the file-viewer registry (F3). *Datasets:* list of datasets; create/delete; clicking a dataset shows it in `dataset-viewer` and `entry-editor`. |
| `org.salus.datasetworkbench` | `dataset-viewer` | **right** | List of **all entries of one dataset** (virtualized, server-side paging/search/filter, a filter and badges by entry state `valid`/`incomplete`/`corrupted`). **"+" button at the top** adds a new entry. **Statistics at the bottom**: number of entries, counts per entry state, per-slot completeness, counts of `source missing`/`source changed` items. Selecting an entry shows it in `entry-editor`. Header action "Edit schema". |
| `org.salus.datasetworkbench` | `entry-editor` | **center** | Shows **one entry** and edits its fields: a cell editor per slot (typed value editors; searchable reference dropdowns with a search field at the top; row slots show the selected columns read-only), each item's state badge with **"Mark as valid"** for `source changed`/`source missing` items (D32), "open referenced file" via `salus.openFile` (F3). **Schema mode** (proposal): the editor also hosts the dataset schema editor (create/edit slots), entered from "New dataset" or the viewer's "Edit schema". |
| `org.salus.relationalviewer` | `main` | **center** (D20) | Reusable data viewer for CSV, JSON records, SQLite: grid with server-side paging, sorting, filtering, column info. Registers as a file viewer for `csv`, `json`, `sqlite`, ... |
| `org.salus.imageviewer` | `main` | **center** (D20) | Existing OpenSeadragon + OpenSlide viewer, changed to open **the file it is told to open** (instead of a fixed slide) and registered as a file viewer for `svs`, `tif`, `tiff`, `ndpi`, ... Drop the `imagecontrols` plugin and the dependency; use OpenSeadragon's own controls (`showNavigationControl: true`, D23). |

The center panel holds the editor and viewer tabs side by side. Viewer plugins have no entry component (D28), so they do not appear in the "+" picker; they open when something asks to open a file.

**One backend for the workbench.** `org.salus.datasetworkbench` has a single Python backend that owns the catalog logic (sources, scan, ingest, schema, validation) and serves all three components through HTTP routes and channels.
Package the catalog logic as an ordinary Python module inside that backend (`backend/workbench/`), unit-tested without any UI.

**How the components coordinate (proposal).**
- *Navigation commands* are sent between components with the SDK's component messaging (D26), which Salus routes only inside the browser (no server or backend involved): `browser` -> `dataset-viewer` `open_dataset {dataset}`; `dataset-viewer` -> `entry-editor` `show_entry {dataset, entry}`; `dataset-viewer` -> `entry-editor` `edit_schema {dataset}`.
  Each component finds the others with `salus.openComponent(name, {reuse: true})`, which focuses an existing instance or opens it in its panel (F2). Because opening returns a handle for the component, the opener can message it straight away.
- *State-change notifications* go through the backend (D34): whenever the backend writes (entry edited, schema changed, scan finished, validation updated) it broadcasts an event on a channel to **all attached components** (`app.send(..., to=salus.ALL)`), and each component refreshes what it shows.
  This keeps components decoupled: none needs to know who else is open.
- The backend learns which component attached from the `attached` event's `component` field (F0).

**Frontend stack (D19).** React + TypeScript + Vite; TanStack Table (virtualized, server-side paging) and TanStack Query (cache for backend routes); headless components (Radix or React Aria) styled with the framework's CSS variables (F4).
Built output (`dist/`) is what the plugin ships, one build per component (e.g. `frontend/browser/dist/index.html`); sources in `frontend/<component>/src/`, with a shared `frontend/shared/` TypeScript folder for common code.
Each component bundles its dependencies (no CDN). Use the JS SDK as an ES module import; generate TypeScript declarations for it (`salus_sdk.d.ts`) as part of this work.

**Backends.** Python, standard library `sqlite3` and `csv`; the image viewer keeps its own venv (OpenSlide). Heavy work (scan, hash, ingest, validation) runs in worker threads/background tasks with progress messages over a channel;
lists use paging/search routes (`GET /items?source=...&q=...&limit=...&offset=...`, `GET /entries?dataset=...&...`).

**Typical flow.** User opens "Dataset Workbench" from the "+" picker -> `browser` (its entry component) opens on the left -> user clicks a dataset -> `browser` opens/focuses `dataset-viewer` (right) and `entry-editor` (center) and tells the viewer which dataset to show ->
user clicks an entry in the viewer -> the viewer tells the editor to show it -> in the editor the user clicks a referenced WSI -> the editor calls `salus.openFile(path)` -> Salus opens the image viewer in the center with that file.
A CSV opens in the relational viewer the same way.

---

## 10. Framework changes (DONE — kept as the specification of what was built)

Changes to **Salus itself**, in dependency order. Pointers to the code are in section 1.2.

### F0. Frontend components (D24, D26) — manifest, process manager, UI

**Manifest.** `[frontend]` is **removed**. It is replaced by an array of tables `[[frontend-component]]` (TOML cannot repeat a plain table). The manifest shape changes, so `api-version` becomes **3** and **version 2 is dropped entirely** (D27): the loader accepts only 3 and
reports a clear error for anything else. Example (the workbench):

```toml
[meta]
api-version = 3
entry-component = "browser"                  # the component the "+" picker opens (D28); a component-name

[backend]
entrypoint = "backend/run.sh"
lifetime = "session"

[[frontend-component]]
component-name = "browser"                    # required, unique within the plugin, [a-z0-9_-]+
title = "Dataset Browser"                     # optional tab title (default: plugin name)
entrypoint = "frontend/browser/dist/index.html"
target-panel = "left"                         # left | center | right | bottom

[[frontend-component]]
component-name = "dataset-viewer"
title = "Entries"
entrypoint = "frontend/dataset-viewer/dist/index.html"
target-panel = "right"

[[frontend-component]]
component-name = "entry-editor"
title = "Entry"
entrypoint = "frontend/entry-editor/dist/index.html"
target-panel = "center"

[dependencies]
plugins = []
```

Rules (validated at load): at least one component; `component-name` unique and matching `[a-z0-9_-]+`; if `[meta] entry-component` is present it must name an existing component; valid `target-panel`. **No `entry-component` key = the plugin is not listed in the "+" picker** (D28), which is right for viewer-only plugins and a deliberate choice for everything else (a single-component plugin that should be listed must name its component).
A component of a viewer plugin additionally carries `file_viewer_for = ["svs", "tif"]` (F3).

**Code.** `models.rs`: `PluginManifest.frontend_specs` becomes `frontend_components: Vec<FrontendComponent { name, title, entry_point_file_path, target_panel, file_viewer_for }>` plus `PluginMetaData.entry_component: Option<String>`; remove `PluginFrontendSpecification`.
`plugin_loader.rs`: parse only v3 (`[[frontend-component]]`); delete the v2 parser. `plugin_process_manager.rs`: `PluginFrontendProcess` gets **`component_name: String`**; `spawn_frontend(plugin_id, component, user, parent)`.
`framework_web_api.rs`/`PluginFrame`: spawning and rendering take a component; `PluginFrame` (in `workspace/tab_content.rs`) resolves the entrypoint from the component and adds `component` to the iframe query next to `fe_process_id` / `parent_fe_process_id`.
`TabContent` variants carry the component name; the tab title is the component `title` or the plugin name. The "+" picker lists only plugins that name an entry component and opens that component.
The `salus://frontend/attached` event's meta gains `{"component": "<name>"}` (receivers ignore unknown meta fields, so this is compatible) so the backend knows which component attached.
Migrate everything that uses v2: the example plugins (`imageviewer` -> single component `main`; `imagecontrols` is deleted), the user's installed test plugin(s) in the plugins directory, `docs/reference/manifest.md`, `docs/getting-started/*`, `docs/guides/image-viewer.md`, and the loader tests (`plugin_loader.rs`, `plugin_process_manager.rs` test fixtures).

### F1. Honor `target-panel`
Now per component (F0). Parsed in `plugin_loader.rs` (variable `plugin_target_panel`, currently dropped) but unused today. When Salus (not the user) opens a component, the tab goes to that component's panel.
The "+" picker keeps opening in the clicked panel. `WorkspaceState::open_tab(panel, content)` already exists; add `focus_frontend(fe_pid)` (activate the tab showing a given frontend) for reuse in F2.

### F2. Generalize "open" (components, dependencies, parameters, reuse, messaging)
Today `salus.openDependency(id)` -> relay `dependency/open` -> server fn `spawn_dependent_frontend_process` -> `Workspace` opens the tab in `PanelId::Bottom` (`workspace/mod.rs`, `frontend_communication_relay.rs`).
Needed:
- **Open a component of one's own plugin:** `salus.openComponent(name, {params, reuse})` -> `Peer`. Needs no `[dependencies]` entry. Control request `salus://component/open {component, params, reuse}`.
- **Open a dependency:** `salus.openDependency(pluginId, {component?, params?, reuse?})` -> `Peer` (component defaults to the target's entry component).
- **Panel** = the opened component's `target-panel` (replaces the hard-coded bottom panel).
- **Parameters:** appended as a URL-encoded JSON `params` query value on the iframe URL; SDK getter `salus.params` and `salus.component` (this frontend's component name). Only used at spawn.
- **Reuse (D30):** `reuse: true` returns an existing instance of that component (same plugin, same user) and focuses its tab instead of spawning; the caller delivers new data with a **component message**, so parameters only matter at spawn. Keeps the SDK tiny. (One instance per component = reuse key `(plugin, component, owner)`.)
- **Component messaging (D26): routed only in the browser.** The SDK offers the same `Peer` API for components of one's own plugin. The frames keep the `peer://` scheme: the **page relay** (`frontend_communication_relay.rs`) delivers a `peer://` frame **itself** (no server round trip) when sender and target are registered frontends of the **same plugin**, and sends everything else to the server (parent/child only).
  The relay's registry (`frontend id -> (plugin id, component name)`) is filled by `PluginFrame`; sender identity is verified via `event.source`. (Same plugin means the same trusted code, and the page only ever holds the current user's iframes.)
- **Discovery:** `salus.components(name?)` returns handles for the open components of the same plugin, answered by the relay from that registry (no server call).
- **Wiring:** opening a component or dependency uses the same plumbing as `dependency/open`: relay control request -> server function -> process manager (`spawn_dependent_frontend_process` generalized with a component and params) -> `Workspace` opens the tab. Opening a component of one's own plugin only needs the server to allocate the frontend process (same plugin, same backend), not to authorize messaging. The `set_dependent_tab_opener` channel in `workspace/mod.rs` generalizes to carry panel and component.

### F3. File-viewer registry (D17, D22, D35)
Plugins only ask Salus to open a file; Salus finds viewers and handles any choice.
- **Manifest:** a component declares `file_viewer_for = ["svs", "tif", "tiff"]` — lowercase extensions without dot, matched case-insensitively. Parsed in `plugin_loader.rs`; the registry maps extension -> list of (plugin, component).
- **Plugin-facing API (one call):** `salus.openFile(path)` -> control request `salus://file/open {file}`, answered by the page relay like `dependency/open`. Nothing else is exposed: **plugins never list or choose viewers** (D35).
- **Salus behavior:** exactly one matching viewer -> spawn it; several -> show a **viewer picker** in the workspace (a modal like `plugin_picker.rs`, listing plugin/component names; a "remember my choice" option can come later); none -> tell the user that no viewer exists for that file type.
  The chosen viewer component opens in its `target-panel` (F1) with `{file}` as **params** (F2). No `[dependencies]` entry and no peer link is needed.
- **Server side:** server functions in `framework_web_api.rs` (e.g. `get_file_viewers(extension)` used by the workspace for the picker, and `spawn_file_viewer(requester_fe, file, viewer)`) that check the session and spawn through the process manager.
- **SDK (tiny):** `salus.openFile(path)`, and in a viewer `salus.params.file`. The Python SDK needs no change.
- **Passing the file:** the framework passes the **path string** as given by the caller; today an absolute server path (no sandbox). Plugins treat it as opaque and forward it to their backend. When the sandbox arrives the framework will grant access to the file
  by a mechanism still to be designed; keep path handling confined to one function per plugin.

### F4. Shared theme (D18)
- The framework serves a stylesheet (e.g. `/salus/theme.css`): CSS variables (`--salus-bg`, `--salus-fg`, `--salus-surface`, `--salus-border`, `--salus-accent`, spacing, radius, font), base element styles, `color-scheme` per theme; **base styles in a low-priority `@layer`** so plugin CSS always wins.
- On iframe load, `PluginFrame` (`workspace/tab_content.rs`) injects `<link rel="stylesheet" href="/salus/theme.css">` into the plugin document and sets `data-theme="light|dark"` on its `<html>`; on a theme switch the workspace updates that attribute on every plugin iframe. Works because plugins are same-origin today.
  If iframes are ever sandboxed to an opaque origin: inject the link server-side into the served HTML and send the theme change by message.
- No SDK code needed; plugin CSS consumes the variables. (Minor known issue: possible brief flash of the default theme on load.) The workspace itself also needs a theme switch (only a dark stylesheet exists now).

### F5. Cleanup/hardening to fold in
- Add `salus_sdk.d.ts` (TypeScript declarations, e.g. generated from the JSDoc with `tsc --declaration --allowJs`), including the new `openComponent`, `components`, `params`, `component`, `openFile`.
- Keep `docs/` in sync: new manifest shape, SDK members, a "file viewers" guide, a "components" guide.

### 10.1 Implementation status (done 2026-10-06)

F0–F5 are implemented, covered by tests (Rust 28, JS SDK 46) and verified in a browser with the **Components Demo** plugin (`examples/plugins/org.salus.componentsdemo`: components opening and messaging each other,
reuse, sibling messaging, the viewer picker, theme switching) and the migrated image viewer/controls pair. Where the implementation differs from or adds to the plan above:

- **Component messaging keeps the `peer://` scheme.** The page relay (`frontend_communication_relay.rs`, `route_between_components`) delivers a `peer://` frame itself when sender and target are registered frontends of the **same plugin**; otherwise it goes to the server as before
  (parent/child only). There is no `component://` scheme, so the SDK `Peer` API and the ready handshake are unchanged. The relay's registry (`register_frontend`/`unregister_frontend`) is filled by `PluginFrame`.
- **Opening without a component name** (`openDependency`) resolves to the plugin's entry component, else its **only** component, else errors (`default_component` in `plugin_process_manager.rs`).
- **`Panel` keeps all tabs mounted** (hidden with CSS, `.panel-tab-hidden`). Found necessary during testing: tabs used to be unmounted when inactive, which destroyed plugin pages (state lost, unreachable for messages) — fatal for components sharing a panel.
- **Spawning is server-first.** The picker calls the server function `open_plugin(plugin_id)` (user from the session cookie; the old `spawn_frontend_plugin_process(plugin_id, user)` trusted a client-supplied user and is gone) and then opens a `TabContent::Component(SpawnedComponent)` tab. `PluginFrame` no longer spawns anything; it only shows an existing process. The picker closes after a pick and lists only plugins with an entry component.
- **Server functions** (`framework_web_api.rs`): `get_plugin_from_id`, `get_available_plugins`, `open_plugin`, `close_frontend_process`, `open_frontend_component` (own component or dependency, `params`, `reuse`), `get_file_viewers`, `open_file_viewer`. Process manager API: `open_plugin_entry`, `open_component(ComponentTarget::{Own,Dependency})`, `open_file_viewer`; params must be valid JSON up to 16 KiB.
- **File open:** `salus.openFile` -> relay -> `get_file_viewers` -> (viewer picker UI, `workspace/viewer_picker.rs`, if several) -> `open_file_viewer`. Viewers are opened **without a parent**, one new instance per opened file, with `params = {"file": path}`; `openFile` uses a 120 s request timeout because a user may be choosing.
- **`target-panel` is honored** for everything Salus opens (components, dependencies, viewers); the "+" picker still opens in the clicked panel. `WorkspaceState::focus_frontend` focuses a reused component's tab.
- **Theme:** `/salus/theme.css` (tokens `--salus-*`, base styles in `@layer salus-base`); `PluginFrame`'s `onload` injects the link and sets `data-theme` (`relay::apply_theme`); `workspace.css` now uses the tokens; a theme toggle sits at the bottom right; the choice is stored in `localStorage` (`salus-theme`) and applied from an effect (browser only).
- **SDKs:** JS SDK has `openComponent`, `components`, `openFile`, `component`, `params`, `Peer.component`, extended `openDependency` and `salus_sdk.d.ts`; Python SDK has `Plugin.frontend_component(frontend_id)` and the `attached` event meta carries `{"component": name}`.
- **Migrations:** example manifests are api-version 3 (`imageviewer`: entry `main`; `imagecontrols`: no entry component, so hidden from the picker and opened as a dependency), and the user's installed `org.salus.testplugin` manifest was migrated. `docs/` is updated (manifest reference, new guides *Frontend components*, *File viewers*, *Theming*, SDK references, protocol, concepts).
- **Lifecycle (D44, added after F0–F5):** `plugin_process_manager.rs` chooses the backend with `choose_backend` (same-plugin parent -> share; else `panel` new / `session` the user's / `system` any), records `page_id` and the backend's `owner`, and `close_frontend` removes a frontend, sends `salus://frontend/detached` (meta `{component}`) and, for a `panel` backend with no frontends left, `terminate_backend` (closes the socket = SDK shutdown signal, kills after 6 s).
  The page generates a random `page_id` (`relay::page_id()`), sends it as `/plugin-stream?page=...` and with every open request (`open_plugin`, `open_frontend_component`, `open_file_viewer` take `page_id`); when that websocket closes the server closes all frontends of the page (`close_page_frontends`). Closing a tab calls the server function `close_frontend_process` (started with `spawn_forever`, because a plain `spawn` is cancelled when the tab's button is destroyed).
  Verified with real processes: `panel` backend shared by two components, survives closing one, ends after closing the last and on page reload; a `session` backend survives closing its tab and is reused (same pid). Unit tests use fake `sleep` backends.
- **Known limits:** the theme stylesheet is injected after load (brief flash possible); the left "Sample Plugin" demo tabs have hard-coded dark colors.

### 10.2 Plugins: implementation status (done 2026-10-06)

Written directly in the Salus plugins directory (each folder is its own project, D42; none is in this repository except the migrated image viewer):

- **`org.salus.datasetworkbench`** — `backend/workbench/` (stdlib only, 34 unit tests: catalog, file scan with move detection, table ingest with issues, uploads, schema/cell states, "Mark as valid"), `backend/plugin.py` (routes + `events` broadcast),
  `frontend/` (React + TypeScript + Vite, one page per component, `src/shared`). See its `README.md`.
- **`org.salus.relationalviewer`** — file viewer for `csv`, `tsv`, `json`, `jsonl`, `ndjson`, `sqlite`, `sqlite3`, `db`; vanilla JS grid, backend loads files into SQLite (temp db) or opens SQLite read-only; paging, sorting, per-column and global filter.
- **`org.salus.imageviewer`** (migrated, in `examples/plugins/`) — opens `salus.params.file`, many slides per backend, `file_viewer_for` for slide types, OpenSeadragon's own controls with inline SVG button images. Docs tutorial rewritten. `examples/plugins/org.salus.imagecontrols` is **kept** (no plugin depends on it any more, `docs/guides/dependencies.md` still uses it as an example); remove it when the dependency docs get another example.

Where the implementation differs from or adds to the plan:

- **Frontend libraries:** React + TanStack Query only. TanStack Table and Radix were not needed (the workbench has lists, not grids; popovers and dialogs are small hand-written components). The relational viewer is plain JS (no build step).
- **Coordination:** navigation commands use component messaging (`nav` channel, received through `salus.onUnclaimed`, sent after `openComponent(name, {reuse: true, params})`; the params carry the same command so a freshly opened component needs no message). The backend additionally remembers the last selection (`GET/PUT /state`) so a component opened later starts where the user is. All data changes are broadcast on the `events` channel and make TanStack Query refetch.
- **`cells.ref_label`** is stored with every reference (listing and search without touching sources); labels refresh on every state computation.
- **File item fingerprints** are `size:mtime_ns` read with `os.stat` at check time (always current); the `items` table is updated by scans and uploads. Row fingerprints hash the selected columns' typed values (D33).
- **State computation:** on cell edit, on scan/ingest/upload commit/source removal (`Workbench.source_changed`), and when the dataset viewer opens a dataset (`POST /datasets/{id}/check`, a background job that also re-ingests changed CSVs).
- **Creating sources:** an existing folder with that name is **adopted** (its files are scanned). A table source is created empty; the CSV arrives by upload (or `POST /sources/{id}/import-csv` for a file on the server); columns/key are then confirmed in a setup dialog. "Remove source" only deletes the metadata, never files.
- **Schema editing:** changing a slot's kind clears its cells (the editor warns); type or rule changes keep the values and mark misfits `invalid`. Removing a slot drops its cells after a confirmation.
- **Upload spike:** 1 GiB in 8 MiB chunks through the HTTP gateway measured about 210 MB/s locally; no framework change needed.

**Not done / next:** user management and sandbox-aware paths (D-later), a framework-level streaming upload (F6), export (D36), resuming an upload after a backend restart (the in-memory upload registry is lost; the `.part` file is cleaned after 24 h), virtualized source file trees for folders with tens of thousands of entries (listings page with "Show more"), drag-to-reorder in the schema editor (buttons exist), a quick Playwright-style UI test suite.

---

## 11. Roadmap

1. ✅ **Framework (done, section 10.1):** F0 (frontend components), F1 (target-panel), F2 (open components/dependencies with params, reuse, frontend-routed component messaging), F3 (file-viewer registry + SDK), F4 (theme), F5 (typings, docs). Tests: Rust unit tests next to the code (see existing `#[cfg(test)]` modules), JS SDK tests, a browser check with `dx serve`.
2. **Workbench backend catalog module** (Python, `backend/workbench/`): workbench-root config, group/source/dataset creation, file scan with ids + move detection, table ingest with type inference and issue reporting, schema + validation engine. Unit-test it without any UI (pytest or unittest) — it is the heart of the system. Include the upload endpoints (section 6.5) and **measure chunked-upload throughput with a multi-GB file** (spike) before building the upload UI.
3. **`browser` component** (React, in the separate workbench repo, D42): Sources/Datasets tabs, source tree with file browsing and status badges, create/rescan, **file upload** (drag and drop, progress, resume), dataset list; opens `dataset-viewer` and `entry-editor`.
4. **Relational viewer plugin**: CSV first, then JSON and SQLite; register as file viewer.
5. **Image viewer**: open a file from params; register as file viewer; drop the controls plugin and the dependency; migrate its manifest to a `main` component.
6. **`dataset-viewer` component** (entry list, "+", statistics, state filter) and **`entry-editor` component** (entry fields, cell editors, searchable reference dropdowns, item states with "Mark as valid", schema mode; open referenced files via F3).
7. **Later:** **export (D36: nothing built now)**, versioning, anonymization, user permissions via framework sandbox, a framework-level streaming upload endpoint (F6), JSON/SQLite/database-backed table sources, multi-CSV sources, dataset groups/nesting, auto-join entry creation, CSV import of entries, list slots.

---

## 12. Open questions

Resolved or dropped (kept for traceability; viewer-choice memory and the seeded demo tabs were dropped by the user; plugin location, build layout and lifecycle -> D42-D44): workbench root -> D21; viewer panels -> D20; concurrency/shared catalog code -> D25; manifest versioning -> D27; component `title` -> D41; entry-component key name -> D28 (implemented); component messaging -> D26; editor reuse -> D30; entry component -> D28; schema editor as a mode of `entry-editor` and statistics content -> confirmed;
viewer selection -> D35; datasets nesting -> D31; row fingerprint -> D33; change notifications -> D34; export policy -> D36 (nothing built); metadata extractors -> D37 (not needed); "Mark as valid" on missing items and schema-rule violations -> D32; upload details -> D39; deleted sources -> D40.

Still open: **none.** The last three items were resolved by the user (2026-10-06): plugin location -> D42, frontend build layout -> D43, tab/process lifecycle -> D44 (implemented, section 10.1).

## 13. Non-goals (for now)

Versioning/snapshots, anonymization, user management and permissions, **export of any kind (D36)**, file metadata extractors (D37), train/validation/test splits, entries created by auto-join or CSV import, multi-CSV table sources, network filesystems, real-time multi-user editing.

## 14. References inside this repo

- Plugin docs: `docs/` (start at `docs/index.md`; HTTP routes: `docs/guides/http.md`; dependencies: `docs/guides/dependencies.md`; manifest: `docs/reference/manifest.md`; protocol: `docs/reference/protocol.md`).
- A complete non-trivial plugin pair to copy patterns from: `examples/plugins/org.salus.imageviewer` (tile cache, routes, own venv, OpenSeadragon) — see also `docs/guides/image-viewer.md`.
- SDKs: `sdk/python/salus_sdk.py`, `sdk/js/salus_sdk.js` (+ `README.md`).
