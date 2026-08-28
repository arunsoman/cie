"""Typed JSON-Schema generation for cie's LLM tool surface (ToolService).

`ToolService.describe()` (`tools/__init__.py`) has always returned a bare
``{name, signature, doc}`` manifest — a human-readable/HTTP-routing summary,
fine for a trusted in-process caller reading a cached manifest. It is not
enough for real LLM tool-calling (OpenAI/Anthropic-style), which needs a
typed JSON Schema per parameter so the model knows argument types and
required-ness without parsing a Python signature string itself.

This module builds that schema from the exact same `inspect.signature`
`describe()` already introspects — one source of truth (the method's own
type hints), nothing hand-maintained to drift. Deliberately does NOT
touch or replace `describe()` itself: a test already enforces zero drift
between `describe()`'s manifest and `cie.routes.TOOLS`'s keys, and this
module has a different job (typed params for tool-calling, not the
existing manifest/HTTP-routing contract).
"""

from __future__ import annotations

import inspect
from typing import Any, get_type_hints

from pydantic import create_model


def tool_schema(method: Any, *, name: str | None = None) -> dict:
    """Anthropic-style tool definition for one `ToolService` method:
    ``{"name", "description", "input_schema"}``.

    `method` may be a bound instance method or the raw unbound function
    from ``vars(ToolService)`` — either way `self` is excluded from the
    generated schema.
    """
    resolved_name = name or method.__name__
    sig = inspect.signature(method)
    try:
        hints = get_type_hints(method)
    except Exception:
        # A type hint that can't be resolved in this module's namespace
        # (rare, but not worth failing schema generation over) falls back
        # to the parameter's own annotation below, or Any.
        hints = {}

    fields: dict[str, tuple] = {}
    for pname, param in sig.parameters.items():
        if pname == "self":
            continue
        annotation = hints.get(pname)
        if annotation is None:
            annotation = param.annotation if param.annotation is not inspect.Parameter.empty else Any
        default = ... if param.default is inspect.Parameter.empty else param.default
        fields[pname] = (annotation, default)

    params_model = create_model(f"_{resolved_name}_Params", **fields)  # type: ignore[call-overload]
    input_schema = params_model.model_json_schema()
    input_schema.pop("title", None)
    for prop in input_schema.get("properties", {}).values():
        prop.pop("title", None)

    doc = (method.__doc__ or "").strip()
    description = doc.splitlines()[0] if doc else resolved_name

    return {
        "name": resolved_name,
        "description": description,
        "input_schema": input_schema,
    }


def tool_schemas(service_cls: type) -> list[dict]:
    """One Anthropic-style tool definition per public method on
    `service_cls` (normally `cie.tools.ToolService`) — same discovery rule
    `describe()` uses (public, callable, not `describe` itself), so this
    list always matches what `describe()` reports exists, just with typed
    params instead of a signature string."""
    out = []
    for attr_name in sorted(vars(service_cls)):
        if attr_name.startswith("_") or attr_name == "describe":
            continue
        attr = vars(service_cls)[attr_name]
        if not callable(attr):
            continue
        out.append(tool_schema(attr, name=attr_name))
    return out
