import sys
import os
import subprocess
import shutil
import argparse
import multiprocessing
import shlex
import urllib.request
import platform
import locale
import signal
import tempfile
import json
import stat
from pathlib import Path
from dataclasses import dataclass
from typing import Callable, List, Optional, Type
from PySide6 import QtCore


@dataclass
class BuildConfig:
    source_dir: Path
    temp_base: Path
    output_dir: Path
    target: str
    is_debug: bool
    use_container: bool
    is_offline: bool
    qt_dir: Optional[Path] = None

    @property
    def build_type(self) -> str:
        return "Debug" if self.is_debug else "Release"

    @property
    def work_dir(self) -> Path:
        return self.temp_base / self.target / self.build_type

    @property
    def dist_dir(self) -> Path:
        return self.source_dir / "dist"


class Logger:
    def __init__(self, log_cb: Callable[[str], None], progress_cb: Callable[[int, str], None]):
        self._log = log_cb
        self._progress = progress_cb

    def log(self, msg: str):
        self._log(msg)

    def section(self, title: str):
        self._log(f">>> {title}")

    def progress(self, val: int, msg: str):
        self._progress(val, msg)


class PlatformBuilder:
    def __init__(self, config: BuildConfig, logger: Logger):
        self.config = config
        self.logger = logger
        self.env = os.environ.copy()
        self.env["GIT_TERMINAL_PROMPT"] = "0"
        self.env["HOMEBREW_NO_AUTO_UPDATE"] = "1"
        self.env["DEBIAN_FRONTEND"] = "noninteractive"
        self.container_name = ""
        self.use_container = False
        self.cancelled = False
        self.current_proc: Optional[subprocess.Popen] = None

    def build(self):
        self.check_cancelled()
        self.logger.progress(10, f"{self.config.build_type} ビルド開始")
        self.logger.section("依存関係の確認")
        self.install_dependencies()
        self.check_cancelled()
        self.logger.section("CMake 設定")
        self.configure()
        self.check_cancelled()
        self.logger.section("翻訳ファイル更新")
        self.update_translations()
        self.check_cancelled()
        self.logger.section("コンパイル")
        self.compile()
        self.check_cancelled()
        self.logger.section("パッケージング")
        self.package()
        self.check_cancelled()
        self.logger.section("アーカイブ作成")
        self.archive()
        self.logger.progress(100, "完了")

    def check_cancelled(self):
        if self.cancelled:
            raise RuntimeError("ビルドをキャンセルしました")

    def cancel(self):
        self.cancelled = True
        proc = self.current_proc
        if proc and proc.poll() is None:
            self.logger.log("実行中のコマンドを停止しています...")
            try:
                if os.name != "nt":
                    os.killpg(proc.pid, signal.SIGTERM)
                else:
                    proc.terminate()
            except ProcessLookupError:
                pass
            except Exception:
                proc.terminate()

    def install_dependencies(self):
        pass

    def configure(self):
        self.run_cmd(self.get_cmake_config_cmd())

    def update_translations(self):
        self.run_cmd(["cmake", "--build", str(self.config.work_dir), "--target", "AviQtl_lupdate"])

    def compile(self):
        j = multiprocessing.cpu_count()
        self.logger.log(f"並列ジョブ数: {j}")
        self.run_cmd(["cmake", "--build", str(self.config.work_dir), "-j", str(j)])

    def package(self):
        pass

    def archive(self):
        self.config.dist_dir.mkdir(parents=True, exist_ok=True)
        archive_name = self.get_archive_name()
        self.create_zip(archive_name)
        self.logger.log(f"アーカイブ: {self.config.dist_dir / (archive_name + '.zip')}")

    def run_cmd(self, cmd: List[str], shell: bool = False, force_host: bool = False):
        self.check_cancelled()
        in_container = self.use_container and not force_host
        display_cmd = ' '.join(cmd) if isinstance(cmd, list) else cmd

        if self.use_container:
            tag = "[Container]" if in_container else "[Host]"
            self.logger.log(f"{tag} {display_cmd}")
        else:
            self.logger.log(display_cmd)

        actual_cmd = cmd
        if in_container:
            cmd_str = shlex.join(cmd)
            cwd = os.getcwd()
            inner = f"cd {shlex.quote(cwd)} && {cmd_str}"
            actual_cmd = f"distrobox enter {self.container_name} -- bash -lc {shlex.quote(inner)}"
            shell = True

        popen_kwargs = {}
        if os.name != "nt":
            popen_kwargs["start_new_session"] = True

        proc = subprocess.Popen(
            actual_cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            shell=shell,
            env=self.env,
            **popen_kwargs,
        )
        self.current_proc = proc
        try:
            for line in proc.stdout:
                self.logger.log(line.rstrip())
            proc.wait()
        finally:
            self.current_proc = None
        self.check_cancelled()
        if proc.returncode != 0:
            raise subprocess.CalledProcessError(proc.returncode, actual_cmd)

    def remove_tree(self, path: Path):
        def make_writable_and_retry(function, target, excinfo):
            try:
                os.chmod(target, stat.S_IWRITE)
                function(target)
            except Exception:
                raise excinfo[1]

        try:
            shutil.rmtree(path, onexc=make_writable_and_retry)
        except TypeError:
            def onerror(function, target, excinfo):
                make_writable_and_retry(function, target, excinfo)
            shutil.rmtree(path, onerror=onerror)

    def get_cmake_config_cmd(self) -> List[str]:
        return [
            "cmake", "-B", str(self.config.work_dir), "-G", "Ninja",
            f"-DCMAKE_BUILD_TYPE={self.config.build_type}",
        ]

    def get_archive_name(self) -> str:
        return "AviQtl-Archive"

    def create_zip(self, archive_name: str):
        # Windows の場合、既存の zip があるとエラーになる場合があるため削除
        zip_file = self.config.dist_dir / (archive_name + ".zip")
        if zip_file.exists():
            zip_file.unlink()
        shutil.make_archive(str(self.config.dist_dir / archive_name), "zip", root_dir=self.config.output_dir)

    def setup_carla_sdk(self, is_windows: bool = False):
        sdk_dir = self.config.source_dir / "vendor" / "carla"
        inc_dir = sdk_dir / "include"
        lib_dir = sdk_dir / "lib"

        # CarlaNativePlugin.h が依存する CarlaHost.h が含まれているかを判定する
        carla_host_header = inc_dir / "CarlaHost.h"
        if not carla_host_header.exists():
            if self.config.is_offline:
                self.logger.log("Carla SDK が見つかりませんが、オフラインモードのため取得をスキップします")
                return
            self.logger.log("Carla ヘッダーを取得中...")
            temp_clone = self.config.source_dir / ".carla_tmp"
            if temp_clone.exists():
                self.remove_tree(temp_clone)
            self.run_cmd(["git", "clone", "--depth", "1", "https://github.com/falkTX/Carla.git", str(temp_clone)], force_host=True)
            inc_dir.mkdir(parents=True, exist_ok=True)
            # source/includes は既に存在していた場合も merge する
            shutil.copytree(temp_clone / "source/includes", inc_dir, dirs_exist_ok=True)
            # CarlaNativePlugin.h が依存する CarlaHost.h / CarlaUtils.h / CarlaBackend.h は
            # source/backend/ に存在するため、フラットに include/ へ追加コピーする
            backend_src = temp_clone / "source" / "backend"
            if backend_src.exists():
                for _hdr in list(backend_src.glob("*.h")) + list(backend_src.glob("*.hpp")):
                    shutil.copy2(str(_hdr), str(inc_dir / _hdr.name))
            self.remove_tree(temp_clone)

        windows_dlls = [
            "libcarla_standalone2.dll",
            "libcarla_native-plugin.dll",
            "libcarla_host-plugin.dll",
            "libcarla_utils.dll",
        ]
        runtime_dir = sdk_dir / "runtime"
        has_runtime = runtime_dir.exists() and all((runtime_dir / d).exists() for d in windows_dlls)
        if is_windows and not has_runtime:
            if self.config.is_offline:
                raise RuntimeError("Carla Windowsランタイムが不完全です。オフラインモードでは自動取得できません: " + str(runtime_dir))
            self.logger.log("Carla Windows バイナリをダウンロード中...")
            version = "2.5.10"
            url = f"https://github.com/falkTX/Carla/releases/download/v{version}/Carla-{version}-win64.zip"
            tmp_extract = sdk_dir / "_carla_extract_tmp"
            if tmp_extract.exists():
                self.remove_tree(tmp_extract)
            tmp_extract.mkdir(parents=True, exist_ok=True)
            self.download_and_extract(url, tmp_extract)

            # ZIP内のトップレベルディレクトリを探す (Carla-2.5.10-win64/)
            top_dirs = [d for d in tmp_extract.iterdir() if d.is_dir()]
            if not top_dirs:
                raise RuntimeError("Carla ZIPの展開結果にディレクトリが見つかりません")
            extracted_root = top_dirs[0]

            # Carla/サブディレクトリを runtime/ に保存
            carla_subdir = extracted_root / "Carla"
            if not carla_subdir.exists():
                raise RuntimeError(f"Carla ZIP内に Carla/ サブディレクトリが見つかりません: {extracted_root}")
            if runtime_dir.exists():
                self.remove_tree(runtime_dir)
            shutil.copytree(str(carla_subdir), str(runtime_dir))

            # lib/ にDLLをコピー (CMake find_library およびリンク用)
            lib_dir.mkdir(parents=True, exist_ok=True)
            for dll_name in windows_dlls:
                src = runtime_dir / dll_name
                if src.exists():
                    shutil.copy2(str(src), str(lib_dir / dll_name))
                else:
                    self.logger.log(f"  警告: {dll_name} が runtime/ に見つかりません")

            self.remove_tree(tmp_extract)
            self.logger.log(f"Carla ランタイム セットアップ完了: {runtime_dir}")


    def download_and_extract(self, url: str, dest_dir: Path):
        tmp_file = dest_dir / "download.tmp"
        try:
            self.logger.log(f"  Download: {url}")
            urllib.request.urlretrieve(url, tmp_file)
            self.logger.log(f"  Extracting to {dest_dir}...")
            
            # .tgz (tar.gz) の場合は shutil.unpack_archive が自動判別するが、
            # 環境によって .tgz を認識しない場合があるためフォーマットを明示
            fmt = None
            if url.endswith(".tgz") or url.endswith(".tar.gz"):
                fmt = "gztar"
            elif url.endswith(".zip"):
                fmt = "zip"
            
            shutil.unpack_archive(str(tmp_file), str(dest_dir), format=fmt)
        finally:
            if tmp_file.exists():
                tmp_file.unlink()

    def copy_assets(self, asset_dest: Path):
        for d in ["effects", "objects"]:
            src = self.config.source_dir / "ui/qml" / d
            dst = asset_dest / d
            if src.exists():
                shutil.copytree(src, dst, ignore=shutil.ignore_patterns("*.frag", "*.vert", "*.comp", "*.glsl"), dirs_exist_ok=True)
        for d in ["effects", "objects"]:
            qsb_src = self.config.work_dir / d
            qsb_dst = asset_dest / d
            if qsb_src.exists() and qsb_dst.exists():
                for f in qsb_src.glob("*.qsb"):
                    shutil.copy2(f, qsb_dst / f.name)
        plugins_src = self.config.source_dir / "plugins"
        if plugins_src.exists() and any(plugins_src.iterdir()):
            shutil.copytree(plugins_src, self.config.output_dir / "plugins", dirs_exist_ok=True)
        i18n_dest = asset_dest / "i18n"
        i18n_dest.mkdir(parents=True, exist_ok=True)
        for qm in self.config.work_dir.rglob("*.qm"):
            if "CMakeFiles" not in qm.parts:
                shutil.copy2(qm, i18n_dest / qm.name)


