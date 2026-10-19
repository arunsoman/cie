"""the webhook feature service — validate then relay."""
from app import store

SUBSCRIBED = {"berth.booked": 1, "invoice.settled": 2}


def deliver(event: str, target: str):
    if event not in SUBSCRIBED:
        raise ValueError("event is not subscribed " + event)
    return store.record(event, target)


def delivery_status(event: str, target: str):
    return store.status(event, target)
