#!/usr/bin/env python3
"""Image viewer backend: serves whole slide images as Deep Zoom pyramids for OpenSeadragon.

Salus opens the viewer for a file (see `file_viewer_for` in the manifest) and the frontend asks
GET /info?file=<path>  (size, tile size, a slide id). OpenSeadragon then asks for
GET /tiles/<slide id>/<level>/<col>_<row>.jpeg.
Each slide is opened once and kept, tiles are rendered on demand with OpenSlide and the encoded JPEGs
are kept in one size-capped in-memory LRU cache shared by all slides.
"""
import asyncio
import hashlib
import io
import logging
import re
import threading
from collections import OrderedDict
from pathlib import Path

import openslide
from openslide.deepzoom import DeepZoomGenerator
from PIL import Image

import salus_sdk as salus

log = logging.getLogger("imageviewer")

TILE_SIZE = 254  # + 1px overlap on each side = 256px tiles
OVERLAP = 1
JPEG_QUALITY = 80
CACHE_BYTES = 512 * 1024 * 1024
PREWARM_MAX_TILES_PER_LEVEL = 64  # overview levels are rendered when a slide is opened
SLIDE_SUFFIXES = {".svs", ".tif", ".tiff", ".ndpi", ".scn", ".mrxs", ".vms", ".vmu", ".bif"}
# Tile URLs contain the slide id (a hash of the file), so a tile never changes under its URL and
# can be cached by the browser. "private" keeps them out of shared caches.
TILE_CACHE_HEADERS = [("cache-control", "private, max-age=86400, immutable")]
# Frontend process ids restart with the server, so anything not keyed like that must not be cached.
NO_CACHE_HEADERS = [("cache-control", "no-store")]

TILE_NAME = re.compile(r"^(\d+)_(\d+)\.jpeg$")


# --8<-- [start:cache]
class TileCache:
    """Encoded tiles in RAM: LRU limited by bytes, and concurrent requests for the same
    tile share one render."""

    def __init__(self, render, max_bytes: int):
        self._render = render  # blocking function (key) -> bytes, runs in a worker thread
        self._max_bytes = max_bytes
        self._entries: OrderedDict = OrderedDict()
        self._inflight: dict = {}
        self.bytes = 0
        self.hits = 0
        self.misses = 0

    async def get(self, key) -> bytes:
        data = self._entries.get(key)
        if data is not None:
            self._entries.move_to_end(key)
            self.hits += 1
            return data

        task = self._inflight.get(key)
        if task is None:
            self.misses += 1
            task = self._inflight[key] = asyncio.ensure_future(self._load(key))
            task.add_done_callback(lambda t: t.cancelled() or t.exception())  # mark as retrieved
        # Shielded: a client giving up on its request must not cancel the shared render.
        return await asyncio.shield(task)

    async def _load(self, key) -> bytes:
        try:
            data = await asyncio.to_thread(self._render, key)
        finally:
            self._inflight.pop(key, None)
        self._entries[key] = data
        self.bytes += len(data)
        while self.bytes > self._max_bytes and len(self._entries) > 1:
            _, evicted = self._entries.popitem(last=False)
            self.bytes -= len(evicted)
        return data

    def stats(self) -> dict:
        return {"tiles": len(self._entries), "bytes": self.bytes, "hits": self.hits, "misses": self.misses}
# --8<-- [end:cache]


