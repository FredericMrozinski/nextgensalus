# Salus JavaScript SDK (plugin frontends)

One file, no dependencies: copy `salus_sdk.js` into your plugin's frontend folder and load it as an ES module.

```html
<script type="module" src="script.js"></script>
```

```js
import { Salus, Payload } from "./salus_sdk.js";

const salus = Salus.connect(); // reads ?fe_process_id=... that Salus adds to the iframe URL
```

## Socket messages (`ws://` channels)

```js
const chat = salus.channel("chat");              // ws://chat on the backend
chat.onMessage((msg) => console.log(msg.payload.asStr()));
chat.send("hello");                              // string, bytes, ArrayBuffer or Payload
chat.send(Payload.json({ type: "ping" }));

for await (const msg of chat) { /* alternative to onMessage */ }

salus.send("other", "no registration needed to just send");
salus.onUnclaimed((msg) => console.log("nobody listens on", msg.channel));
```

Messages are delivered while a handler or iterator is attached; others are dropped (same as the backend SDK). Large messages are fragmented and reassembled automatically.

`Payload` mirrors the Python SDK, so compound records interoperate:

```js
Payload.builder().u8(1).str("name").f64(2.5).build();   // frontend -> backend
const r = msg.payload.reader(); r.u8(); r.str(); r.f64(); r.expectEnd();
msg.payload.asJson(); Payload.json({ a: 1 }); Payload.u32(7); msg.payload.asU32();
```

64 bit integers are `BigInt`.

## HTTP routes the backend opened

```js
const doc = await salus.http.json("GET", "/doc/42");                       // parsed JSON, throws HttpError if not 2xx
await salus.http.post("/doc", { json: { title: "x" } });                   // plain Response, no throw on 4xx/5xx
await salus.http.get("/search", { query: { q: "a b", tag: ["x", "y"] } });
img.src = salus.http.url("/doc/42/image");                                 // for <img>, <a href>, downloads
```

Encode dynamic path segments with `encodeURIComponent`. Requests carry the session cookie; the framework does not forward cookies or authorization headers to the backend.

## Opening components, other plugins and files

```js
const editor = await salus.openComponent("editor", { params: { id: 7 }, reuse: true }); // another component of your plugin
editor.send("show", Payload.json({ id: 7 }));                                           // ...and talk to it
const others = await salus.components("editor");                                        // open components of your plugin

const controls = await salus.openDependency("org.example.controls", { component: "main" }); // a plugin from your manifest's [dependencies]
await salus.openFile("/data/slide.svs");                                                // Salus picks a viewer (asks the user if several)

salus.component   // this frontend's component-name
salus.params      // what it was opened with (JSON), e.g. { file } for a viewer
salus.parent      // Peer of the component that opened it, or null
```

Components open in the `target-panel` of their manifest entry. `reuse: true` returns and focuses the running instance. Messages between components of one plugin are
delivered inside the browser; messages to another plugin go through the server and only between opener and opened. Channels are per direction and unbuffered, so register them at startup.

`salus.request(topic, params)` is the generic request to the framework (`salus://<topic>`) these calls are built on; it resolves with `{meta, body}`, rejects with `SalusError` when refused and `SalusTimeout` when unanswered.

TypeScript declarations: `salus_sdk.d.ts` (regenerate with `tsc salus_sdk.js --allowJs --declaration --emitDeclarationOnly --target es2022 --moduleResolution bundler --lib es2022,dom`).

## Notes

- Must run inside the Salus iframe; `Salus.connect()` throws otherwise. Only messages from the embedding page (same origin) are accepted.
- Frames cross `postMessage` and the websocket bridge as number arrays, so very large transfers are slow. Prefer HTTP for bulk data.
- Tests: `node --test sdk/js/salus_sdk.test.mjs`