class LinuxBuilderBase(PlatformBuilder):
    def __init__(self, config: BuildConfig, logger: Logger):
        super().__init__(config, logger)
        # コンテナが使えない環境（CI内など）でも実行できるように、チェックを緩和
        self.use_container = config.use_container
        self.image_name = ""

    def warmup_container(self):
        self.logger.log(f"コンテナ初期化を待機中 (distrobox init)...")
        self.run_cmd(["true"])
        self.logger.log("コンテナ初期化完了")

    def create_container(self):
        if not (shutil.which("distrobox") and shutil.which("podman")):
            raise RuntimeError("distrobox または podman が見つかりません")
        self.logger.log(f"コンテナ '{self.container_name}' を準備中...")
        try:
            self.run_cmd(
                ["distrobox", "create", "--name", self.container_name, "--image", self.image_name, "--yes"],
                force_host=True,
            )
        except subprocess.CalledProcessError:
            self.logger.log("コンテナは既に存在します。そのまま使用します。")
        self.warmup_container()

    def install_dependencies(self):
        if not self.use_container:
            return
        if self.config.is_offline:
            self.logger.log("依存関係インストールをスキップします (--offline)")
            self.warmup_container()
            return
        self.create_container()

    def get_cmake_config_cmd(self) -> List[str]:
        cmd = super().get_cmake_config_cmd()
        cmd.extend(["-DCMAKE_C_COMPILER=clang", "-DCMAKE_CXX_COMPILER=clang++"])
        if not self.config.is_debug:
            cmd.extend([
                "-DCMAKE_CXX_FLAGS=-O3 -flto -fno-semantic-interposition -funsafe-math-optimizations",
                "-DCMAKE_POLICY_DEFAULT_CMP0056=NEW",
                "-DCMAKE_SKIP_INSTALL_RPATH=ON",
            ])
        cmd.append(str(self.config.source_dir))
        return cmd

    def package(self):
        self.config.output_dir.mkdir(parents=True, exist_ok=True)
        dest_bin = self.config.output_dir / "AviQtl"
        # CMAKE_RUNTIME_OUTPUT_DIRECTORY = bin/ のため bin/ 下に生成される
        src_bin = self.config.work_dir / "bin" / "AviQtl"
        if dest_bin.exists():
            dest_bin.unlink()
        if not src_bin.exists():
            raise FileNotFoundError(f"実行ファイルが見つかりません: {src_bin}")
        shutil.copy2(src_bin, dest_bin)
        self.copy_assets(self.config.output_dir)
        self.logger.log(f"実行ファイル: {dest_bin}")


