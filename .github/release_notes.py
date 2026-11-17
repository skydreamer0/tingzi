"""Write the notes for a draft GitHub release from the template and CHANGELOG.md.

python .github/release_notes.py <version> <previous-tag> <installer.sha256> <output.md>

The version's section in CHANGELOG.md fills {{CHANGES}}, with its ### headings raised to ##.
When CHANGELOG.md has no section for the version yet, the section under 尚未發行 is used and the
notes say so, so a maintainer sees it before publishing. {{SUMMARY}}, {{UPGRADE_NOTES}} and
{{KNOWN_ISSUES}} are left for the maintainer to fill in the draft.
"""
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent


def section(changelog, title):
    """The body of the `## [title]` section, or None."""
    match = re.search(r'^## \[' + re.escape(title) + r'\][^\n]*\n(.*?)(?=^## |^\[[^\]]+\]: |\Z)', changelog, re.M | re.S)
    return match.group(1).strip() if match else None


def main(version, previous, sha_file, output):
    changelog = (ROOT / 'CHANGELOG.md').read_text(encoding='utf-8')
    changes = section(changelog, version)
    if changes is None:
        changes = section(changelog, '尚未發行') or ''
        changes = '（CHANGELOG.md 還沒有 ' + version + ' 的段落，以下取自「尚未發行」，發佈前請確認。）\n\n' + changes
    changes = re.sub(r'^### ', '## ', changes, flags=re.M)
    sha = pathlib.Path(sha_file).read_text(encoding='utf-8').split()[0]
    notes = (ROOT / '.github' / 'release-notes.md').read_text(encoding='utf-8')
    for key, value in dict(VERSION=version, PREVIOUS=previous.lstrip('v'), SHA256=sha, CHANGES=changes).items():
        notes = notes.replace('{{' + key + '}}', value)
    pathlib.Path(output).write_text(notes, encoding='utf-8')


if __name__ == '__main__':
    if len(sys.argv) != 5:
        sys.exit(__doc__)
    main(*sys.argv[1:])
