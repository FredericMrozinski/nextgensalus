# JavaScript frontend

The frontend SDK is one ES module without dependencies, `salus_sdk.js`. Copy it into your `frontend/` folder.
Every class and method is listed in the [JavaScript SDK reference](../reference/javascript-sdk.md).

```html
<script type="module" src="script.js"></script>
```

```js
import { Salus, Payload } from "./salus_sdk.js";

const salus = Salus.connect();     // reads ?fe_process_id=... that Salus adds to the page URL
```

`Salus.connect()` throws if the page is not running inside the Salus iframe, so you cannot open `index.html` directly
in a browser tab to test it. It only accepts messages from the embedding Salus page.

## Channels

```js
const chat = salus.channel("chat");                    // the backend's @app.channel("chat")

chat.onMessage((message) => {
  console.log(message.payload.asStr());                // message.payload is a Payload
});

chat.send("hello");                                    // string, Uint8Array, ArrayBuffer or Payload
chat.send(Payload.json({ type: "ping" }));
```

Or read messages with a loop:

```js
for await (const message of chat) {
  ...
}
```

- `onMessage` returns a function that removes the handler again. Async handlers are fine; errors are logged and do not
  affect other messages.
- Messages arriving while no handler or loop is attached are dropped, so register channels at startup.
- `chat.close()` unregisters the channel and ends loops.
- `salus.send("name", data)` sends on a channel without registering it.
- `salus.onUnclaimed(handler)` receives messages for channels you did not register.

Large messages are split into 4 MiB fragments and reassembled for you.

## HTTP

Call the routes your backend opened. `salus.http` adds the right URL prefix and handles the common cases:

```js
const doc = await salus.http.json("GET", "/doc/42");                  // parsed JSON; throws HttpError if not 2xx
const res = await salus.http.post("/doc", { json: { title: "x" } });   // a normal Response; no throw on 4xx/5xx
await salus.http.get("/search", { query: { q: "a b", tag: ["x", "y"] } });

image.src = salus.http.url("/doc/42/image");                           // for <img>, <a href>, downloads
```

All details, including headers and errors, are in [HTTP routes](http.md).

## Other components, other plugins, files

```js
const editor = await salus.openComponent("editor");                    // another component of your plugin
editor.send("show", Payload.json({ id: 7 }));

const controls = await salus.openDependency("org.example.controls");  // a plugin from your manifest
await salus.openFile("/data/slide.svs");                               // Salus picks a viewer
```

See [Frontend components](components.md), [Plugin dependencies](dependencies.md) and [File viewers](file-viewers.md). The page's own
component name and parameters are `salus.component` and `salus.params`; `salus.parent` is the component that opened it.

## Errors

All SDK errors derive from `SalusError`: `SalusTimeout` (the framework did not answer), `ProtocolError`,
`PayloadError` and `HttpError` (a non-2xx response from `salus.http.json`, with `.status` and `.body`).

## Tips

- Frames cross `postMessage` and the websocket as number arrays, so use HTTP for bulk data.
- Test your logic by faking the window: `Salus.connect({ window, frontendId })` accepts a stand-in; the SDK's own
  tests (`node --test sdk/js/salus_sdk.test.mjs`) show how.
