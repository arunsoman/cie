"""TE-05: Smart Mock Server (spec section 16.2) — the real, runnable half.

Unlike a transparent network-level proxy (which would need TLS
termination + a locally-trusted CA certificate to intercept HTTPS
traffic — a substantial, security-sensitive undertaking this module does
not attempt, and arguably its own separate security-review-worthy
feature), this is a REAL, standalone FastAPI application a test
environment points its OWN HTTP client base URL at — e.g.
`STRIPE_API_BASE_URL=http://127.0.0.1:PORT/mock/api.stripe.com`
overriding the real `https://api.stripe.com` — the same "explicit
base-URL override" pattern real tools like WireMock use, not invisible
network-level interception.

Route: `ANY /mock/{service_name}/{path:path}` looks up the `MockEndpoint`
for `service_name`, validates the call (TE-07), returns a stub response
(TE-05 stub mode — see `cie.mocking.generate_stub_response`), and logs a
`MockCall`. Stub mode ONLY, matching `cie.mocking`'s own scoping —
record/replay/smart modes are not attempted (no captured traffic or
OpenAPI spec exists to build them from).

`start_mock_server`/`stop_mock_server` run this app via `uvicorn`'s
programmatic `Server` API in a background thread — real process
lifecycle, not a subprocess shell-out, so start/stop is synchronous and
testable without needing a second OS process.
"""

from __future__ import annotations

import threading
import time
from dataclasses import dataclass
from typing import Any, Optional

from fastapi import APIRouter, FastAPI, Request
from fastapi.responses import JSONResponse

router = APIRouter(tags=["cie-mock-server"])


def _handle_mock_call(engine: Any, project: str, service_name: str, path: str, method: str) -> JSONResponse:
    """Synchronous handler body — kept as a plain function (not inline in
    the async route) so it's directly unit-testable without spinning up
    a real ASGI request."""
    from cie import mocking

    endpoints = engine._repo.analysis_nodes("MockEndpoint", project=project)  # noqa: SLF001
    matches = [e for e in endpoints if e.label == service_name]
    if not matches:
        return JSONResponse(
            status_code=404,
            content={"error": f"no MockEndpoint registered for {service_name!r}; run mock_registry_run first"},
        )
    endpoint = matches[0]
    request_path = f"/{path}"
    violation = mocking.validate_mock_call(endpoint, method, request_path)
    status_code = 409 if violation is not None else 200
    call_entry = mocking.log_mock_call(endpoint.id, method, request_path, status_code)
    engine._repo.merge_delta([call_entry], [], project=project)  # noqa: SLF001
    if violation is not None:
        engine._repo.replace_analysis_nodes(  # noqa: SLF001
            "ContractViolation", [violation], [], project=project,
        )
        return JSONResponse(
            status_code=status_code,
            content={"error": "contract violation", "detail": violation["label"]},
        )
    response = mocking.generate_stub_response(endpoint, method, request_path)
    return JSONResponse(status_code=response["status_code"], content=response["body"])


@router.api_route(
    "/mock/{service_name}/{path:path}",
    methods=["GET", "POST", "PUT", "PATCH", "DELETE"],
)
async def mock_endpoint_call(
    service_name: str, path: str, request: Request, project: str = "",
) -> JSONResponse:
    """The one route this server exposes — every registered service's
    stub traffic flows through here, disambiguated by `service_name` in
    the URL path (matching the base-URL-override pattern this module's
    own docstring describes)."""
    from starlette.concurrency import run_in_threadpool

    from cie import factory

    engine = factory.get_engine(project)
    return await run_in_threadpool(
        _handle_mock_call, engine, project, service_name, path, request.method,
    )


def build_mock_server_app() -> FastAPI:
    """A standalone FastAPI app for this router — separate from the main
    CIE app (`app/api.py`) so it can run as its own process/port during
    test execution, matching TE-05's "start before test execution"
    framing."""
    app = FastAPI(title="CIE Mock Server")
    app.include_router(router)
    return app


@dataclass
class MockServerHandle:
    """A running mock server instance — `stop()` blocks until the
    background thread has actually exited, so a caller's `stop()` call
    is a real synchronization point, not a fire-and-forget signal."""

    host: str
    port: int
    _server: Any
    _thread: threading.Thread

    @property
    def base_url(self) -> str:
        return f"http://{self.host}:{self.port}"

    def stop(self, timeout: float = 5.0) -> None:
        self._server.should_exit = True
        self._thread.join(timeout=timeout)


def start_mock_server(host: str = "127.0.0.1", port: int = 0) -> MockServerHandle:
    """TE-05: start the mock server for real, in a background thread.
    `port=0` (the default) lets the OS assign a free port — the actual
    bound port is read back from the socket after startup and returned
    on the handle, so a caller doesn't have to guess or pre-reserve one.
    Blocks briefly until the server has actually started listening
    (bounded wait, not a fixed sleep) so a caller can use `handle.
    base_url` immediately after this returns.
    """
    import uvicorn

    app = build_mock_server_app()
    config = uvicorn.Config(app, host=host, port=port, log_level="warning")
    server = uvicorn.Server(config)

    thread = threading.Thread(target=server.run, daemon=True)
    thread.start()

    deadline = time.monotonic() + 10.0
    while not getattr(server, "started", False):
        if time.monotonic() > deadline:
            server.should_exit = True
            thread.join(timeout=2.0)
            raise RuntimeError("mock server did not start within 10s")
        time.sleep(0.02)

    bound_port = port
    servers = getattr(server, "servers", None)
    if servers:
        sockets = servers[0].sockets
        if sockets:
            bound_port = sockets[0].getsockname()[1]

    return MockServerHandle(host=host, port=bound_port, _server=server, _thread=thread)
