# Example plugins

Install a plugin by copying its folder into the Salus plugins directory
(`~/Library/Application Support/org.fredericmrozinski.salus/plugins/` on macOS) and restarting the server.

## org.salus.imageviewer

A whole slide image viewer (OpenSeadragon, with its own zoom buttons). It is a file viewer: it has no entry in the **+** list and opens when a plugin calls
`salus.openFile(path)` for a slide file (`svs`, `tif`, `tiff`, `ndpi`, ...).

The viewer's backend uses OpenSlide, so it needs a virtualenv (not in git):

```sh
cd org.salus.imageviewer/backend
./setup.sh                      # creates .venv with openslide-python, openslide-bin, pillow (Python 3.10+)
```

The backend serves any slide Salus hands it as a Deep Zoom pyramid: `GET /info?file=<path>` (size, tile size, slide id) and `GET /tiles/<slide id>/<level>/<col>_<row>.jpeg`.
Encoded tiles are cached in RAM (512 MiB LRU, overview levels are rendered when a slide is opened).
