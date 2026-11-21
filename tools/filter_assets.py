"""Assemble uAssets includes for the desktop WebKit scriptlet environment."""
import hashlib
import posixpath
import re

CAPABILITIES = {
    "env_safari": True,
    "cap_html_filtering": False,
    "cap_ipaddress": False,
    "env_firefox": False,
    "env_chromium": False,
    "env_mobile": False,
    "env_mv3": False,
    "ext_ubol": False,
}


def assemble(root, fetch):
    inputs = {}

    def expand(path, ancestors):
        if path in ancestors or len(ancestors) >= 16:
            raise ValueError("Recursive filter include: " + path)
        if not re.fullmatch(r"filters/[a-zA-Z0-9_-]+\.txt", path):
            raise ValueError("Unexpected filter path: " + path)
        if len(inputs) >= 96:
            raise ValueError("Too many filter includes")
        data = fetch(path)
        if len(data) > 4 * 1024 * 1024:
            raise ValueError("Filter input exceeds the byte limit")
        inputs[path] = {"path": path, "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}
        output, stack = [], []
        active = True
        for line in data.decode().splitlines():
            if line.startswith("!#if "):
                expression = line[5:].strip()
                capability = expression.removeprefix("!")
                if capability not in CAPABILITIES:
                    raise ValueError("Unknown filter capability: " + expression)
                value = CAPABILITIES[capability] != expression.startswith("!")
                stack.append((active, value, False))
                active = active and value
            elif line == "!#else":
                if not stack or stack[-1][2]:
                    raise ValueError("Unexpected filter else: " + path)
                parent, value, _ = stack[-1]
                stack[-1] = (parent, value, True)
                active = parent and not value
            elif line == "!#endif":
                if not stack:
                    raise ValueError("Unexpected filter endif: " + path)
                active = stack.pop()[0]
            elif line.startswith("!#include "):
                if active:
                    child = posixpath.join(posixpath.dirname(path), line[10:].strip())
                    output.extend(expand(child, ancestors + (path,)))
            elif line.startswith("!#"):
                raise ValueError("Unknown filter directive: " + line)
            elif active:
                output.append(line)
        if stack:
            raise ValueError("Unterminated filter condition: " + path)
        return output

    result = ("\n".join(expand(root, ())) + "\n").encode()
    if len(result) > 8 * 1024 * 1024:
        raise ValueError("Assembled filters exceed the byte limit")
    return result, [inputs[path] for path in sorted(inputs)]