class ArchBuilder(LinuxBuilderBase):
    def __init__(self, config: BuildConfig, logger: Logger):
        super().__init__(config, logger)
        self.container_name = "archlinux-aviqtl"
        self.image_name = "archlinux:latest"


        if not self.use_container:
            self.logger.log("No Container モード: システムパッケージのインストールをスキップ")
            return
            
        super().install_dependencies()
        if self.config.is_offline:
            return
        self.logger.log("pacman ロックファイルを確認中...")
        try:
            self.run_cmd(["sudo", "rm", "-f", "/var/lib/pacman/db.lck"])
        except Exception:
            pass
        self.logger.log("pacman -Syu --needed を実行中...")
        deps = [
            "base-devel", "git", "cmake", "ninja", "clang", "mold", "zip",
            "mesa", "vulkan-devel", "spirv-tools", "libxkbcommon", "wayland", "wayland-protocols",
            "libffi", "ffmpeg", "luajit", "fftw",
            "qt6-base", "qt6-wayland", "qt6-declarative", "qt6-quick3d", "qt6-multimedia",
            "qt6-shadertools", "qt6-svg", "qt6-5compat", "qt6-tools",
            "lilv", "ladspa", "carla",
            "openmp", "extra-cmake-modules",
            # carla が間接依存する fluidsynth (--as-needed 対策)
            "fluidsynth",
        ]
        self.run_cmd(["sudo", "pacman", "-Syu", "--needed", "--noconfirm"] + deps)
        self.logger.log("Arch Linux 依存関係インストール完了")

    def get_archive_name(self) -> str:
        return "AviQtl-Arch-Linux-x86_64"


class Msys2Builder(PlatformBuilder):
    CARLA_DLL_PATTERNS = (
        "carla*.dll",
        "libcarla*.dll",
        "CarlaVst*.dll",
    )
    CARLA_DLL_ALIASES = {
        "libcarla_native-plugin.dll": "CarlaNativePlugin.dll",
    }

    def install_dependencies(self):
        if self.config.is_offline:
            self.logger.log("依存関係インストールをスキップします (--offline)")
            self.setup_carla_sdk(is_windows=True)
            return
        if "MSYSTEM" not in os.environ:
            self.logger.log("警告: MSYS2 環境外です。依存関係インストールをスキップします。")
            self.setup_carla_sdk(is_windows=True)
            return
        if os.environ["MSYSTEM"] != "UCRT64":
            self.logger.log("警告: UCRT64 以外の環境が検出されました。UCRT64 を推奨します。")
        self.logger.log("pacman -Syu --needed を実行中...")
        deps = [
            "mingw-w64-ucrt-x86_64-toolchain", "mingw-w64-ucrt-x86_64-cmake",
            "mingw-w64-ucrt-x86_64-ninja", "git",
            "mingw-w64-ucrt-x86_64-qt6",
            "mingw-w64-ucrt-x86_64-ffmpeg", "mingw-w64-ucrt-x86_64-luajit",
            "mingw-w64-ucrt-x86_64-vulkan-loader", "mingw-w64-ucrt-x86_64-vulkan-headers",
            "mingw-w64-ucrt-x86_64-pkg-config", "mingw-w64-ucrt-x86_64-mold",
            "mingw-w64-ucrt-x86_64-lilv", "mingw-w64-ucrt-x86_64-ladspa-sdk",
            "mingw-w64-ucrt-x86_64-curl", "mingw-w64-ucrt-x86_64-extra-cmake-modules",
            "zip", "mingw-w64-ucrt-x86_64-clang-tools-extra",
        ]
        self.run_cmd(["pacman", "-Syu", "--needed", "--noconfirm"] + deps)
        self.logger.log("MSYS2 依存関係インストール完了")
        self.setup_carla_sdk(is_windows=True)

    def get_cmake_config_cmd(self) -> List[str]:
        cmd = super().get_cmake_config_cmd()
        cmd.append("-DCMAKE_BUILD_TYPE=Release")
        if (self.config.source_dir / "vendor" / "carla").exists():
            cmd.append(f"-DCARLA_SDK_DIR={Path(self.config.source_dir / 'vendor' / 'carla').as_posix()}")
        cmd.append(str(self.config.source_dir))
        return cmd

    def package(self):
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
        
        # MSYS2 ucrt64 の Qt6 パッケージでは windeployqt6 が正式ツール名。
        # windeployqt (Qt5 用) は存在しないため windeployqt6 を優先し、
        # 見つからない場合のみ windeployqt にフォールバックする。
        deploy_tool = "windeployqt6" if shutil.which("windeployqt6") else "windeployqt"

        # windeployqt6 は QML スキャン時に内部で qmlimportscanner を QProcess 経由で
        # 子プロセス起動する。MSYS2 では qmlimportscanner.exe が share/qt6/bin/ にあるため、
        # そちらも PATH に追加する。
        deploy_exe = shutil.which(deploy_tool)
        if deploy_exe:
            qt_bin_dir = str(Path(deploy_exe).resolve().parent)
            qt_share_bin_dir = str(Path(deploy_exe).resolve().parent.parent / "share" / "qt6" / "bin")
            current_path = self.env.get("PATH", os.environ.get("PATH", ""))
            extra_dirs = []
            if qt_bin_dir not in current_path.split(os.pathsep):
                extra_dirs.append(qt_bin_dir)
            if qt_share_bin_dir not in current_path.split(os.pathsep):
                extra_dirs.append(qt_share_bin_dir)
            if extra_dirs:
                self.env["PATH"] = os.pathsep.join(extra_dirs + [current_path])
                self.logger.log(f"Qt bin を PATH に追加: {', '.join(extra_dirs)}")

        self.logger.log(f"{deploy_tool} を実行中...")
        self.run_cmd([
            deploy_tool,
            "--qmldir", str(self.config.source_dir / "ui" / "qml"),
            "--no-translations",
            "--release" if not self.config.is_debug else "--debug",
            str(dest_bin),
            "--dir", str(self.config.output_dir),
        ])
        with open(self.config.output_dir / "qt.conf", "w", encoding="utf-8") as f:
            f.write("[Paths]\nPlugins = .\n")
        self.logger.log(f"実行ファイル: {dest_bin}")

    def copy_carla_runtime(self):
        """Carlaランタイム一式 (runtime/ディレクトリ) をパッケージに同梱する"""
        carla_runtime = self.config.source_dir / "vendor" / "carla" / "runtime"
        if not carla_runtime.exists():
            self.logger.log("Carla runtime/ が見つかりません。Carla を同梱しません")
            return
        dest = self.config.output_dir / "Carla"
        if dest.exists():
            self.remove_tree(dest)
        shutil.copytree(str(carla_runtime), str(dest))
        self.logger.log(f"Carla ランタイムを同梱: {dest}")

    def find_carla_discovery_tool(self) -> Path | None:
        # runtime/ 内の carla-discovery-native.exe を探す
        carla_runtime = self.config.source_dir / "vendor" / "carla" / "runtime"
        candidate = carla_runtime / "carla-discovery-native.exe"
        if candidate.exists():
            return candidate
        return None

    def get_archive_name(self) -> str:
        return "AviQtl-MSYS2-UCRT64-x86_64"


