# JavaScript SDK reference

`salus_sdk.js` (in `sdk/js/` of the Salus repository) is a single ES module without dependencies. For a guided tour see
the [JavaScript frontend guide](../guides/frontend.md). Types below use JSDoc notation. TypeScript declarations are in `sdk/js/salus_sdk.d.ts`.

```js
import { Salus, Payload, Peer, HttpError, SalusError } from "./salus_sdk.js";
```

## `Salus`

The connection of this frontend to the framework.

### `Salus.connect(options?)` / `new Salus(options?)`

Creates the connection. Throws `SalusError` if the page is not inside a Salus iframe or the frontend id is unknown.

| Option | Default | Meaning |
|---|---|---|
| `frontendId` | `fe_process_id` URL parameter | Id of this frontend. |
| `parentFrontendId` | `parent_fe_process_id` URL parameter | Id of the frontend that opened this one. |
| `maxFragmentPayload` | 4 MiB | Largest payload per frame; larger messages are fragmented. |
| `maxMessageSize` | 256 MiB | Cap for reassembled incoming messages. |
| `parentOrigin` | this page's origin | Origin of the embedding Salus page. |
| `window`, `fetch` | the globals | Stand-ins for tests. |

### Properties

| | |
|---|---|
| `salus.frontendId` | Id of this frontend instance. |
| `salus.http` | An [`Http`](#http) helper bound to this frontend. |
| `salus.component` | This frontend's `component-name` from the manifest (`null` if unknown). |
| `salus.params` | What this component was opened with: the `params` of `openComponent`/`openDependency`, or `{file}` for a [file viewer](../guides/file-viewers.md). `null` if none. |
| `salus.parent` | `Peer` of the frontend that opened this one, or `null`. |

### Methods

| Method | Description |
|---|---|
| `channel(name, handler?)` → `Channel` | Registers the `ws://<name>` channel. Throws if it already exists. |
| `send(name, data)` | Sends on `ws://<name>` without registering the channel. |
| `sendMessage(logicalChannel, data)` | Low level: sends on any full channel name, fragmenting if needed. |
| `onUnclaimed(handler)` → `() => void` | Receives complete messages for channels you did not register: `{channel, payload, messageId}`. |
| `openComponent(name, {params?, reuse?, timeout?})` → `Promise<Peer>` | Opens another [component of your own plugin](../guides/components.md) in its `target-panel`; resolves when it has loaded. `reuse: true` returns and focuses the running instance. |
| `openDependency(identifier, {component?, params?, reuse?, timeout?})` → `Promise<Peer>` | Opens a plugin from your manifest's `[dependencies]` (its entry component, its only component, or the named one) in that component's `target-panel`; resolves when it has loaded. |
| `components(name?)` → `Promise<Peer[]>` | The other components of your plugin that are open now (optionally only those named `name`). |
| `openFile(path)` → `Promise<void>` | Asks Salus to open a file in a [viewer](../guides/file-viewers.md); Salus picks or asks the user. Rejects with `SalusError` if there is no viewer or the user cancels. |
| `peer(frontendId)` → `Peer` | Handle for a related frontend. |
| `request(topic, params?, {timeout}?)` → `Promise<{meta, body}>` | Request to the framework on `salus://<topic>`. Rejects with `SalusError` or `SalusTimeout` (default 15 s). |
| `close()` | Stops listening and closes all channels. |

`data` is a `Sendable`: a `Payload`, a string (sent as UTF-8), `Uint8Array`, `ArrayBuffer`, a typed array or an array of bytes.

## `Channel`

Returned by `salus.channel()` and `peer.channel()`.

| Member | Description |
|---|---|
| `name` | Channel name without its scheme. |
| `wireName` | Full channel name, e.g. `ws://chat` or `peer://30001/view`. |
| `peerId` | Frontend id of the peer for peer channels, otherwise `null`. |
| `closed` | `true` after `close()`. |
| `send(data)` | Sends on this channel (to the backend, or to the peer). |
| `onMessage(handler)` → `() => void` | Adds a handler; the return value removes it. |
| `close()` | Unregisters the channel and ends iterators. |
| `for await (const message of channel)` | Reads messages as they arrive. |

## `ChannelMessage`

| Field | Description |
|---|---|
| `channel` | Channel name without its scheme. |
| `payload` | A [`Payload`](#payload-payloadreader-payloadbuilder). |
| `messageId` | Id assigned by the sender. |
| `sender` | Frontend id of the sending peer; `null` for messages from the backend. |

## `Peer`

A related frontend: the one that opened this one, one this one opened, or another component of the same plugin.

| Member | Description |
|---|---|
| `id` | Frontend id of the peer. |
| `component` | The peer's component name if known (opened components, `components()` results). |
| `channel(name, handler?)` → `Channel` | Receives what the peer sends to `name`. |
| `send(name, data)` | Sends to the channel `name` of the peer. |

## `Http`

`salus.http`. Calls routes the backend opened. Requests carry the session cookie.

| Method | Description |
|---|---|
| `url(path, query?)` → `string` | URL of a route, for `<img src>`, `<a href>` and downloads. `path` starts with `/`. |
| `request(method, path, options?)` → `Promise<Response>` | Sends a request. Like `fetch`, error statuses do not throw. |
| `get`, `post`, `put`, `patch`, `delete` `(path, options?)` | Shorthands for `request`. |
| `json(method, path, options?)` → `Promise<any>` | Parsed JSON (or `undefined` for 204 or an empty body). Throws `HttpError` for non-2xx. |

`options`:

| Option | Meaning |
|---|---|
| `query` | Object of query parameters; arrays repeat the key, `null` and `undefined` are skipped. |
| `headers` | Request headers. |
| `body` | Raw body: `string`, `Blob`, `FormData`, `Uint8Array`, ... or a `Payload`. |
| `json` | Value sent as JSON (sets `content-type` unless you set it). Do not combine with `body`. |
| `signal` | An `AbortSignal`. |

## `Payload`, `PayloadReader`, `PayloadBuilder`

Typed message data; see the [Payloads guide](../guides/payloads.md).

| `Payload` | |
|---|---|
| `new Payload(bytes?)` | Copies `Uint8Array`, `ArrayBuffer` or `number[]`. |
| `Payload.str(s)`, `Payload.json(v)` | Text and JSON payloads. |
| `Payload.u8/i8/u16/i16/u32/i32/u64/i64/f32/f64/bool(v)` | A single number. 64-bit values are `BigInt`. |
| `Payload.builder()` | Starts a record. |
| `asBytes()`, `asStr()`, `asJson()` | Read as bytes, text or JSON. |
| `asU8()` … `asBool()` | Read a single number. |
| `reader()` | Reads a record. |
| `length`, `equals(other)` | Size and comparison. |

| `PayloadReader` | |
|---|---|
| `u8()` … `bool()`, `str()`, `bytes()`, `json()` | Read the next field. |
| `raw(n)`, `rest()` | Read `n` bytes, or everything that is left. |
| `remaining`, `atEnd`, `expectEnd()` | Position helpers; `expectEnd()` throws on leftovers. |

| `PayloadBuilder` | |
|---|---|
| `u8(v)` … `bool(v)`, `str(s)`, `bytes(b)`, `json(v)`, `raw(b)` | Append a field. Each returns the builder. |
| `build()` | Returns the `Payload`. |

## Errors

| Class | Thrown when |
|---|---|
| `SalusError` | Base class of all SDK errors; also for refused requests and a closed connection. |
| `SalusTimeout` | The framework did not answer a request in time. |
| `ProtocolError` | A frame or meta container is malformed. |
| `PayloadError` | A payload cannot be parsed or built. |
| `HttpError` | `http.json` received a non-2xx response. Has `status`, `response` and `body` (text). |

## Helpers

| | |
|---|---|
| `packMeta(meta, body?)` → `Uint8Array` | Builds the meta container used by `salus://` requests. |
| `unpackMeta(data)` → `{meta, body}` | Parses it. |
