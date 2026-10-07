# Python SDK reference

Generated from the docstrings of `salus_sdk.py` (in `sdk/python/` of the Salus repository). For a guided tour see the
[Python backend guide](../guides/backend.md).

```python
import salus_sdk as salus
```

::: salus_sdk.Plugin
    options:
      members:
        - channel
        - add_channel
        - remove_channel
        - send
        - send_blocking
        - send_message
        - route
        - add_route
        - remove_route
        - frontends
        - frontend_component
        - on_frontend_attached
        - on_frontend_detached
        - request
        - messages
        - run
      merge_init_into_class: true
      show_if_no_docstring: true

## Channels

::: salus_sdk.ChannelMessage
    options:
      show_if_no_docstring: true

::: salus_sdk.Channel
    options:
      show_if_no_docstring: true

## HTTP

::: salus_sdk.Request
    options:
      show_if_no_docstring: true

::: salus_sdk.Response
    options:
      show_if_no_docstring: true

::: salus_sdk.HTTPError
    options:
      show_if_no_docstring: true

::: salus_sdk.Route
    options:
      show_if_no_docstring: true

## Payloads

::: salus_sdk.Payload
    options:
      show_if_no_docstring: true

::: salus_sdk.PayloadReader
    options:
      show_if_no_docstring: true

::: salus_sdk.PayloadBuilder
    options:
      show_if_no_docstring: true

## Framework requests

::: salus_sdk.SalusResponse
    options:
      show_if_no_docstring: true

## Errors

::: salus_sdk.SalusError

::: salus_sdk.SalusTimeout

::: salus_sdk.PayloadError

::: salus_sdk.ProtocolError

::: salus_sdk.ConnectionClosed

## Constants

| Name | Value | Meaning |
|---|---|---|
| `salus.ALL` | `0xFFFFFFFF` | Target of a message that goes to every frontend bound to the backend. |
| `salus.NO_FRONTEND` | `0` | Frontend id used on messages addressed to the framework itself. |
