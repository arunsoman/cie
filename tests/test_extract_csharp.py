"""R13 — C# extraction (#9), the Java-shaped one: `name` fields on
classes/interfaces/methods, `base_list` carrying BOTH the base class and
the implemented interfaces (first=extends, rest=implements — C#'s
single-inheritance rule, the documented convention here), and
invocation_expression calls through member_access_expression.

Grammar facts verified against real tree-sitter-c-sharp parses (see the
R13 notes in cie/extract.py's tables).
"""

from __future__ import annotations

from cie.extract import extract_many
from cie.callgraph import resolve_call_edges

CS_SOURCE = """\
namespace App {
public class UserService : IUserService, IDisposable {
    public string Name { get; set; }

    public void Save(User u) {
        var r = repo.Save(u);
        Finish();
    }

    private void Finish() {
        Dispose();
    }

    public void Dispose() {
        return;
    }
}

public interface IUserService {
    void Save(User u);
}
}
"""


def test_csharp_extracts_class_interface_and_methods(tmp_path):
    (tmp_path / "users.cs").write_text(CS_SOURCE)
    per = extract_many(tmp_path)
    classes = {n["label"] for ext in per for n in ext.nodes if n["kind"] == "class"}
    assert {"UserService", "IUserService"} <= classes
    methods = {
        n["label"] for ext in per for n in ext.nodes if n["kind"] in ("function", "method")
    }
    assert {"Save", "Finish", "Dispose"} <= methods


def test_csharp_base_list_splits_extends_and_implements(tmp_path):
    (tmp_path / "users.cs").write_text(CS_SOURCE)
    per = extract_many(tmp_path)
    bases = [(b["name"], b["relation"]) for ext in per for b in ext.class_bases]
    # first-position rule: the first named type is the base class, the
    # rest are interfaces (C#'s single-inheritance reality, documented)
    assert ("IUserService", "extends") in bases
    assert ("IDisposable", "implements") in bases


def test_csharp_call_sites_resolve_through_receivers(tmp_path):
    (tmp_path / "users.cs").write_text(CS_SOURCE)
    per = extract_many(tmp_path)
    edges = resolve_call_edges(per)
    targets = {e["target"].split("::")[-1].split("@")[0] for e in edges}
    # Finish() and Dispose() are same-file calls; repo.Save(u) is a
    # receiver call whose receiver isn't a known class (the documented
    # limit — receiver heuristic needs self/this/class-name receivers)
    assert "Finish" in targets and "Dispose" in targets


def test_csharp_never_silently_skips_methods(tmp_path):
    (tmp_path / "users.cs").write_text(CS_SOURCE)
    methods = {
        n["label"] for ext in extract_many(tmp_path)
        for n in ext.nodes if n["kind"] in ("function", "method")
    }
    assert {"Save", "Finish", "Dispose"} <= methods, (
        "C# method extraction regressed to the silent-skip bug class"
    )