#!/usr/bin/env python3
import os
import re
import sys
import argparse

# コメント判定正規表現パターン
DEFAULT_PATTERNS = [
    # パターン1: 過剰な飾り線・セクション区切り
    r'^\s*(//|#)\s*[─━═\-_~*=\s🚀✨📝]{2,}.*[─━═\-_~*=\s]{2,}\s*$',
    r'^\s*(//|#)\s*[─━═\-_~*=\s🚀✨📝]{3,}.*$',
    
    # パターン2: ナンバリングや箇条書き
    r'^\s*(//|#)\s*\d+[\.\)、]\s*.*$',
    
    # パターン3: AI特有の過剰な防衛・定型句
    r'^\s*(//|#)\s*.*(行えばよい|許容する|受け取る|保証|防止|担保|完了しているため|～系：|🚀|ダミーの不可視).*$',
    
    # パターン4: ソースコードの挙動の逐一実況
    r'^\s*(//|#)\s*.*(目的とする|のための措置|の意図で|整合性を保つ|担当|制御|管理|伝播|解放|マージ|クランプ|適用|判定|移行|等幅フォント).*$',
    
    # パターン5: ハイフンによる区切り線
    r'^\s*(//|#)\s*---\s*.*[^-\s]+.*\s*---$',
    
    # パターン6: 説明調の長文（語尾の判定）
    r'^\s*(//|#)\s*.*(ため、|必要がある|要がある|留まる必要がある|由来|構成となっている|を待つ）|を防止する。)。?$',
]

# ホワイトリスト（誤検知保護キーワード）
PROTECTION_KEYWORDS = [
    "FIX-", "TODO", "FIXME", "BUG", "Issue", "O(1)", "av_frame_ref", 
    "SIGSEGV", "アンダーフロー", "NaN/Inf", "KDE", "nullアクセスの防止", 
    "nullアクセスを防ぐ", "nullポインタ参照を起こす"
]

def clean_file(file_path, patterns, dry_run=True, exclude_keywords=None):
    try:
        with open(file_path, 'r', encoding='utf-8', errors='ignore') as f:
            lines = f.readlines()
    except Exception as e:
        print(f"Skipped (Read Error) {file_path}: {e}")
        return 0

    new_lines = []
    removed_comments = []
    
    all_excludes = list(PROTECTION_KEYWORDS)
    if exclude_keywords:
        all_excludes.extend(exclude_keywords)

    for line_num, line in enumerate(lines, 1):
        is_contaminated = False
        
        if re.match(r'^\s*(//|#)', line):
            if any(kw in line for kw in all_excludes):
                new_lines.append(line)
                continue

            for pattern in patterns:
                if re.match(pattern, line):
                    is_contaminated = True
                    break
        
        if is_contaminated:
            removed_comments.append((line_num, line.strip()))
            continue
        else:
            new_lines.append(line)

    if removed_comments:
        print(f"[{'MATCH' if dry_run else 'REMOVE'}] {file_path} ({len(removed_comments)} items)")
        for num, text in removed_comments:
            print(f"  L{num}: {text}")
            
        if not dry_run:
            try:
                with open(file_path, 'w', encoding='utf-8') as f:
                    f.writelines(new_lines)
            except Exception as e:
                print(f"Error (Write Error) {file_path}: {e}")
                
    return len(removed_comments)

def main():
    parser = argparse.ArgumentParser(description="LLM-generated comment cleaner")
    parser.add_argument("dir", nargs="?", default="./", help="Target directory (default: current)")
    parser.add_argument("-x", "--execute", action="store_true", help="Execute deletion (default: dry-run)")
    parser.add_argument("-e", "--ext", nargs="+", default=[".cpp", ".hpp", ".qml", ".frag", ".vert", ".glsl", ".json", ".js"], help="Target extensions")
    parser.add_argument("--all-files", action="store_true", help="Scan all files regardless of extension")
    parser.add_argument("--add-pattern", nargs="+", default=[], help="Add custom regex patterns")
    parser.add_argument("--add-keyword", nargs="+", default=[], help="Add custom keywords")
    parser.add_argument("--exclude-keyword", nargs="+", default=[], help="Add protection keywords")

    args = parser.parse_args()

    if not os.path.exists(args.dir):
        print(f"Error: Path '{args.dir}' not found.", file=sys.stderr)
        sys.exit(1)

    active_patterns = list(DEFAULT_PATTERNS)
    for p in args.add_pattern:
        active_patterns.append(p)
    for kw in args.add_keyword:
        active_patterns.append(r'^\s*(//|#)\s*.*' + re.escape(kw) + r'.*$')

    dry_run = not args.execute
    
    print("--------------------------------------------------------------------------------")
    print(f"Starting scanner: {'DRY-RUN MODE' if dry_run else 'EXECUTE MODE'}")
    print(f"Target directory: {os.path.abspath(args.dir)}")
    print("--------------------------------------------------------------------------------")

    total_detected = 0
    file_count = 0
    target_extensions = tuple(args.ext)

    for root, dirs, files in os.walk(args.dir):
        if any(ignored in root for ignored in ['.git', 'build', '.build_tmp', 'vcpkg']):
            continue

        for file in files:
            file_path = os.path.join(root, file)
            should_scan = args.all_files or file.endswith(target_extensions)
            
            if should_scan:
                detected = clean_file(file_path, active_patterns, dry_run=dry_run, exclude_keywords=args.exclude_keyword)
                if detected > 0:
                    total_detected += detected
                    file_count += 1

    print("--------------------------------------------------------------------------------")
    if dry_run:
        print(f"Scan finished. Found {total_detected} comments in {file_count} files.")
        print("Run with '-x' or '--execute' to permanently delete these comments.")
    else:
        print(f"Cleanup finished. Removed {total_detected} comments across {file_count} files.")
    print("--------------------------------------------------------------------------------")

if __name__ == "__main__":
    main()