# Example plugins

Install a plugin by copying its folder into the Salus plugins directory
(`~/Library/Application Support/org.fredericmrozinski.salus/plugins/` on macOS) and restarting the server.

## org.salus.imageviewer

A whole slide image viewer (OpenSeadragon, with its own zoom buttons). It is a file viewer: it cannot be opened from the dock and opens when a plugin calls
`salus.openFile(path)` for a slide file (`svs`, `tif`, `tiff`, `ndpi`, ...).

The viewer's backend uses OpenSlide, so it needs a virtualenv (not in git):

```sh
cd org.salus.imageviewer/backend
./setup.sh                      # creates .venv with openslide-python, openslide-bin, pillow (Python 3.10+)
```

The backend serves any slide Salus hands it as a Deep Zoom pyramid: `GET /info?file=<path>` (size, tile size, slide id) and `GET /tiles/<slide id>/<level>/<col>_<row>.jpeg`.
Encoded tiles are cached in RAM (512 MiB LRU, overview levels are rendered when a slide is opened).

## org.salus.filebrowser

Lists the files and folders of the server, starting in the home folder. Click a folder to enter it, use the arrow, the home button or
the path field to move around, and double-click a file to open it in a viewer (`salus.openFile(path)`). It opens in the left panel from the dock.
Needs only Python 3.10+, no virtualenv. The backend has one route, `GET /list?path=<directory>`.
It has two settings (`configurable.toml`): the start folder and whether hidden files are shown, an example of the [settings](../../docs/guides/config.md) feature.

It lists any directory the server process can read; until the framework sandboxes plugins, install it only where that is acceptable.

## org.salus.settings

Shows the settings of Salus and of every plugin (the plugins on the left, the settings of the selected one on the right) and saves changes
right away. It does not read any settings itself: it asks the framework's settings API (`/settings-api/<frontend id>`, only available to this
plugin), which returns the configurables and reads and writes the config files. Its backend does nothing. Opens in the center panel from the dock.

## org.salus.terminal

A terminal: a shell on the server, shown with [xterm.js](https://xtermjs.org) (MIT, in `frontend/vendor`, with its license). Opens in the bottom panel from the dock.
Each open terminal has its own login shell, started in a pseudo-terminal by the backend (`backend/session.py`, Python's `pty`, so Unix only; no
dependencies). Keystrokes and output travel as raw bytes on the channels `input` and `output`; `control` carries the size and the end of the shell.
When the shell ends, Enter starts a new one; closing the tab ends the shell and everything it started. The terminal's colors follow the Salus theme.

Settings (`configurable.toml`): the shell program, the start folder and the font size.

**Everyone who can use this plugin has a shell with the rights of the user running Salus.** Until plugins are sandboxed and access is per user, install it only
where that is acceptable.

Tests of the shell handling: `cd org.salus.terminal/backend && python3 -m unittest discover -s tests`.
