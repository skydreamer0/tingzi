"""Write the notes for a draft GitHub Release from .github/release-notes.md and CHANGELOG.md.

    python windows/release_notes.py 0.8.0 0.7.7 Tingzi-Setup-0.8.0-x64.exe.sha256 notes.md

Fills in the version, the previous version, the installer's SHA-256 and the version's section of
CHANGELOG.md. {{SUMMARY}}, {{UPGRADE_NOTES}} and {{KNOWN_ISSUES}} are left for the maintainer to
write in the draft before publishing.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEFT_FOR_THE_MAINTAINER = ("{{SUMMARY}}", "{{UPGRADE_NOTES}}", "{{KNOWN_ISSUES}}")


def changes(changelog: str, version: str) -> str:
    """The section of CHANGELOG.md for `version`, without its heading, with headings one level lower kept."""
    heading = re.compile(r"^## \[" + re.escape(version) + r"\][^\n]*\n", re.M)
    found = heading.search(changelog)
    if not found:
        raise SystemExit(f"CHANGELOG.md has no section for {version}. Rename 尚未發行 to [{version}] first.")
    rest = changelog[found.end():]
    end = re.search(r"^## \[|^\[[^\]]+\]: ", rest, re.M)
    section = rest[: end.start() if end else len(rest)].strip()
    if not section:
        raise SystemExit(f"The section for {version} in CHANGELOG.md is empty.")
    return section


def checksum(path: Path) -> str:
    digest = path.read_text(encoding="utf-8").split()[0].lower()
    if not re.fullmatch(r"[0-9a-f]{64}", digest):
        raise SystemExit(f"{path} does not start with a SHA-256 value.")
    return digest


def notes(template: str, changelog: str, version: str, previous: str, digest: str) -> str:
    filled = (
        template.replace("{{VERSION}}", version)
        .replace("{{PREVIOUS}}", previous)
        .replace("{{SHA256}}", digest)
        .replace("{{CHANGES}}", changes(changelog, version))
    )
    unknown = set(re.findall(r"\{\{[A-Z_]+\}\}", filled)) - set(LEFT_FOR_THE_MAINTAINER)
    if unknown:
        raise SystemExit(f"The template has placeholders this script does not fill: {', '.join(sorted(unknown))}")
    return filled


def main() -> None:
    if len(sys.argv) != 5:
        raise SystemExit(__doc__)
    version, previous, sha_file, output = sys.argv[1:]
    template = (ROOT / ".github" / "release-notes.md").read_text(encoding="utf-8")
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    text = notes(template, changelog, version.lstrip("v"), previous.lstrip("v"), checksum(Path(sha_file)))
    Path(output).write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    main()
