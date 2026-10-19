"""the module that knows where webhook deliveries land."""

deliveries: dict[str, str] = {}


def record(event: str, target: str) -> str:
    key = f"{event}:{target}"
    deliveries[key] = "sent"
    return key


def status(event: str, target: str) -> str:
    return deliveries.get(f"{event}:{target}", "unknown")
