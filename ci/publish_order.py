#!/usr/bin/env python3
"""Print publishable workspace crates in dependency order."""

from __future__ import annotations

import json
import subprocess
import sys


def main() -> int:
    metadata = json.loads(
        subprocess.check_output(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            text=True,
        )
    )

    workspace_ids = set(metadata["workspace_members"])
    packages = {
        package["id"]: package
        for package in metadata["packages"]
        if package["id"] in workspace_ids and package.get("publish") != []
    }
    packages_by_name = {package["name"]: package for package in packages.values()}

    incoming = {name: set() for name in packages_by_name}
    outgoing = {name: set() for name in packages_by_name}

    for package in packages_by_name.values():
        for dependency in package.get("dependencies", []):
            dependency_name = dependency.get("name")
            if dependency_name in packages_by_name:
                incoming[package["name"]].add(dependency_name)
                outgoing[dependency_name].add(package["name"])

    ready = sorted(name for name, deps in incoming.items() if not deps)
    order = []

    while ready:
        name = ready.pop(0)
        order.append(name)
        for dependent in sorted(outgoing[name]):
            incoming[dependent].discard(name)
            if not incoming[dependent] and dependent not in order and dependent not in ready:
                ready.append(dependent)
                ready.sort()

    if len(order) != len(packages_by_name):
        missing = sorted(set(packages_by_name) - set(order))
        print(
            f"Failed to compute publish order for: {', '.join(missing)}",
            file=sys.stderr,
        )
        return 1

    for name in order:
        print(name)

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
