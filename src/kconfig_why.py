# Why Kconfig settled on a value: for each named symbol, whether its prompt
# is shown, what selects or implies it, and its conditional defaults.
# kconfig_menus.json leaves these out. Loaded the way kconfgen loads a build
# (see `kconfgen.core.main`), with idf-next's arguments as JSON on stdin; the
# answer is written to the "output" path.
import json
import os
import sys

out_stream = sys.stdout
sys.stdout = sys.stderr  # keep kconfiglib's notes out of the answer

import esp_kconfiglib.core as kc  # noqa: E402
from esp_kconfiglib.deprecated import load_rename_files_from_env  # noqa: E402

args = json.load(sys.stdin)
os.environ.update(args["env"])
with open(args["env_file"]) as f:
    os.environ.update(json.load(f))
os.environ["KCONFIG_REPORT_VERBOSITY"] = "quiet"

k = kc.Kconfig(
    args["kconfig"],
    parser_version=int(os.environ.get("KCONFIG_PARSER_VERSION", "1")),
    print_report=False,
)
load_rename_files_from_env(k, sdkconfig_rename=args["rename"], list_separator="semicolon")
if args.get("defaults"):
    k.load_config(args["defaults"], replace=False)


def active(expr):
    return kc.expr_value(expr) > 0


def terms(expr):
    if expr is k.n:
        return []
    return [{"expr": kc.expr_str(t), "active": active(t)} for t in kc.split_expr(expr, kc.OR)]


choices = k.named_choices

answer = {}
for name in args["names"]:
    # "choice:NAME" is the choice; IDF often names a string symbol the same.
    is_choice = name.startswith("choice:")
    sym = choices.get(name[7:]) if is_choice else k.syms.get(name)
    if sym is None:
        continue
    prompts = [
        {"text": n.prompt[0], "if": None if n.prompt[1] is k.y else kc.expr_str(n.prompt[1]),
         "active": active(n.prompt[1])}
        for n in sym.nodes
        if n.prompt
    ]
    answer[name] = {
        "visible": sym.visibility > 0,
        "prompts": prompts,
        "depends_on": None if sym.direct_dep is k.y else kc.expr_str(sym.direct_dep),
        "selected_by": [] if is_choice else terms(sym.rev_dep),
        "implied_by": [] if is_choice else terms(sym.weak_rev_dep),
        "defaults": [
            {"value": kc.expr_str(v), "if": None if c is k.y else kc.expr_str(c), "active": active(c)}
            for v, c in sym.defaults
        ],
        "defined_at": ["%s:%d" % (n.filename, n.linenr) for n in sym.nodes],
    }

with open(args["output"], "w") as f:
    json.dump(answer, f)
