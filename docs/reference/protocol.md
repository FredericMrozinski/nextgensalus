# Wire protocol

!!! info "Who needs this"
    Plugin authors using the Python or JavaScript SDK never touch the wire format. This page is for writing a backend in
    another language, writing a new SDK, or debugging.

Every plugin backend talks to Salus over **one Unix domain stream socket**. Everything is multiplexed over it: messages
with the plugin's frontends, HTTP requests, and requests to the framework. The multiplexing key is the **logical
channel**, a string carried in every frame.

Salus listens on a socket path and passes it to the backend as its **first command-line argument**. The backend connects
as a client. There is exactly one connection per backend process, and several frontends may be bound to it.

*Conventions.* All integers are little-endian. Strings are UTF-8. "Must" marks a requirement for interoperability.
The Python SDK is the reference implementation where this page is silent.

## Frames

Each frame on the stream is a length prefix followed by the frame body:

| Field | Type | Meaning |
|---|---|---|
| `frame_len` | `u32` | Number of bytes in the body below. Does not include itself. |
| `frontend_process_id` | `u32` | Towards the framework: the frontend the data comes from, or the reserved id `0`. Towards the backend: the target frontend. See [addressing](#addressing). |
| `message_id` | `u32` | Identifies a message among those in flight from the same sender. See [fragmentation](#fragmentation). |
| `flags` | `u8` | Bit 0 (`0x01`) is `MORE_FRAGMENTS`. All other bits are reserved and must be zero. |
| `channel_len` | `u16` | Length of the logical channel in bytes. |
| `logical_channel` | bytes | `channel_len` bytes of UTF-8. |
| `payload` | bytes | The rest of the frame body. |

The fixed header is 11 bytes. `frame_len` must not exceed `67108864` (64 MiB); the channel must not exceed 65535 bytes.

### Validation

| Condition | Action |
|---|---|
| `frame_len` greater than 64 MiB | Fatal. Close the connection; the stream cannot be resynchronized. |
| Connection closes inside a length prefix or frame | Treat as an error close. |
| Body shorter than 11 bytes, channel runs past the end, channel not UTF-8, reserved flag bits set | Drop the frame, log a warning, keep reading. |

### Atomic writes

Frames from different sources are interleaved on the stream. A sender must write the length prefix and body of one
frame without any other bytes in between (hold a lock per frame, not per message).

## Fragmentation

A single frame carries at most 64 MiB. Larger messages are split into several frames, each a complete valid frame.

- All fragments of a message carry the same `frontend_process_id`, `message_id` and `logical_channel`.
- Every fragment except the last has `MORE_FRAGMENTS` set. A message that fits one frame is one frame with the flag clear.
- Fragments of one message appear **in order**. Frames of *different* messages may be interleaved between them.
- The reassembly key is `(frontend_process_id, message_id)`. A message is delivered when its last fragment arrives.
- `message_id` is chosen by each sender independently (a wrapping `u32` counter is enough) and must not be reused while a
  fragmented message with that id is in flight.
- A fragment whose channel differs from the first one is a protocol violation: drop the message and discard its remaining fragments.
- Receivers cap the reassembled size (the SDK and framework use 1 GiB).

`message_id` has no meaning beyond reassembly. It is unrelated to the request ids inside `http://` and `salus://` metadata.

## Addressing

`frontend_process_id` depends on direction:

| Direction | Meaning |
|---|---|
| Frontend → backend | The sender: the frontend that originated the data. |
| Backend → frontend | The target frontend. |

Two values are reserved:

| Value | Name | Use |
|---|---|---|
| `0` | `NO_FRONTEND` | `salus://` frames: backend requests and the framework's replies. |
| `4294967295` | `ALL_FRONTENDS` | Broadcast to every frontend bound to the backend. Only meaningful for `ws://`. |

The framework delivers a backend's frames only to frontends bound to that backend and drops others.

## Logical channels

The prefix of the channel selects the protocol.

| Prefix | Meaning |
|---|---|
| `ws://<name>` | Opaque messages between a frontend and the backend. |
| `http://...` | HTTP requests forwarded to the backend and its responses. |
| `salus://<topic>` | Requests from the backend to the framework, replies, and framework events. |
| `peer://<frontend id>/<name>` | Messages between two frontends. Never reaches a backend. See [peer messages](#peer-messages). |

Frames with another prefix are forwarded unchanged where a target is clear; this is not an error.

### `ws://`

The framework forwards `ws://` frames between frontend and backend **unchanged**: same channel, same payload. It does not
interpret or validate channel names. There is no registration step; a backend drops (with a warning) messages for
channels it has no handler for.

## Meta container

`http://` and `salus://` payloads share one layout:

| Field | Type | Meaning |
|---|---|---|
| `meta_len` | `u32` | Length of the meta in bytes. |
| `meta` | bytes | UTF-8 JSON text of a JSON **object** (an empty meta is `{}`, two bytes). |
| `body` | bytes | The rest of the payload; raw bytes, may be empty. |

`NaN` and `Infinity` are not valid JSON. Receivers **ignore unknown meta fields** so that fields can be added later. A
payload shorter than 4 bytes, a `meta_len` past the end, invalid JSON or a non-object meta is malformed: drop it and log.

## `salus://` control requests

The backend sends a request on `salus://<topic>` with `frontend_process_id = 0`. The meta carries an integer `id` chosen
by the backend, plus topic parameters. The framework answers **every** request on the **same channel** with
`frontend_process_id = 0` and this meta:

| Field | Meaning |
|---|---|
| `id` | Copied from the request. |
| `ok` | `true` on success, `false` on failure. |
| `error` | Human-readable reason when `ok` is `false`. |
| others | Topic-specific results. |

Unknown topics are answered with `ok: false, error: "unknown topic"`. The Python SDK waits 5 seconds for a reply.
Replies may come in any order.

| Topic | Direction | Meaning |
|---|---|---|
| `salus://http/open` | backend → framework | Open an HTTP route. |
| `salus://http/close` | backend → framework | Close an HTTP route. |
| `salus://frontend/attached` | framework → backend | Event: a frontend was bound. The frontend is in the envelope's `frontend_process_id`; meta is `{"component": "<component-name>"}`, the frontend component it runs. |
| `salus://frontend/detached` | framework → backend | Event: a frontend was closed (its tab or its browser page went away). Same shape as `attached`: the frontend is in the envelope's `frontend_process_id`, meta is `{"component": "<component-name>"}`. |

Frontend lifecycle events carry no `id` and need no reply. After accepting a backend's connection, the framework sends
`attached` for every frontend already bound to it, then one for each new frontend. The SDK builds its set of bound
frontends only from these events.

## HTTP

The framework owns the HTTP side: it receives requests from frontends, matches them against the routes the backend
opened, forwards them over the socket, and turns the backend's response back into an HTTP response.

### Opening and closing routes

`salus://http/open` and `salus://http/close` take (besides `id`):

| Field | Meaning |
|---|---|
| `method` | `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD` or `OPTIONS`, uppercase. |
| `path` | Pattern starting with `/`, relative to the plugin (the framework namespaces it per plugin). |

Pattern syntax: split on `/`; a literal segment must match exactly; a `{name}` segment matches exactly one non-empty
segment and captures it as parameter `name`; a literal wins over a parameter.

Opening an already open route, an invalid pattern, or a pattern conflicting with an existing one (for example
`/doc/{id}` and `/doc/{name}` for the same method) is rejected with `ok: false`. Closing a route that is not open is
rejected. When the connection ends, all routes opened over it are dropped.

### Forwarded requests

For a request matching an open route, the framework sends a frame to the backend:

- **Channel:** `http://request`. The SDK identifies requests by their meta, so any `http://` channel works.
- **`frontend_process_id`:** the frontend that made the request.
- **Payload:** the meta container with the raw request body as `body`.

| Meta field | Type | Meaning |
|---|---|---|
| `id` | integer | Request id chosen by the framework, unique among in-flight requests of the connection. Echoed in the response. |
| `method` | string | HTTP method. |
| `path` | string | Concrete request path, relative to the plugin, e.g. `/doc/42`. |
| `route` | string | The pattern that matched, as registered, e.g. `/doc/{id}`. |
| `params` | object of strings | Captured parameters, percent-decoded. |
| `query_string` | string | Raw query string without the `?`. |
| `headers` | array of `[name, value]` | Forwarded request headers (an array so repeated headers survive). |

### Responses

The backend answers with a frame on channel `http://response`, to the frontend that made the request, with this meta and
the response body:

| Meta field | Type | Meaning |
|---|---|---|
| `id` | integer | The `id` of the request it answers. |
| `status` | integer | HTTP status, 100 to 599. |
| `headers` | array of `[name, value]` | Response headers. |

Responses for unknown or already completed ids are ignored. The framework applies a timeout for unanswered requests
(`504`) and fails pending requests with `502` when the backend disconnects.

## Peer messages

Frontends exchange messages with `peer://<other frontend id>/<name>`. A frontend sends with the **target** in the
channel and its own id as `frontend_process_id`. The receiver gets the frame with `frontend_process_id` set to the target and the channel
rewritten to `peer://<sender id>/<name>`, so it sees who sent it. Two routes exist:

- **Components of the same plugin** (same page, same user): the Salus page delivers the frame **itself, inside the browser**; it never reaches the server.
  The page knows which plugin and component each iframe shows.
- **Anything else:** the frame goes to the server, which delivers it only if the two frontends are related (one opened the other as a
  dependency) and refuses it otherwise.

Peer frames use the same frame layout and fragmentation rules. As the reassembly key the receiver must include the
sending peer, since message ids are chosen per sender.

### Control requests of frontends

Frontends send `salus://` requests with meta `{id, ...}` to the Salus page, which answers on the same channel with `{id, ok, error?, ...}` (they are never
forwarded to a backend):

| Topic | Request meta | Reply fields |
|---|---|---|
| `salus://component/open` | `component`, `params?`, `reuse?` | `frontend_id`, `plugin_id`, `component`, `reused` |
| `salus://dependency/open` | `plugin`, `component?`, `params?`, `reuse?` | as above |
| `salus://component/list` | `component?` | `components`: `[{frontend_id, component}]` of the other open components of the requester's plugin |
| `salus://file/open` | `file` | `frontend_id`, `plugin_id`, `component` of the viewer (after the user picked one if there were several) |

`params` is any JSON value; the opened component finds it as `?params=<url-encoded JSON>` in its page URL. A component that was not already running announces itself to the one that
opened it by sending an empty message on `peer://<opener id>/salus.ready` once it has loaded; the SDKs wait for it before an open resolves.

## Frames in the browser

Between a plugin iframe and the Salus page, a frame is a structured-clone object:

```json
{ "frontend_process_id": 30001, "message_id": 4, "flags": 0,
  "logical_channel": "ws://chat", "payload": [104, 105] }
```

(`payload` is an array of byte values; `Uint8Array` is accepted when receiving.) The Salus page checks that the message
comes from the iframe of that frontend, then forwards the frame, binary-encoded exactly as above, over a websocket to
the server.

## Connection lifecycle

| Event | Behavior |
|---|---|
| Backend connects | Accepted. `attached` events for bound frontends; `salus://http/open` accepted immediately. |
| Frontend binds | `attached` event. |
| Frontend closes | `detached` event. For a `panel` backend whose last frontend closed, the framework then closes its end of the socket (the SDK's shutdown signal) and kills the process if it has not exited after a few seconds. |
| Backend closes the socket | Pending HTTP requests fail, routes and partial messages are dropped. |
| Framework closes the socket | The SDK treats it as shutdown: running handlers get 5 seconds, then the backend exits. |
| Backend sends garbage | See [validation](#validation). Only a bad length prefix or truncated frame is fatal. |

## Conformance vectors

Hex, including the 4-byte length prefix, as it appears on the stream.

**V1: plain `ws://` message.** Frontend `1`, message id `2`, channel `ws://a`, payload `hi`:

```text
13000000 01000000 02000000 00 0600 77733a2f2f61 6869
```

**V2: one message in three fragments.** Frontend `3`, message id `5`, channel `ws://big`, payload `0123456789` split as
`0123`, `4567`, `89`:

```text
17000000 03000000 05000000 01 0800 77733a2f2f626967 30313233
17000000 03000000 05000000 01 0800 77733a2f2f626967 34353637
15000000 03000000 05000000 00 0800 77733a2f2f626967 3839
```

Another message may be interleaved between these frames, but not inside one.

**V3: frontend attached event.** Frontend `12`, message id `3`, channel `salus://frontend/attached`, meta `{}`:

```text
2a000000 0c000000 03000000 00 1900 73616c75733a2f2f66726f6e74656e642f6174746163686564 020000007b7d
```

**V4: route open and reply.** Backend to framework with `frontend_process_id = 0`, then the framework's reply on the same channel:

```text
channel  salus://http/open
meta     {"id":1,"method":"GET","path":"/doc/{id}"}
...
channel  salus://http/open
meta     {"id":1,"ok":true}
```

**V5: a forwarded request and its response.**

```text
channel  http://request                 (framework to backend, from frontend 12)
meta     {"id":7,"method":"POST","path":"/doc/42","route":"/doc/{id}",
          "params":{"id":"42"},"query_string":"a=1",
          "headers":[["content-type","text/plain"]]}
body     hello

channel  http://response                (backend to framework, to frontend 12)
meta     {"id":7,"status":200,"headers":[["content-type","text/plain"]]}
body     ok
```