class MsvcBuilder(PlatformBuilder):
    QT_ENV_VARS = ("QT_MSVC_DIR", "QT_DIR", "QTDIR")
    QT_VCPKG_PACKAGES = {
        "qtbase",
        "qtdeclarative",
        "qtquick3d",
        "qtmultimedia",
        "qtshadertools",
        "qtsvg",
        "qt5compat",
        "qttools",
    }
    CARLA_DLL_PATTERNS = (
        "carla*.dll",
        "libcarla*.dll",
        "CarlaVst*.dll",
    )
    CARLA_DLL_ALIASES = {
        "libcarla_native-plugin.dll": "CarlaNativePlugin.dll",
    }

    def __init__(self, config: BuildConfig, logger: Logger):
        super().__init__(config, logger)
        if os.name != "nt":
            raise RuntimeError("MSVC ビルドは Windows でのみ実行できます")
        self.vcpkg_root: Path | None = None
        default_triplet = "x64-windows" if self.config.is_debug else "x64-windows-release"
        self.vcpkg_triplet = os.environ.get("VCPKG_DEFAULT_TRIPLET", default_triplet)
        default_host_triplet = "x64-windows" if self.config.is_debug else self.vcpkg_triplet
        self.vcpkg_host_triplet = os.environ.get("VCPKG_DEFAULT_HOST_TRIPLET", default_host_triplet)
        self.vs_install_dir: Path | None = None
        self.cmake_path: str | None = None
        self.ninja_path: str | None = None
        self.qt_prefix: Path | None = None
        self.vcpkg_manifest_dir: Path | None = None

    def install_dependencies(self):
        self.setup_msvc_environment()
        self.ensure_vcpkg()
        self.qt_prefix = self.find_qt_prefix()
        if not self.qt_prefix:
            raise RuntimeError(
                "Qt MSVC が見つかりません。公式 Qt をインストールし、--qt-dir で Qt ルート/kit を指定するか、"
                "QT_MSVC_DIR, QT_DIR, QTDIR のいずれかを設定してください。"
            )
        self.logger.log(f"Qt: {self.qt_prefix}")
        self.write_external_qt_manifest()
        self.prepare_vcpkg_installed_tree()
        self.setup_vcpkg_environment()
        self.cmake_path = self.find_msvc_tool("cmake")
        self.ninja_path = self.find_msvc_tool("ninja")
        if not self.cmake_path:
            raise RuntimeError("cmake が見つかりません。CMake を PATH に追加してください")
        if not self.ninja_path:
            raise RuntimeError("ninja が見つかりません。Ninja を PATH に追加してください")
        if not shutil.which("cl", path=self.env.get("PATH")):
            raise RuntimeError("cl.exe が見つかりません。vcvarsall.bat の読み込みに失敗した可能性があります")
        self.setup_carla_sdk(is_windows=True)

    def ensure_vcpkg(self):
        vcpkg_root_env = self.env.get("VCPKG_ROOT") or os.environ.get("VCPKG_ROOT")
        if vcpkg_root_env:
            env_root = Path(vcpkg_root_env)
            if not (env_root / "scripts").is_dir():
                raise RuntimeError(f"VCPKG_ROOT が不正です。vcpkg の scripts ディレクトリが見つかりません: {env_root}")
            if not (env_root / "vcpkg.exe").exists() and not (env_root / "bootstrap-vcpkg.bat").exists():
                raise RuntimeError(f"VCPKG_ROOT が不完全です。vcpkg.exe または bootstrap-vcpkg.bat が見つかりません: {env_root}")

        self.vcpkg_root = self.find_vcpkg_root(need_executable=True)
        if self.vcpkg_root:
            self.logger.log(f"vcpkg 発見: {self.vcpkg_root}")
            self.env["VCPKG_ROOT"] = str(self.vcpkg_root)
            return

        incomplete = self.find_vcpkg_root(need_executable=False)
        if incomplete and (incomplete / "bootstrap-vcpkg.bat").exists():
            self.vcpkg_root = incomplete
            self.logger.log(f"vcpkg ディレクトリを検出 (vcpkg.exe なし)。ブートストラップを試みます: {self.vcpkg_root}")
            self._bootstrap_vcpkg()
            self.env["VCPKG_ROOT"] = str(self.vcpkg_root)
            return

        self.vcpkg_root = self.config.source_dir / "vcpkg"
        if self.config.is_offline:
            raise RuntimeError("vcpkg が見つかりません。オフラインモードでは自動取得できないため、VCPKG_ROOT を設定してください。")
        if not shutil.which("git", path=self.env.get("PATH")):
            raise RuntimeError("git が見つかりません。vcpkg の自動取得には Git が必要です。Git をインストールするか VCPKG_ROOT を設定してください。")
        if self.vcpkg_root.exists() and not (self.vcpkg_root / "scripts").is_dir():
            self.logger.log(f"不完全な vcpkg ディレクトリを削除します: {self.vcpkg_root}")
            self.remove_tree(self.vcpkg_root)
        self.logger.log(f"vcpkg が見つかりません。{self.vcpkg_root} にクローン中...")
        self.run_cmd(["git", "clone", "--depth", "1", "https://github.com/microsoft/vcpkg.git", str(self.vcpkg_root)], force_host=True)
        self._bootstrap_vcpkg()
        self.env["VCPKG_ROOT"] = str(self.vcpkg_root)

    def _bootstrap_vcpkg(self):
        bootstrap = self.vcpkg_root / "bootstrap-vcpkg.bat"
        if not bootstrap.exists():
            raise RuntimeError(f"vcpkg のブートストラップスクリプトが見つかりません: {bootstrap}")
        self.logger.log("vcpkg をブートストラップ中...")
        self.run_cmd([str(bootstrap)], force_host=True)
        if not (self.vcpkg_root / "vcpkg.exe").exists():
            raise RuntimeError("vcpkg のブートストラップに失敗しました。vcpkg.exe が生成されていません。ネットワーク接続を確認してください。")
        self.logger.log("vcpkg の準備完了")

    def prepare_vcpkg_installed_tree(self):
        installed_root = self.config.work_dir / "vcpkg_installed"
        marker = self.config.work_dir / ".vcpkg-triplets"
        expected = (
            f"target={self.vcpkg_triplet}\n"
            f"host={self.vcpkg_host_triplet}\n"
            f"qt={self.qt_prefix or 'vcpkg'}\n"
        )
        status_file = installed_root / "vcpkg" / "status"
        info_dir = installed_root / "vcpkg" / "info"
        has_installed_status = (
            status_file.exists()
            and "Status: install ok installed" in status_file.read_text(encoding="utf-8", errors="ignore")
        )
        has_package_lists = info_dir.exists() and any(info_dir.glob("*.list"))
        is_consistent = not has_installed_status or has_package_lists
        if marker.exists() and marker.read_text(encoding="utf-8") == expected and is_consistent:
            return

        if installed_root.exists():
            self.logger.log("古い、または不完全な vcpkg_installed を削除します")
            self.remove_tree(installed_root)
        self.config.work_dir.mkdir(parents=True, exist_ok=True)
        marker.write_text(expected, encoding="utf-8")

    def write_external_qt_manifest(self):
        manifest_in = self.config.source_dir / "vcpkg.json"
        manifest_out_dir = self.config.work_dir / "vcpkg-manifest"
        manifest_out = manifest_out_dir / "vcpkg.json"
        data = json.loads(manifest_in.read_text(encoding="utf-8"))
        data["name"] = f"{data.get('name', 'aviqtl')}-msvc-external-qt"
        data["dependencies"] = [
            dep for dep in data.get("dependencies", [])
            if (dep if isinstance(dep, str) else dep.get("name")) not in self.QT_VCPKG_PACKAGES
        ]
        manifest_out_dir.mkdir(parents=True, exist_ok=True)
        manifest_out.write_text(json.dumps(data, indent=4) + "\n", encoding="utf-8")
        self.vcpkg_manifest_dir = manifest_out_dir

    def configure(self):
        self.run_cmd(self.get_cmake_config_cmd())

    def update_translations(self):
        self.run_cmd([self.cmake_path or "cmake", "--build", str(self.config.work_dir), "--target", "AviQtl_lupdate"])

    def compile(self):
        j = multiprocessing.cpu_count()
        self.logger.log(f"並列ジョブ数: {j}")
        self.run_cmd([self.cmake_path or "cmake", "--build", str(self.config.work_dir), "-j", str(j)])

    def find_vcvarsall(self) -> Path | None:
        candidates = []
        for var in ("VCVARSALL", "VCVARSALL_BAT"):
            value = os.environ.get(var)
            if value:
                candidates.append(Path(value))

        vswhere_roots = [
            Path(os.environ.get("ProgramFiles(x86)", r"C:\Program Files (x86)")) / "Microsoft Visual Studio" / "Installer" / "vswhere.exe",
            Path(r"C:\Program Files (x86)\Microsoft Visual Studio\Installer\vswhere.exe"),
        ]
        for vswhere in vswhere_roots:
            if not vswhere.exists():
                continue
            for args in (
                ["-latest", "-products", "*", "-requires", "Microsoft.VisualStudio.Component.VC.Tools.x86.x64", "-property", "installationPath"],
                ["-latest", "-products", "*", "-property", "installationPath"],
            ):
                try:
                    result = subprocess.run([str(vswhere)] + args, capture_output=True, text=True, encoding=locale.getpreferredencoding(False), errors="replace")
                except OSError:
                    continue
                if result.returncode == 0 and result.stdout.strip():
                    candidates.append(Path(result.stdout.strip()) / "VC" / "Auxiliary" / "Build" / "vcvarsall.bat")

        for var in ("VSINSTALLDIR", "VCINSTALLDIR"):
            value = os.environ.get(var)
            if value:
                root = Path(value)
                candidates.append(root / "VC" / "Auxiliary" / "Build" / "vcvarsall.bat")
                candidates.append(root.parent / "Auxiliary" / "Build" / "vcvarsall.bat")

        candidates.extend([
            Path(r"C:\Program Files\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat"),
            Path(r"C:\Program Files\Microsoft Visual Studio\2022\Community\VC\Auxiliary\Build\vcvarsall.bat"),
            Path(r"C:\Program Files\Microsoft Visual Studio\2022\Professional\VC\Auxiliary\Build\vcvarsall.bat"),
            Path(r"C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Auxiliary\Build\vcvarsall.bat"),
        ])

        for candidate in candidates:
            if candidate.exists():
                return candidate
        return None

    def setup_msvc_environment(self):
        vcvarsall = self.find_vcvarsall()
        if not vcvarsall:
            raise RuntimeError("vcvarsall.bat が見つかりません。Visual Studio Build Tools の C++ ツールセットをインストールしてください")
        self.logger.log(f"vcvarsall: {vcvarsall}")
        self.vs_install_dir = vcvarsall.parents[3]
        wrapper_path = None
        try:
            with tempfile.NamedTemporaryFile("w", suffix=".bat", delete=False, encoding="utf-8") as wrapper:
                wrapper.write("@echo off\n")
                wrapper.write(f'call "{vcvarsall}" x64 > nul\n')
                wrapper.write("if errorlevel 1 exit /b %errorlevel%\n")
                wrapper.write("set\n")
                wrapper_path = wrapper.name
            proc = subprocess.run(
                ["cmd.exe", "/d", "/c", wrapper_path],
                capture_output=True,
                text=True,
                encoding=locale.getpreferredencoding(False),
                errors="replace",
            )
            if proc.returncode != 0:
                raise RuntimeError(f"vcvarsall.bat の実行に失敗しました:\n{proc.stdout}\n{proc.stderr}")
            for line in proc.stdout.splitlines():
                if "=" not in line:
                    continue
                key, _, value = line.partition("=")
                self.env[key] = value
            if "Path" in self.env:
                self.env["PATH"] = self.env["Path"]
            elif "PATH" in self.env:
                self.env["Path"] = self.env["PATH"]
            self.sanitize_msvc_environment()
        finally:
            if wrapper_path:
                try:
                    Path(wrapper_path).unlink()
                except OSError:
                    pass

    def is_msys_path(self, path: str | None) -> bool:
        if not path:
            return False
        lowered = path.replace("/", "\\").lower()
        return "\\msys2\\" in lowered or "\\mingw" in lowered or "\\ucrt64\\" in lowered

    def sanitize_msvc_environment(self):
        for name in ("PATH", "Path", "PKG_CONFIG_PATH", "CMAKE_PREFIX_PATH"):
            value = self.env.get(name)
            if not value:
                continue
            filtered = [part for part in value.split(os.pathsep) if part and not self.is_msys_path(part)]
            if filtered:
                self.env[name] = os.pathsep.join(filtered)
            else:
                self.env.pop(name, None)
        if "PATH" in self.env:
            self.env["Path"] = self.env["PATH"]
        elif "Path" in self.env:
            self.env["PATH"] = self.env["Path"]

    def find_msvc_tool(self, name: str) -> str | None:
        exe = f"{name}.exe"
        env_name = f"{name.upper()}_EXE"
        if self.env.get(env_name) and Path(self.env[env_name]).exists():
            return self.env[env_name]

        candidates = []
        if self.vs_install_dir:
            if name == "cmake":
                candidates.append(self.vs_install_dir / "Common7" / "IDE" / "CommonExtensions" / "Microsoft" / "CMake" / "CMake" / "bin" / exe)
            elif name == "ninja":
                candidates.append(self.vs_install_dir / "Common7" / "IDE" / "CommonExtensions" / "Microsoft" / "CMake" / "Ninja" / exe)
        if self.vcpkg_root:
            candidates.append(self.vcpkg_root / "downloads" / "tools" / name / exe)

        for candidate in candidates:
            if candidate.exists():
                return str(candidate)

        found = shutil.which(exe, path=self.env.get("PATH"))
        if found and not self.is_msys_path(found):
            return found
        if found:
            self.logger.log(f"警告: MSYS2/MinGW の {exe} を検出したため MSVC ビルドでは使用しません: {found}")
        return None

    def find_vcpkg_root(self, *, need_executable: bool = True) -> Path | None:
        candidates = []
        vcpkg_root_env = self.env.get("VCPKG_ROOT") or os.environ.get("VCPKG_ROOT")
        if vcpkg_root_env:
            candidates.append(Path(vcpkg_root_env))
        candidates.extend([
            Path.home() / "vcpkg",
            Path(r"C:\vcpkg"),
            self.config.source_dir / "vcpkg",
        ])
        if self.vs_install_dir:
            candidates.append(self.vs_install_dir / "VC" / "vcpkg")
        for candidate in candidates:
            if not (candidate / "scripts").is_dir():
                continue
            if need_executable and not (candidate / "vcpkg.exe").exists():
                continue
            return candidate
        return None

    def vcpkg_installed_dir(self) -> Path:
        return self.config.work_dir / "vcpkg_installed" / self.vcpkg_triplet

    def setup_vcpkg_environment(self):
        if not self.vcpkg_root:
            return
        installed = self.vcpkg_installed_dir()
        self.env["VCPKG_DEFAULT_TRIPLET"] = self.vcpkg_triplet
        self.env["VCPKG_DEFAULT_HOST_TRIPLET"] = self.vcpkg_host_triplet
        paths = [
            installed / "bin",
            installed / "tools" / "pkgconf",
            installed / "tools" / "pkg-config",
        ]
        if self.qt_prefix:
            paths.insert(0, self.qt_prefix / "bin")
        else:
            paths.append(installed / "tools" / "Qt6" / "bin")
        self.env["PATH"] = os.pathsep.join([str(path) for path in paths if path.exists()] + [self.env.get("PATH", "")])
        self.env["Path"] = self.env["PATH"]
        pkg_paths = [installed / "lib" / "pkgconfig"]
        if self.config.is_debug:
            pkg_paths.append(installed / "debug" / "lib" / "pkgconfig")
        existing_pkg_path = self.env.get("PKG_CONFIG_PATH", "")
        self.env["PKG_CONFIG_PATH"] = os.pathsep.join([str(path) for path in pkg_paths if path.exists()] + ([existing_pkg_path] if existing_pkg_path else []))
        self.logger.log(f"vcpkg: {self.vcpkg_root} (target={self.vcpkg_triplet}, host={self.vcpkg_host_triplet}), installed: {installed}")

    def resolve_qt_prefix(self, path: Path) -> Path | None:
        if (path / "bin" / "windeployqt.exe").exists():
            return path
        candidates = sorted(path.glob("*/*msvc*_64/bin/windeployqt.exe"), reverse=True)
        if candidates:
            return candidates[0].parent.parent
        return None

    def find_qt_prefix(self) -> Path | None:
        if self.config.qt_dir:
            qt_prefix = self.resolve_qt_prefix(self.config.qt_dir)
            if qt_prefix:
                return qt_prefix
            raise RuntimeError(f"指定された Qt ディレクトリから MSVC 版 Qt を検出できません: {self.config.qt_dir}")

        for name in self.QT_ENV_VARS:
            value = self.env.get(name) or os.environ.get(name)
            if value:
                qt_prefix = self.resolve_qt_prefix(Path(value))
                if qt_prefix:
                    return qt_prefix
        deployqt = shutil.which("windeployqt", path=self.env.get("PATH"))
        if deployqt and not self.is_msys_path(deployqt):
            return Path(deployqt).parent.parent
        if deployqt:
            self.logger.log(f"警告: MSYS2/MinGW の windeployqt を検出したため MSVC ビルドでは使用しません: {deployqt}")
        for qt_root in (Path(r"C:\Qt"),):
            if qt_root.exists():
                qt_prefix = self.resolve_qt_prefix(qt_root)
                if qt_prefix:
                    return qt_prefix
        return None

    def get_cmake_config_cmd(self) -> List[str]:
        cmd = [
            self.cmake_path or "cmake", "-B", str(self.config.work_dir), "-G", "Ninja",
            f"-DCMAKE_BUILD_TYPE={self.config.build_type}",
            f"-DCMAKE_MAKE_PROGRAM={self.ninja_path or 'ninja'}",
        ]
        cmd.extend(["-DCMAKE_C_COMPILER=cl", "-DCMAKE_CXX_COMPILER=cl"])
        if self.vcpkg_root:
            cmd.extend([
                f"-DCMAKE_TOOLCHAIN_FILE={self.vcpkg_root / 'scripts/buildsystems/vcpkg.cmake'}",
                f"-DVCPKG_TARGET_TRIPLET={self.vcpkg_triplet}",
                f"-DVCPKG_HOST_TRIPLET={self.vcpkg_host_triplet}",
                f"-DVCPKG_OVERLAY_TRIPLETS={self.config.source_dir / 'triplets'}",
            ])
            if self.vcpkg_manifest_dir:
                cmd.append(f"-DVCPKG_MANIFEST_DIR={self.vcpkg_manifest_dir}")
        if self.qt_prefix:
            cmd.append(f"-DCMAKE_PREFIX_PATH={self.qt_prefix}")
        if (self.config.source_dir / "vendor" / "carla").exists():
            cmd.append(f"-DCARLA_SDK_DIR={Path(self.config.source_dir / 'vendor' / 'carla').as_posix()}")
        cmd.append(str(self.config.source_dir))
        return cmd

    def find_windeployqt(self) -> str | None:
        if self.qt_prefix:
            deployqt = self.qt_prefix / "bin" / "windeployqt.exe"
            if deployqt.exists():
                return str(deployqt)
        deployqt = shutil.which("windeployqt", path=self.env.get("PATH"))
        if deployqt and not self.is_msys_path(deployqt):
            return deployqt
        if deployqt:
            self.logger.log(f"警告: MSYS2/MinGW の windeployqt を検出したため MSVC ビルドでは使用しません: {deployqt}")
        return None

    def package(self):
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

        deployqt = self.find_windeployqt()
        if not deployqt:
            raise RuntimeError("windeployqt が見つかりません。Qt MSVC の bin ディレクトリを PATH に追加するか QT_MSVC_DIR を設定してください")
        self.logger.log("windeployqt を実行中...")
        self.run_cmd([
            deployqt,
            "--qmldir", str(self.config.source_dir / "ui/qml"),
            "--no-translations", "--no-compiler-runtime",
            "--release" if not self.config.is_debug else "--debug",
            str(dest_bin),
            "--dir", str(self.config.output_dir),
        ])
        with open(self.config.output_dir / "qt.conf", "w", encoding="utf-8") as f:
            f.write("[Paths]\nPlugins = .\n")
        self.logger.log(f"実行ファイル: {dest_bin}")

    def get_archive_name(self) -> str:
        return "AviQtl-MSVC-x86_64"

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
        return None