class Slide:
    """An opened slide file."""

    def __init__(self, path: Path):
        self.path = path
        self.slide = openslide.OpenSlide(str(path))
        stat = path.stat()
        self.id = hashlib.sha1(f"{path}:{stat.st_size}:{stat.st_mtime_ns}".encode()).hexdigest()[:12]
        self.dz = DeepZoomGenerator(self.slide, tile_size=TILE_SIZE, overlap=OVERLAP, limit_bounds=True)

    def render(self, level: int, col: int, row: int) -> bytes:
        tile = self.dz.get_tile(level, (col, row))
        if "A" in tile.getbands():
            # Transparent areas outside the scanned region: show them white like a glass slide.
            background = Image.new("RGB", tile.size, (255, 255, 255))
            background.paste(tile, mask=tile.getchannel("A"))
            tile = background
        out = io.BytesIO()
        tile.convert("RGB").save(out, "JPEG", quality=JPEG_QUALITY)
        return out.getvalue()

    def info(self) -> dict:
        props = self.slide.properties
        return {
            "id": self.id,
            "name": self.path.name,
            "width": self.slide.dimensions[0],
            "height": self.slide.dimensions[1],
            "tile_size": TILE_SIZE,
            "overlap": OVERLAP,
            "format": "jpeg",
            "vendor": props.get("openslide.vendor"),
            "objective": props.get("openslide.objective-power"),
            "mpp": props.get("openslide.mpp-x"),
        }


slides: dict[str, Slide] = {}  # by slide id
slides_by_path: dict[str, Slide] = {}
slides_lock = threading.Lock()


def open_slide(file: str) -> Slide:
    """Opens (or finds) the slide for a file path. Raises HTTPError for files that cannot be shown."""
    path = Path(file).expanduser()
    if path.suffix.lower() not in SLIDE_SUFFIXES or not path.is_file():
        raise salus.HTTPError(404, "That is not a slide file on the server.")
    with slides_lock:
        key = str(path)
        slide = slides_by_path.get(key)
        if slide is not None:
            stat = path.stat()
            if hashlib.sha1(f"{path}:{stat.st_size}:{stat.st_mtime_ns}".encode()).hexdigest()[:12] == slide.id:
                return slide
        try:
            slide = Slide(path)
        except openslide.OpenSlideError as e:
            raise salus.HTTPError(415, f"OpenSlide cannot read this file: {e}") from None
        slides[slide.id] = slide
        slides_by_path[key] = slide
        log.info("Opened %s: %dx%d px, %d deep zoom levels", path.name, *slide.slide.dimensions, slide.dz.level_count)
        return slide


def render_tile(key) -> bytes:
    slide_id, level, col, row = key
    return slides[slide_id].render(level, col, row)


cache = TileCache(render_tile, CACHE_BYTES)
app = salus.Plugin()


# --8<-- [start:routes]
@app.route("GET", "/tiles/{slide}/{level}/{tile}")
async def tile(req):
    match = TILE_NAME.match(req.params["tile"])
    slide = slides.get(req.params["slide"])
    if slide is None or not match or not req.params["level"].isdigit():
        raise salus.HTTPError(404)
    dz = slide.dz
    level, col, row = int(req.params["level"]), int(match[1]), int(match[2])
    if level >= dz.level_count or col >= dz.level_tiles[level][0] or row >= dz.level_tiles[level][1]:
        raise salus.HTTPError(404)
    data = await cache.get((slide.id, level, col, row))
    return salus.Response.bytes(data, content_type="image/jpeg", headers=TILE_CACHE_HEADERS)


@app.route("GET", "/info")
async def info(req):
    file = req.query.get("file")
    if not file:
        raise salus.HTTPError(400, "Missing file.")
    slide = await asyncio.to_thread(open_slide, file)
    asyncio.ensure_future(prewarm(slide))
    return salus.Response.json({**slide.info(), "cache": cache.stats()}, headers=NO_CACHE_HEADERS)
# --8<-- [end:routes]


async def prewarm(slide: Slide):
    """Render the overview levels up front so the first view appears immediately."""
    dz = slide.dz
    for level in range(dz.level_count):
        cols, rows = dz.level_tiles[level]
        if cols * rows > PREWARM_MAX_TILES_PER_LEVEL:
            break
        await asyncio.gather(*(cache.get((slide.id, level, c, r)) for c in range(cols) for r in range(rows)))
    log.info("Overview levels of %s cached: %s", slide.path.name, cache.stats())


if __name__ == "__main__":
    app.run()
