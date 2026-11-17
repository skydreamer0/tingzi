"""Sends the hand-made dictations in cases.json to the refinement service saved in Tingzi's settings, and reports
which answers kept what must stay and dropped what must go.

    python app/src-tauri/refine-cases/try_refine.py <path to Tingzi.exe or the Tingzi binary>

It runs `Tingzi --refine` once per case, so it uses the same instructions, word list, note and checks as a real
dictation, and it costs one request per case. Nothing is written anywhere.
"""

import json
import subprocess
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    program = sys.argv[1]
    cases = json.loads((Path(__file__).with_name("cases.json")).read_text(encoding="utf-8"))
    passed = 0
    for number, case in enumerate(cases, 1):
        run = subprocess.run([program, "--refine", case["said"]], capture_output=True, timeout=60)
        answer = run.stdout.decode("utf-8", "replace").strip()
        notes = run.stderr.decode("utf-8", "replace").strip().splitlines()
        problems = []
        if run.returncode != 0:
            problems.append("沒有潤稿：" + (notes[0] if notes else f"結束代碼 {run.returncode}"))
        else:
            problems += [f"少了「{word}」" for word in case["keep"] if word not in answer]
            problems += [f"還留著「{word}」" for word in case["gone"] if word in answer]
        took = next((line for line in notes if line.endswith(" ms")), "")
        mark = "通過" if not problems else "未通過"
        passed += not problems
        print(f"{number:2}. {mark} {case['what']} {took}")
        print(f"    說的：{case['said']}")
        if answer:
            print("    潤稿：" + answer.replace("\n", "\n          "))
        for problem in problems:
            print(f"    {problem}")
    print(f"\n{passed}／{len(cases)} 通過")
    return 0 if passed == len(cases) else 1


if __name__ == "__main__":
    sys.exit(main())
