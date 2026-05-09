import sys
import os
import subprocess
import shutil
import multiprocessing
import argparse
import shlex
import urllib.request
import platform
from pathlib import Path
from dataclasses import dataclass
from typing import Callable, List, Type
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

    def build(self):
        self.logger.progress(10, f"{self.config.build_type} ビルド開始")
        self.logger.section("依存関係の確認")
        self.install_dependencies()
        self.logger.section("CMake 設定")
        self.configure()
        self.logger.section("翻訳ファイル更新")
        self.update_translations()
        self.logger.section("コンパイル")
        self.compile()
        self.logger.section("パッケージング")
        self.package()
        self.logger.section("アーカイブ作成")
        self.archive()
        self.logger.progress(100, "完了")

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
        in_container = self.use_container and not force_host
        display_cmd = ' '.join(cmd) if isinstance(cmd, list) else cmd
        tag = "[Container]" if in_container else "[Host]"
        self.logger.log(f"{tag} {display_cmd}")

        actual_cmd = cmd
        if in_container:
            cmd_str = shlex.join(cmd)
            cwd = os.getcwd()
            inner = f"cd {shlex.quote(cwd)} && {cmd_str}"
            actual_cmd = f"distrobox enter {self.container_name} -- bash -lc {shlex.quote(inner)}"
            shell = True

        proc = subprocess.Popen(
            actual_cmd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
            shell=shell,
            env=self.env,
        )
        for line in proc.stdout:
            self.logger.log(line.rstrip())
        proc.wait()
        if proc.returncode != 0:
            raise subprocess.CalledProcessError(proc.returncode, actual_cmd)

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

        if not inc_dir.exists():
            self.logger.log("Carla ヘッダーを取得中...")
            temp_clone = self.config.source_dir / ".carla_tmp"
            if temp_clone.exists():
                shutil.rmtree(temp_clone)
            self.run_cmd(["git", "clone", "--depth", "1", "https://github.com/falkTX/Carla.git", str(temp_clone)], force_host=True)
            inc_dir.mkdir(parents=True, exist_ok=True)
            shutil.copytree(temp_clone / "source/includes", inc_dir, dirs_exist_ok=True)
            shutil.rmtree(temp_clone)

        if is_windows and not (lib_dir / "carla-standalone.dll").exists():
            self.logger.log("Carla Windows バイナリをダウンロード中...")
            lib_dir.mkdir(parents=True, exist_ok=True)
            version = "2.6.0"
            url = f"https://github.com/falkTX/Carla/releases/download/v{version}/Carla_{version}-win64.zip"
            self.download_and_extract(url, sdk_dir)
            for lib_file in sdk_dir.rglob("*.dll"):
                shutil.move(str(lib_file), lib_dir / lib_file.name)

    def setup_filament_sdk(self, platform_suffix: str, lib_arch: str, is_universal: bool = False):
        filament_dir = self.config.source_dir / "vendor" / "filament"
        if (filament_dir / "include" / "filament" / "Engine.h").exists():
            return

        self.logger.log(f"Filament バイナリ ({platform_suffix}) を取得中...")
        version = "1.71.3"
        ext = "zip" if "windows" in platform_suffix else "tgz"
        
        suffix = "mac" if is_universal else platform_suffix
        url = f"https://github.com/google/filament/releases/download/v{version}/filament-v{version}-{suffix}.{ext}"
        
        filament_dir.mkdir(parents=True, exist_ok=True)
        self.download_and_extract(url, filament_dir)

        # CMakeLists.txt の期待する構造 (lib/x86_64 or lib/arm64) に合わせる
        dest_lib = filament_dir / "lib" / lib_arch
        if not dest_lib.exists():
            src_lib = filament_dir / "lib" / lib_arch if is_universal else filament_dir / "lib"
            if not src_lib.exists(): src_lib = filament_dir / "lib" # 構造が直下の場合
            
            temp_lib = filament_dir / "_lib_tmp"
            shutil.move(str(src_lib), str(temp_lib))
            dest_lib.mkdir(parents=True, exist_ok=True)
            for item in temp_lib.iterdir():
                shutil.move(str(item), str(dest_lib / item.name))
            shutil.rmtree(temp_lib)

    def download_and_extract(self, url: str, dest_dir: Path):
        tmp_file = dest_dir / "download.tmp"
        try:
            self.logger.log(f"  Download: {url}")
            urllib.request.urlretrieve(url, tmp_file)
            self.logger.log(f"  Extracting to {dest_dir}...")
            
            # .tgz (tar.gz) の場合は shutil.unpack_archive が自動判別するが、
            # 環境によって .tgz を認識しない場合があるためフォーマットを明示
            fmt = None
            if url.endswith(".tgz"):
                fmt = "gztar"
            
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
            self.logger.log("ホスト環境でビルドするため、コンテナ作成をスキップします")
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

    def install_dependencies(self):
        if not self.use_container:
            self.logger.log("ホスト環境（CI等）でのビルドのため、システムパッケージのインストールをスキップします")
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
            "mesa", "vulkan-devel", "libxkbcommon", "wayland", "wayland-protocols",
            "libffi", "ffmpeg", "luajit", "fftw",
            "qt6-base", "qt6-declarative", "qt6-quick3d", "qt6-multimedia",
            "qt6-shadertools", "qt6-svg", "qt6-5compat", "qt6-tools",
            "lilv", "ladspa", "carla",
            "openmp", "extra-cmake-modules",
            # vendor/filament は libc++ でビルドされているためリンクに必須
            "libc++",
            # carla が間接依存する fluidsynth (--as-needed 対策)
            "fluidsynth",
        ]
        self.run_cmd(["sudo", "pacman", "-Syu", "--needed", "--noconfirm"] + deps)
        self.logger.log("Arch Linux 依存関係インストール完了")

    def get_archive_name(self) -> str:
        return "AviQtl-Arch-Linux-x86_64"