class XcodeBuilder(PlatformBuilder):
    def install_dependencies(self):
        if self.config.is_offline:
            self.logger.log("依存関係インストールをスキップします (--offline)")
            return # Early exit if offline build
        if not shutil.which("brew"):
            raise RuntimeError("Homebrew が見つかりません") # Homebrew is essential for macOS
        self.logger.log("brew install を実行中...")
        # Removed KDE specific dependencies
        deps = [
            "cmake", "ninja", "qt6", "ffmpeg", "luajit",
            "vulkan-headers", "vulkan-loader", "spirv-tools", "pkg-config",
            "lilv", "extra-cmake-modules", "carla",
        ]
        self.run_cmd(["brew", "install"] + deps)
        self.logger.log("macOS 依存関係インストール完了")
        
        # macOS は Universal バイナリを使用

    def get_cmake_config_cmd(self) -> List[str]:
        cmd = super().get_cmake_config_cmd()
        cmd.append("-DCMAKE_BUILD_TYPE=Release")
        try:
            brew_prefix = subprocess.check_output(["brew", "--prefix"], text=True).strip()
        except Exception:
            brew_prefix = "/opt/homebrew"
        cmd.append(f"-DCMAKE_PREFIX_PATH={brew_prefix}")
        cmd.append(str(self.config.source_dir))
        return cmd

    def package(self):
        self.config.output_dir.mkdir(parents=True, exist_ok=True)
        src_app = self.config.work_dir / "bin" / "AviQtl.app"
        dest_app = self.config.output_dir / "AviQtl.app"
        if dest_app.exists():
            self.remove_tree(dest_app)
        if not src_app.exists():
            raise FileNotFoundError(f"App バンドルが見つかりません: {src_app}")
        shutil.copytree(src_app, dest_app)
        self.copy_assets(dest_app / "Contents/Resources")
        self.logger.log("macdeployqt を実行中...")
        qt_prefix = subprocess.check_output(["brew", "--prefix", "qt6"], text=True).strip()
        self.run_cmd([
            f"{qt_prefix}/bin/macdeployqt", str(dest_app),
            f"-qmldir={self.config.source_dir / 'ui/qml'}",
            "-verbose=1",
            "-no-codesign",
        ])

        self.logger.log("RPATH を修正中...")
        binary = dest_app / "Contents/MacOS/AviQtl"
        rpaths_to_remove = [
            "/opt/homebrew/lib",
        ]
        for rp in rpaths_to_remove:
            try:
                self.run_cmd(["install_name_tool", "-delete_rpath", rp, str(binary)])
                self.logger.log(f"  削除: {rp}")
            except subprocess.CalledProcessError:
                self.logger.log(f"  スキップ: {rp}")

        # 既知のQtバグ対策: macdeployqt が AviQtl QMLモジュールを Resources/qml にコピーすると、
        # qrc:/ 内の同名モジュールと衝突して "AviQtl is ambiguous" エラーが発生する。
        # リソースシステム内にのみ存在させることで回避する。
        self.logger.log("QMLモジュールの重複コピーをクリーンアップ中...")
        duplicate_qml_dirs = [
            dest_app / "Contents/Resources/qml/AviQtl",
            dest_app / "Contents/PlugIns/qml/AviQtl",
        ]
        for d in duplicate_qml_dirs:
            if d.exists():
                self.remove_tree(d)
                self.logger.log(f"  削除: {d}")

        # carla-discovery-native をバイナリ同梱
        self.logger.log("carla-discovery-native を同梱中...")
        try:
            carla_prefix = subprocess.check_output(["brew", "--prefix", "carla"], text=True).strip()
            # Homebrew の構造に合わせてパスを探索
            carla_bin_src = Path(carla_prefix) / "lib/carla/carla-discovery-native"
            if not carla_bin_src.exists():
                 carla_bin_src = next(Path(carla_prefix).glob("**/carla-discovery-native"), None)
            
            carla_bin_dst = dest_app / "Contents/MacOS/carla-discovery-native"
            if carla_bin_src and carla_bin_src.exists():
                shutil.copy2(carla_bin_src, carla_bin_dst)
                self.logger.log(f"  同梱完了: {carla_bin_src}")
        except Exception as e:
            self.logger.log(f"  警告: carla-discovery-native の特定に失敗しました: {e}")

        self.logger.log("codesign を実行中...")
        self.run_cmd(["codesign", "--deep", "--force", "--sign", "-", str(dest_app)])
        self.logger.log(f"App バンドル: {dest_app}")

    def get_archive_name(self) -> str:
        return "AviQtl-macOS-Xcode-Universal"

    def create_zip(self, archive_name: str):
        shutil.make_archive(
            str(self.config.dist_dir / archive_name), "zip",
            root_dir=self.config.output_dir, base_dir="AviQtl.app",
        )


