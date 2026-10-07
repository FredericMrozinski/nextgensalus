# HTTP routes

Channels are good for events. When the frontend wants **an answer** (a document, an image, a tile, search results),
use an HTTP route: the backend declares it, the frontend calls it like any web endpoint.

=== "Python backend"

    ```python
    @app.route("GET", "/doc/{id}")
    async def get_doc(req):
        doc = load(req.params["id"])
        if doc is None:
            raise salus.HTTPError(404)
        return doc                                  # a dict becomes JSON
    ```

=== "JavaScript frontend"

    ```js
    const doc = await salus.http.json("GET", "/doc/42");   // throws HttpError on 404
    ```

## How a request travels

The frontend calls `/plugin-api/<frontend id>/doc/42`. The server

1. checks the session and that the frontend belongs to that user,
2. finds the route among those your backend opened,
3. forwards the request over the backend's socket (the SDK turns it into a `Request` for your handler), and
4. sends your handler's result back as the HTTP response.

Because it is plain HTTP, anything in a browser works: `fetch`, `XMLHttpRequest`, `<img src>`, `<a href>`,
downloads, libraries such as OpenSeadragon that load tiles by URL. `salus.http.url("/doc/42/image")` gives the URL.

## Declaring routes

```python
@app.route("GET", "/doc/{id}")          # {id} matches exactly one path segment
@app.route("POST", "/doc")
@app.route("GET", "/files/report.pdf")  # literal paths work too
```

- Methods: `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`.
- Paths start with `/`. A `{name}` segment matches one non-empty segment and is available as `req.params["name"]`
  (already percent-decoded). **A literal segment wins over a parameter**, so `/doc/new` and `/doc/{id}` can coexist.
- Two patterns that would match the same paths (`/doc/{id}` and `/doc/{name}`) conflict and the plugin fails to start.
- Routes are namespaced by plugin: your paths never collide with another plugin's.
- `{name}` always takes a whole segment. For `1_2.jpeg` style names, take the segment and parse it yourself.

Add or remove routes while running with `await app.add_route(...)` and `await app.remove_route(...)`.

## The request

`Request` fields:

| | |
|---|---|
| `req.method`, `req.path` | `GET`, `/doc/42` |
| `req.route` | the pattern that matched, e.g. `/doc/{id}` |
| `req.params` | `{"id": "42"}` |
| `req.query` | first value of each query parameter: `{"a": "1"}`; `req.query_all` gives lists |
| `req.header("content-type")` | case-insensitive header lookup; `req.headers` has all |
| `req.body` | the request body as a [`Payload`](payloads.md): `req.body.as_json()`, `.as_str()`, `.as_bytes()` |
| `req.frontend_id` | which frontend made the request |

Only a few headers reach the backend: `content-type`, `accept`, `accept-language`, `if-match`, `if-none-match`,
`if-modified-since`, `range`, `x-requested-with`. The session cookie and `Authorization` are never forwarded.

## The response

Return any of these from the handler:

| Return value | Becomes |
|---|---|
| `dict` or `list` | `200` with `application/json` |
| `str` | `200` with `text/plain; charset=utf-8` |
| `bytes`, `bytearray`, `Payload` | `200` with `application/octet-stream` |
| `None` | `204 No Content` |
| `salus.Response(...)` | exactly what you specify |

```python
return salus.Response.json({"ok": True}, status=201, headers=[("location", "/doc/7")])
return salus.Response.bytes(jpeg, content_type="image/jpeg",
                            headers=[("cache-control", "private, max-age=3600")])
return salus.Response.text("accepted", status=202)
```

Raise `salus.HTTPError(status, message=None, headers=None)` to answer with an error. An unexpected exception becomes
`500`, and the traceback goes to the backend log. `Set-Cookie`, `Content-Length` and hop-by-hop response headers are
removed by the framework.

## Calling from the frontend

```js
await salus.http.json("GET", "/doc/42");                          // parsed JSON, throws HttpError if not 2xx
await salus.http.post("/doc", { json: { title: "x" } });          // plain Response, no throw on 4xx/5xx
await salus.http.get("/search", { query: { q: "a b", tag: ["x", "y"] } });
await salus.http.put("/blob", { body: new Uint8Array([1, 2, 3]) });
```

Encode dynamic path segments yourself with `encodeURIComponent`. For the root path `/` the SDK uses
`/plugin-api/<id>` without a trailing slash.

## Things to know

- **Startup race.** A frontend usually loads before its backend has connected. Salus waits up to 10 seconds for the
  backend and a further 2 seconds for the route to appear, so call routes freely at startup.
- **Whole messages.** Request and response bodies are held in memory, not streamed. Requests are limited to 32 MiB;
  responses can be larger, and the SDK splits big frames automatically.
- **Timeouts.** A request not answered within 30 seconds becomes `504`; if the backend dies it becomes `502`.
- **Caching.** Frontend ids restart with the server, so a `Cache-Control` header on a URL that does not identify
  its content can serve stale data after a restart. Put a content id in the URL, as the
  [slide viewer tutorial](image-viewer.md) does for tiles.
- **Concurrency.** Each request runs in its own task, and plain `def` handlers run in worker threads. Cache shared
  state carefully.
