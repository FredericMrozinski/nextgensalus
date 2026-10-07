# Plugin anatomy

## Folder layout

```text
org.example.myplugin/            the folder name is the plugin's identifier
├── manifest.toml                required (api-version 3)
├── backend/                     anything your backend needs
│   ├── plugin.py                the entrypoint named in the manifest, executable
│   └── salus_sdk.py
└── frontend/
    ├── index.html               the entrypoint of a component named in the manifest
    └── ...                      more components, scripts, styles, images, libraries
```

- **Identifier.** Other plugins refer to yours by its folder name, for example in
  [`[dependencies]`](../guides/dependencies.md). Choose it once and keep it stable.
- **Entrypoint paths** in the manifest are relative to the plugin folder.
- Salus serves everything in the plugin folder to the plugin's own frontend, so frontend assets can sit anywhere
  below it and be referenced with relative URLs.

## The backend

The backend entrypoint is **executed directly** by Salus with one argument: the path of the Unix socket to
connect to. That means:

- the file must be executable and start with a shebang (`#!/usr/bin/env python3`), or be a compiled binary or script
  of any language that speaks the [wire protocol](../reference/protocol.md);
- the Python SDK reads the socket path from `sys.argv[1]` for you;
- the process is stopped when Salus stops.

One backend process serves **all frontends of the plugin**; the SDK tells you which frontend a message came from.

### Dependencies of your backend

If your backend needs Python packages, give the plugin its own virtual environment and point the manifest at a small
launcher script, so nothing is installed into the system Python. The image viewer example does this:

```sh
#!/bin/sh
cd "$(dirname "$0")" || exit 1
exec .venv/bin/python plugin.py "$@"
```

`exec` matters: Salus must be talking to the Python process itself, not to a shell that wraps it. Keep a
`requirements.txt` and a `setup.sh` next to it that creates the environment.

## The frontend

A plugin's user interface is one or more **frontend components** (`[[frontend-component]]` in the manifest). Each one is a normal web page that
Salus shows in its own iframe, in the panel named by its `target-panel`. Most plugins have a single component; use
[several](../guides/components.md) when the interface naturally splits, for example a list on the left and an editor in the center.

Salus serves a component's page from `/plugins/<folder>/<entrypoint>` and adds query parameters:

| Parameter | Meaning |
|---|---|
| `fe_process_id` | The id of this frontend instance. The JS SDK reads it for you. |
| `component` | The component's `component-name` (`salus.component`). |
| `parent_fe_process_id` | Only present if another component or plugin opened this one (`salus.parent`). |
| `params` | JSON the opener passed, or `{"file": ...}` for a [file viewer](../guides/file-viewers.md) (`salus.params`). |

Use ES modules (`<script type="module">`) to load the JavaScript SDK. Salus also adds its [theme stylesheet](../guides/theming.md) to every page, so plain HTML already follows the light/dark theme. Third-party libraries are plain files in
your folder: the image viewer example ships OpenSeadragon as `frontend/openseadragon.min.js`.

## The manifest

`manifest.toml` is described key by key in the [manifest reference](../reference/manifest.md).
