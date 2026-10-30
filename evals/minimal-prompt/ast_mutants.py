"""Generic mutants of an agent's own solution.py, for mutation-testing its saved tests.

Operators: flip a comparison (< <=, > >=, == !=), negate an `if`/`while` test, force an
`if` test to True or False, delete an early `return` inside an `if`, and use the other
end of a list (`pop(0)` <-> `pop()`, `[0]` <-> `[-1]`). Calls to `emit` and logging
are never mutated, so observation tests are scored on behavior, not instrumentation.
"""
import ast
import copy
import random

FLIP = {ast.Lt: ast.LtE, ast.LtE: ast.Lt, ast.Gt: ast.GtE, ast.GtE: ast.Gt,
        ast.Eq: ast.NotEq, ast.NotEq: ast.Eq, ast.In: ast.NotIn, ast.NotIn: ast.In,
        ast.Is: ast.IsNot, ast.IsNot: ast.Is}


def _sites(tree):
    """Yield (kind, node index, detail) for every mutable site, in source order."""
    nodes = list(ast.walk(tree))
    instrumented = set()
    for n in nodes:
        if isinstance(n, ast.Call):
            f = n.func
            name = f.id if isinstance(f, ast.Name) else getattr(f, "attr", "")
            if name in ("emit", "debug", "info", "warning", "log", "print"):
                instrumented.update(id(x) for x in ast.walk(n))
    for i, n in enumerate(nodes):
        if id(n) in instrumented:
            continue
        if isinstance(n, ast.Compare):
            for j, op in enumerate(n.ops):
                if type(op) in FLIP:
                    yield ("flip", i, j)
        if isinstance(n, (ast.If, ast.While)):
            yield ("negate", i, None)
            if isinstance(n, ast.If):
                yield ("true", i, None)
                yield ("false", i, None)
                if any(isinstance(s, ast.Return) for s in n.body):
                    yield ("drop-return", i, None)
        if isinstance(n, ast.Call) and getattr(n.func, "attr", "") == "pop":
            yield ("pop-end", i, None)
        if isinstance(n, ast.Subscript) and isinstance(n.slice, ast.Constant) and n.slice.value in (0, -1):
            yield ("index-end", i, None)


def _apply(tree, site):
    tree = copy.deepcopy(tree)
    kind, idx, detail = site
    n = list(ast.walk(tree))[idx]
    if kind == "flip":
        n.ops[detail] = FLIP[type(n.ops[detail])]()
    elif kind == "negate":
        n.test = ast.UnaryOp(op=ast.Not(), operand=n.test)
    elif kind in ("true", "false"):
        # evaluate the test for its side effects (e.g. compare_swap), discard the result
        evaluated = ast.Call(func=ast.Name("bool", ast.Load()), args=[n.test], keywords=[])
        n.test = ast.BoolOp(op=ast.Or() if kind == "true" else ast.And(),
                            values=[evaluated, ast.Constant(kind == "true")])
    elif kind == "drop-return":
        n.body = [s for s in n.body if not isinstance(s, ast.Return)] or [ast.Pass()]
    elif kind == "pop-end":
        n.args = [] if n.args else [ast.Constant(0)]
    elif kind == "index-end":
        n.slice = ast.Constant(-1 if n.slice.value == 0 else 0)
    return ast.unparse(ast.fix_missing_locations(tree))


def mutants(source, limit=40, seed=0):
    """Return [(label, mutated source)], at most `limit`, sampled deterministically."""
    tree = ast.parse(source)
    sites = list(_sites(tree))
    random.Random(seed).shuffle(sites)
    out, seen = [], {ast.unparse(tree)}
    for site in sites:
        text = _apply(tree, site)
        if text in seen:
            continue
        seen.add(text)
        line = getattr(list(ast.walk(tree))[site[1]], "lineno", 0)
        out.append((f"{site[0]}@{line}", text))
        if len(out) >= limit:
            break
    return out
