#!/usr/bin/env python3
import os
import json
import argparse
from datetime import datetime

# Files/Directories to always exclude (even if they match include patterns)
EXCLUDE_DIRS = {
    ".git", "__pycache__", "build", "dist", "node_modules",
    ".idea", ".vscode", "cmake-build-debug", "cmake-build-release",
    "bin", "obj", "lib", ".build_tmp", "assets", "i18n"
}

EXCLUDE_FILES = {
    ".DS_Store", "Thumbs.db", "package-lock.json", "yarn.lock", "Icons.js"
}

INCLUDE_EXTENSIONS = {
    # C/C++
    ".cpp", ".hpp", ".c", ".h",
    # Qt/QML
    ".qml", ".qrc", ".ui", ".pro", ".pri", ".js",
    # Build Systems
    ".cmake", "CMakeLists.txt", "Makefile",
    # Scripts
    ".sh", ".bash", ".py", ".lua", ".fish",
    # Config/Data
    ".json", ".xml", ".yaml", ".yml", ".toml", ".ini", ".conf", ".clang-format", ".clang-tidy", ".gitignore",
    # Documentation
    ".md", ".txt", ".rst",
    # GLSL
    ".glsl", ".frag", ".vert"
    
}

INCLUDE_FILENAMES = {
    "CMakeLists.txt", "Makefile", "Dockerfile", "Vagrantfile", ".gitignore", "LICENSE", "README"
}

def is_text_file(filepath):
    """
    Heuristic to check if a file is text (vs binary).
    Reads the first 1024 bytes and checks for null bytes.
    """
    try:
        with open(filepath, 'rb') as f:
            chunk = f.read(1024)
            if b'\x00' in chunk:
                return False
            return True
    except Exception:
        return False

def generate_tree(abs_root, exclude_dirs):
    """
    Generates a visual directory tree string for AI to understand the structure quickly.
    """
    tree_str = []
    for root, dirs, files in os.walk(abs_root):
        dirs[:] = [d for d in dirs if d not in exclude_dirs]
        level = os.path.relpath(root, abs_root).count(os.sep)
        if os.path.basename(root) == ".":
            tree_str.append(f"{os.path.basename(abs_root)}/")
        else:
            indent = "  " * level
            tree_str.append(f"{indent}├── {os.path.basename(root)}/")
        
        sub_indent = "  " * (level + 1)
        for f in sorted(files):
            # 簡易的なフィルタリング（should_processと合わせるのが理想的だが、ツリーは全体像優先）
            if f in EXCLUDE_FILES or f.startswith("."):
                continue
            # 出力ファイル自体は除外
            if "project_context" in f:
                continue
            tree_str.append(f"{sub_indent}└── {f}")
            
    return "\n".join(tree_str)

def should_process(filepath, output_file):
    """
    Determines if a file should be included in the export.
    """
    filename = os.path.basename(filepath)
    # Prevent recursive inclusion of export files
    if filename.startswith("project_context") and filename.endswith(".json"):
        return False
    if os.path.abspath(filepath) == os.path.abspath(output_file):
        return False

    ext = os.path.splitext(filename)[1].lower()

    if filename in EXCLUDE_FILES:
        return False
    
    if filename in INCLUDE_FILENAMES:
        return True
    
    if ext in INCLUDE_EXTENSIONS:
        # Double check to ensure it's readable text
        return is_text_file(filepath)
    
    return False

def generate_export(output_file="project_context.json", root_dir="."):
    """
    Traverses the directory and writes structured content to a JSON file.
    """
    abs_root = os.path.abspath(root_dir)
    timestamp = datetime.now().strftime("%Y-%m-%d %H:%M:%S")
    
    print(f"🔍 Scanning structure...")
    tree_structure = generate_tree(abs_root, EXCLUDE_DIRS)

    project_data = {
        "meta": {
            "project_name": os.path.basename(abs_root),
            "date": timestamp,
            "generated_by": "export.py",
            "structure": tree_structure,
            "summary": {}
        },
        "files": []
    }

    extension_counts = {}

    print(f"📄 Reading files...")
    try:
        for root, dirs, files in os.walk(abs_root):
            dirs[:] = [d for d in dirs if d not in EXCLUDE_DIRS]  # Skip dirs

            for filename in sorted(files):
                filepath = os.path.join(root, filename)

                if os.path.basename(filepath) == "export.py":
                    continue

                if should_process(filepath, output_file):
                    rel_path = os.path.relpath(filepath, abs_root)
                    ext = os.path.splitext(filename)[1].lower()
                    try:
                        with open(filepath, 'r', encoding='utf-8', errors='replace') as f:
                            content = f.read()

                        extension_counts[ext] = extension_counts.get(ext, 0) + 1
                        
                        project_data["files"].append({
                            "path": rel_path,
                            "size": len(content),
                            "extension": ext,
                            "content": content
                        })
                    except Exception as e:
                        print(f"⚠️ Skipping {rel_path}: {e}")

        project_data["meta"]["summary"] = {"file_counts": extension_counts, "total_files": len(project_data["files"])}

        with open(output_file, 'w', encoding='utf-8') as out:
            json.dump(project_data, out, indent=2, ensure_ascii=False)

        print(f"✅ Export completed: {output_file}")

    except IOError as e:
        print(f"❌ Error writing output file: {e}")

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Export project source code to a single text file.")
    parser.add_argument("-o", "--output", help="Output filename")
    parser.add_argument("-d", "--dir", default=".", help="Root directory to scan")
    
    args = parser.parse_args()

    output_file = args.output
    if not output_file:
        timestamp = datetime.now().strftime("%Y%m%d_%H%M%S")
        output_file = f"project_context_{timestamp}.json"

    generate_export(output_file, args.dir)
