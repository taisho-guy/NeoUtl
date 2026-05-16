import re

with open("BUILD.py", "r", encoding="utf-8") as f:
    content = f.read()

# 1. Update Msys2Builder dependencies
old_deps = """        deps = [
            "mingw-w64-ucrt-x86_64-toolchain", "mingw-w64-ucrt-x86_64-cmake",
            "mingw-w64-ucrt-x86_64-ninja", "git",
            "mingw-w64-ucrt-x86_64-qt6-base", "mingw-w64-ucrt-x86_64-qt6-declarative",
            "mingw-w64-ucrt-x86_64-qt6-quick3d", "mingw-w64-ucrt-x86_64-qt6-multimedia",
            "mingw-w64-ucrt-x86_64-qt6-shadertools", "mingw-w64-ucrt-x86_64-qt6-svg",
            "mingw-w64-ucrt-x86_64-qt6-5compat", "mingw-w64-ucrt-x86_64-qt6-tools",
            "mingw-w64-ucrt-x86_64-ffmpeg", "mingw-w64-ucrt-x86_64-luajit",
            "mingw-w64-ucrt-x86_64-vulkan-loader", "mingw-w64-ucrt-x86_64-vulkan-headers",
            "mingw-w64-ucrt-x86_64-pkg-config", "mingw-w64-ucrt-x86_64-mold",
            "mingw-w64-ucrt-x86_64-lilv", "mingw-w64-ucrt-x86_64-ladspa-sdk",
            "mingw-w64-ucrt-x86_64-curl", "mingw-w64-ucrt-x86_64-extra-cmake-modules",
            "zip", "mingw-w64-ucrt-x86_64-clang-tools-extra",
        ]"""
new_deps = """        deps = [
            "mingw-w64-ucrt-x86_64-toolchain", "mingw-w64-ucrt-x86_64-cmake",
            "mingw-w64-ucrt-x86_64-ninja", "git",
            "mingw-w64-ucrt-x86_64-qt6",
            "mingw-w64-ucrt-x86_64-ffmpeg", "mingw-w64-ucrt-x86_64-luajit",
            "mingw-w64-ucrt-x86_64-vulkan-loader", "mingw-w64-ucrt-x86_64-vulkan-headers",
            "mingw-w64-ucrt-x86_64-pkg-config", "mingw-w64-ucrt-x86_64-mold",
            "mingw-w64-ucrt-x86_64-lilv", "mingw-w64-ucrt-x86_64-ladspa-sdk",
            "mingw-w64-ucrt-x86_64-curl", "mingw-w64-ucrt-x86_64-extra-cmake-modules",
            "zip", "mingw-w64-ucrt-x86_64-clang-tools-extra",
        ]"""
content = content.replace(old_deps, new_deps)

# 2. Add CARLA class attributes to Msys2Builder
class_msys2 = "class Msys2Builder(PlatformBuilder):"
new_class_msys2 = """class Msys2Builder(PlatformBuilder):
    CARLA_DLL_PATTERNS = (
        "carla*.dll",
        "libcarla*.dll",
        "CarlaVst*.dll",
    )
    CARLA_DLL_ALIASES = {
        "libcarla_native-plugin.dll": "CarlaNativePlugin.dll",
    }
"""
content = content.replace(class_msys2, new_class_msys2)

# 3. Update Msys2Builder.package
old_package = """    def package(self):
        self.config.output_dir.mkdir(parents=True, exist_ok=True)
        dest_bin = self.config.output_dir / "AviQtl.exe"
        src_bin = self.config.work_dir / "AviQtl.exe"
        if dest_bin.exists():
            dest_bin.unlink()
        if not src_bin.exists():
            raise FileNotFoundError(f"実行ファイルが見つかりません: {src_bin}")
        shutil.copy2(src_bin, dest_bin)
        self.copy_assets(self.config.output_dir)
        self.logger.log("windeployqt を実行中...")
        self.run_cmd([
            "windeployqt",
            "--qmldir", str(self.config.source_dir / "ui/qml"),
            "--no-translations", "--no-compiler-runtime",
            "--release" if not self.config.is_debug else "--debug",
            str(dest_bin),
            "--dir", str(self.config.output_dir),
        ])
        with open(self.config.output_dir / "qt.conf", "w", encoding="utf-8") as f:
            f.write("[Paths]\\nPlugins = .\\n")
        self.logger.log(f"実行ファイル: {dest_bin}")"""

new_package = """    def package(self):
        self.config.output_dir.mkdir(parents=True, exist_ok=True)
        dest_bin = self.config.output_dir / "AviQtl.exe"
        src_bin = self.config.work_dir / "bin" / "AviQtl.exe"
        if dest_bin.exists():
            dest_bin.unlink()
        if not src_bin.exists():
            raise FileNotFoundError(f"実行ファイルが見つかりません: {src_bin}")
        shutil.copy2(src_bin, dest_bin)
        self.copy_assets(self.config.output_dir)
        self.copy_carla_runtime()
        
        # Msys2環境では実行ファイルと同じディレクトリにあるDLLも必要になる場合があるため、
        # pacman経由でインストールされた外部依存DLL(ffmpeg等)をコピーするためのパス解決等も
        # MSYS2の場合はwindeployqt以外にもntlddなどが有用ですが、一旦windeployqtに任せます。
        
        self.logger.log("windeployqt を実行中...")
        self.run_cmd([
            "windeployqt",
            "--qmldir", str(self.config.source_dir / "ui/qml"),
            "--no-translations",
            "--release" if not self.config.is_debug else "--debug",
            str(dest_bin),
            "--dir", str(self.config.output_dir),
        ])
        with open(self.config.output_dir / "qt.conf", "w", encoding="utf-8") as f:
            f.write("[Paths]\\nPlugins = .\\n")
        self.logger.log(f"実行ファイル: {dest_bin}")

    def copy_carla_runtime(self):
        carla_lib = self.config.source_dir / "vendor" / "carla" / "lib"
        if not carla_lib.exists():
            return
        copied = set()
        for pattern in self.CARLA_DLL_PATTERNS:
            for dll in carla_lib.glob(pattern):
                if dll.name in copied:
                    continue
                shutil.copy2(dll, self.config.output_dir / dll.name)
                copied.add(dll.name)
                if dll.name in self.CARLA_DLL_ALIASES:
                    alias = self.CARLA_DLL_ALIASES[dll.name]
                    shutil.copy2(dll, self.config.output_dir / alias)
                    copied.add(alias)
        discovery = self.find_carla_discovery_tool()
        if discovery:
            shutil.copy2(discovery, self.config.output_dir / "carla-discovery-native.exe")
            copied.add("carla-discovery-native.exe")
        if copied:
            self.logger.log(f"Carla ランタイムを同梱: {len(copied)} files")

    def find_carla_discovery_tool(self) -> Path | None:
        carla_root = self.config.source_dir / "vendor" / "carla"
        candidates = [
            carla_root / "carla-discovery-native.exe",
            carla_root / "Carla" / "carla-discovery-native.exe",
        ]
        candidates.extend(carla_root.glob("**/Carla/carla-discovery-native.exe"))
        candidates.extend(carla_root.glob("**/carla-discovery-native.exe"))
        for candidate in candidates:
            if candidate.exists():
                return candidate
        return None"""

content = content.replace(old_package, new_package)

with open("BUILD.py", "w", encoding="utf-8") as f:
    f.write(content)
