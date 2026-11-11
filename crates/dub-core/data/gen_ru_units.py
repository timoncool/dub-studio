import sys
from ru_normalizr.numerals import _constants as C
from ru_normalizr.numerals._helpers import inflect_unit_lemma
CASES = ["nomn", "gent", "datv", "accs", "ablt", "loct"]
out = open(sys.argv[1], "w", encoding="utf-8", newline="\n")
out.write("# key\tcase_sensitive\tlemma\tgender\tcategory\tsuffix\tsing nomn..loct\tplur nomn..loct\tpaucal gent sing\n")
def row(key, d, cs):
    forms = [inflect_unit_lemma(d, {c, "sing"}) for c in CASES] + [inflect_unit_lemma(d, {c, "plur"}) for c in CASES]
    paucal = inflect_unit_lemma(d, {"gent", "sing"}, paucal=True)
    if d.morphology.animacy != "anim":
        forms[9] = forms[6]
        if d.gender in ("masc", "neut"):
            forms[3] = forms[0]
    out.write("\t".join([key, "1" if cs else "0", d.lemma, d.gender, d.category, d.suffix or ""] + forms + [paucal]) + "\n")
for k, d in C.UNITS_DATA.items():
    row(k, d, False)
for k, d in C.CASE_SENSITIVE_UNITS_DATA.items():
    row(k, d, True)
for k, d in C.CURRENCY_MINOR_UNITS.items():
    row("minor:" + k, d, False)