BUILDERS: dict[str, Type[PlatformBuilder]] = {
    "arch": ArchBuilder,
    "msys2": Msys2Builder,
    "msvc": MsvcBuilder,
    "xcode": XcodeBuilder,
}


class BuildWorker(QtCore.QThread):
    progress_signal = QtCore.Signal(int, str)
    log_signal = QtCore.Signal(str)
    finished_signal = QtCore.Signal(bool, str)

    def __init__(self, config: BuildConfig):
        super().__init__()
        self.config = config
        self.builder: Optional[PlatformBuilder] = None
        self.cancel_requested = False

    def run(self):
        try:
            logger = Logger(self.log_signal.emit, self.progress_signal.emit)
            self.builder = BUILDERS[self.config.target](self.config, logger)
            if self.cancel_requested:
                self.builder.cancel()
            self.builder.build()
            self.finished_signal.emit(True, "ビルド成功")
        except Exception as e:
            self.finished_signal.emit(False, str(e))

    def cancel(self):
        self.cancel_requested = True
        self.requestInterruption()
        if self.builder:
            self.builder.cancel()


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="BUILD.py",
        description="AviQtl ビルドスクリプト",
        formatter_class=argparse.RawTextHelpFormatter,
        epilog=(
            "使用例:\n"
            "  python BUILD.py --arch\n"
            "  python BUILD.py --msys2 --debug\n"
            "  python BUILD.py --msvc --qt-dir F:\\Qt\n"
            "  python BUILD.py --qt-dir F:\\Qt  # Windows では既定で MSVC\n"
            "  python BUILD.py --xcode --offline\n"
        ),
    )
    target_group = parser.add_mutually_exclusive_group(required=False)
    target_group.add_argument(
        "--arch", action="store_true",
        help="Linux (Arch) 向けビルド",
    )
    target_group.add_argument(
        "--msys2", action="store_true",
        help="Windows (MSYS2) 向けビルド",
    )
    target_group.add_argument(
        "--msvc", action="store_true",
        help="Windows (MSVC x64) 向けビルド。vcvarsall.bat を自動検出して MSVC 環境を読み込みます。",
    )
    target_group.add_argument(
        "--xcode", action="store_true",
        help="macOS (Xcode) 向けビルド",
    )
    parser.add_argument(
        "--offline", action="store_true",
        help="依存関係のダウンロード・インストールをスキップします。既に環境が整っている場合に使用してください。",
    )
    parser.add_argument(
        "--debug", action="store_true",
        help="Debug ビルドを実行します。デフォルトは Release です。",
    )
    parser.add_argument(
        "--no-container", action="store_true",
        help="Linux ターゲットでコンテナを使わずホスト環境でビルドします (非推奨)。",
    )
    parser.add_argument(
        "--qt-dir", type=Path,
        help="MSVC ビルドで使用する公式 Qt のディレクトリ。未指定時は QT_MSVC_DIR, QT_DIR, QTDIR, PATH を確認します。MSVC では Qt が必須です。",
    )
    return parser.parse_args()


