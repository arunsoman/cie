"""the webhook API — flask routes wired to the feature service."""
from flask import Flask, jsonify, request
from app import app, events


@app.post("/webhook/<event>")
def deliver(event):
    if request.get_json(silent=True) is None:
        return jsonify(error="body must be json"), 400
    target = request.json.get("target")
    try:
        events.deliver(event, target)
        return jsonify(status="sent", event=event)
    except ValueError:
        return jsonify(error="not subscribed"), 404


@app.get("/webhook/<event>/status")
def delivery_status(event):
    target = request.args.get("target", "any")
    return jsonify(state=events.delivery_status(event, target))
