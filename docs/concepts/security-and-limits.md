# Security and limits

## What Salus enforces

- **Sessions.** Plugin files, HTTP routes and the message websocket all require a valid session cookie.
- **Ownership.** A frontend belongs to the user who opened it. HTTP requests and channel messages for a frontend are
  only accepted from that user's session.
- **Sender identity.** The Salus page only relays a message from an iframe if the message comes from the window of
  the frontend it claims to be, so one plugin cannot send as another.
- **Peers.** Across plugins, frontends can only message the frontend that opened them and the ones they opened, and a plugin can
  only open plugins it lists under `[dependencies]` in its manifest. Components of the *same* plugin may message each other (the Salus page routes this inside the browser).
- **Backend isolation of credentials.** The session cookie and `Authorization` header are **never** forwarded to a
  backend. Only these request headers are: `content-type`, `accept`, `accept-language`, `if-match`,
  `if-none-match`, `if-modified-since`, `range`, `x-requested-with`.
- **Response headers.** `Set-Cookie`, `Content-Length` and hop-by-hop headers from a backend are dropped, and
  `X-Content-Type-Options: nosniff` is added.

!!! warning "Plugins are trusted code"
    Plugin frontends currently run on the same origin as the Salus page, without an iframe sandbox. Treat an
    installed plugin as trusted code and only install plugins you would run on the server itself. Stronger
    isolation is planned but is not there yet.

## Limits and timeouts

| What | Limit | What happens beyond it |
|---|---|---|
| One frame on the backend socket | 64 MiB | Larger messages are split into fragments automatically (see below). |
| Message fragmenting by the JS SDK | 4 MiB per frame | Splits and reassembles on its own. |
| Message reassembly | 1 GiB (framework and Python SDK), 256 MiB (JS SDK) | The message is dropped with a warning. |
| HTTP request body | 32 MiB | Rejected by the server (`413`). |
| HTTP request without an answer | 30 seconds | `504 Gateway Timeout` to the frontend. |
| Backend disconnects during a request | immediately | `502 Bad Gateway`. |
| Backend not connected yet | waits up to 10 seconds | then `503 Service Unavailable`. |
| Route not opened yet after the backend connected | 2 seconds | then `404 Not Found`. |
| Framework request from the Python SDK (`app.request`) | 5 seconds | `SalusTimeout`. |
| Framework request from the JS SDK (`salus.request`) | 15 seconds | `SalusTimeout`. |
| Running Python handlers when Salus closes the connection | 5 seconds | cancelled. |

The waiting rules for HTTP exist because a frontend usually loads a moment before its backend has connected and
registered its routes. Your frontend can call a route as soon as it starts.

## Good to know

- **Bulk data.** Channel frames cross `postMessage` and a websocket as number arrays in the browser, so very large
  transfers over channels are slow. Use [HTTP routes](../guides/http.md) for images, files and other bulk data.
- **Caching.** Frontend ids restart with the server. If you set `Cache-Control` on a response, make sure the URL
  contains something that identifies the content (the image viewer puts a hash of the slide file in tile URLs).
- **Opening twice.** Opening the same component or dependency twice opens two independent instances unless you pass `reuse: true`.
