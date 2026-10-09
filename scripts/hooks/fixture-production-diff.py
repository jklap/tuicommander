#!/usr/bin/env python3
"""Diff indexed Rust production code; test-only scopes cannot trigger capture gates."""
import difflib
import re
import subprocess
import sys


def read_git(ref):
    result = subprocess.run(["git", "show", ref], capture_output=True, check=False)
    return result.stdout if result.returncode == 0 else b""


def lexical_code(source):
    """Hide comments and literals, retaining offsets for balanced Rust item bodies."""
    chars = list(source)
    pattern = re.compile(r'//[^\n]*|/\*|(?:br|r)\#*"|b?"|(?:b)?\'(?:\\.|[^\'\\\n])\'')
    pos = 0
    while match := pattern.search(source, pos):
        start, end = match.span()
        token = match.group()
        if token.startswith("//"):
            pass
        elif token == "/*":
            depth = 1
            while depth and end < len(source):
                if source.startswith("/*", end):
                    depth += 1
                    end += 2
                elif source.startswith("*/", end):
                    depth -= 1
                    end += 2
                else:
                    end += 1
        elif token.endswith('"'):
            if token.startswith(("r", "br")):
                suffix = '"' + "#" * token.count("#")
                close = source.find(suffix, end)
                end = len(source) if close < 0 else close + len(suffix)
            else:
                while end < len(source):
                    if source[end] == "\\":
                        end += 2
                    elif source[end] == '"':
                        end += 1
                        break
                    else:
                        end += 1
        for i in range(start, min(end, len(chars))):
            if chars[i] != "\n":
                chars[i] = " "
        pos = end
    return "".join(chars)


TEST_ATTR = re.compile(r'#\s*\[\s*(?:cfg\s*\(\s*test\s*\)|(?:\w+::)?test(?:\s*\([^]]*\))?)\s*\]')
# Optional attributes, then a `mod`/`fn` declaration; group "kw" starts at the keyword.
ITEM = re.compile(r'(?P<attrs>(?:#\s*\[[^\]]*\]\s*)*)(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?(?:unsafe\s+)?'
                  r'(?P<kw>(?:mod|fn)\s+\w+)')


BODY_START = re.compile(r"[{;]")


def item_end(code, match):
    """Offset just past the item's `;` or balanced `{...}` body, or None if unclosed."""
    body = BODY_START.search(code, match.end())
    if body is None:
        return None
    end = body.end()
    if body.group() == "{":
        depth = 1
        while depth and end < len(code):
            depth += (code[end] == "{") - (code[end] == "}")
            end += 1
        if depth:
            return None
    return end


def own_lines(code, start, end):
    """True when the item shares neither its first nor its last line with other code."""
    line_end = code.find("\n", end)
    return (not code[code.rfind("\n", 0, start) + 1:start].strip()
            and not code[end:line_end if line_end >= 0 else len(code)].strip())


def items(source):
    """Classify every `mod`/`fn` item: (start, end, is_test, text from the keyword on).

    `text` is the original source (literals and comments included) from the `mod`/`fn`
    keyword to the end of the body, so it is identical whether or not test attributes
    precede it.
    """
    code = lexical_code(source)
    found = []
    for match in ITEM.finditer(code):
        is_test = bool(TEST_ATTR.search(match.group("attrs")))
        end = item_end(code, match)
        if end is None:
            if is_test:
                raise ValueError("unclosed test item; refusing to classify the staged diff")
            continue
        if is_test and code[code.rfind("\n", 0, match.start()) + 1:match.start()].strip():
            raise ValueError("mixed production/test declaration line")
        if is_test and not own_lines(code, match.start(), end):
            raise ValueError("mixed production/test closing line")
        found.append((match.start(), end, is_test, source[match.start("kw"):end]))
    return code, found


def production(source, exempt=None):
    """Source lines outside test items.

    `exempt` maps an item's text (see `items`) to how many attribute-less items with
    exactly that text are also test-only. The caller computes it from the other view of
    the same file: an item whose text is unchanged but which became a test item (e.g. a
    test function that was missing its `#[test]`) is test code in both views.
    """
    code, found = items(source)
    exempt = dict(exempt or {})
    excluded = set()
    for start, end, is_test, text in found:
        if not is_test:
            if exempt.get(text, 0) <= 0 or not own_lines(code, start, end):
                continue
            exempt[text] -= 1
        excluded.update(range(source.count("\n", 0, start), source.count("\n", 0, end) + 1))
    return [line for n, line in enumerate(source.splitlines(True)) if n not in excluded]


def became_tests(old, new):
    """Texts of items that are production-shaped in `old` and verbatim test items in `new`.

    Counted, so an item is exempted only as many times as it newly appears as a test AND
    disappears as a production item: keeping the production copy while adding a test copy
    exempts nothing, and any edit to the body (beyond its attributes) exempts nothing.
    """
    def counts(source):
        test, prod = {}, {}
        for _start, _end, is_test, text in items(source)[1]:
            bucket = test if is_test else prod
            bucket[text] = bucket.get(text, 0) + 1
        return test, prod

    old_test, old_prod = counts(old)
    new_test, new_prod = counts(new)
    exempt = {}
    for text, n in new_test.items():
        allowance = min(n - old_test.get(text, 0), old_prod.get(text, 0) - new_prod.get(text, 0))
        if allowance > 0:
            exempt[text] = allowance
    return exempt


def recorded_capture_staged():
    paths = subprocess.check_output(["git", "diff", "--cached", "--name-only", "--diff-filter=ACMR",
                                     "--", "src-tauri/src/fixtures/agent_prompts"]).decode().splitlines()
    for path in paths:
        if not path.endswith(".tcap"):
            continue
        data = read_git(":" + path)
        if not data.startswith(b"TUICCAP2\n") or len(data) < 13:
            continue
        rows = int.from_bytes(data[9:11], "little")
        cols = int.from_bytes(data[11:13], "little")
        pos, records, valid = 13, 0, rows > 0 and cols > 0
        while valid and pos < len(data):
            if len(data) - pos < 13 or data[pos] > 1:
                valid = False
                break
            size = int.from_bytes(data[pos + 9:pos + 13], "little")
            pos += 13 + size
            records += 1
            valid = pos <= len(data)
        provenance = read_git(":" + path[:-5] + ".md").decode(errors="replace").lower()
        if valid and records and "captur" in provenance and "sha-256" in provenance:
            return True
    return False


if __name__ == "__main__":
    if sys.argv[1] == "--capture":
        sys.exit(0 if recorded_capture_staged() else 1)
    path = sys.argv[1]
    old = read_git("HEAD:" + path)
    if not old and path.startswith("src-tauri/crates/tuic-terminal/src/"):
        old = read_git("HEAD:src-tauri/src/" + path.rsplit("/", 1)[1])
    new = read_git(":" + path)
    if old == new:
        sys.exit(0)
    before = production(old.decode(), became_tests(old.decode(), new.decode()))
    after = production(new.decode())
    # Give each production hunk its enclosing function, matching git's Rust hunk context.
    for group in difflib.SequenceMatcher(None, before, after).get_grouped_opcodes(0):
        first = group[0]
        context = ""
        for line in after[:first[3]] or before[:first[1]]:
            if re.match(r"\s*(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+", line):
                context = line.strip()
        print("@@ production @@ " + context)
        for tag, i, j, a, b in group:
            if tag in ("replace", "delete"):
                for line in before[i:j]:
                    print("-" + line.rstrip("\n"))
            if tag in ("replace", "insert"):
                for line in after[a:b]:
                    print("+" + line.rstrip("\n"))
