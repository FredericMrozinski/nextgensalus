import { Salus } from "./salus_sdk.js";

const salus = Salus.connect();
const statusEl = document.getElementById("status");

// Salus opened this viewer for a file and passes its path in the parameters (see `file_viewer_for` in the manifest).
const file = salus.params?.file;

let slide = null;      // metadata from GET /info
let viewer = null;

function render() {
    const parts = [slide?.name ?? "Loading slide..."];
    if (viewer && viewer.world.getItemCount() > 0) {
        const viewport = viewer.viewport;
        // Image pixels per screen pixel; at 1.0 the scan is shown at its native (objective) magnification.
        const pixelRatio = viewport.viewportToImageZoom(viewport.getZoom(true));
        const magnification = slide?.objective ? ` (${(pixelRatio * Number(slide.objective)).toFixed(1)}x)` : "";
        parts.push(`zoom ${Math.round(pixelRatio * 100)}%${magnification}`);
    }
    statusEl.textContent = parts.join("  |  ");
}

if (!file) {
    statusEl.textContent = "No slide was given. Open a slide file from the Dataset Workbench.";
    throw new Error("no file parameter");
}

try {
    slide = await salus.http.json("GET", "/info", { query: { file } });
} catch (error) {
    statusEl.textContent = `Could not load the slide: ${error.body || error.message}`;
    throw error;
}

// OpenSeadragon's own zoom, home and full-page buttons. The images are inline so the plugin needs no extra files.
// --8<-- [start:controls]
function icon(paths, opacity) {
    const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="28" height="28" viewBox="0 0 28 28">` +
        `<rect x="1" y="1" width="26" height="26" rx="5" fill="#1b1e25" fill-opacity="0.75" stroke="#fff" stroke-opacity="${opacity}"/>` +
        `<g stroke="#fff" stroke-opacity="${opacity + 0.2}" stroke-width="2" stroke-linecap="round" fill="none">${paths}</g></svg>`;
    return `data:image/svg+xml,${encodeURIComponent(svg)}`;
}
const buttons = (paths) => ({ REST: icon(paths, 0.5), GROUP: icon(paths, 0.5), HOVER: icon(paths, 0.9), DOWN: icon(paths, 1) });
// --8<-- [end:controls]

// A Deep Zoom image described directly; OpenSeadragon then requests the tiles as plain GETs under
// <Url><level>/<col>_<row>.jpeg. The Salus framework forwards those to the route the backend opened.
// The slide id in the URL keeps cached tiles from ever being mixed up between slides.
// --8<-- [start:openseadragon]
viewer = OpenSeadragon({
    element: document.getElementById("viewer"),
    tileSources: {
        Image: {
            xmlns: "http://schemas.microsoft.com/deepzoom/2008",
            Url: salus.http.url(`/tiles/${slide.id}/`),
            Format: slide.format,
            Overlap: slide.overlap,
            TileSize: slide.tile_size,
            Size: { Width: slide.width, Height: slide.height },
        },
    },
    showNavigationControl: true,
    prefixUrl: "", // the button images below are complete data: URLs, nothing is prepended
    navImages: {
        zoomIn: buttons('<path d="M14 8v12M8 14h12"/>'),
        zoomOut: buttons('<path d="M8 14h12"/>'),
        home: buttons('<path d="M7 14l7-6 7 6M9 13v7h10v-7"/>'),
        fullpage: buttons('<path d="M8 12V8h4M16 8h4v4M20 16v4h-4M12 20H8v-4"/>'),
    },
    showNavigator: true,
    navigatorPosition: "TOP_RIGHT",
    navigatorSizeRatio: 0.18,
    animationTime: 0.35,
    blendTime: 0.1,
    maxZoomPixelRatio: 2,
    minZoomImageRatio: 0.8,
    visibilityRatio: 0.5,
    imageLoaderLimit: 8,
    gestureSettingsMouse: { clickToZoom: false },
});
// --8<-- [end:openseadragon]

viewer.addHandler("open", render);
// "animation" fires every frame of an animated zoom or pan, "animation-finish" at the end.
viewer.addHandler("animation", render);
viewer.addHandler("animation-finish", render);
viewer.addHandler("open-failed", (event) => {
    slide = { name: `Could not open the slide: ${event.message}` };
    render();
});

render();
