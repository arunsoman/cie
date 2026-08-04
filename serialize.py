"""JSON-ready dict converters shared by cie.routes (HTTP) and
cie.cli (--json).

Split out of the former cie/server.py so the CLI doesn't have to
import FastAPI (and the whole routes module) just to serialize a Node.
"""

from __future__ import annotations

from cie.tasks import AtomicTask


def node_to_dict(n) -> dict:
    """JSON-ready dict for a graph Node (all Wave A fields included)."""
    return {
        "id": n.id,
        "label": n.label,
        "source_file": n.source_file,
        "source_location": n.source_location,
        "file_type": n.file_type,
        "community": n.community,
        "kind": n.kind,
        "signature": n.signature,
        "line_start": getattr(n, "line_start", 0),
        "line_end": getattr(n, "line_end", 0),
        "docstring": getattr(n, "docstring", ""),
        "project": getattr(n, "project", ""),
    }


def edge_record_to_dict(rec) -> dict:
    """JSON-ready dict for an EdgeRecord (edge + endpoint labels)."""
    return {
        "source": rec.edge.source,
        "target": rec.edge.target,
        "relation": rec.edge.relation,
        "confidence": rec.edge.confidence.value,
        "source_label": rec.source_label,
        "target_label": rec.target_label,
    }


def task_to_dict(t: AtomicTask) -> dict:
    return {
        "id": t.id,
        "name": t.name,
        "userstory_id": t.userstory_id,
        "parent_feature": t.parent_feature,
        "task_type": t.task_type,
        "layer": t.layer,
        "action": t.action,
        "file_path": t.file_path,
        "description": t.description,
        "exact_imports": list(t.exact_imports),
        "function_signatures": list(t.function_signatures),
        "step_by_step_implementation": list(t.step_by_step_implementation),
        "dependencies": list(t.dependencies),
        "api_spec": t.api_spec.model_dump() if t.api_spec else None,
        "test_triad": t.test_triad.model_dump() if t.test_triad else None,
    }


def signature_to_dict(sig) -> dict:
    return {
        "name": sig.name,
        "parameters": list(sig.parameters),
        "return_type": sig.return_type,
        "enclosing_class": sig.enclosing_class,
        "source_file": sig.node.source_file,
        "source_location": sig.node.source_location,
        "signature": sig.node.signature,
    }
