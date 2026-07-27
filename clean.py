#!/usr/bin/env python3
import argparse
import re
from pathlib import Path

LINE_COMMENT = {".rs": "//", ".slint": "//", ".wgsl": "//", ".py": "#"}

NOISE_PATTERNS = [
    r"^-{3,}$",
    r"^={3,}$",
    r"^(here|this)\s",
    r"^(this function|this method|this struct|this class)\s",
    r"^(create|creates|initialize|initializes|construct|constructs|declare|declares)\s+(a|the|an)\s",
    r"^(作成|生成|初期化|定義|宣言)する",
    r"^(increment|decrement|set|get|return|call|loop over|iterate over)\s",
    r"^(ここで|まず|次に|そして|最後に)",
    r"^step\s*\d+",
    r"^(todo|fixme|xxx)\b.*(later|後で|いつか)",
    r"^(note|補足)\s*:\s*(this is|これは)",
    r"^(src|crates)[\\/].*\.(rs|slint|wgsl|py)$",
]

TRIVIAL_ECHO_RATIO = 0.85
TRIVIAL_ECHO_MAXLEN = 40


def strip_marker(text, lc):
    t = text.strip()
    return t[len(lc):].strip() if t.startswith(lc) else t


def token_overlap(comment, code_line):
    c = set(re.findall(r"[A-Za-z0-9_]+", comment.lower()))
    k = set(re.findall(r"[A-Za-z0-9_]+", code_line.lower()))
    if not c or not k:
        return 0.0
    return len(c & k) / len(c)


def classify(comment_text, next_code_line, lc):
    body = strip_marker(comment_text, lc)
    if not body:
        return "empty"
    low = body.lower()
    for pat in NOISE_PATTERNS:
        if re.search(pat, low) or re.search(pat, body):
            return "noise"
    if next_code_line is not None and len(body) < TRIVIAL_ECHO_MAXLEN:
        if token_overlap(low, next_code_line) >= TRIVIAL_ECHO_RATIO:
            return "echo"
    return "keep"


def comment_blocks(lines, lc):
    blocks = []
    i = 0
    n = len(lines)
    while i < n:
        if lines[i].strip().startswith(lc):
            start = i
            while i < n and lines[i].strip().startswith(lc):
                i += 1
            blocks.append((start, i))
        else:
            i += 1
    return blocks


def scan_lines(lines, lc):
    out = []
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped.startswith(lc):
            next_code = None
            for j in range(i + 1, len(lines)):
                if lines[j].strip() and not lines[j].strip().startswith(lc):
                    next_code = lines[j]
                    break
            out.append((i, classify(line, next_code, lc)))
    return out


def vocab_hit_blocks(lines, lc):
    idx = set()
    for start, end in comment_blocks(lines, lc):
        block_text = "\n".join(strip_marker(l, lc) for l in lines[start:end])
        for pat in NOISE_PATTERNS:
            if re.search(pat, block_text, re.IGNORECASE):
                idx.update(range(start, end))
                break
    return idx


def process(path: Path, fix: bool, levels: set):
    lc = LINE_COMMENT.get(path.suffix)
    if not lc:
        return
    lines = path.read_text(encoding="utf-8").splitlines()
    verdicts = scan_lines(lines, lc)
    hit = False
    remove_idx = set()
    for idx, verdict in verdicts:
        if verdict in levels:
            hit = True
            print(f"{path}:{idx + 1}: [{verdict}] {lines[idx].strip()}")
            remove_idx.add(idx)
    vocab_idx = vocab_hit_blocks(lines, lc)
    for idx in sorted(vocab_idx - remove_idx):
        hit = True
        print(f"{path}:{idx + 1}: [vocab-block] {lines[idx].strip()}")
    remove_idx |= vocab_idx
    if fix and remove_idx:
        new_lines = [l for i, l in enumerate(lines) if i not in remove_idx]
        path.write_text("\n".join(new_lines) + "\n", encoding="utf-8")
    return hit


def main():
    ap = argparse.ArgumentParser(description="LLM冗長コメント検出・除去")
    ap.add_argument("root", type=Path, nargs="?", default=Path("."))
    ap.add_argument("--exclude", action="append", default=["aviutl2_sdk", "target", ".git", "slang"])
    ap.add_argument("--fix", action="store_true")
    ap.add_argument("--levels", default="noise,echo,empty")
    args = ap.parse_args()
    levels = set(args.levels.split(","))
    excl = set(args.exclude)

    n = 0
    for ext in LINE_COMMENT:
        for p in sorted(args.root.rglob(f"*{ext}")):
            if any(part in excl for part in p.parts):
                continue
            if process(p, args.fix, levels):
                n += 1
    print(f"検出/処理ファイル数: {n}")


if __name__ == "__main__":
    main()
