# Python backend

The backend SDK is one file, `salus_sdk.py`. Copy it next to your `plugin.py`. It needs Python 3.10 or newer and has
no dependencies. For every class and method see the [Python SDK reference](../reference/python-sdk.md).

```python
import salus_sdk as salus

app = salus.Plugin()          # connects to the socket path Salus passes as argv[1]
...
if __name__ == "__main__":
    app.run()                 # serve until Salus closes the connection
```

`app.run()` runs the asyncio event loop. Handlers are `async def` functions; plain `def` handlers also work and run in
a worker thread, so blocking calls (disk, databases, OpenSlide, ...) do not stall other requests.

## Channels

A **channel** carries messages between your backend and the frontends of your plugin. Register a handler with
a decorator:

```python
@app.channel("chat")
async def chat(msg):
    text = msg.payload.as_str()           # msg.payload is a Payload, see "Payloads"
    await app.send(salus.Payload.str(f"Echo: {text}"))
```

The handler gets a `ChannelMessage` with:

| Attribute | Meaning |
|---|---|
| `msg.sender` | id of the frontend that sent it |
| `msg.channel` | channel name (without the `ws://` prefix) |
| `msg.payload` | the data, a [`Payload`](payloads.md) |

Every message runs in its own task, so handlers run concurrently.

### Sending

`await app.send(data, channel=None, to=None)` accepts a `Payload`, a `str` or bytes.

- **Inside a handler**, `channel` defaults to the channel being handled and `to` to the sender, so
  `await app.send(...)` is a reply.
- **Anywhere else** you must give a `channel`, and `to` defaults to *all* frontends.
- To send to one frontend: `to=frontend_id`. To broadcast from inside a handler: `to=salus.ALL`.

```python
await app.send(salus.Payload.json({"progress": 0.4}), channel="status")             # to everyone
await app.send(salus.Payload.json({"done": True}), channel="status", to=frontend_id)  # to one
```

From a plain `def` handler running in a worker thread use `app.send_blocking(...)`.

### Without a decorator

`add_channel` returns a `Channel` you can iterate. Use it when a loop reads better than a callback:

```python
status = app.add_channel("status")

async def main(plugin):
    async for msg in status:
        ...

app.run(main)
```

Messages for channels nobody registered are logged and dropped. To receive them yourself, iterate `app.messages()`.

## Frontends coming and going

```python
@app.on_frontend_attached
async def attached(frontend_id):
    await app.send(salus.Payload.json({"hello": True}), channel="status", to=frontend_id)

print(app.frontends)      # ids of the frontends currently bound to this backend
print(app.frontend_component(frontend_id))   # which frontend component it is (manifest component-name)
```

`on_frontend_detached` is called when a frontend goes away (its tab was closed, or its page was closed or reloaded):

```python
@app.on_frontend_detached
async def detached(frontend_id):
    ...                                  # app.frontends no longer contains frontend_id
```

How long your backend lives depends on `lifetime` in the manifest: a `panel` backend exits after its last frontend detached, `session` and `system` backends keep
running. See the [manifest reference](../reference/manifest.md#backend-required).

## HTTP routes

Routes let a frontend fetch things from your backend with normal HTTP. Declare them before `app.run()`:

```python
@app.route("GET", "/hello/{name}")
async def hello(req):
    return {"greeting": f"Hello, {req.params['name']}!"}
```

The handler receives a `Request` and returns a response. See [HTTP routes](http.md) for patterns, request fields,
responses, errors and what the framework adds or removes. To open and close routes while the plugin is
running, use `await app.add_route(method, path, handler)` and `await app.remove_route(method, path)`.

## Asking the framework

`await app.request("topic", {"param": 1})` sends a request on `salus://topic` and returns the reply. It raises
`SalusError` if the framework refuses and `SalusTimeout` after 5 seconds. Salus uses this internally to open routes;
you will rarely need it directly.

## Your own main coroutine

`app.run(main)` runs `async def main(plugin)` next to the framework loop. Use it for startup work such as loading
data, warming caches or periodic tasks. The plugin keeps serving after `main` returns. The image viewer uses it to
pre-render the overview levels of a slide.

## Lifecycle and logging

- Routes declared with `@app.route` are opened right after connecting, before your `main` starts.
- When Salus closes the connection, running handlers get 5 seconds to finish, then the process exits.
- The SDK logs to stderr (which appears in the Salus server log) and to `salus_sdk_log_<timestamp>.log` in the system
  temp folder. Use Python's `logging` for your own messages.
- If a handler raises, the SDK logs the traceback and keeps running; an HTTP handler that raises answers `500`.

## Using other languages

The SDK only implements the [wire protocol](../reference/protocol.md). A backend in any language that can speak it over
a Unix socket works the same way.
