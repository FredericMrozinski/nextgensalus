# Manifest reference

Every plugin has a `manifest.toml` in its folder root. Salus reads all manifests **when the server starts**, so restart
it after adding or changing a plugin. A manifest with errors is skipped, and the reason is printed to the server log.

```toml
[description]
name = "Dataset Workbench"
description = "Assemble and edit datasets."
developer = "Salus Organization"
contact = "dev@example.org"
version = "0.1.0"

[meta]
api-version = 3
entry-component = "browser"

[backend]
entrypoint = "backend/run.sh"
lifetime = "session"

[[frontend-component]]
component-name = "browser"
title = "Browser"
entrypoint = "frontend/browser/index.html"
target-panel = "left"

[[frontend-component]]
component-name = "editor"
entrypoint = "frontend/editor/index.html"
target-panel = "center"
file_viewer_for = ["txt"]

[dependencies]                         # optional
plugins = ["org.salus.imageviewer"]
```

!!! warning "api-version 3 only"
    Older manifests (`api-version = 2` with a `[frontend]` table) are rejected with an error that names the problem.
    Replace `[frontend]` with a `[[frontend-component]]` table.

## `[description]` (required)

All values are strings and all keys are required.

| Key | Meaning |
|---|---|
| `name` | Name shown to users, for example in the plugin list and, by default, on the tab. |
| `description` | One or two sentences about the plugin. |
| `developer` | Who wrote it. |
| `contact` | How to reach the developer. |
| `version` | Your plugin's version string. |

## `[meta]` (required)

| Key | Meaning |
|---|---|
| `api-version` | Must be `3`. |
| `entry-component` | Optional. The `component-name` of the component that opens when a user picks the plugin from the **+** list. **Only plugins with this key appear in that list**; without it the plugin can only be opened by other plugins or by Salus (for example a pure [file viewer](../guides/file-viewers.md)). It must name a declared component. |

## `[backend]` (required)

| Key | Meaning |
|---|---|
| `entrypoint` | Path of the executable that Salus starts, relative to the plugin folder. It receives the socket path as its first argument. See [Plugin anatomy](../getting-started/plugin-anatomy.md#the-backend). |
| `lifetime` | `"panel"`, `"session"` or `"system"`. Required. How long the backend lives and who shares it, see below. |

All components of one plugin instance share one backend: a component opened by another component of the same plugin always connects to its opener's backend.

| `lifetime` | A frontend opened from the **+** list, as a dependency or for a file connects to... | The backend ends... |
|---|---|---|
| `panel` | a **new** backend | when its **last frontend is closed** (tab closed, or the browser page closed or reloaded) |
| `session` | the **user's running** backend of this plugin, or starts one | not when frontends close; it keeps running (until Salus stops) |
| `system` | **any user's** running backend of this plugin, or starts one | not when frontends close; it keeps running (until Salus stops) |

Closing a frontend (tab) always tells its backend with a `salus://frontend/detached` event, whatever the lifetime. A `panel` backend is then given a few
seconds to finish running handlers (the connection closing is the SDK's shutdown signal) and is killed if it is still running after that.

## `[[frontend-component]]` (required, one or more)

A plugin's user interface is made of one or more **frontend components**: separate pages that can sit in different panels and
[open and message each other](../guides/components.md). Repeat the table once per component.

| Key | Meaning |
|---|---|
| `component-name` | Required. Unique within the plugin, lowercase letters, digits, `-` and `_`. How you refer to the component in code and in `entry-component`. |
| `entrypoint` | Required. The HTML file shown in the iframe, relative to the plugin folder. |
| `target-panel` | Required. Where the component opens when Salus (not the user) opens it: `"left"`, `"center"`, `"right"` or `"bottom"`. A component picked from the **+** list opens in the panel whose **+** was clicked. |
| `title` | Optional. The tab label. Defaults to the plugin's `name`. Useful when several components of one plugin would otherwise have identical tabs. |
| `file_viewer_for` | Optional. Array of file extensions this component can display, lowercase and without a dot, e.g. `["svs", "tif"]`. See [File viewers](../guides/file-viewers.md). |

At least one component is required.

## `[dependencies]` (optional)

| Key | Meaning |
|---|---|
| `plugins` | Array of plugin identifiers (folder names) this plugin may open with `salus.openDependency()`. |

Without the table, the plugin cannot open other plugins. Anything other than an array of strings is an error.

## The plugin identifier

A plugin has no id key: **its identifier is the name of its folder** in the plugins directory.
That is what other plugins write in `[dependencies]`.