def determine_target(args: argparse.Namespace, system_name: str | None = None) -> str | None:
    if args.arch:
        return "arch"
    if args.msys2:
        return "msys2"
    if args.msvc:
        return "msvc"
    if args.xcode:
        return "xcode"
    system_name = (system_name or platform.system()).lower()
    return {
        "linux": "arch",
        "windows": "msvc",
        "darwin": "xcode",
    }.get(system_name)


def main():
    if hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    if hasattr(sys.stderr, "reconfigure"):
        sys.stderr.reconfigure(encoding="utf-8", errors="replace")

    args = parse_args()

    # ターゲットの決定
    target = determine_target(args)

    if not target:
        print("エラー: ビルドターゲットを特定できません。--arch, --msys2, --msvc, --xcode のいずれかを指定してください。")
        sys.exit(1)

    source_dir = Path.cwd()
    config = BuildConfig(
        source_dir=source_dir,
        temp_base=source_dir / ".build_tmp",
        output_dir=source_dir / "build",
        target=target,
        is_debug=args.debug,
        # コンテナ利用は Linux かつ --no-container が指定されていない場合のみ
        use_container=(target == "arch" and not args.no_container),
        is_offline=args.offline,
        qt_dir=args.qt_dir,
    )

    app = QtCore.QCoreApplication(sys.argv)
    worker = BuildWorker(config)
    worker.log_signal.connect(print)
    worker.progress_signal.connect(lambda val, msg: print(f"[{val}%] {msg}"))
    mode = "Container" if config.use_container else "No Container"
    cancelled = False

    def cancel_build():
        nonlocal cancelled
        if cancelled:
            print("\n強制終了します。")
            os._exit(130)
        cancelled = True
        print("\nビルドをキャンセルしています...")
        worker.cancel()

    signal.signal(signal.SIGINT, lambda _signum, _frame: cancel_build())

    # Qt のイベントループ中でも Python が SIGINT を処理できるようにする。
    sigint_timer = QtCore.QTimer()
    sigint_timer.timeout.connect(lambda: None)
    sigint_timer.start(200)

    def on_finished(success: bool, msg: str):
        if cancelled:
            print("\nビルドをキャンセルしました。")
            app.exit(130)
            return
        if success:
            app.quit()
        else:
            print(f"\nビルド失敗: {msg}")
            app.exit(1)

    worker.finished_signal.connect(on_finished)
    print(f"ビルド開始 | ターゲット={target} | {config.build_type} | {mode} | オフライン={config.is_offline}")
    worker.start()
    try:
        exit_code = app.exec()
    except KeyboardInterrupt:
        cancel_build()
        worker.wait()
        exit_code = 130
    if worker.isRunning():
        worker.wait(3000)
    sys.exit(exit_code)


if __name__ == "__main__":
    main()