class Msys2Builder(PlatformBuilder):
    def install_dependencies(self):
        if self.config.is_offline:
            self.logger.log("依存関係インストールをスキップします (--offline)")
            return
        if "MSYSTEM" not in os.environ:
            self.logger.log("警告: MSYS2 環境外です。依存関係インストールをスキップします。")
            return
        if os.environ["MSYSTEM"] != "UCRT64":
            self.logger.log("警告: UCRT64 以外の環境が検出されました。UCRT64 を推奨します。")
        self.logger.log("pacman -Syu --needed を実行中...")
        deps = [
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
        ]
        self.run_cmd(["pacman", "-Syu", "--needed", "--noconfirm"] + deps)
        self.logger.log("MSYS2 依存関係インストール完了")
        self.setup_carla_sdk(is_windows=True)
        self.setup_filament_sdk("windows", "x86_64")

    def get_cmake_config_cmd(self) -> List[str]:
        cmd = super().get_cmake_config_cmd()
        cmd.append("-DCMAKE_BUILD_TYPE=Release")
        if (self.config.source_dir / "vendor" / "carla").exists():
            cmd.append(f"-DCARLA_SDK_DIR={self.config.source_dir / 'vendor' / 'carla'}")
        cmd.append(str(self.config.source_dir))
        return cmd

    def package(self):
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
            f.write("[Paths]\nPlugins = .\n")
        self.logger.log(f"実行ファイル: {dest_bin}")

    def get_archive_name(self) -> str:
        return "AviQtl-MSYS2-UCRT64-x86_64"


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
            "vulkan-headers", "vulkan-loader", "pkg-config",
            "lilv", "extra-cmake-modules", "carla",
        ]
        self.run_cmd(["brew", "install"] + deps)
        self.logger.log("macOS 依存関係インストール完了")
        self.setup_carla_sdk()
        
        # macOS は Universal バイナリを使用
        self.setup_filament_sdk("mac", "arm64" if platform.machine() == "arm64" else "x86_64", is_universal=True)

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
            shutil.rmtree(dest_app)
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
                shutil.rmtree(d)
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
    "xcode": XcodeBuilder,
}


class BuildWorker(QtCore.QThread):
    progress_signal = QtCore.Signal(int, str)
    log_signal = QtCore.Signal(str)
    finished_signal = QtCore.Signal(bool, str)

    def __init__(self, config: BuildConfig):
        super().__init__()
        self.config = config

    def run(self):
        try:
            logger = Logger(self.log_signal.emit, self.progress_signal.emit)
            builder = BUILDERS[self.config.target](self.config, logger)
            builder.build()
            self.finished_signal.emit(True, "ビルド成功")
        except Exception as e:
            self.finished_signal.emit(False, str(e))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        prog="BUILD.py",
        description="AviQtl ビルドスクリプト",
        formatter_class=argparse.RawTextHelpFormatter,
        epilog=(
            "使用例:\n"
            "  python BUILD.py --arch\n"
            "  python BUILD.py --msys2 --debug\n"
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
    return parser.parse_args()


def main():
    args = parse_args()

    # ターゲットの決定: --no-container は Linux 専用なので暗黙的に arch とみなす
    if args.arch or args.no_container:
        target = "arch"
    elif args.msys2: target = "msys2"
    elif args.xcode: target = "xcode"
    else:
        # フラグによる指定がない場合のみ OS 判別を実施
        target = {
            "linux": "arch",
            "windows": "msys2",
            "darwin": "xcode"
        }.get(platform.system().lower())

    if not target:
        print("エラー: ビルドターゲットを特定できません。--arch, --msys2, --xcode のいずれかを指定してください。")
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
    )

    app = QtCore.QCoreApplication(sys.argv)
    worker = BuildWorker(config)
    worker.log_signal.connect(print)
    worker.progress_signal.connect(lambda val, msg: print(f"[{val}%] {msg}"))

    def on_finished(success: bool, msg: str):
        if success:
            app.quit()
        else:
            print(f"\nビルド失敗: {msg}")
            app.exit(1)

    worker.finished_signal.connect(on_finished)
    print(f"ビルド開始 | ターゲット={target} | {config.build_type} | オフライン={config.is_offline}")
    worker.start()
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
